//! The chat gallery enumerator — every image that exists in one conversation
//! (v4 `lib/photos/chat-gallery.ts`, `86d59660c`).
//!
//! A Salon conversation accumulates pictures from many directions, and each
//! direction records its picture somewhere different: an upload lands in
//! `files.linkedTo`, an `attach_image` re-show lands only in a message's
//! `attachments`, a Lantern backdrop lands in `files.linkedTo` *and*
//! `chats.storyBackgroundImageId` (and usually posts no message at all), a
//! participant's standing portrait lands in `characters.defaultImageId` and is
//! never tied to a chat in any way. There is no single table to read.
//!
//! This module is the single place that knows all nine of them. Nothing else —
//! not the `/chats/{id}/files` listing, not the sidebar count, not the gallery
//! modal — re-derives "which images are in this chat".
//!
//! Two facts shape everything here:
//!
//!  - **Ids are of two species.** `files.id` and `doc_mount_file_links.id` both
//!    appear in `attachments`, in `chats.characterAvatars`, in
//!    `characters.defaultImageId`. Every entry therefore carries its `idKind`
//!    and every action branches on it rather than guessing.
//!  - **Announcement messages are optional.** `postLanternImageNotification` is
//!    skipped when `alertCharactersOfLanternImages` is off, which is the
//!    default, so backgrounds and avatars usually have no message row at all.
//!    A gallery built by walking messages would miss most of them.
//!
//! **Entries are `serde_json::Value` objects, not a typed struct, and that is
//! deliberate.** v4's entry key order is its JS construction order, which
//! differs per pass (a portrait carries no `width`/`height`; a message
//! attachment carries `messageId` where a linked file carries `characterId`) —
//! and `EntryCollector::note_message` may APPEND `messageId` to an entry that
//! was built without one, landing it after `deletable`/`linkSummary` rather
//! than in a declaration slot. A `#[derive(Serialize)]` struct fixes one key
//! order for every shape and diverges on exactly that arm. `serde_json`'s
//! `preserve_order` gives insertion-order maps, so building the object in v4's
//! construction order and mutating through `insert` reproduces JS exactly.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::db::characters_read;
use crate::db::chats_messages_read;
use crate::db::chats_read;
use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::DbError;
use crate::photos::photo_link_summary::get_photo_link_summary_by_sha256;
use crate::photos::photos_paths::is_photos_relative_path;
use crate::photos::resolve_character_avatar::{resolve_character_avatar, AvatarKind};
use crate::services::mount_index::path_utils::native_text_attachment_mime;
use crate::tools::photo::encode_uri;

// ============================================================================
// Types
// ============================================================================

/// Every source, in the order the UI shows its filter chips (v4
/// `CHAT_GALLERY_SOURCES`). The `counts` map is seeded from this list, so the
/// response ALWAYS carries all seven keys in this order.
pub const CHAT_GALLERY_SOURCES: [&str; 7] = [
    "story-background",
    "avatar",
    "portrait",
    "generated",
    "attachment",
    "kept",
    "inline",
];

/// Sources whose records the chat itself owns and may therefore delete (v4
/// `OWNED_SOURCES`).
const OWNED_SOURCES: [&str; 4] = ["attachment", "generated", "story-background", "avatar"];

/// One mount-index attachment lifted off a message. Shared by the gallery and
/// by `chatFilesList`, which is why it carries the non-image fields (a
/// native-text document has no blob) the file listing needs. v4
/// `MountAttachmentEntry`.
#[derive(Clone, Debug)]
pub struct MountAttachmentEntry {
    /// `doc_mount_file_links.id`.
    pub id: String,
    pub filename: String,
    /// The API URL that serves the bytes (blob endpoint, or the files endpoint
    /// for a document).
    pub url: String,
    pub mime_type: String,
    pub size: i64,
    /// The announcing message's `createdAt` — a link row has no chat-scoped
    /// timestamp.
    pub created_at: Value,
    pub message_id: String,
    pub mount_point_id: String,
    pub relative_path: String,
    /// True when the bytes live in `doc_mount_blobs`; false for a native-text
    /// document.
    pub has_blob: bool,
    pub sha256: Option<String>,
}

// ============================================================================
// Pass 2 — the message walk (shared with chatFilesList)
// ============================================================================

/// Walk a chat's messages and resolve every attachment id that names a
/// mount-index file link (v4 `resolveMessageAttachmentEntries`).
///
/// Mount-file attachments are recorded *only* on the message that announced
/// them — `files.addLink` on a link id is a silent no-op, so there is no
/// `linkedTo` row to find them by. This is sources #4 (`attach_image` re-show
/// of a kept vault image) and #5 (a Librarian attach from a document store).
///
/// Lifted out of `chat_files_list` so the file listing and the gallery cannot
/// drift apart; the listing still owns its own response shape.
///
/// Best effort throughout: an id that resolves to nothing is skipped, and a
/// repository failure aborts the walk with a `warn` rather than failing the
/// request. An empty gallery is a worse answer than a short one, but a 500 is
/// worse than both.
///
/// ⚠ v4 carries an asymmetry, reproduced here: the legacy fallback resolves an
/// attachment id as a `fileId`, and then the seen set gains the LINK id while
/// the loop keeps testing attachment ids.
pub fn resolve_message_attachment_entries(
    mount: &Connection,
    events: &[Value],
    skip_ids: &HashSet<String>,
) -> Vec<MountAttachmentEntry> {
    let mut resolved: Vec<MountAttachmentEntry> = Vec::new();

    // v4 wraps the whole walk in one `try`: the first repository failure warns
    // and returns what has been resolved so far.
    if let Err(err) = walk_message_attachments(mount, events, skip_ids, &mut resolved) {
        tracing::warn!(
            error = %err,
            resolved_so_far = resolved.len(),
            "Failed to enumerate mount-file attachments"
        );
    }

    resolved
}

/// [`resolve_message_attachment_entries`] over a [`Db`](crate::db::runtime::Db)
/// rather than a borrowed mount-index connection — the shape `chat_files_list`
/// needs. A missing mount-index partition takes the same fail-soft arm any
/// other repository failure does.
pub fn resolve_message_attachment_entries_db(
    db: &crate::db::runtime::Db,
    events: &[Value],
    skip_ids: &HashSet<String>,
) -> Vec<MountAttachmentEntry> {
    match db
        .read_mount_index(|mount| Ok(resolve_message_attachment_entries(mount, events, skip_ids)))
    {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(
                error = %err,
                resolved_so_far = 0,
                "Failed to enumerate mount-file attachments"
            );
            Vec::new()
        }
    }
}

fn walk_message_attachments(
    mount: &Connection,
    events: &[Value],
    skip_ids: &HashSet<String>,
    resolved: &mut Vec<MountAttachmentEntry>,
) -> Result<(), DbError> {
    let mut seen: HashSet<String> = HashSet::new();
    {
        let links = DocMountFileLinksRepository::new(mount);
        let blobs = DocMountBlobsRepository::new(mount);
        let documents = DocMountDocumentsRepository::new(mount);
        for event in events {
            if event.get("type").and_then(Value::as_str) != Some("message") {
                continue;
            }
            let ids: Vec<String> = event
                .get("attachments")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let created_at = event.get("createdAt").cloned().unwrap_or(Value::Null);
            let message_id = event
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            for attachment_id in ids {
                if skip_ids.contains(&attachment_id) || seen.contains(&attachment_id) {
                    continue;
                }

                // Try as a link id (modern) or fall back to file id.
                let mount_link = match links.find_by_id_with_content(&attachment_id)? {
                    Some(l) => Some(l),
                    None => {
                        // v4's `findByFileId` returns the SAME joined shape, so
                        // re-read the winning row through the joined getter.
                        match links.find_by_file_id(&attachment_id)?.into_iter().next() {
                            Some(first) => links.find_by_id_with_content(&first.id)?,
                            None => None,
                        }
                    }
                };
                let Some(mount_link) = mount_link else {
                    continue;
                };
                if seen.contains(&mount_link.id) || skip_ids.contains(&mount_link.id) {
                    continue;
                }

                let filename = mount_link
                    .original_file_name
                    .clone()
                    .unwrap_or_else(|| mount_link.file_name.clone());
                let link_sha = Some(mount_link.sha256.clone()).filter(|s| !s.is_empty());

                if let Some(blob) = blobs.find_by_file_id(&mount_link.file_id)? {
                    resolved.push(MountAttachmentEntry {
                        id: mount_link.id.clone(),
                        filename,
                        url: build_blob_url(&mount_link.mount_point_id, &mount_link.relative_path),
                        mime_type: blob.stored_mime_type,
                        size: blob.size_bytes,
                        created_at: created_at.clone(),
                        message_id: message_id.clone(),
                        mount_point_id: mount_link.mount_point_id.clone(),
                        relative_path: mount_link.relative_path.clone(),
                        has_blob: true,
                        // v4 `:198` is `blob.sha256 ?? mountLink.sha256 ?? null` —
                        // NULLISH, so a stored empty-string blob hash would survive
                        // as `''` there where this filter falls through to the
                        // link's. Recorded, not matched: no fixture stores `''`
                        // (the column is written from a real digest), and the
                        // walk is shared with the chat files listing whose bytes
                        // this round proved unchanged.
                        sha256: Some(blob.sha256).filter(|s| !s.is_empty()).or(link_sha),
                    });
                    seen.insert(mount_link.id);
                    continue;
                }

                // No blob → a native-text document (Bug 38). Surface it from the
                // document row so the attached markdown shows in the file list.
                // The gallery drops these on the mime check.
                let Some(text_mime) = native_text_attachment_mime(&mount_link.relative_path) else {
                    continue;
                };
                if documents
                    .find_content_by_file_id(&mount_link.file_id)?
                    .is_none()
                {
                    continue;
                }
                resolved.push(MountAttachmentEntry {
                    id: mount_link.id.clone(),
                    filename,
                    url: build_document_url(&mount_link.mount_point_id, &mount_link.relative_path),
                    mime_type: text_mime.to_string(),
                    size: mount_link.file_size_bytes,
                    created_at: created_at.clone(),
                    message_id: message_id.clone(),
                    mount_point_id: mount_link.mount_point_id.clone(),
                    relative_path: mount_link.relative_path.clone(),
                    has_blob: false,
                    sha256: link_sha,
                });
                seen.insert(mount_link.id);
            }
        }
    }
    Ok(())
}

// ============================================================================
// The enumerator
// ============================================================================

/// List every image in a chat, whatever produced it (v4 `listChatGallery`).
///
/// Four passes, in order; the first pass to see an image wins its `source`, and
/// a later pass may only *add* a `messageId`. Deduped by sha256 where known and
/// by id otherwise; sorted newest first, with portraits carrying their
/// character's `createdAt` so a standing portrait lands at the end of the roll
/// rather than the top of it.
///
/// Returns `[]` for a chat that does not exist.
pub fn list_chat_gallery(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
) -> Result<Vec<Value>, DbError> {
    let Some(chat) = chats_read::find_by_id(main, chat_id)? else {
        tracing::debug!(
            chat_id = %chat_id,
            "Gallery requested for a chat that does not exist"
        );
        return Ok(Vec::new());
    };

    let mut collector = EntryCollector::new();
    let cast = load_cast(main, mount, &chat)?;
    let current = resolve_current_assets(main, mount, &chat)?;

    pass_linked_files(main, mount, chat_id, &cast, &current, &mut collector)?;

    let events = match chats_messages_read::get_messages(main, chat_id) {
        Ok(e) => e,
        Err(err) => {
            tracing::warn!(
                chat_id = %chat_id,
                error = %err,
                "Failed to read chat messages for the gallery"
            );
            Vec::new()
        }
    };

    pass_message_attachments(mount, &events, &mut collector)?;
    pass_cast_portraits(main, mount, &chat, &cast, &mut collector)?;
    pass_inline_markdown(main, mount, &chat, &cast, &events, &mut collector)?;

    let entries = collector.finish();
    tracing::debug!(
        chat_id = %chat_id,
        total = entries.len(),
        by_source = %count_by_source(&entries),
        "Chat gallery enumerated"
    );
    Ok(entries)
}

/// [`list_chat_gallery`] plus the per-source tally the filter chips and the
/// sidebar count read (v4 `getChatGallery`). One call so the route never counts
/// a list it just built.
pub fn get_chat_gallery(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
) -> Result<Value, DbError> {
    let entries = list_chat_gallery(main, mount, chat_id)?;
    let counts = count_by_source(&entries);
    let total = entries.len();
    Ok(json!({
        "entries": entries,
        "counts": counts,
        "total": total,
    }))
}

/// v4 `countBySource` — seeded at 0 from [`CHAT_GALLERY_SOURCES`], so every key
/// is present, in chip order.
pub fn count_by_source(entries: &[Value]) -> Value {
    let mut counts = Map::new();
    for source in CHAT_GALLERY_SOURCES {
        counts.insert(source.to_string(), json!(0));
    }
    for entry in entries {
        if let Some(source) = entry.get("source").and_then(Value::as_str) {
            let n = counts
                .get(source)
                .and_then(Value::as_u64)
                .unwrap_or_default();
            counts.insert(source.to_string(), json!(n + 1));
        }
    }
    Value::Object(counts)
}

// ============================================================================
// Pass 1 — files linked to the chat
// ============================================================================

/// What the chat is currently *showing*: the background on the wall and the
/// avatar each character is wearing.
///
/// Matched on id **and** sha256, for the reason the stale-chat collapse sweep
/// matches on both: those fields hold a `files.id` when the story-background
/// and avatar jobs wrote them, and a `doc_mount_file_links.id` when an import
/// or a migration did, so an id comparison alone misses half the cases.
struct CurrentAssets {
    background_ids: HashSet<String>,
    background_shas: HashSet<String>,
    /// imageId (or its sha256) → the character wearing it.
    avatar_ids: HashMap<String, String>,
    avatar_shas: HashMap<String, String>,
}

/// Every character in the cast, loaded once (v4 `Cast`). Three passes want them
/// — the repaint's owner, the standing portrait, and the vault a relative
/// Markdown path resolves against — and a per-pass read would fetch the same
/// rows three times over.
struct Cast {
    by_character_id: HashMap<String, Value>,
}

fn load_cast(main: &Connection, mount: &Connection, chat: &Value) -> Result<Cast, DbError> {
    let mut by_character_id = HashMap::new();
    let ids = participant_character_ids(chat);
    if ids.is_empty() {
        return Ok(Cast { by_character_id });
    }
    // `find_by_ids` drops broken-vault characters rather than throwing, which is
    // the behaviour a gallery wants: one unreadable character costs its own
    // portrait, not the whole roll.
    match characters_read::find_by_ids(main, mount, &ids) {
        Ok(characters) => {
            for character in characters {
                if let Some(id) = character.get("id").and_then(Value::as_str) {
                    by_character_id.insert(id.to_string(), character);
                }
            }
        }
        Err(err) => {
            // The id is lifted out of the macro: `tracing::warn!` shadows
            // `Value`, so `Value::as_str` inside it is "expected a type, found
            // a trait" (`tracing-macro-shadows-serde-json-value`).
            let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or_default();
            tracing::warn!(
                chat_id = %chat_id,
                error = %err,
                "Failed to load the chat cast for the gallery"
            );
        }
    }
    Ok(Cast { by_character_id })
}

fn resolve_current_assets(
    main: &Connection,
    mount: &Connection,
    chat: &Value,
) -> Result<CurrentAssets, DbError> {
    let mut current = CurrentAssets {
        background_ids: HashSet::new(),
        background_shas: HashSet::new(),
        avatar_ids: HashMap::new(),
        avatar_shas: HashMap::new(),
    };

    if let Some(background_id) = chat
        .get("storyBackgroundImageId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        current.background_ids.insert(background_id.to_string());
        if let Some(resolved) = safe_resolve_avatar(main, mount, background_id) {
            if let Some(sha) = resolved.sha256 {
                current.background_shas.insert(sha);
            }
        }
    }

    // `characterAvatars` is an object map keyed by characterId:
    //   { [characterId]: { imageId, generatedAt, afterMessageCount } }
    if let Some(avatars) = chat.get("characterAvatars").and_then(Value::as_object) {
        for (character_id, entry) in avatars {
            let Some(entry) = entry.as_object() else {
                continue;
            };
            let Some(image_id) = entry
                .get("imageId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            current
                .avatar_ids
                .insert(image_id.to_string(), character_id.clone());
            if let Some(resolved) = safe_resolve_avatar(main, mount, image_id) {
                if let Some(sha) = resolved.sha256 {
                    current.avatar_shas.insert(sha, character_id.clone());
                }
            }
        }
    }

    Ok(current)
}

/// One `files` row as pass 1 reads it (v4's `FileEntry`, restricted to the
/// columns the classifier and the entry need). Read with raw SQL for the same
/// reason `chat_files_list` does: `db/files.rs`'s typed projections carry
/// neither `source` nor `tags`, and widening them is another lane's surface.
struct LinkedFileRow {
    id: String,
    sha256: Option<String>,
    original_filename: String,
    mime_type: String,
    size: i64,
    width: Option<i64>,
    height: Option<i64>,
    created_at: String,
    source: String,
    tags: Vec<String>,
    linked_to: Vec<String>,
    folder_path: Option<String>,
}

fn json_string_array(raw: Option<String>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| {
            v.as_array().map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
        })
        .unwrap_or_default()
}

fn find_files_linked_to(conn: &Connection, entity_id: &str) -> Result<Vec<LinkedFileRow>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, sha256, originalFilename, mimeType, size, width, height, createdAt, \
         source, tags, linkedTo, folderPath FROM files \
         WHERE EXISTS (SELECT 1 FROM json_each(files.linkedTo) WHERE value = ?1)",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![entity_id], |r| {
            Ok(LinkedFileRow {
                id: r.get(0)?,
                sha256: r.get::<_, Option<String>>(1)?.filter(|s| !s.is_empty()),
                original_filename: r.get(2)?,
                mime_type: r.get(3)?,
                size: r.get::<_, Option<f64>>(4)?.unwrap_or(0.0) as i64,
                width: r.get::<_, Option<f64>>(5)?.map(|v| v as i64),
                height: r.get::<_, Option<f64>>(6)?.map(|v| v as i64),
                created_at: r.get(7)?,
                source: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
                tags: json_string_array(r.get(9)?),
                linked_to: json_string_array(r.get(10)?),
                folder_path: r.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn pass_linked_files(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    cast: &Cast,
    current: &CurrentAssets,
    collector: &mut EntryCollector,
) -> Result<(), DbError> {
    let files = find_files_linked_to(main, chat_id)?;

    // Which characters this chat has ever overridden an avatar for. A superseded
    // repaint is no longer in `chat.characterAvatars`, so this is how it keeps
    // its character attribution.
    let override_owners = resolve_avatar_override_owners(cast, chat_id);

    let mut found = 0usize;
    for file in files.iter().filter(|f| is_image_mime(Some(&f.mime_type))) {
        let link_summary = match &file.sha256 {
            Some(sha) => safe_link_summary(mount, sha),
            None => None,
        };
        let paths: Vec<String> = link_summary
            .as_ref()
            .and_then(|s| s.get("linkers"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|l| l.get("relativePath").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let is_current_background = current.background_ids.contains(&file.id)
            || file
                .sha256
                .as_deref()
                .is_some_and(|s| current.background_shas.contains(s));
        let avatar_owner = current.avatar_ids.get(&file.id).cloned().or_else(|| {
            file.sha256
                .as_deref()
                .and_then(|s| current.avatar_shas.get(s).cloned())
        });

        let source: &str;
        let mut character_id: Option<String> = None;
        let mut is_current = false;

        if is_current_background || paths.iter().any(|p| is_story_background_path(p)) {
            source = "story-background";
            is_current = is_current_background;
        } else if avatar_owner.is_some()
            || is_avatar_file(file, &paths)
            || override_owners.contains_key(&file.id)
        {
            source = "avatar";
            is_current = avatar_owner.is_some();
            character_id = avatar_owner
                .clone()
                .or_else(|| override_owners.get(&file.id).cloned())
                // The avatar job tags the file with the character it painted, and
                // links it to `[chatId, characterId]`; either is a usable fallback
                // for a repaint the chat has since moved on from.
                .or_else(|| file.tags.iter().find(|t| *t != chat_id).cloned())
                .or_else(|| file.linked_to.iter().find(|t| *t != chat_id).cloned());
        } else if file.source == "GENERATED" {
            source = "generated";
        } else {
            source = "attachment";
        }

        let mut entry = Map::new();
        entry.insert("id".into(), json!(file.id));
        entry.insert("idKind".into(), json!("file"));
        entry.insert("url".into(), json!(format!("/api/v1/files/{}", file.id)));
        entry.insert("filename".into(), json!(file.original_filename));
        entry.insert("mimeType".into(), json!(file.mime_type));
        entry.insert("size".into(), json!(file.size));
        if let Some(w) = file.width {
            entry.insert("width".into(), json!(w));
        }
        if let Some(h) = file.height {
            entry.insert("height".into(), json!(h));
        }
        if let Some(sha) = &file.sha256 {
            entry.insert("sha256".into(), json!(sha));
        }
        entry.insert("createdAt".into(), json!(file.created_at));
        entry.insert("source".into(), json!(source));
        if let Some(cid) = &character_id {
            entry.insert("characterId".into(), json!(cid));
        }
        entry.insert("isCurrent".into(), json!(is_current));
        // The chat minted this record, so the chat may retire it — but never the
        // one it is currently showing.
        entry.insert(
            "deletable".into(),
            json!(!is_current && OWNED_SOURCES.contains(&source)),
        );
        if let Some(summary) = link_summary {
            entry.insert("linkSummary".into(), summary);
        }
        collector.add(Value::Object(entry));
        found += 1;
    }

    tracing::debug!(
        chat_id = %chat_id,
        pass = "linked-files",
        found,
        "Chat gallery pass complete"
    );
    Ok(())
}

/// Which superseded repaints belong to which character, read off every
/// participant's `avatarOverrides` row for this chat.
fn resolve_avatar_override_owners(cast: &Cast, chat_id: &str) -> HashMap<String, String> {
    let mut owners = HashMap::new();
    for (character_id, character) in &cast.by_character_id {
        let overrides = character
            .get("avatarOverrides")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for over in overrides {
            if over.get("chatId").and_then(Value::as_str) != Some(chat_id) {
                continue;
            }
            if let Some(image_id) = over
                .get("imageId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                owners.insert(image_id.to_string(), character_id.clone());
            }
        }
    }
    owners
}

/// The Lantern writes a chat's backdrops to `generated/` in its own mount.
fn is_story_background_path(relative_path: &str) -> bool {
    relative_path.to_lowercase().starts_with("generated/")
}

/// An Aurora repaint, recognised by where it was stored: `images/history/` in
/// the character's vault, or the legacy `/character-avatars/` project folder
/// for a chat whose files live in a project mount.
fn is_avatar_file(file: &LinkedFileRow, paths: &[String]) -> bool {
    if file.folder_path.as_deref() == Some("/character-avatars/") {
        return true;
    }
    paths
        .iter()
        .any(|p| p.to_lowercase().starts_with("images/history/"))
}

// ============================================================================
// Pass 2 — message attachments
// ============================================================================

fn pass_message_attachments(
    mount: &Connection,
    events: &[Value],
    collector: &mut EntryCollector,
) -> Result<(), DbError> {
    // A file already found in pass 1 needs no mount lookup — but it may still
    // learn which message it hangs beneath, which is what gives the detail view
    // its "Jump to message" link.
    for event in events {
        if event.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(message_id) = event.get("id").and_then(Value::as_str) else {
            continue;
        };
        if let Some(attachments) = event.get("attachments").and_then(Value::as_array) {
            for attachment_id in attachments.iter().filter_map(Value::as_str) {
                collector.note_message(attachment_id, message_id);
            }
        }
    }

    let mount_entries = resolve_message_attachment_entries(mount, events, &collector.known_ids());

    let mut found = 0usize;
    for entry in mount_entries {
        if !entry.has_blob || !is_image_mime(Some(&entry.mime_type)) {
            continue;
        }
        let link_summary = entry
            .sha256
            .as_deref()
            .and_then(|sha| safe_link_summary(mount, sha));

        let mut obj = Map::new();
        obj.insert("id".into(), json!(entry.id));
        obj.insert("idKind".into(), json!("link"));
        obj.insert("url".into(), json!(entry.url));
        obj.insert("filename".into(), json!(entry.filename));
        obj.insert("mimeType".into(), json!(entry.mime_type));
        obj.insert("size".into(), json!(entry.size));
        if let Some(sha) = &entry.sha256 {
            obj.insert("sha256".into(), json!(sha));
        }
        obj.insert("createdAt".into(), entry.created_at.clone());
        // A link that lives in a `photos/` folder is an album image the chat is
        // being shown again; anything else is a document store's file, attached.
        obj.insert(
            "source".into(),
            json!(if is_photos_relative_path(Some(&entry.relative_path)) {
                "kept"
            } else {
                "attachment"
            }),
        );
        obj.insert("messageId".into(), json!(entry.message_id));
        obj.insert("isCurrent".into(), json!(false));
        // The album or the store owns these bytes, not the chat.
        obj.insert("deletable".into(), json!(false));
        if let Some(summary) = link_summary {
            obj.insert("linkSummary".into(), summary);
        }
        collector.add(Value::Object(obj));
        found += 1;
    }

    tracing::debug!(
        pass = "message-attachments",
        found,
        "Chat gallery pass complete"
    );
    Ok(())
}

// ============================================================================
// Pass 3 — the cast's standing portraits
// ============================================================================

fn pass_cast_portraits(
    main: &Connection,
    mount: &Connection,
    chat: &Value,
    cast: &Cast,
    collector: &mut EntryCollector,
) -> Result<(), DbError> {
    let mut found = 0usize;
    for character_id in participant_character_ids(chat) {
        let Some(character) = cast.by_character_id.get(&character_id) else {
            continue;
        };
        let Some(default_image_id) = character
            .get("defaultImageId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };

        let Some(resolved) = safe_resolve_avatar(main, mount, default_image_id) else {
            continue;
        };

        // An Aurora repaint that was promoted to the character's default is
        // already on the roll under its own source; do not hang it twice.
        if let Some(sha) = &resolved.sha256 {
            if collector.has_sha(sha) {
                continue;
            }
        }

        let character_name = character
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        // v4 `chat-gallery.ts:591`: `(relativePath ? basename(relativePath) :
        // null) ?? \`${name}.webp\`` — an EMPTY path is falsy and takes the
        // fallback (the §3 unification review of the `78b381a96` round).
        let filename = resolved
            .relative_path
            .as_deref()
            .filter(|p| !p.is_empty())
            .map(posix_basename)
            .unwrap_or_else(|| format!("{character_name}.webp"));

        let mut obj = Map::new();
        obj.insert("id".into(), json!(default_image_id));
        obj.insert(
            "idKind".into(),
            json!(if resolved.kind == AvatarKind::VaultLink {
                "link"
            } else {
                "file"
            }),
        );
        obj.insert("url".into(), json!(resolved.url));
        obj.insert("filename".into(), json!(filename));
        obj.insert(
            "mimeType".into(),
            json!(resolved.mime_type.as_deref().unwrap_or("image/webp")),
        );
        obj.insert("size".into(), json!(0));
        if let Some(sha) = &resolved.sha256 {
            obj.insert("sha256".into(), json!(sha));
        }
        // A portrait is not an event in the conversation, so it sorts by the
        // character's own age and lands at the end of the roll.
        obj.insert(
            "createdAt".into(),
            character.get("createdAt").cloned().unwrap_or(Value::Null),
        );
        obj.insert("source".into(), json!("portrait"));
        obj.insert("characterId".into(), json!(character_id));
        obj.insert("characterName".into(), json!(character_name));
        obj.insert("isCurrent".into(), json!(true));
        // The character owns their portrait; the chat is only looking at it.
        obj.insert("deletable".into(), json!(false));
        collector.add(Value::Object(obj));
        found += 1;
    }

    tracing::debug!(pass = "cast-portraits", found, "Chat gallery pass complete");
    Ok(())
}

// ============================================================================
// Pass 4 — images woven into the prose
// ============================================================================

/// v4 `IMAGE_EXTENSIONS` — the relative-path allow-list.
const IMAGE_EXTENSIONS: [&str; 7] = [".webp", ".png", ".jpg", ".jpeg", ".gif", ".avif", ".svg"];

/// Scan message prose for Markdown image references (v4 `passInlineMarkdown`).
///
/// Absolute `/api/v1/files/<uuid>` and `/api/v1/mount-points/<id>/blobs/<path>`
/// URLs are taken as written. A relative path is resolved against the author's
/// own vault mount, mirroring what the message renderer does.
///
/// A resolvable reference becomes a real entry carrying a real id — the
/// `files.id` from the URL, or the `doc_mount_file_links.id` the blob path
/// names — so Save works on it like any other picture. A reference that
/// resolves to no record is skipped with a `debug` line, never an error.
/// Anything already on the roll from an earlier pass keeps the source it earned
/// there, so a `generate_image` output the model also wrote into its prose
/// stays `generated`.
fn pass_inline_markdown(
    main: &Connection,
    mount: &Connection,
    chat: &Value,
    cast: &Cast,
    events: &[Value],
    collector: &mut EntryCollector,
) -> Result<(), DbError> {
    let vault_by_participant = vault_mount_by_participant(chat, cast);
    let mut found = 0usize;
    let mut skipped = 0usize;

    for event in events {
        if event.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        if event.get("role").and_then(Value::as_str) == Some("SYSTEM") {
            continue;
        }
        let Some(content) = event
            .get("content")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Some(message_id) = event.get("id").and_then(Value::as_str) else {
            continue;
        };

        for raw in markdown_image_urls(content) {
            if raw.is_empty() || raw.starts_with("data:") || is_scheme_qualified(&raw) {
                continue;
            }

            if let Some(file_id) = file_url_id(&raw) {
                if add_file_reference(main, &file_id, message_id, collector)? {
                    found += 1;
                } else {
                    skipped += 1;
                }
                continue;
            }

            if let Some((mount_point_id, path)) = blob_url_parts(&raw) {
                if add_blob_reference(
                    mount,
                    &mount_point_id,
                    &safe_decode_uri(&path),
                    message_id,
                    collector,
                )? {
                    found += 1;
                } else {
                    skipped += 1;
                }
                continue;
            }

            if raw.starts_with('/') {
                // Some other absolute path in the app. Nothing here can resolve
                // it to a record, so it is not a gallery entry.
                skipped += 1;
                continue;
            }

            // Relative — resolve against the author's vault, the way the
            // renderer does. An author with no vault (or a message with no
            // author) leaves the reference unresolvable, which is a skip, never
            // an error.
            let mount_point_id = event
                .get("participantId")
                .and_then(Value::as_str)
                .and_then(|pid| vault_by_participant.get(pid).cloned());
            let Some(mount_point_id) = mount_point_id else {
                skipped += 1;
                continue;
            };
            if !IMAGE_EXTENSIONS.contains(&posix_extname(&raw).to_lowercase().as_str()) {
                skipped += 1;
                continue;
            }
            if add_blob_reference(mount, &mount_point_id, &raw, message_id, collector)? {
                found += 1;
            } else {
                skipped += 1;
            }
        }
    }

    tracing::debug!(
        pass = "inline-markdown",
        found,
        skipped,
        "Chat gallery pass complete"
    );
    Ok(())
}

/// Turn a `(mountPointId, relativePath)` reference into an entry carrying the
/// link row's own id, so the detail view's Save has a real record to hand
/// `save_image_to_album`. Returns false when nothing in the mount index answers
/// to that path — a stale reference in old prose, which is a skip.
fn add_blob_reference(
    mount: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    message_id: &str,
    collector: &mut EntryCollector,
) -> Result<bool, DbError> {
    let link = match DocMountFileLinksRepository::new(mount)
        .find_by_mount_point_and_path(mount_point_id, relative_path)
    {
        Ok(l) => l,
        Err(err) => {
            tracing::debug!(
                mount_point_id = %mount_point_id,
                relative_path = %relative_path,
                error = %err,
                "Inline image reference did not resolve"
            );
            return Ok(false);
        }
    };
    let Some(link) = link else {
        tracing::debug!(
            mount_point_id = %mount_point_id,
            relative_path = %relative_path,
            "Inline image reference names no mount-index row"
        );
        return Ok(false);
    };
    if collector.has(&link.id) {
        collector.note_message(&link.id, message_id);
        return Ok(false);
    }
    // v4 `link.originalFileName ?? link.fileName ?? basename(relativePath)` —
    // `??` falls through on NULL only, so a stored empty string wins.
    let filename = link
        .original_file_name
        .clone()
        .unwrap_or_else(|| link.file_name.clone());
    let mime_type = link
        .original_mime_type
        .clone()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| mime_from_extension(&filename).to_string());

    let mut obj = Map::new();
    obj.insert("id".into(), json!(link.id));
    obj.insert("idKind".into(), json!("link"));
    obj.insert(
        "url".into(),
        json!(build_blob_url(mount_point_id, relative_path)),
    );
    obj.insert("filename".into(), json!(filename));
    obj.insert("mimeType".into(), json!(mime_type));
    obj.insert("size".into(), json!(link.file_size_bytes));
    if !link.sha256.is_empty() {
        obj.insert("sha256".into(), json!(link.sha256));
    }
    obj.insert("createdAt".into(), json!(link.created_at));
    obj.insert("source".into(), json!("inline"));
    obj.insert("messageId".into(), json!(message_id));
    obj.insert("isCurrent".into(), json!(false));
    // Woven into someone's prose and owned by whatever store holds it.
    obj.insert("deletable".into(), json!(false));
    collector.add(Value::Object(obj));
    Ok(true)
}

/// Turn a `/api/v1/files/<uuid>` reference into an entry off the file row it
/// names, so the picture carries its real filename, size and hash rather than a
/// shape guessed from the URL. Returns false when the row is gone.
fn add_file_reference(
    main: &Connection,
    file_id: &str,
    message_id: &str,
    collector: &mut EntryCollector,
) -> Result<bool, DbError> {
    if collector.has(file_id) {
        collector.note_message(file_id, message_id);
        return Ok(false);
    }
    let file = find_file_row(main, file_id).unwrap_or(None);
    let Some(file) = file.filter(|f| is_image_mime(Some(&f.mime_type))) else {
        tracing::debug!(file_id = %file_id, "Inline image reference names no file row");
        return Ok(false);
    };

    let mut obj = Map::new();
    obj.insert("id".into(), json!(file.id));
    obj.insert("idKind".into(), json!("file"));
    obj.insert("url".into(), json!(format!("/api/v1/files/{}", file.id)));
    obj.insert("filename".into(), json!(file.original_filename));
    obj.insert("mimeType".into(), json!(file.mime_type));
    obj.insert("size".into(), json!(file.size));
    if let Some(w) = file.width {
        obj.insert("width".into(), json!(w));
    }
    if let Some(h) = file.height {
        obj.insert("height".into(), json!(h));
    }
    if let Some(sha) = &file.sha256 {
        obj.insert("sha256".into(), json!(sha));
    }
    obj.insert("createdAt".into(), json!(file.created_at));
    obj.insert("source".into(), json!("inline"));
    obj.insert("messageId".into(), json!(message_id));
    obj.insert("isCurrent".into(), json!(false));
    // Woven into someone's prose; the chat did not mint the record.
    obj.insert("deletable".into(), json!(false));
    collector.add(Value::Object(obj));
    Ok(true)
}

fn find_file_row(conn: &Connection, file_id: &str) -> Result<Option<LinkedFileRow>, DbError> {
    conn.query_row(
        "SELECT id, sha256, originalFilename, mimeType, size, width, height, createdAt, \
         source, tags, linkedTo, folderPath FROM files WHERE id = ?1",
        rusqlite::params![file_id],
        |r| {
            Ok(LinkedFileRow {
                id: r.get(0)?,
                sha256: r.get::<_, Option<String>>(1)?.filter(|s| !s.is_empty()),
                original_filename: r.get(2)?,
                mime_type: r.get(3)?,
                size: r.get::<_, Option<f64>>(4)?.unwrap_or(0.0) as i64,
                width: r.get::<_, Option<f64>>(5)?.map(|v| v as i64),
                height: r.get::<_, Option<f64>>(6)?.map(|v| v as i64),
                created_at: r.get(7)?,
                source: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
                tags: json_string_array(r.get(9)?),
                linked_to: json_string_array(r.get(10)?),
                folder_path: r.get(11)?,
            })
        },
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
}

/// The vault mount each participant's character writes into, so a relative
/// `![](images/foo.webp)` in that participant's message can be resolved.
fn vault_mount_by_participant(chat: &Value, cast: &Cast) -> HashMap<String, String> {
    let mut by_participant = HashMap::new();
    let participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for participant in participants {
        let Some(character_id) = participant
            .get("characterId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let mount_point_id = cast
            .by_character_id
            .get(character_id)
            .and_then(|c| c.get("characterDocumentMountPointId"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        if let (Some(pid), Some(mp)) = (
            participant
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty()),
            mount_point_id,
        ) {
            by_participant.insert(pid.to_string(), mp.to_string());
        }
    }
    by_participant
}

// ============================================================================
// Collector
// ============================================================================

/// Accumulates entries across the four passes, holding the two dedup rules the
/// design turns on: the first pass to see an image wins its source, and a later
/// pass may only add a `messageId` (v4 `EntryCollector`).
struct EntryCollector {
    /// The entries themselves. `by_id` / `by_sha` hold INDEXES into this vector
    /// rather than references, because v4's aliasing (`byId[id] = twin`) needs
    /// two keys to name one mutable entry.
    entries: Vec<Value>,
    by_id: HashMap<String, usize>,
    by_sha: HashMap<String, usize>,
    /// messageId for an attachment id seen before its entry existed.
    pending_messages: HashMap<String, String>,
}

impl EntryCollector {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            by_id: HashMap::new(),
            by_sha: HashMap::new(),
            pending_messages: HashMap::new(),
        }
    }

    fn add(&mut self, mut entry: Value) {
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if self.by_id.contains_key(&id) {
            return;
        }
        let sha = entry
            .get("sha256")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(sha) = &sha {
            if let Some(&twin_idx) = self.by_sha.get(sha) {
                // The same bytes under a second id — one picture, whichever pass
                // saw it first. The later sighting may still contribute the
                // message the picture hangs beneath, which is the one field a
                // later pass may add.
                self.by_id.insert(id.clone(), twin_idx);
                let message_id = entry
                    .get("messageId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| self.pending_messages.get(&id).cloned());
                if let Some(message_id) = message_id {
                    let twin = &mut self.entries[twin_idx];
                    if twin.get("messageId").and_then(Value::as_str).is_none() {
                        if let Some(obj) = twin.as_object_mut() {
                            obj.insert("messageId".into(), json!(message_id));
                        }
                    }
                }
                return;
            }
        }

        if let Some(pending) = self.pending_messages.get(&id).cloned() {
            if entry.get("messageId").and_then(Value::as_str).is_none() {
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert("messageId".into(), json!(pending));
                }
            }
        }

        let idx = self.entries.len();
        self.entries.push(entry);
        self.by_id.insert(id, idx);
        if let Some(sha) = sha {
            self.by_sha.insert(sha, idx);
        }
    }

    /// Record the message an id hangs beneath, whether or not its entry exists
    /// yet.
    fn note_message(&mut self, id: &str, message_id: &str) {
        if let Some(&idx) = self.by_id.get(id) {
            let entry = &mut self.entries[idx];
            if entry.get("messageId").and_then(Value::as_str).is_none() {
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert("messageId".into(), json!(message_id));
                }
            }
            return;
        }
        self.pending_messages
            .entry(id.to_string())
            .or_insert_with(|| message_id.to_string());
    }

    fn has(&self, id: &str) -> bool {
        self.by_id.contains_key(id)
    }

    fn has_sha(&self, sha256: &str) -> bool {
        self.by_sha.contains_key(sha256)
    }

    fn known_ids(&self) -> HashSet<String> {
        self.by_id.keys().cloned().collect()
    }

    /// Newest first. v4 sorts on `new Date(b.createdAt).getTime() - new
    /// Date(a.createdAt).getTime()` with **no tie-break**, relying on JS's
    /// stable sort — so ties fall to pass order (linked files → message
    /// attachments → portraits → inline).
    ///
    /// **Deliberate divergence, in the safe direction:** an unparseable
    /// `createdAt` makes v4's comparator return `NaN`, which V8 treats as
    /// "equal" — an order that depends on the array's run structure and is not
    /// a total order at all (`js-nan-comparator-is-not-a-total-order`; the
    /// P4.D129 precedent). Rust's `sort_by_key` requires a total order, so an
    /// unparseable (or absent) timestamp sorts as `i64::MIN` — the END of a
    /// newest-first roll, which is where a picture with no readable date
    /// belongs. Agrees with v4 on every shape Quilltap data actually holds (a
    /// valid ISO string), and is structurally immune to the class rather than
    /// accidentally clear of it.
    fn finish(self) -> Vec<Value> {
        let mut entries = self.entries;
        entries.sort_by_key(|e| {
            std::cmp::Reverse(
                e.get("createdAt")
                    .and_then(Value::as_str)
                    .and_then(crate::episodic::js_date_parse_ms)
                    .unwrap_or(i64::MIN),
            )
        });
        entries
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// v4 `buildBlobUrl`.
pub fn build_blob_url(mount_point_id: &str, relative_path: &str) -> String {
    format!(
        "/api/v1/mount-points/{}/blobs/{}",
        mount_point_id,
        encode_uri(relative_path)
    )
}

/// v4 `buildDocumentUrl`.
pub fn build_document_url(mount_point_id: &str, relative_path: &str) -> String {
    format!(
        "/api/v1/mount-points/{}/files/{}",
        mount_point_id,
        encode_uri(relative_path)
    )
}

fn is_image_mime(mime_type: Option<&str>) -> bool {
    mime_type.is_some_and(|m| m.to_lowercase().starts_with("image/"))
}

fn mime_from_extension(filename: &str) -> &'static str {
    match posix_extname(filename).to_lowercase().as_str() {
        ".png" => "image/png",
        ".jpg" | ".jpeg" => "image/jpeg",
        ".gif" => "image/gif",
        ".avif" => "image/avif",
        ".svg" => "image/svg+xml",
        _ => "image/webp",
    }
}

fn participant_character_ids(chat: &Value) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for participant in participants {
        if participant.get("type").and_then(Value::as_str) != Some("CHARACTER") {
            continue;
        }
        if participant.get("status").and_then(Value::as_str) == Some("removed") {
            continue;
        }
        if let Some(cid) = participant
            .get("characterId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            if !ids.iter().any(|existing| existing == cid) {
                ids.push(cid.to_string());
            }
        }
    }
    ids
}

fn safe_resolve_avatar(
    main: &Connection,
    mount: &Connection,
    id: &str,
) -> Option<crate::photos::resolve_character_avatar::ResolvedCharacterAvatar> {
    match resolve_character_avatar(main, mount, Some(id)) {
        Ok(v) => v,
        Err(err) => {
            tracing::debug!(id = %id, error = %err, "Avatar id did not resolve");
            None
        }
    }
}

fn safe_link_summary(mount: &Connection, sha256: &str) -> Option<Value> {
    match get_photo_link_summary_by_sha256(mount, sha256) {
        Ok(v) => Some(v),
        Err(err) => {
            tracing::debug!(sha256 = %sha256, error = %err, "Photo link summary failed");
            None
        }
    }
}

// --- the pure string helpers the regexes stand in for --------------------

/// v4 `MARKDOWN_IMAGE_RE = /!\[[^\]]*\]\(\s*([^)\s]+)/g` — every capture, in
/// order. Hand-rolled rather than a `regex` crate call so the JS semantics are
/// visible: the alt text runs to the first `]`, optional leading whitespace is
/// skipped, and the URL runs to the first whitespace or `)`.
fn markdown_image_urls(content: &str) -> Vec<String> {
    // v4's regex runs over UTF-16 units; every construct here is ASCII, so a
    // byte walk over the UTF-8 is equivalent (a multi-byte character can only
    // appear inside the alt text or the URL, where it is copied through).
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] != b'!' || bytes[i + 1] != b'[' {
            i += 1;
            continue;
        }
        // `[^\]]*` then `\]`
        let mut j = i + 2;
        while j < bytes.len() && bytes[j] != b']' {
            j += 1;
        }
        if j >= bytes.len() {
            // No closing `]` anywhere — the regex cannot match from here on.
            break;
        }
        j += 1; // past `]`
        if j >= bytes.len() || bytes[j] != b'(' {
            i += 1;
            continue;
        }
        j += 1; // past `(`
                // `\s*` — JS `\s` (the port's shared spelling).
        while j < bytes.len() && is_js_space_byte(bytes, j) {
            j += js_space_len(bytes, j);
        }
        // `([^)\s]+)` — at least one char.
        let start = j;
        while j < bytes.len() && bytes[j] != b')' && !is_js_space_byte(bytes, j) {
            // Advance one UTF-8 character.
            let mut step = 1;
            while j + step < bytes.len() && (bytes[j + step] & 0xC0) == 0x80 {
                step += 1;
            }
            j += step;
        }
        if j == start {
            // The `+` failed; the regex engine would retry from the next index.
            i += 1;
            continue;
        }
        out.push(String::from_utf8_lossy(&bytes[start..j]).into_owned());
        // A global regex resumes at `lastIndex` = the end of the whole match,
        // which is the end of the capture (the pattern has nothing after it).
        i = j;
    }
    out
}

/// The bytes JS `\s` matches, restricted to what can appear in UTF-8: the ASCII
/// set plus the multi-byte Unicode spaces. Returns whether a match starts here.
fn is_js_space_byte(bytes: &[u8], i: usize) -> bool {
    js_space_len(bytes, i) > 0
}

/// The length in bytes of the JS `\s` match starting at `i`, or 0.
fn js_space_len(bytes: &[u8], i: usize) -> usize {
    match bytes[i] {
        b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r' => 1,
        _ => {
            let rest = &bytes[i..];
            let s = String::from_utf8_lossy(&rest[..rest.len().min(4)]);
            match s.chars().next() {
                Some(c)
                    if matches!(
                        c,
                        '\u{00A0}' | '\u{1680}' | '\u{2000}'
                            ..='\u{200A}'
                                | '\u{2028}'
                                | '\u{2029}'
                                | '\u{202F}'
                                | '\u{205F}'
                                | '\u{3000}'
                                | '\u{FEFF}'
                    ) =>
                {
                    c.len_utf8()
                }
                _ => 0,
            }
        }
    }
}

/// v4 `/^([a-z]+:)?\/\//i` — a scheme-qualified or protocol-relative URL.
fn is_scheme_qualified(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    if i > 0 && i < bytes.len() && bytes[i] == b':' {
        return bytes.len() > i + 2 && bytes[i + 1] == b'/' && bytes[i + 2] == b'/';
    }
    raw.starts_with("//")
}

/// v4 `FILE_URL_RE = /^\/api\/v1\/files\/([A-Za-z0-9_-]+)(?:[/?#]|$)/`.
fn file_url_id(raw: &str) -> Option<String> {
    const PREFIX: &str = "/api/v1/files/";
    let rest = raw.strip_prefix(PREFIX)?;
    let id: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if id.is_empty() {
        return None;
    }
    let after = &rest[id.len()..];
    match after.chars().next() {
        None => Some(id),
        Some('/') | Some('?') | Some('#') => Some(id),
        _ => None,
    }
}

/// v4 `BLOB_URL_RE = /^\/api\/v1\/mount-points\/([^/]+)\/blobs\/(.+)$/`.
fn blob_url_parts(raw: &str) -> Option<(String, String)> {
    const PREFIX: &str = "/api/v1/mount-points/";
    let rest = raw.strip_prefix(PREFIX)?;
    let slash = rest.find('/')?;
    let mount_point_id = &rest[..slash];
    if mount_point_id.is_empty() {
        return None;
    }
    let tail = rest[slash..].strip_prefix("/blobs/")?;
    if tail.is_empty() {
        return None;
    }
    Some((mount_point_id.to_string(), tail.to_string()))
}

/// v4 `safeDecodeUri` — `decodeURI`, falling back to the input on a throw.
/// `decodeURI` leaves the reserved set (`; / ? : @ & = + $ , #`) escaped.
fn safe_decode_uri(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        if i + 2 >= bytes.len() {
            return value.to_string(); // URIError
        }
        let hex = match std::str::from_utf8(&bytes[i + 1..i + 3])
            .ok()
            .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            Some(b) => b,
            None => return value.to_string(), // URIError
        };
        // `decodeURI` preserves the escape sequences of the reserved set.
        if matches!(
            hex,
            b';' | b'/' | b'?' | b':' | b'@' | b'&' | b'=' | b'+' | b'$' | b',' | b'#'
        ) {
            out.extend_from_slice(&bytes[i..i + 3]);
            i += 3;
            continue;
        }
        out.push(hex);
        i += 3;
    }
    match String::from_utf8(out) {
        Ok(s) => s,
        // A percent-escape that decodes to invalid UTF-8 is a URIError in JS.
        Err(_) => value.to_string(),
    }
}

/// `path.posix.basename` for the shapes a relative path takes.
fn posix_basename(relative_path: &str) -> String {
    let trimmed = relative_path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(i) => trimmed[i + 1..].to_string(),
        None => trimmed.to_string(),
    }
}

/// `path.posix.extname` — the last `.` in the basename, `""` when the basename
/// starts with it or has none.
fn posix_extname(path: &str) -> String {
    let base = posix_basename(path);
    match base.rfind('.') {
        Some(0) | None => String::new(),
        Some(i) => base[i..].to_string(),
    }
}
