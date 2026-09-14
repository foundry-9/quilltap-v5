//! Avatar rolls — the Aurora gallery's window onto the avatar configuration
//! cache. Port of v4 `lib/photos/avatar-rolls-service.ts` (`4dcbe0d21`).
//!
//! An **avatar roll** is one plate the house has already developed for a
//! character: the image the wardrobe avatar job stored for a particular
//! configuration of outfit, provider, profile and model. The cache looks those
//! up by key so a character putting the same coat back on costs nothing; this
//! module looks them up by *character*, so the operator can see the collection,
//! promote a plate to the character's portrait, copy one into the photo album,
//! or throw one away.
//!
//! **A roll is a `files` row carrying a non-null `generationKey`** (P4.D182's
//! column, P4.D184's writer). That column exists for exactly one purpose — the
//! avatar cache writes it, and v4's `collapse-duplicate-avatar-rolls-v1`
//! migration backfilled it onto every pre-cache portrait — so "keyed and tagged
//! with this character" is the whole definition, with no path matching to drift.
//! It has to be that way: rolls predating the vault change live at
//! `character-avatars/…` in a project mount, newer ones at `images/history/…` in
//! the character's own vault, and a roll that has been copied into the album has
//! a second link under `photos/`.
//!
//! Deleting a roll never takes an album photo with it. `delete_mount_blob` drops
//! *every* link to a blob's file, which is the wrong verb here — a roll the
//! operator has already kept is two links over one set of bytes, and only the
//! roll's own link is ours. See [`delete_avatar_roll`].
//!
//! ## Why the album save is split in two, where v4's is one function
//!
//! v4's `saveAvatarRollToAlbum` calls `saveFileToCharacterGallery`, which reads
//! the roll's bytes through `fileStorageManager.downloadFile` and hands them to
//! `saveToCharacterGallery`. In this port those bytes come from the injected
//! [`FileBytesStore`] seam, and that seam cannot run on the writer thread (see
//! `ProductionFileBytes::ingest_image_buffer`'s own guard) — so the read half
//! ([`plan_album_save`]) resolves the roll, the vault and the links off the
//! writer, the caller fetches the bytes, and the write half
//! ([`commit_album_save`]) runs inside `Db::write`. That is the same split
//! `quilltap-web`'s `characters_photos_post` already makes for v4's `{fileId}`
//! leg; the ORDER of v4's guards is preserved across the seam.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::db::characters_read;
use crate::db::chats::{ChatUpdate, ChatsRepository};
use crate::db::chats_read;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::files::FilesRepository;
use crate::db::DbError;
use crate::photos::character_gallery_service::{
    resolve_character_vault, save_to_character_gallery, GalleryError,
};
use crate::photos::photo_link_summary::get_photo_link_summary_by_sha256;
use crate::photos::resolve_character_avatar::{build_legacy_file_url, build_mount_file_url};
use crate::services::file_storage::parse_mount_blob_storage_key;

/// v4 `DEFAULT_LIMIT` / `MAX_LIMIT`.
const DEFAULT_LIMIT: i64 = 60;
const MAX_LIMIT: i64 = 200;

/// The error surface the verbs and the REST edges map to responses. Each
/// variant's [`std::fmt::Display`] is v4's thrown message byte-for-byte, because
/// v4's route ladder (`avatar-rolls/[fileId]/route.ts:37`) classifies by reading
/// that message — the variants are what v5 matches on, and the messages are what
/// reaches the caller.
#[derive(Debug)]
pub enum AvatarRollError {
    /// v4 `Character not found: ${characterId}`.
    CharacterNotFound(String),
    /// v4 `Avatar roll not found: ${fileId}`.
    RollNotFound(String),
    /// v4 `Character ${characterId} has no linked database-backed vault`.
    NoVault(String),
    /// A message the gallery save leg raises verbatim (`… is not an image`,
    /// `… has empty bytes`, `Image already in …'s photo album at …`).
    BadRequest(String),
    Db(DbError),
}

impl std::fmt::Display for AvatarRollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AvatarRollError::CharacterNotFound(id) => write!(f, "Character not found: {id}"),
            AvatarRollError::RollNotFound(id) => write!(f, "Avatar roll not found: {id}"),
            AvatarRollError::NoVault(id) => {
                write!(f, "Character {id} has no linked database-backed vault")
            }
            AvatarRollError::BadRequest(m) => write!(f, "{m}"),
            AvatarRollError::Db(e) => write!(f, "{e}"),
        }
    }
}

impl From<DbError> for AvatarRollError {
    fn from(e: DbError) -> Self {
        AvatarRollError::Db(e)
    }
}

impl From<GalleryError> for AvatarRollError {
    fn from(e: GalleryError) -> Self {
        match e {
            // The save leg reaches `CharacterNotFound` only if the character
            // vanished between this module's own lookup and the write; v4's
            // message for it is the same sentence the ladder reads.
            GalleryError::CharacterNotFound => {
                AvatarRollError::CharacterNotFound(String::from("<unknown>"))
            }
            GalleryError::BadRequest(m) => AvatarRollError::BadRequest(m),
            GalleryError::Db(e) => AvatarRollError::Db(e),
        }
    }
}

/// One `files` row, in the shape this module's reads need. `FileEntry` carries
/// neither `createdAt` nor `tags`, and `db/**` is read-only to this lane, so the
/// row is read here — the `photos::chat_gallery` precedent for a photos module
/// reading `files` directly.
#[derive(Debug, Clone)]
struct RollRow {
    id: String,
    sha256: String,
    original_filename: String,
    mime_type: Option<String>,
    size: i64,
    width: Option<i64>,
    height: Option<i64>,
    generation_prompt: Option<String>,
    generation_model: Option<String>,
    storage_key: Option<String>,
    created_at: String,
}

/// v4's membership test, as one SQL predicate plus the JS filter it stands for:
/// `!!file.generationKey && file.category === 'IMAGE'`, scoped by the character
/// TAG. `findByTag` is `{ tags: { $in: [tagId] } }`, which v4's query translator
/// emits as a `json_each` membership test — the
/// `FilesRepository::find_sweep_rows_by_linked_to` precedent over the sibling
/// JSON array column.
///
/// v4's `findByFilter` carries no `QueryOptions`, so the rows arrive in rowid
/// order and the caller's sort is what orders them. (v4 also scopes `findByTag`
/// to the calling user; this port is single-user and no `files` read here
/// filters on `userId` — the established shape of every sibling read.)
const ROLL_SELECT: &str = "SELECT id, sha256, originalFilename, mimeType, size, width, height, \
     generationPrompt, generationModel, storageKey, createdAt FROM files \
     WHERE EXISTS (SELECT 1 FROM json_each(files.tags) WHERE value = ?1) \
       AND generationKey IS NOT NULL AND category = 'IMAGE'";

fn map_roll_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RollRow> {
    Ok(RollRow {
        id: r.get(0)?,
        sha256: r.get(1)?,
        original_filename: r.get(2)?,
        mime_type: r.get(3)?,
        // `size` is REAL-affinity in v4's DDL.
        size: r.get::<_, Option<f64>>(4)?.unwrap_or(0.0) as i64,
        width: r.get::<_, Option<f64>>(5)?.map(|v| v as i64),
        height: r.get::<_, Option<f64>>(6)?.map(|v| v as i64),
        generation_prompt: r.get(7)?,
        generation_model: r.get(8)?,
        storage_key: r.get(9)?,
        created_at: r.get(10)?,
    })
}

/// v4 `findRollsForCharacter` — keyed `files` rows tagged with this character,
/// newest first. `generationKey` is written by the avatar cache and by nothing
/// else, so it is the membership test; the tag scopes it to one character.
fn find_rolls_for_character(
    main: &Connection,
    character_id: &str,
) -> Result<Vec<RollRow>, DbError> {
    let mut stmt = main.prepare(ROLL_SELECT)?;
    let mut rows = stmt
        .query_map(rusqlite::params![character_id], map_roll_row)?
        .collect::<Result<Vec<_>, _>>()?;
    // v4: `.sort((a, b) => String(b.createdAt).localeCompare(String(a.createdAt)))`
    // — a STABLE descending compare over the ISO stamps, so ties keep the query's
    // order. `character_gallery_service` spells v4's `localeCompare` on the same
    // shape of value the same way.
    rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(rows)
}

/// v4 `findRoll` — the row, or `None` when the id names no file, names one that
/// is not a roll, or names a roll of a different character.
fn find_roll(
    main: &Connection,
    character_id: &str,
    file_id: &str,
) -> Result<Option<RollRow>, DbError> {
    let sql = format!("{ROLL_SELECT} AND id = ?2");
    let mut stmt = main.prepare(&sql)?;
    let mut rows = stmt
        .query_map(rusqlite::params![character_id, file_id], map_roll_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows.pop())
}

/// v4 `requireRoll` — [`find_roll`] or the throw the ladder turns into a 404.
fn require_roll(
    main: &Connection,
    character_id: &str,
    file_id: &str,
) -> Result<RollRow, AvatarRollError> {
    find_roll(main, character_id, file_id)?
        .ok_or_else(|| AvatarRollError::RollNotFound(file_id.to_string()))
}

/// One resolved mount-index link (v4's `PhotoLinker`, read out of the shared
/// summary's JSON).
#[derive(Debug, Clone)]
struct RollLink {
    link_id: String,
    mount_point_id: String,
    relative_path: String,
}

fn linker(v: &Value) -> Option<RollLink> {
    Some(RollLink {
        link_id: v.get("linkId")?.as_str()?.to_string(),
        mount_point_id: v.get("mountPointId")?.as_str()?.to_string(),
        relative_path: v.get("relativePath")?.as_str()?.to_string(),
    })
}

/// v4 `classifyRollLinks` — split a roll's mount-index links into "the roll's
/// own" and "the album copy".
///
/// The roll's own link is the one in the mount point its `storageKey` names — a
/// project store's `character-avatars/` for pre-vault rolls, the character's own
/// vault `images/history/` since — and never a `photos/` path, because a roll
/// that has been kept has a second link there over the same bytes.
fn classify_roll_links(
    mount: &Connection,
    file: &RollRow,
    vault_mount_point_id: Option<&str>,
) -> Result<(Option<RollLink>, Option<RollLink>), DbError> {
    if file.sha256.is_empty() {
        return Ok((None, None));
    }
    let summary = get_photo_link_summary_by_sha256(mount, &file.sha256)?;
    let storage_mount_point_id = file
        .storage_key
        .as_deref()
        .and_then(parse_mount_blob_storage_key)
        .map(|(mp, _blob)| mp);

    let empty: Vec<Value> = Vec::new();
    let linkers = summary
        .get("linkers")
        .and_then(Value::as_array)
        .unwrap_or(&empty);

    let roll_link = linkers
        .iter()
        .find(|l| {
            let is_album = l
                .get("isPhotoAlbum")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let in_storage_mount = match storage_mount_point_id.as_deref() {
                // v4: `!storageMountPointId || l.mountPointId === storageMountPointId`.
                None => true,
                Some(mp) => l.get("mountPointId").and_then(Value::as_str) == Some(mp),
            };
            !is_album && in_storage_mount
        })
        .and_then(linker);

    let album_link = vault_mount_point_id.and_then(|vault_mp| {
        linkers
            .iter()
            .find(|l| {
                l.get("isPhotoAlbum")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    && l.get("mountPointId").and_then(Value::as_str) == Some(vault_mp)
            })
            .and_then(linker)
    });

    Ok((roll_link, album_link))
}

/// The `imageId` a chat's `characterAvatars` binds for one character, if any
/// (v4 `readBoundAvatarId`).
fn read_bound_avatar_id(character_avatars: Option<&Value>, character_id: &str) -> Option<String> {
    let entry = character_avatars?.as_object()?.get(character_id)?;
    let image_id = entry.as_object()?.get("imageId")?.as_str()?;
    if image_id.is_empty() {
        return None;
    }
    Some(image_id.to_string())
}

/// v4 `countChatAvatarUsage` — `files.id` → how many of the character's chats
/// are displaying it. One read of the character's chats answers the whole page.
fn count_chat_avatar_usage(
    main: &Connection,
    character_id: &str,
) -> Result<std::collections::HashMap<String, i64>, DbError> {
    let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for chat in chats_read::find_by_character_id(main, character_id)? {
        if let Some(image_id) = read_bound_avatar_id(chat.get("characterAvatars"), character_id) {
            *counts.entry(image_id).or_insert(0) += 1;
        }
    }
    Ok(counts)
}

/// v4 `listAvatarRolls` — every roll the cache holds for a character, newest
/// first. The BARE `{ entries, total, hasMore }`.
pub fn list_avatar_rolls(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Value, AvatarRollError> {
    let Some(character) = characters_read::find_by_id(main, mount, character_id)? else {
        return Err(AvatarRollError::CharacterNotFound(character_id.to_string()));
    };

    let rolls = find_rolls_for_character(main, character_id)?;
    // v4: `Math.max(1, Math.min(limit ?? 60, 200))` / `Math.max(0, offset ?? 0)`.
    let effective_limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let effective_offset = offset.unwrap_or(0).max(0);
    let total = rolls.len() as i64;
    let start = effective_offset.min(total) as usize;
    let end = effective_offset.saturating_add(effective_limit).min(total) as usize;
    let page = &rolls[start..end];

    let vault = resolve_character_vault(mount, &character)?;
    let vault_mp = vault.as_ref().map(|v| v.mount_point_id.as_str());
    // One pass over the character's chats builds the whole page's usage counts;
    // asking per roll would be one query per thumbnail.
    let chat_usage = count_chat_avatar_usage(main, character_id)?;
    let default_image_id = character.get("defaultImageId").and_then(Value::as_str);

    let mut entries: Vec<Value> = Vec::with_capacity(page.len());
    for file in page {
        let (roll_link, album_link) = classify_roll_links(mount, file, vault_mp)?;
        let display_link = roll_link.as_ref().or(album_link.as_ref());

        // §C.6's key order, which is v4's declaration order.
        let mut o = Map::new();
        o.insert("fileId".into(), json!(file.id));
        o.insert(
            "rollLinkId".into(),
            json!(roll_link.as_ref().map(|l| l.link_id.clone())),
        );
        o.insert(
            "albumLinkId".into(),
            json!(album_link.as_ref().map(|l| l.link_id.clone())),
        );
        o.insert("fileName".into(), json!(file.original_filename));
        o.insert(
            "url".into(),
            json!(match display_link {
                Some(l) => build_mount_file_url(&l.mount_point_id, &l.relative_path),
                None => build_legacy_file_url(&file.id),
            }),
        );
        o.insert("mimeType".into(), json!(file.mime_type));
        o.insert("fileSizeBytes".into(), json!(file.size));
        o.insert("width".into(), json!(file.width));
        o.insert("height".into(), json!(file.height));
        o.insert("createdAt".into(), json!(file.created_at));
        o.insert("generationPrompt".into(), json!(file.generation_prompt));
        o.insert("generationModel".into(), json!(file.generation_model));
        o.insert("sha256".into(), json!(file.sha256));
        // v4: the portrait pointer may be the roll's `files` id (a legacy
        // pointer) OR the album link's id (the post-Phase-3 shape).
        o.insert(
            "isPortrait".into(),
            json!(
                default_image_id == Some(file.id.as_str())
                    || album_link
                        .as_ref()
                        .is_some_and(|l| default_image_id == Some(l.link_id.as_str()))
            ),
        );
        o.insert(
            "usedInChatCount".into(),
            json!(chat_usage.get(&file.id).copied().unwrap_or(0)),
        );
        entries.push(Value::Object(o));
    }

    // v4: `hasMore: effectiveOffset + page.length < rolls.length`.
    let has_more = effective_offset + (page.len() as i64) < total;
    Ok(json!({
        "entries": entries,
        "total": total,
        "hasMore": has_more,
    }))
}

/// What the read half of an album save resolved (see the module doc for why the
/// save is split).
pub enum AlbumSavePlan {
    /// v4's idempotent arm: the roll is already in the album, so nothing is
    /// written and the existing link is the answer.
    AlreadyInAlbum { link_id: String },
    /// The roll must be copied in — the caller fetches these bytes' `files` row
    /// through the host seam and calls [`commit_album_save`].
    NeedsBytes(AlbumSaveBytes),
}

/// The `files` identity the caller reads bytes for, and the metadata v4's
/// `saveFileToCharacterGallery` forwards to `saveToCharacterGallery`.
pub struct AlbumSaveBytes {
    pub file_id: String,
    pub original_filename: String,
    pub mime_type: String,
}

/// v4 `saveAvatarRollToAlbum`, read half: the roll, the vault, and whether the
/// album already holds these bytes — in v4's guard order (roll first, vault
/// second, existing-link third).
pub fn plan_album_save(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    file_id: &str,
) -> Result<AlbumSavePlan, AvatarRollError> {
    let file = require_roll(main, character_id, file_id)?;

    let Some(character) = characters_read::find_by_id(main, mount, character_id)? else {
        return Err(AvatarRollError::CharacterNotFound(character_id.to_string()));
    };
    let Some(vault) = resolve_character_vault(mount, &character)? else {
        return Err(AvatarRollError::NoVault(character_id.to_string()));
    };

    let (_roll_link, album_link) = classify_roll_links(mount, &file, Some(&vault.mount_point_id))?;
    if let Some(album) = album_link {
        return Ok(AlbumSavePlan::AlreadyInAlbum {
            link_id: album.link_id,
        });
    }

    Ok(AlbumSavePlan::NeedsBytes(AlbumSaveBytes {
        file_id: file.id,
        // v4 forwards `fileEntry.originalFilename` / `fileEntry.mimeType`
        // straight through `saveFileToCharacterGallery`.
        original_filename: file.original_filename,
        mime_type: file.mime_type.unwrap_or_default(),
    }))
}

/// v4 `saveAvatarRollToAlbum`, write half — the hard link itself, through the
/// same `saveToCharacterGallery` chokepoint every other album write uses.
/// Answers the new link id. A WRITE: run inside `Db::write`.
#[allow(clippy::too_many_arguments)]
pub fn commit_album_save(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    file_id: &str,
    data: &[u8],
    filename: &str,
    mime_type: &str,
    kept_at: &str,
) -> Result<String, AvatarRollError> {
    let saved = save_to_character_gallery(
        main,
        mount,
        character_id,
        data,
        filename,
        mime_type,
        None,
        &[],
        kept_at,
    )?;
    let link_id = saved
        .get("linkId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    tracing::info!(
        context = "photos.avatar-rolls",
        character_id,
        file_id,
        link_id = %link_id,
        "[AvatarRolls] Roll copied into the photo album"
    );
    Ok(link_id)
}

/// v4 `setAvatarRollAsPortrait`, write half — point `defaultImageId` at the
/// ALBUM LINK, never at the `files` row.
///
/// Post-Phase-3 every avatar pointer is a `doc_mount_file_links.id`;
/// `resolve_character_avatar` still tolerates a legacy `files.id` for
/// un-migrated imports, but minting a fresh one here would push the album's own
/// delete path (which scrubs pointers by LINK id) back out of step.
pub fn point_portrait_at_link(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    file_id: &str,
    link_id: &str,
    added_to_album: bool,
) -> Result<(), AvatarRollError> {
    let mut patch = Map::new();
    patch.insert("defaultImageId".into(), json!(link_id));
    crate::db::vault_character_update::update_character(main, mount, character_id, &patch)
        .map_err(crate::db::document_store_overlay::OverlayError::into_db)?;
    tracing::info!(
        context = "photos.avatar-rolls",
        character_id,
        file_id,
        link_id,
        added_to_album,
        "[AvatarRolls] Roll promoted to the character portrait"
    );
    Ok(())
}

/// v4 `DeleteAvatarRollOutput`.
#[derive(Debug, Clone, Copy)]
pub struct DeleteAvatarRollOutput {
    pub deleted: bool,
    /// True when the bytes went with the roll (no album copy was holding them).
    pub blob_removed: bool,
    /// Chats whose `characterAvatars` entry was pointing at the deleted roll.
    pub chats_scrubbed: i64,
    /// True when the album still holds a copy of these bytes.
    pub kept_in_album: bool,
}

impl DeleteAvatarRollOutput {
    pub fn to_json(self) -> Value {
        json!({
            "deleted": self.deleted,
            "blobRemoved": self.blob_removed,
            "chatsScrubbed": self.chats_scrubbed,
            "keptInAlbum": self.kept_in_album,
        })
    }
}

/// v4 `scrubChatAvatars` — drop every `chats.characterAvatars[characterId]`
/// entry pointing at a roll that is about to stop existing. Without this the
/// Salon keeps rendering a file id nothing backs until the next avatar job
/// rebinds the seat.
fn scrub_chat_avatars(
    main: &Connection,
    character_id: &str,
    file_id: &str,
) -> Result<i64, DbError> {
    let chats = ChatsRepository::new(main);
    let mut scrubbed = 0i64;
    for chat in chats_read::find_by_character_id(main, character_id)? {
        if read_bound_avatar_id(chat.get("characterAvatars"), character_id).as_deref()
            != Some(file_id)
        {
            continue;
        }
        let Some(chat_id) = chat.get("id").and_then(Value::as_str) else {
            continue;
        };
        // v4 spreads the existing map and `delete`s the one key.
        let mut next = chat
            .get("characterAvatars")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        next.remove(character_id);
        chats.update(
            chat_id,
            &ChatUpdate {
                character_avatars: Some(Value::Object(next)),
                ..Default::default()
            },
        )?;
        scrubbed += 1;
    }
    Ok(scrubbed)
}

/// v4 `deleteAvatarRoll` — throw a roll away.
///
/// Order matters: every pointer at the roll is scrubbed *before* the bytes go,
/// so nothing is left naming a file that has stopped existing —
/// `chats.characterAvatars` (where the avatar job binds a roll), the character's
/// `avatarOverrides`, and `defaultImageId`. Only then is the roll's own mount
/// link dropped with GC, which reclaims the blob when it was the last reference.
///
/// An album copy is never a casualty. If the operator kept this plate in the
/// character's `photos/` folder, that link stays and the bytes stay with it;
/// only the cache row and the roll's own link go. A roll whose *only* link is
/// the album copy therefore loses its `files` row and nothing else — which is
/// exactly right, since the picture is still in the album where they put it.
///
/// The cache treats a keyed row whose blob is gone as a miss, so the next time
/// this configuration comes round the house simply draws it again.
///
/// A WRITE: run inside `Db::write`.
pub fn delete_avatar_roll(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    file_id: &str,
) -> Result<DeleteAvatarRollOutput, AvatarRollError> {
    // v4 answers a MISS rather than throwing — the route turns it into a 404.
    let Some(file) = find_roll(main, character_id, file_id)? else {
        return Ok(DeleteAvatarRollOutput {
            deleted: false,
            blob_removed: false,
            chats_scrubbed: 0,
            kept_in_album: false,
        });
    };

    let character = characters_read::find_by_id(main, mount, character_id)?;
    let vault = match &character {
        Some(c) => resolve_character_vault(mount, c)?,
        None => None,
    };
    let (roll_link, album_link) = classify_roll_links(
        mount,
        &file,
        vault.as_ref().map(|v| v.mount_point_id.as_str()),
    )?;

    // 1. Chats displaying this plate.
    let chats_scrubbed = scrub_chat_avatars(main, character_id, file_id)?;

    // 2. The character's own pointers. `defaultImageId` can name either shape:
    //    the roll's `files` row (a legacy pointer) or the album link we are
    //    deliberately leaving in place — only the former is cleared.
    if let Some(c) = &character {
        let mut patch = Map::new();
        if c.get("defaultImageId").and_then(Value::as_str) == Some(file_id) {
            patch.insert("defaultImageId".into(), Value::Null);
        }
        let overrides: Vec<Value> = c
            .get("avatarOverrides")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let remaining: Vec<Value> = overrides
            .iter()
            .filter(|o| o.get("imageId").and_then(Value::as_str) != Some(file_id))
            .cloned()
            .collect();
        if remaining.len() != overrides.len() {
            patch.insert("avatarOverrides".into(), Value::Array(remaining));
        }
        if !patch.is_empty() {
            crate::db::vault_character_update::update_character(main, mount, character_id, &patch)
                .map_err(crate::db::document_store_overlay::OverlayError::into_db)?;
        }
    }

    // 3. The roll's own link — never the album's.
    let mut blob_removed = false;
    if let Some(link) = &roll_link {
        blob_removed = DocMountFileLinksRepository::new(mount).delete_with_gc(&link.link_id)?;
        // v4 also calls `invalidateMountPoint(rollLink.mountPointId)`; v5 has no
        // mount chunk cache, so that call has NO COUNTERPART (the same recorded
        // no-op `services::embedding_reapply_profile` names for v4's
        // `invalidateMountChunkCacheAll`). `refreshStats` is real, and v4
        // swallows its failure (`.catch(() => {})`) — so does this.
        let _ = DocMountPointsRepository::new(mount).refresh_stats(&link.mount_point_id);
    }

    // 4. The cache row itself.
    FilesRepository::new(main).delete(file_id)?;

    let kept_in_album = album_link.is_some();
    tracing::info!(
        context = "photos.avatar-rolls",
        character_id,
        file_id,
        roll_link_id = roll_link.as_ref().map(|l| l.link_id.as_str()).unwrap_or(""),
        kept_in_album,
        blob_removed,
        chats_scrubbed,
        "[AvatarRolls] Roll deleted"
    );

    Ok(DeleteAvatarRollOutput {
        deleted: true,
        blob_removed,
        chats_scrubbed,
        kept_in_album,
    })
}
