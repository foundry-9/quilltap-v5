//! Writer for Document Mode chat notifications (the Librarian) — v4
//! `lib/services/librarian-notifications/writer.ts`.
//!
//! When a document is opened in Document Mode, saved, renamed, deleted, moved,
//! copied, written, or otherwise touched, this helper injects a synthetic
//! ASSISTANT-role chat message announcing the event on behalf of the Librarian.
//! Characters see the announcement in their recent history and know the document
//! is available (and who touched it), without the user losing their turn.
//!
//! Every announcement is a `MessageEvent` tagged `systemSender: "librarian"` with
//! a per-kind `systemKind`, built as a JSON object literal (matching v4's
//! `MessageEvent`) and validated into a [`ChatEventInput`] so the stored row is
//! byte-for-byte the `db::chats_messages` insert marshaling (the `carina_runner`
//! writer idiom).
//!
//! Errors never propagate — a document operation must never fail because an
//! announcement couldn't be written; a post failure is surfaced to the caller as
//! `None` rather than thrown (host-side logging is out of scope for the port).

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::chats_messages::{ChatEventInput, ChatMessagesRepository};
use crate::db::runtime::Db;

// `SUMMARY_CONTENT_PREFIX`'s canonical home is v4 `writer.ts:790`; v5 first landed
// it in `context_summary` (the sweep that matches on it). Re-export that single
// definition so there is exactly one copy shared by the sweep and this writer.
pub use crate::services::context_summary::SUMMARY_CONTENT_PREFIX;

/// v4 `SUMMARY_OPAQUE_CONTENT_PREFIX` (`writer.ts:793`).
pub const SUMMARY_OPAQUE_CONTENT_PREFIX: &str =
    "Précis of the conversation to date — file it for reference:";

// ---------------------------------------------------------------------------
// Shared scope / origin vocabulary.
// ---------------------------------------------------------------------------

/// v4 `'project' | 'document_store' | 'general'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarianScope {
    Project,
    DocumentStore,
    General,
}

/// Who opened a document (v4 `LibrarianOpenKind`).
#[derive(Debug, Clone)]
pub enum LibrarianOpenKind {
    ByUser,
    ByCharacter { character_name: String },
}

/// Who initiated a destructive or structural change (v4 `LibrarianActorOrigin`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibrarianActorOrigin {
    ByUser,
    ByCharacter { character_name: String },
}

// ---------------------------------------------------------------------------
// Suppression for character_read:false documents.
// ---------------------------------------------------------------------------

/// True when a document's own frontmatter hides it from characters. Pure — v4
/// `contentHiddenFromCharacters`.
pub fn content_hidden_from_characters(content: &str) -> bool {
    !crate::db::doc_mount_file_links::policy_from_content(content).character_read
}

/// True when the indexed document at `(mount_point_id, relative_path)` is
/// `character_read:false`. Returns false for missing ids/paths, missing links, or
/// any lookup error — an announcement is never blocked by a transient fault (v4
/// `documentHiddenFromCharacters`).
pub async fn document_hidden_from_characters(
    db: &Db,
    mount_point_id: Option<&str>,
    relative_path: Option<&str>,
) -> bool {
    let (Some(mount_point_id), Some(relative_path)) = (mount_point_id, relative_path) else {
        return false;
    };
    if mount_point_id.is_empty() || relative_path.is_empty() {
        return false;
    }
    let mount_point_id = mount_point_id.to_string();
    let relative_path = relative_path.to_string();
    match db.read_mount_index(move |mount| {
        crate::db::doc_mount_file_links::DocMountFileLinksRepository::new(mount)
            .find_by_mount_point_and_path(&mount_point_id, &relative_path)
    }) {
        Ok(Some(link)) => !link.allow_character_read,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Label helpers (v4 private functions).
// ---------------------------------------------------------------------------

/// v4 `librarianUri`: the single canonical `qtap://` URI for an announced
/// document. `mount_point` may be the reserved `self` literal or empty.
fn librarian_uri(scope: LibrarianScope, mount_point: Option<&str>, path: &str) -> String {
    use crate::doc_edit::qtap_uri::{
        format_doc_store_uri, format_scoped_uri, format_self_uri, ScopedAuthority,
    };
    match scope {
        LibrarianScope::Project => format_scoped_uri(ScopedAuthority::Project, path, None, None),
        LibrarianScope::General => format_scoped_uri(ScopedAuthority::General, path, None, None),
        LibrarianScope::DocumentStore => {
            // v4: `if (!mountPoint || mountPoint.toLowerCase() === 'self') return formatSelfUri(path);`
            match mount_point {
                Some(mp) if !mp.is_empty() && mp.to_lowercase() != "self" => {
                    format_doc_store_uri(mp, "", path, false, None, None)
                }
                _ => format_self_uri(path, None, None),
            }
        }
    }
}

/// v4 `scopeLabel`.
fn scope_label(scope: LibrarianScope, mount_point: Option<&str>) -> String {
    if scope == LibrarianScope::DocumentStore {
        if let Some(mp) = mount_point {
            if !mp.is_empty() {
                return format!("the document store \"{mp}\"");
            }
        }
    }
    if scope == LibrarianScope::Project {
        return "the project library".to_string();
    }
    "the general library".to_string()
}

/// v4 `requesterLabel`.
fn requester_label(origin: &LibrarianOpenKind) -> String {
    match origin {
        LibrarianOpenKind::ByUser => "the user's request".to_string(),
        LibrarianOpenKind::ByCharacter { character_name } => format!("{character_name}'s request"),
    }
}

/// v4 `actorLabel`.
fn actor_label(origin: &LibrarianActorOrigin) -> String {
    match origin {
        LibrarianActorOrigin::ByUser => "the user's instruction".to_string(),
        LibrarianActorOrigin::ByCharacter { character_name } => {
            format!("{character_name}'s instruction")
        }
    }
}

fn is_image_mime(mime: &str) -> bool {
    mime.to_lowercase().starts_with("image/")
}

/// Soft caps for an embedded diff / new-file body in an announcement.
const ANNOUNCEMENT_MAX_LINES: usize = 150;
const ANNOUNCEMENT_MAX_CHARS: usize = 6000;

/// v4 `truncateForAnnouncement` — cap a diff / new-file body so a large change
/// can't bloat the chat history. UTF-16 char math (JS `.length`/`.slice`).
fn truncate_for_announcement(text: &str, uri: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut kept = text.to_string();
    let mut truncated = false;
    if lines.len() > ANNOUNCEMENT_MAX_LINES {
        kept = lines[..ANNOUNCEMENT_MAX_LINES].join("\n");
        truncated = true;
    }
    if crate::jsstr::utf16_len(&kept) > ANNOUNCEMENT_MAX_CHARS {
        kept = crate::jsstr::utf16_truncate(&kept, ANNOUNCEMENT_MAX_CHARS);
        truncated = true;
    }
    if !truncated {
        return text.to_string();
    }
    let kept_line_count = kept.split('\n').count();
    let remaining = lines.len().saturating_sub(kept_line_count);
    let more_note = if remaining > 0 {
        format!(
            "{remaining} more line{}",
            if remaining == 1 { "" } else { "s" }
        )
    } else {
        "more content".to_string()
    };
    format!("{kept}\n… [truncated — {more_note}; open the full document at {uri}]")
}

// ---------------------------------------------------------------------------
// Open announcement.
// ---------------------------------------------------------------------------

/// v4 `LibrarianOpenAnnouncement`.
#[derive(Debug, Clone)]
pub struct LibrarianOpenAnnouncement {
    pub chat_id: String,
    pub display_title: String,
    pub file_path: String,
    pub scope: LibrarianScope,
    pub mount_point: Option<String>,
    pub is_new: bool,
    pub origin: LibrarianOpenKind,
    /// When true, suppress the announcement (`character_read:false`).
    pub hidden_from_characters: bool,
}

pub fn build_open_content(params: &LibrarianOpenAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let who = requester_label(&params.origin);
    let path_details = librarian_uri(
        params.scope,
        params.mount_point.as_deref(),
        &params.file_path,
    );
    let title = &params.display_title;
    if params.is_new {
        format!("The Librarian has laid out a fresh, blank page titled \"{title}\" upon the table at {who}. You may use doc_read_file and the other doc_* editing tools to read or amend it ({path_details}).")
    } else {
        format!("The Librarian has set out \"{title}\" from {where_} at {who}. You may use doc_read_file and the other doc_* editing tools to consult or revise it ({path_details}).")
    }
}

pub fn build_open_opaque_content(params: &LibrarianOpenAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let who = requester_label(&params.origin);
    let path_details = librarian_uri(
        params.scope,
        params.mount_point.as_deref(),
        &params.file_path,
    );
    let title = &params.display_title;
    if params.is_new {
        format!("Document created: \"{title}\" — opened blank at {who}. Use doc_read_file and the other doc_* editing tools to read or amend it ({path_details}).")
    } else {
        format!("Document opened: \"{title}\" from {where_} at {who}. Use doc_read_file and the other doc_* editing tools to consult or revise it ({path_details}).")
    }
}

pub async fn post_librarian_open_announcement(
    db: &Db,
    params: &LibrarianOpenAnnouncement,
) -> Option<Value> {
    if params.hidden_from_characters {
        return None;
    }
    let content = build_open_content(params);
    let opaque = build_open_opaque_content(params);
    let kind_label = match &params.origin {
        LibrarianOpenKind::ByUser => "opened-by-user".to_string(),
        LibrarianOpenKind::ByCharacter { .. } => "opened-by-character".to_string(),
    };
    post_librarian_message(
        db,
        &params.chat_id,
        &content,
        Some(&opaque),
        &kind_label,
        &[],
        None,
        None,
    )
    .await
}

// ---------------------------------------------------------------------------
// Rename announcement.
// ---------------------------------------------------------------------------

/// v4 `LibrarianRenameAnnouncement`.
#[derive(Debug, Clone)]
pub struct LibrarianRenameAnnouncement {
    pub chat_id: String,
    pub old_display_title: String,
    pub new_display_title: String,
    pub old_file_path: String,
    pub new_file_path: String,
    pub scope: LibrarianScope,
    pub mount_point: Option<String>,
    pub hidden_from_characters: bool,
}

pub fn build_rename_content(params: &LibrarianRenameAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let old_uri = librarian_uri(
        params.scope,
        params.mount_point.as_deref(),
        &params.old_file_path,
    );
    let new_uri = librarian_uri(
        params.scope,
        params.mount_point.as_deref(),
        &params.new_file_path,
    );
    let path_details = format!("was {old_uri}, now {new_uri}");
    let old = &params.old_display_title;
    let new = &params.new_display_title;
    format!("The Librarian has rechristened the volume formerly catalogued as \"{old}\" in {where_} — it now answers to \"{new}\", and the card in the catalogue has been amended to suit. Subsequent references should use the new name ({path_details}).")
}

pub fn build_rename_opaque_content(params: &LibrarianRenameAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let old_uri = librarian_uri(
        params.scope,
        params.mount_point.as_deref(),
        &params.old_file_path,
    );
    let new_uri = librarian_uri(
        params.scope,
        params.mount_point.as_deref(),
        &params.new_file_path,
    );
    let path_details = format!("was {old_uri}, now {new_uri}");
    let old = &params.old_display_title;
    let new = &params.new_display_title;
    format!("Document renamed in {where_}: \"{old}\" → \"{new}\". Subsequent references should use the new name ({path_details}).")
}

pub async fn post_librarian_rename_announcement(
    db: &Db,
    params: &LibrarianRenameAnnouncement,
) -> Option<Value> {
    if params.hidden_from_characters {
        return None;
    }
    let content = build_rename_content(params);
    let opaque = build_rename_opaque_content(params);
    post_librarian_message(
        db,
        &params.chat_id,
        &content,
        Some(&opaque),
        "renamed",
        &[],
        None,
        None,
    )
    .await
}

// ---------------------------------------------------------------------------
// Save announcement (Document-Mode editor autosave/manual-save; diff owned by caller).
// ---------------------------------------------------------------------------

/// v4 `LibrarianSaveAnnouncement`.
#[derive(Debug, Clone)]
pub struct LibrarianSaveAnnouncement {
    pub chat_id: String,
    /// Pre-formatted diff content (from `formatAutosaveNotification`).
    pub diff_content: String,
    pub hidden_from_characters: bool,
}

pub fn build_save_content(diff_content: &str) -> String {
    // v4: replace(/^I've made changes to (".+?"):/, 'The Librarian has filed the following alterations to $1:')
    let re = save_prefix_regex();
    let rephrased = re.replace(
        diff_content,
        "The Librarian has filed the following alterations to $1:",
    );
    if rephrased.as_ref() == diff_content {
        format!("The Librarian has filed the following alterations:\n\n{diff_content}")
    } else {
        rephrased.into_owned()
    }
}

pub fn build_save_opaque_content(diff_content: &str) -> String {
    let re = save_prefix_regex();
    let rephrased = re.replace(diff_content, "The following changes were filed to $1:");
    if rephrased.as_ref() == diff_content {
        format!("The following changes were filed:\n\n{diff_content}")
    } else {
        rephrased.into_owned()
    }
}

fn save_prefix_regex() -> &'static regex::Regex {
    use std::sync::LazyLock;
    // JS `/^I've made changes to (".+?"):/` — no `s`/`m` flags, so `.` excludes
    // line terminators and `^` anchors the absolute start; first-match replace.
    static RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"^I've made changes to ("[^\n\r\u{2028}\u{2029}]+?"):"#).unwrap()
    });
    &RE
}

pub async fn post_librarian_save_announcement(
    db: &Db,
    params: &LibrarianSaveAnnouncement,
) -> Option<Value> {
    if params.hidden_from_characters {
        return None;
    }
    if params.diff_content.is_empty() || crate::jsstr::js_trim(&params.diff_content).is_empty() {
        return None;
    }
    let content = build_save_content(&params.diff_content);
    let opaque = build_save_opaque_content(&params.diff_content);
    post_librarian_message(
        db,
        &params.chat_id,
        &content,
        Some(&opaque),
        "saved",
        &[],
        None,
        None,
    )
    .await
}

// ---------------------------------------------------------------------------
// Delete announcement.
// ---------------------------------------------------------------------------

/// v4 `LibrarianDeleteAnnouncement`.
#[derive(Debug, Clone)]
pub struct LibrarianDeleteAnnouncement {
    pub chat_id: String,
    pub display_title: String,
    pub file_path: String,
    pub scope: LibrarianScope,
    pub mount_point: Option<String>,
    pub origin: LibrarianActorOrigin,
    pub hidden_from_characters: bool,
}

pub fn build_delete_content(params: &LibrarianDeleteAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let who = actor_label(&params.origin);
    let path_details = librarian_uri(
        params.scope,
        params.mount_point.as_deref(),
        &params.file_path,
    );
    let title = &params.display_title;
    format!("The Librarian has removed \"{title}\" from {where_} at {who}. The volume is gone from the shelves, and its card struck from the catalogue ({path_details}).")
}

pub fn build_delete_opaque_content(params: &LibrarianDeleteAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let who = actor_label(&params.origin);
    let path_details = librarian_uri(
        params.scope,
        params.mount_point.as_deref(),
        &params.file_path,
    );
    let title = &params.display_title;
    format!("Document removed: \"{title}\" from {where_} at {who} ({path_details}).")
}

pub async fn post_librarian_delete_announcement(
    db: &Db,
    params: &LibrarianDeleteAnnouncement,
) -> Option<Value> {
    if params.hidden_from_characters {
        return None;
    }
    let content = build_delete_content(params);
    let opaque = build_delete_opaque_content(params);
    let kind_label = match &params.origin {
        LibrarianActorOrigin::ByUser => "deleted-by-user",
        LibrarianActorOrigin::ByCharacter { .. } => "deleted-by-character",
    };
    post_librarian_message(
        db,
        &params.chat_id,
        &content,
        Some(&opaque),
        kind_label,
        &[],
        None,
        None,
    )
    .await
}

// ---------------------------------------------------------------------------
// Folder created / deleted announcements.
// ---------------------------------------------------------------------------

/// v4 `LibrarianFolderCreatedAnnouncement`.
#[derive(Debug, Clone)]
pub struct LibrarianFolderCreatedAnnouncement {
    pub chat_id: String,
    pub folder_path: String,
    pub scope: LibrarianScope,
    pub mount_point: Option<String>,
    pub origin: LibrarianActorOrigin,
}

pub fn build_folder_created_content(params: &LibrarianFolderCreatedAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let who = actor_label(&params.origin);
    let path_details = librarian_uri(
        params.scope,
        params.mount_point.as_deref(),
        &params.folder_path,
    );
    let folder = &params.folder_path;
    format!("The Librarian has set aside a fresh shelf for \"{folder}\" in {where_} at {who} — a new folder, presently empty, awaiting its tenants ({path_details}).")
}

pub fn build_folder_created_opaque_content(params: &LibrarianFolderCreatedAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let who = actor_label(&params.origin);
    let path_details = librarian_uri(
        params.scope,
        params.mount_point.as_deref(),
        &params.folder_path,
    );
    let folder = &params.folder_path;
    format!("Folder created: \"{folder}\" in {where_} at {who} — currently empty ({path_details}).")
}

/// v4 `LibrarianFolderDeletedAnnouncement`.
#[derive(Debug, Clone)]
pub struct LibrarianFolderDeletedAnnouncement {
    pub chat_id: String,
    pub folder_path: String,
    pub scope: LibrarianScope,
    pub mount_point: Option<String>,
    pub origin: LibrarianActorOrigin,
}

pub fn build_folder_deleted_content(params: &LibrarianFolderDeletedAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let who = actor_label(&params.origin);
    let path_details = librarian_uri(
        params.scope,
        params.mount_point.as_deref(),
        &params.folder_path,
    );
    let folder = &params.folder_path;
    format!("The Librarian has dismantled the empty shelf at \"{folder}\" in {where_} at {who}. The folder has been cleared from the catalogue ({path_details}).")
}

pub fn build_folder_deleted_opaque_content(params: &LibrarianFolderDeletedAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let who = actor_label(&params.origin);
    let path_details = librarian_uri(
        params.scope,
        params.mount_point.as_deref(),
        &params.folder_path,
    );
    let folder = &params.folder_path;
    format!("Folder removed: \"{folder}\" in {where_} at {who} ({path_details}).")
}

pub async fn post_librarian_folder_created_announcement(
    db: &Db,
    params: &LibrarianFolderCreatedAnnouncement,
) -> Option<Value> {
    let content = build_folder_created_content(params);
    let opaque = build_folder_created_opaque_content(params);
    let kind_label = match &params.origin {
        LibrarianActorOrigin::ByUser => "folder-created-by-user",
        LibrarianActorOrigin::ByCharacter { .. } => "folder-created-by-character",
    };
    post_librarian_message(
        db,
        &params.chat_id,
        &content,
        Some(&opaque),
        kind_label,
        &[],
        None,
        None,
    )
    .await
}

pub async fn post_librarian_folder_deleted_announcement(
    db: &Db,
    params: &LibrarianFolderDeletedAnnouncement,
) -> Option<Value> {
    let content = build_folder_deleted_content(params);
    let opaque = build_folder_deleted_opaque_content(params);
    let kind_label = match &params.origin {
        LibrarianActorOrigin::ByUser => "folder-deleted-by-user",
        LibrarianActorOrigin::ByCharacter { .. } => "folder-deleted-by-character",
    };
    post_librarian_message(
        db,
        &params.chat_id,
        &content,
        Some(&opaque),
        kind_label,
        &[],
        None,
        None,
    )
    .await
}

// ---------------------------------------------------------------------------
// Attach announcement (a Scriptorium document-store file pinned to the chat).
// ---------------------------------------------------------------------------

/// v4 `LibrarianAttachAnnouncement`.
#[derive(Debug, Clone)]
pub struct LibrarianAttachAnnouncement {
    pub chat_id: String,
    pub display_title: String,
    pub file_path: String,
    pub mount_point: Option<String>,
    /// `doc_mount_file_links.id` of the attached file.
    pub mount_file_id: String,
    pub mime_type: String,
    pub description: Option<String>,
}

/// v4 `attachIdHint`.
fn attach_id_hint(link_id: &str, is_image: bool) -> String {
    if !is_image {
        format!("Catalogue handle: `{link_id}`.")
    } else {
        format!("The illustration is catalogued under uuid `{link_id}` — it may be filed away in your own album later with keep_image, or re-summoned with attach_image.")
    }
}

pub fn build_attach_content(params: &LibrarianAttachAnnouncement) -> String {
    let where_ = attach_store_label(params.mount_point.as_deref());
    let path_details = librarian_uri(
        LibrarianScope::DocumentStore,
        params.mount_point.as_deref(),
        &params.file_path,
    );
    let is_image = is_image_mime(&params.mime_type);
    let title = &params.display_title;
    let kind_phrase = if is_image {
        format!("the illustration \"{title}\"")
    } else {
        format!("the volume \"{title}\"")
    };
    let lead = format!("The user has bid the Librarian set {kind_phrase} from {where_} upon the table for your perusal — please consult it as part of your reply ({path_details}).");
    let handle = attach_id_hint(&params.mount_file_id, is_image);
    let trimmed = params
        .description
        .as_deref()
        .map(crate::jsstr::js_trim)
        .filter(|s| !s.is_empty());
    if let Some(trimmed) = trimmed {
        format!("{lead}\n\n{handle}\n\nThe Librarian's catalogue describes the illustration thus:\n\n{trimmed}")
    } else {
        format!("{lead}\n\n{handle}")
    }
}

pub fn build_attach_opaque_content(params: &LibrarianAttachAnnouncement) -> String {
    let where_ = attach_store_label(params.mount_point.as_deref());
    let path_details = librarian_uri(
        LibrarianScope::DocumentStore,
        params.mount_point.as_deref(),
        &params.file_path,
    );
    let is_image = is_image_mime(&params.mime_type);
    let title = &params.display_title;
    let kind_phrase = if is_image {
        format!("Image attached: \"{title}\"")
    } else {
        format!("Document attached: \"{title}\"")
    };
    let lead = format!("{kind_phrase} from {where_} — the user has placed it before you for your reply ({path_details}).");
    let handle = attach_id_hint(&params.mount_file_id, is_image);
    let trimmed = params
        .description
        .as_deref()
        .map(crate::jsstr::js_trim)
        .filter(|s| !s.is_empty());
    if let Some(trimmed) = trimmed {
        format!("{lead}\n\n{handle}\n\nDescription:\n\n{trimmed}")
    } else {
        format!("{lead}\n\n{handle}")
    }
}

/// v4 `const where = mountPoint ? \`the document store "${mountPoint}"\` : 'the document store'`.
fn attach_store_label(mount_point: Option<&str>) -> String {
    match mount_point {
        Some(mp) if !mp.is_empty() => format!("the document store \"{mp}\""),
        _ => "the document store".to_string(),
    }
}

pub async fn post_librarian_attach_announcement(
    db: &Db,
    params: &LibrarianAttachAnnouncement,
) -> Option<Value> {
    let content = build_attach_content(params);
    let opaque = build_attach_opaque_content(params);
    let attachments = [params.mount_file_id.clone()];
    post_librarian_message(
        db,
        &params.chat_id,
        &content,
        Some(&opaque),
        "attached",
        &attachments,
        None,
        None,
    )
    .await
}

// ---------------------------------------------------------------------------
// Character-initiated content writes, moves, copies, blob uploads.
// ---------------------------------------------------------------------------

/// The change reported by a write announcement (v4 `change` union).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibrarianWriteChange {
    Created { body: String },
    Edited { diff: String },
}

/// v4 `LibrarianWriteAnnouncement`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarianWriteAnnouncement {
    pub chat_id: String,
    pub display_title: String,
    /// Canonical qtap:// URI precomputed by the handler.
    pub uri: String,
    pub scope: LibrarianScope,
    pub mount_point: Option<String>,
    pub origin: LibrarianActorOrigin,
    pub change: LibrarianWriteChange,
    pub hidden_from_characters: bool,
}

pub fn build_write_content(params: &LibrarianWriteAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let who = actor_label(&params.origin);
    let title = &params.display_title;
    let uri = &params.uri;
    match &params.change {
        LibrarianWriteChange::Created { body } => {
            if crate::jsstr::js_trim(body).is_empty() {
                format!("The Librarian has set down a fresh, empty page titled \"{title}\" in {where_} at {who} ({uri}).")
            } else {
                let fenced = format!("```\n{}\n```", truncate_for_announcement(body, uri));
                format!("The Librarian has set down a new volume, \"{title}\", in {where_} at {who} ({uri}). Its contents read:\n\n{fenced}")
            }
        }
        LibrarianWriteChange::Edited { diff } => {
            let fenced = format!("```diff\n{}\n```", truncate_for_announcement(diff, uri));
            format!("The Librarian has filed fresh alterations to \"{title}\" in {where_} at {who} ({uri}):\n\n{fenced}")
        }
    }
}

pub fn build_write_opaque_content(params: &LibrarianWriteAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let who = actor_label(&params.origin);
    let title = &params.display_title;
    let uri = &params.uri;
    match &params.change {
        LibrarianWriteChange::Created { body } => {
            if crate::jsstr::js_trim(body).is_empty() {
                format!("Document created (empty): \"{title}\" in {where_} at {who} ({uri}).")
            } else {
                let fenced = format!("```\n{}\n```", truncate_for_announcement(body, uri));
                format!("Document created: \"{title}\" in {where_} at {who} ({uri}). Contents:\n\n{fenced}")
            }
        }
        LibrarianWriteChange::Edited { diff } => {
            let fenced = format!("```diff\n{}\n```", truncate_for_announcement(diff, uri));
            format!("Document edited: \"{title}\" in {where_} at {who} ({uri}):\n\n{fenced}")
        }
    }
}

/// Assemble the write announcement's `MessageEvent` (applying the
/// `hidden_from_characters` + empty-edit gates), returning the event to insert and
/// the message JSON v4 returns — or `None` when the announcement is suppressed or
/// validation fails. Shared by the async ([`post_librarian_write_announcement`])
/// and synchronous ([`post_librarian_write_announcement_conn`]) posters so the
/// content/kind assembly lives once.
fn write_announcement_message(
    params: &LibrarianWriteAnnouncement,
) -> Option<(ChatEventInput, Value)> {
    if params.hidden_from_characters {
        return None;
    }
    // An edit with an empty diff means nothing actually changed — stay silent.
    if let LibrarianWriteChange::Edited { diff } = &params.change {
        if crate::jsstr::js_trim(diff).is_empty() {
            return None;
        }
    }
    let content = build_write_content(params);
    let opaque = build_write_opaque_content(params);
    let verb = match &params.change {
        LibrarianWriteChange::Created { .. } => "created",
        LibrarianWriteChange::Edited { .. } => "edited",
    };
    let kind_label = match &params.origin {
        LibrarianActorOrigin::ByUser => format!("{verb}-by-user"),
        LibrarianActorOrigin::ByCharacter { .. } => format!("{verb}-by-character"),
    };
    build_librarian_message(&content, Some(&opaque), &kind_label, &[], None, None)
}

pub async fn post_librarian_write_announcement(
    db: &Db,
    params: &LibrarianWriteAnnouncement,
) -> Option<Value> {
    let (event, message) = write_announcement_message(params)?;
    let chat_id = params.chat_id.clone();
    match db
        .write(move |writers| writers.main().chat_messages().add_message(&chat_id, &event))
        .await
    {
        Ok(()) => Some(message),
        Err(_) => None,
    }
}

/// Synchronous sibling of [`post_librarian_write_announcement`] — posts the write
/// announcement over an already-held RW `main` connection (the differential drives
/// the doc-edit handlers with a raw [`crate::db::Writer`], not the async writer
/// task). Same gates + marshaling; errors never propagate (returns `None`).
pub fn post_librarian_write_announcement_conn(
    main: &Connection,
    params: &LibrarianWriteAnnouncement,
) -> Option<Value> {
    let (event, message) = write_announcement_message(params)?;
    match ChatMessagesRepository::new(main).add_message(&params.chat_id, &event) {
        Ok(()) => Some(message),
        Err(_) => None,
    }
}

/// v4 `LibrarianMoveAnnouncement`.
#[derive(Debug, Clone)]
pub struct LibrarianMoveAnnouncement {
    pub chat_id: String,
    pub old_display_title: String,
    pub new_display_title: String,
    pub old_uri: String,
    pub new_uri: String,
    pub scope: LibrarianScope,
    pub mount_point: Option<String>,
    pub origin: LibrarianActorOrigin,
    pub is_folder: bool,
    pub hidden_from_characters: bool,
}

pub fn build_move_content(params: &LibrarianMoveAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let who = actor_label(&params.origin);
    let thing = if params.is_folder { "shelf" } else { "volume" };
    let old = &params.old_display_title;
    let new = &params.new_display_title;
    let old_uri = &params.old_uri;
    let new_uri = &params.new_uri;
    format!("The Librarian has relocated the {thing} \"{old}\" within {where_} at {who} — it now rests as \"{new}\", and the card in the catalogue has been amended to suit. Subsequent references should use the new address (was {old_uri}, now {new_uri}).")
}

pub fn build_move_opaque_content(params: &LibrarianMoveAnnouncement) -> String {
    let where_ = scope_label(params.scope, params.mount_point.as_deref());
    let noun = if params.is_folder { "Folder" } else { "File" };
    let old = &params.old_display_title;
    let new = &params.new_display_title;
    let old_uri = &params.old_uri;
    let new_uri = &params.new_uri;
    format!("{noun} moved in {where_}: \"{old}\" → \"{new}\". Subsequent references should use the new address (was {old_uri}, now {new_uri}).")
}

pub async fn post_librarian_move_announcement(
    db: &Db,
    params: &LibrarianMoveAnnouncement,
) -> Option<Value> {
    if params.hidden_from_characters {
        return None;
    }
    let content = build_move_content(params);
    let opaque = build_move_opaque_content(params);
    let kind_label = match &params.origin {
        LibrarianActorOrigin::ByUser => "moved-by-user",
        LibrarianActorOrigin::ByCharacter { .. } => "moved-by-character",
    };
    post_librarian_message(
        db,
        &params.chat_id,
        &content,
        Some(&opaque),
        kind_label,
        &[],
        None,
        None,
    )
    .await
}

/// v4 `LibrarianCopyAnnouncement`.
#[derive(Debug, Clone)]
pub struct LibrarianCopyAnnouncement {
    pub chat_id: String,
    pub source_display_title: String,
    pub dest_display_title: String,
    pub source_mount_point: String,
    pub dest_mount_point: String,
    pub source_uri: String,
    pub dest_uri: String,
    pub origin: LibrarianActorOrigin,
    pub hidden_from_characters: bool,
}

pub fn build_copy_content(params: &LibrarianCopyAnnouncement) -> String {
    let who = actor_label(&params.origin);
    let src_t = &params.source_display_title;
    let dst_t = &params.dest_display_title;
    let src_mp = &params.source_mount_point;
    let dst_mp = &params.dest_mount_point;
    let src_uri = &params.source_uri;
    let dst_uri = &params.dest_uri;
    format!("The Librarian has transcribed a copy of \"{src_t}\" from the document store \"{src_mp}\" into \"{dst_mp}\" at {who} — the original remains where it sat, and a faithful duplicate now answers to \"{dst_t}\" (from {src_uri} to {dst_uri}).")
}

pub fn build_copy_opaque_content(params: &LibrarianCopyAnnouncement) -> String {
    let src_t = &params.source_display_title;
    let dst_t = &params.dest_display_title;
    let src_mp = &params.source_mount_point;
    let dst_mp = &params.dest_mount_point;
    let src_uri = &params.source_uri;
    let dst_uri = &params.dest_uri;
    format!("File copied: \"{src_t}\" from \"{src_mp}\" to \"{dst_mp}\" as \"{dst_t}\" (from {src_uri} to {dst_uri}).")
}

pub async fn post_librarian_copy_announcement(
    db: &Db,
    params: &LibrarianCopyAnnouncement,
) -> Option<Value> {
    if params.hidden_from_characters {
        return None;
    }
    let content = build_copy_content(params);
    let opaque = build_copy_opaque_content(params);
    let kind_label = match &params.origin {
        LibrarianActorOrigin::ByUser => "copied-by-user",
        LibrarianActorOrigin::ByCharacter { .. } => "copied-by-character",
    };
    post_librarian_message(
        db,
        &params.chat_id,
        &content,
        Some(&opaque),
        kind_label,
        &[],
        None,
        None,
    )
    .await
}

/// v4 `LibrarianBlobWriteAnnouncement`.
#[derive(Debug, Clone)]
pub struct LibrarianBlobWriteAnnouncement {
    pub chat_id: String,
    pub display_title: String,
    pub uri: String,
    pub mount_point: Option<String>,
    pub mime_type: String,
    pub size_bytes: i64,
    pub description: Option<String>,
    pub origin: LibrarianActorOrigin,
}

pub fn build_blob_write_content(params: &LibrarianBlobWriteAnnouncement) -> String {
    let where_ = attach_store_label(params.mount_point.as_deref());
    let who = actor_label(&params.origin);
    let kind_phrase = if is_image_mime(&params.mime_type) {
        "illustration"
    } else {
        "asset"
    };
    let title = &params.display_title;
    let mime = &params.mime_type;
    let size = params.size_bytes;
    let uri = &params.uri;
    let lead = format!("The Librarian has affixed the {kind_phrase} \"{title}\" to {where_} at {who} — {mime}, {size} bytes ({uri}).");
    let trimmed = params
        .description
        .as_deref()
        .map(crate::jsstr::js_trim)
        .filter(|s| !s.is_empty());
    match trimmed {
        Some(t) => format!("{lead}\n\nThe catalogue describes it thus: {t}"),
        None => lead,
    }
}

pub fn build_blob_write_opaque_content(params: &LibrarianBlobWriteAnnouncement) -> String {
    let where_ = attach_store_label(params.mount_point.as_deref());
    let kind_word = if is_image_mime(&params.mime_type) {
        "Image"
    } else {
        "Asset"
    };
    let title = &params.display_title;
    let mime = &params.mime_type;
    let size = params.size_bytes;
    let uri = &params.uri;
    let lead =
        format!("{kind_word} added: \"{title}\" to {where_} — {mime}, {size} bytes ({uri}).");
    let trimmed = params
        .description
        .as_deref()
        .map(crate::jsstr::js_trim)
        .filter(|s| !s.is_empty());
    match trimmed {
        Some(t) => format!("{lead}\n\nDescription: {t}"),
        None => lead,
    }
}

pub async fn post_librarian_blob_write_announcement(
    db: &Db,
    params: &LibrarianBlobWriteAnnouncement,
) -> Option<Value> {
    let content = build_blob_write_content(params);
    let opaque = build_blob_write_opaque_content(params);
    let kind_label = match &params.origin {
        LibrarianActorOrigin::ByUser => "blob-written-by-user",
        LibrarianActorOrigin::ByCharacter { .. } => "blob-written-by-character",
    };
    post_librarian_message(
        db,
        &params.chat_id,
        &content,
        Some(&opaque),
        kind_label,
        &[],
        None,
        None,
    )
    .await
}

// ---------------------------------------------------------------------------
// Upload announcement (user inline image attachments).
// ---------------------------------------------------------------------------

/// A single uploaded file (v4 `{ fileId, filename }`).
#[derive(Debug, Clone)]
pub struct LibrarianUpload {
    pub file_id: String,
    pub filename: String,
}

/// v4 `LibrarianUploadAnnouncement`.
#[derive(Debug, Clone)]
pub struct LibrarianUploadAnnouncement {
    pub chat_id: String,
    pub uploads: Vec<LibrarianUpload>,
}

pub fn build_upload_content(params: &LibrarianUploadAnnouncement) -> String {
    let uploads = &params.uploads;
    if uploads.is_empty() {
        return String::new();
    }
    if uploads.len() == 1 {
        let u = &uploads[0];
        let (file_id, filename) = (&u.file_id, &u.filename);
        return format!("The Librarian has catalogued the user's freshly-uploaded illustration \"{filename}\" under uuid `{file_id}`. The bytes ride with the user's message above; the image may be filed away later with keep_image, or re-summoned with attach_image.");
    }
    let list = uploads
        .iter()
        .map(|u| format!("- \"{}\" — uuid `{}`", u.filename, u.file_id))
        .collect::<Vec<_>>()
        .join("\n");
    format!("The Librarian has catalogued the user's freshly-uploaded illustrations. The bytes ride with the user's message above; each may be filed away later with keep_image, or re-summoned with attach_image:\n\n{list}")
}

pub fn build_upload_opaque_content(params: &LibrarianUploadAnnouncement) -> String {
    let uploads = &params.uploads;
    if uploads.is_empty() {
        return String::new();
    }
    if uploads.len() == 1 {
        let u = &uploads[0];
        let (file_id, filename) = (&u.file_id, &u.filename);
        return format!("The user has uploaded an illustration: \"{filename}\" — uuid `{file_id}`. The bytes ride with the user's message above; the image may be filed away later with keep_image, or re-summoned with attach_image.");
    }
    let list = uploads
        .iter()
        .map(|u| format!("- \"{}\" — uuid `{}`", u.filename, u.file_id))
        .collect::<Vec<_>>()
        .join("\n");
    format!("The user has uploaded illustrations. The bytes ride with the user's message above; each may be filed away later with keep_image, or re-summoned with attach_image:\n\n{list}")
}

pub async fn post_librarian_upload_announcement(
    db: &Db,
    params: &LibrarianUploadAnnouncement,
) -> Option<Value> {
    if params.uploads.is_empty() {
        return None;
    }
    let content = build_upload_content(params);
    let opaque = build_upload_opaque_content(params);
    // `attachments` stays empty — the user's own message already carries the ids.
    post_librarian_message(
        db,
        &params.chat_id,
        &content,
        Some(&opaque),
        "uploaded",
        &[],
        None,
        None,
    )
    .await
}

// ---------------------------------------------------------------------------
// Summary announcement (conversation-summary whisper).
// ---------------------------------------------------------------------------

/// v4 `LibrarianSummaryAnnouncement`.
#[derive(Debug, Clone)]
pub struct LibrarianSummaryAnnouncement {
    pub chat_id: String,
    pub summary: String,
    pub target_participant_ids: Option<Vec<String>>,
    /// Anchor tying this whisper to the compaction generation it was produced under.
    pub summary_anchor: Option<i64>,
}

pub fn build_summary_content(summary: &str) -> String {
    format!(
        "{SUMMARY_CONTENT_PREFIX}\n\n{}",
        crate::jsstr::js_trim(summary)
    )
}

pub fn build_summary_opaque_content(summary: &str) -> String {
    format!(
        "{SUMMARY_OPAQUE_CONTENT_PREFIX}\n\n{}",
        crate::jsstr::js_trim(summary)
    )
}

pub async fn post_librarian_summary_announcement(
    db: &Db,
    params: &LibrarianSummaryAnnouncement,
) -> Option<Value> {
    if params.summary.is_empty() || crate::jsstr::js_trim(&params.summary).is_empty() {
        return None;
    }
    let content = build_summary_content(&params.summary);
    let opaque = build_summary_opaque_content(&params.summary);
    let targets = params.target_participant_ids.as_deref().unwrap_or(&[]);
    let anchor = params
        .summary_anchor
        .map(|g| json!({ "compactionGeneration": g }));
    post_librarian_message(
        db,
        &params.chat_id,
        &content,
        Some(&opaque),
        "summary",
        &[],
        Some(targets),
        anchor,
    )
    .await
}

// ---------------------------------------------------------------------------
// The shared post primitive.
// ---------------------------------------------------------------------------

/// Build + persist the Librarian `MessageEvent` (v4 `postLibrarianMessage`). Mints
/// `id` + `createdAt`, assembles the exact v4 object literal, validates it into a
/// [`ChatEventInput`], and inserts it via the single-writer path. Returns the
/// posted `MessageEvent` JSON (v4 returns the message on success), or `None` on
/// any failure (v4 catches + logs + returns null; errors never propagate).
#[allow(clippy::too_many_arguments)]
async fn post_librarian_message(
    db: &Db,
    chat_id: &str,
    content: &str,
    opaque_content: Option<&str>,
    kind_label: &str,
    attachments: &[String],
    target_participant_ids: Option<&[String]>,
    summary_anchor: Option<Value>,
) -> Option<Value> {
    let (event, message) = build_librarian_message(
        content,
        opaque_content,
        kind_label,
        attachments,
        target_participant_ids,
        summary_anchor,
    )?;
    let chat_id = chat_id.to_string();
    match db
        .write(move |writers| writers.main().chat_messages().add_message(&chat_id, &event))
        .await
    {
        Ok(()) => Some(message),
        Err(_) => None,
    }
}

/// Assemble the Librarian `MessageEvent` (mint `id` + `createdAt`, build the exact
/// v4 object literal, validate into a [`ChatEventInput`]), returning the event to
/// insert and the message JSON v4 returns on success — or `None` if validation
/// fails. Shared by the async post primitive and the synchronous write-announcement
/// poster so the marshaling lives in exactly one place.
fn build_librarian_message(
    content: &str,
    opaque_content: Option<&str>,
    kind_label: &str,
    attachments: &[String],
    target_participant_ids: Option<&[String]>,
    summary_anchor: Option<Value>,
) -> Option<(ChatEventInput, Value)> {
    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();

    // v4: `targetParticipantIds && targetParticipantIds.length > 0 ? … : null`.
    let targets: Value = match target_participant_ids {
        Some(ids) if !ids.is_empty() => {
            Value::Array(ids.iter().map(|id| Value::String(id.clone())).collect())
        }
        _ => Value::Null,
    };

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": content,
        "opaqueContent": opaque_content,
        "attachments": attachments,
        "createdAt": now,
        "participantId": Value::Null,
        "systemSender": "librarian",
        "systemKind": kind_label,
        "targetParticipantIds": targets,
        "summaryAnchor": summary_anchor.unwrap_or(Value::Null),
    });

    let event: ChatEventInput = serde_json::from_value(message.clone()).ok()?;
    Some((event, message))
}
