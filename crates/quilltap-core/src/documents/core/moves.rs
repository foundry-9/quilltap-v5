//! Renaming or moving a document within its store, the rename-target math,
//! and the one `chat_documents` sweep that keeps open documents pointing at
//! the new path — v4 `lib/documents/core/move.ts`.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::db::database_store::{self, DbStoreErrorCode, StoreError};
use crate::doc_edit::path_resolver::{move_resolved_file, ResolvedPath};
use crate::documents::{node_basename, node_dirname, node_extname, DocError};
use crate::inventory::lint_scope::TitleLintScope;

use super::read::stat_document;
use super::write::{after_document_write, Refresh};

/// v4 `RenameDocumentResult` — `{ok: true}` or one of three refusals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameOutcome {
    Moved,
    SourceMissing,
    SourceNotAFile,
    DestinationTaken,
}

/// v4 `isCaseOnlyRename` — two paths that differ only in letter case: a legal
/// rename that a case-insensitive existence probe would report as a conflict
/// with itself. (JS `toLowerCase` on both — `str::to_lowercase` is
/// byte-identical, Phase 1.)
fn is_case_only_rename(from: &str, to: &str) -> bool {
    from != to && from.to_lowercase() == to.to_lowercase()
}

/// v4 `renameDocument` — move a document from one resolved path to another in
/// the same store. A database store moves the link row
/// (`moveDatabaseDocument`); a filesystem path is renamed on disk and then
/// refreshed ([`after_document_write`] on `to`). Never overwrites.
/// `DatabaseStoreError` UNSUPPORTED (a target extension the database store
/// cannot hold) bubbles as [`DocError::Unsupported`]. **No refresh on the
/// database branch** (v4 `move.ts:69-71` calls `afterDocumentWrite` only on
/// disk) — `moveDatabaseDocument` re-chunks the moved link itself.
pub fn rename_document(
    main: &Connection,
    mount: &Connection,
    from: &ResolvedPath,
    to: &ResolvedPath,
    refresh: Refresh<'_>,
) -> Result<RenameOutcome, DocError> {
    match stat_document(mount, from)? {
        None => return Ok(RenameOutcome::SourceMissing),
        Some(super::DocKind::Folder) => return Ok(RenameOutcome::SourceNotAFile),
        Some(super::DocKind::File) => {}
    }

    if let (Some("database"), Some(mp)) =
        (from.mount_type.as_deref(), from.mount_point_id.as_deref())
    {
        if !mp.is_empty() {
            if !is_case_only_rename(&from.relative_path, &to.relative_path) {
                // v4 `statDocument({...to, mountPointId: from.mountPointId})` —
                // ANY hit, a folder too, is taken.
                let mut probe = to.clone();
                probe.mount_point_id = Some(mp.to_string());
                if stat_document(mount, &probe)?.is_some() {
                    return Ok(RenameOutcome::DestinationTaken);
                }
            }
            match database_store::move_database_document(
                mount,
                mp,
                &from.relative_path,
                &to.relative_path,
                TitleLintScope::Unlinted { main },
            ) {
                Ok(()) => {}
                Err(StoreError::Store(s)) if s.code == DbStoreErrorCode::Conflict => {
                    return Ok(RenameOutcome::DestinationTaken)
                }
                Err(StoreError::Store(s)) if s.code == DbStoreErrorCode::Unsupported => {
                    return Err(DocError::Unsupported(s.message))
                }
                Err(e) => {
                    return Err(DocError::Store(
                        crate::tools::doc_edit::shared::store_error_message(e),
                    ))
                }
            }
            tracing::debug!(
                target: "quilltap::documents",
                mountPointId = %mp,
                from = %from.relative_path,
                to = %to.relative_path,
                "renameDocument: moved database document"
            );
            return Ok(RenameOutcome::Moved);
        }
    }

    if !is_case_only_rename(&from.absolute_path, &to.absolute_path)
        && stat_document(mount, to)?.is_some()
    {
        return Ok(RenameOutcome::DestinationTaken);
    }
    move_resolved_file(from, to).map_err(|e| {
        DocError::Store(super::node_fs_error_message(
            &e,
            "rename",
            &from.absolute_path,
            Some(&to.absolute_path),
        ))
    })?;
    tracing::debug!(
        target: "quilltap::documents",
        from = %from.relative_path,
        to = %to.relative_path,
        "renameDocument: moved file"
    );
    after_document_write(refresh, to);
    Ok(RenameOutcome::Moved)
}

/// The rename-target computation result (v4 `RenameTarget`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameTarget {
    Ok {
        new_file_path: String,
        new_display_title: String,
    },
    Err {
        reason: String,
    },
}

/// v4 `computeRenameTarget` — turn a user-typed title into the rename target
/// path: the input is the new basename (directory preserved); a missing
/// extension inherits the old one ("backstory" → "backstory.md"). Path
/// separators / `.` / `..` are rejected — a rename within the directory, not a
/// move. **Pure.**
pub fn compute_rename_target(current_file_path: &str, new_title: &str) -> RenameTarget {
    let raw = crate::jsstr::js_trim(new_title);
    if raw.is_empty() {
        return RenameTarget::Err {
            reason: "Name cannot be empty".to_string(),
        };
    }
    if raw.contains('/') || raw.contains('\\') {
        return RenameTarget::Err {
            reason: "Name cannot contain path separators".to_string(),
        };
    }
    if raw == "." || raw == ".." || raw.split(['\\', '/']).any(|s| s == "..") {
        return RenameTarget::Err {
            reason: "Invalid name".to_string(),
        };
    }

    let old_ext = node_extname(current_file_path);
    let old_dir = node_dirname(current_file_path);
    let new_basename = if node_extname(raw).is_empty() {
        format!("{raw}{old_ext}")
    } else {
        raw.to_string()
    };
    let new_file_path = if old_dir == "." || old_dir.is_empty() {
        new_basename
    } else {
        format!("{}/{}", old_dir.replace('\\', "/"), new_basename)
    };
    let new_display_title = node_basename(&new_file_path).to_string();
    RenameTarget::Ok {
        new_file_path,
        new_display_title,
    }
}

// ===========================================================================
// The chat_documents sweeps (v4 `30a3b4cb7`, `move.ts:108-290`)
// ===========================================================================

/// Where a move landed, as the sweep needs it (v4 `SweepStore`).
#[derive(Debug, Clone, Copy)]
pub struct SweepStore<'a> {
    /// The scope the caller addressed the file through.
    pub scope: &'a str,
    /// The store name the caller addressed — the resolved store's name, never a
    /// caller's `self` (bug 256). `None` is v4's `undefined` (the key is left
    /// out of the lines); `Some(None)` is an explicit `null` (a Document Mode
    /// row's `mountPoint`).
    pub mount_point: Option<Option<&'a str>>,
    /// The resolved store's id. When given, a row recorded under a different
    /// address for the same store — `project` for the project's official
    /// store, or that store by name — is swept too. Without it only rows under
    /// the caller's exact `(scope, mountPoint)` are.
    pub mount_point_id: Option<&'a str>,
}

impl SweepStore<'_> {
    fn mount_point_value(&self) -> Option<&str> {
        self.mount_point.flatten()
    }
    /// The `mountPoint` field as v4's line carries it: omitted for `undefined`,
    /// `null` for `null`.
    fn mount_point_field(&self) -> Option<&str> {
        self.mount_point.map(|m| m.unwrap_or("null"))
    }
}

/// The matcher's two lookups (v4's `repos.docMountPoints.findByName` and the
/// `chats.findById` → `projects.findById` hop), behind a seam so a unit test
/// can count them (P4.D380 item 20 — one lookup per cache key).
pub trait SweepLookups {
    /// Every enabled store whose trimmed, lower-cased name matches — their ids.
    fn store_ids_for_name(&mut self, name: &str) -> Vec<String>;
    /// A chat's project's official store id.
    fn official_store_for_chat(&mut self, chat_id: &str) -> Option<String>;
}

/// The production lookups — v4's repository reads, each its fallback.
pub struct DbSweepLookups<'a> {
    pub main: &'a Connection,
    pub mount: &'a Connection,
}

impl SweepLookups for DbSweepLookups<'_> {
    fn store_ids_for_name(&mut self, name: &str) -> Vec<String> {
        // v4 `findByName` → `findEnabled` → `findByFilter` (the inner fallback
        // answers a failure first).
        let repo = crate::db::doc_mount_points::DocMountPointsRepository::new(self.mount);
        crate::db::fallback::find_by_filter_or_empty("doc_mount_points", || repo.find_by_name(name))
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }
    fn official_store_for_chat(&mut self, chat_id: &str) -> Option<String> {
        // "read project / group owners raw" (round ruling, §R.11).
        let chat = crate::db::chats_read::find_by_id_or_none(self.main, chat_id)?;
        let project_id = chat.get("projectId").and_then(|v| v.as_str())?.to_string();
        if project_id.is_empty() {
            return None;
        }
        crate::db::fallback::find_by_id_or_none("projects", &project_id, || {
            crate::db::projects::find_official_mount_point_id_raw(self.main, &project_id)
        })
        .flatten()
    }
}

/// v4 `sameStoreMatcher` — decide, row by row, whether a `chat_documents` row
/// addresses the store a move happened in. "With the store's id in hand, a
/// row's own address is resolved to a store id and compared: a
/// `document_store` row by its store name, a `project` row through its chat's
/// project's official store — so a `project` row from another project's chat is
/// not swept just because it too says `project`. `general` is a filesystem
/// directory with no store id and matches literally, as does every row when
/// the caller has no store id. Lookups are cached for one sweep." (v4's
/// comment.) The caches are keyed as v4 keys them: the name's
/// `trim().toLowerCase()`, and the chat id.
struct SameStoreMatcher<'s, L: SweepLookups> {
    store: SweepStore<'s>,
    lookups: L,
    by_name: HashMap<String, Option<String>>,
    by_chat: HashMap<String, Option<String>>,
}

impl<'s, L: SweepLookups> SameStoreMatcher<'s, L> {
    fn new(store: SweepStore<'s>, lookups: L) -> Self {
        Self {
            store,
            lookups,
            by_name: HashMap::new(),
            by_chat: HashMap::new(),
        }
    }

    fn store_id_for_name(&mut self, name: &str) -> Option<String> {
        let key = crate::jsstr::js_trim(name).to_lowercase();
        if let Some(hit) = self.by_name.get(&key) {
            return hit.clone();
        }
        let ids = self.lookups.store_ids_for_name(name);
        let id = if ids.len() == 1 {
            Some(ids[0].clone())
        } else {
            None
        };
        self.by_name.insert(key, id.clone());
        id
    }

    fn store_id_for_chat_project(&mut self, chat_id: &str) -> Option<String> {
        if let Some(hit) = self.by_chat.get(chat_id) {
            return hit.clone();
        }
        let id = self.lookups.official_store_for_chat(chat_id);
        self.by_chat.insert(chat_id.to_string(), id.clone());
        id
    }

    fn matches(&mut self, row: &crate::db::chat_documents::ChatDocumentFull) -> bool {
        let literal = row.scope == self.store.scope
            && row.mount_point.as_deref() == self.store.mount_point_value();
        let Some(store_id) = self.store.mount_point_id.filter(|s| !s.is_empty()) else {
            return literal;
        };
        if row.scope == "general" {
            return literal;
        }
        if row.scope == "document_store" {
            if let Some(name) = row.mount_point.as_deref().filter(|n| !n.is_empty()) {
                if literal || name == store_id {
                    return true;
                }
                return self.store_id_for_name(name).as_deref() == Some(store_id);
            }
        }
        if row.scope == "project" {
            return self.store_id_for_chat_project(&row.chat_id).as_deref() == Some(store_id);
        }
        false
    }
}

/// Node `path.basename` for the sweep's new display title.
fn basename_of(p: &str) -> String {
    node_basename(p).to_string()
}

/// v4 `sweepChatDocumentsAfterMove` — keep every `chat_documents` row still
/// pointing at the old path (any chat's, or the standalone sentinel's) in step
/// with a move, so an open document and the recent-documents list follow it
/// instead of 404ing on the next read. Rows are matched by the store they
/// reach, not the words they were recorded with. Best-effort: the move already
/// landed, so a tracking failure is logged and never returned. Answers how
/// many rows moved. `chat_id` is the caller's `logFields: {chatId}` (spread
/// FIRST in every line).
#[allow(clippy::too_many_arguments)]
pub fn sweep_chat_documents_after_move(
    main: &Connection,
    mount: &Connection,
    store: SweepStore<'_>,
    old_file_path: &str,
    new_file_path: &str,
    new_display_title: Option<&str>,
    chat_id: Option<&str>,
) -> usize {
    sweep_chat_documents_after_move_with(
        main,
        DbSweepLookups { main, mount },
        store,
        old_file_path,
        new_file_path,
        new_display_title,
        chat_id,
    )
}

/// [`sweep_chat_documents_after_move`] over an explicit lookup seam.
#[allow(clippy::too_many_arguments)]
pub fn sweep_chat_documents_after_move_with<L: SweepLookups>(
    main: &Connection,
    lookups: L,
    store: SweepStore<'_>,
    old_file_path: &str,
    new_file_path: &str,
    new_display_title: Option<&str>,
    chat_id: Option<&str>,
) -> usize {
    let result: Result<usize, crate::db::DbError> = (|| {
        let repo = crate::db::chat_documents::ChatDocumentsRepository::new(main);
        let mut matcher = SameStoreMatcher::new(store, lookups);
        let title = new_display_title
            .map(str::to_string)
            .unwrap_or_else(|| basename_of(new_file_path));
        let mut rows_updated = 0usize;
        for row in repo.find_by_file_path(old_file_path) {
            if !matcher.matches(&row) {
                continue;
            }
            let patch = crate::db::chat_documents::CdUpdate {
                file_path: Some(new_file_path.to_string()),
                display_title: Some(title.clone()),
                updated_at: crate::clock::now_iso(),
                ..Default::default()
            };
            if repo.update(&row.id, &patch)? {
                rows_updated += 1;
            }
        }
        Ok(rows_updated)
    })();
    match result {
        Ok(rows_updated) if rows_updated > 0 => {
            tracing::info!(
                target: "quilltap::documents",
                chatId = chat_id,
                scope = store.scope,
                mountPoint = store.mount_point_field(),
                mountPointId = store.mount_point_id.unwrap_or("null"),
                from = old_file_path,
                to = new_file_path,
                rowsUpdated = rows_updated as u64,
                "Swept chat_documents after a document move"
            );
            rows_updated
        }
        Ok(_) => {
            tracing::debug!(
                target: "quilltap::documents",
                chatId = chat_id,
                scope = store.scope,
                mountPoint = store.mount_point_field(),
                from = old_file_path,
                "No chat_documents rows to sweep after a document move"
            );
            0
        }
        Err(e) => {
            tracing::warn!(
                target: "quilltap::documents",
                chatId = chat_id,
                scope = store.scope,
                mountPoint = store.mount_point_field(),
                from = old_file_path,
                to = new_file_path,
                error = %crate::db::fallback::error_text(&e),
                "Failed to sweep chat_documents after a document move"
            );
            0
        }
    }
}

/// v4 `sweepChatDocumentsAfterFolderMove` — the folder counterpart: rewrites
/// the `oldFolderPath/` prefix on every row under it that reaches the same
/// store, titling each by its new basename. Best-effort; answers how many rows
/// moved. **No DEBUG arm** (v4's folder twin logs only a positive sweep).
pub fn sweep_chat_documents_after_folder_move(
    main: &Connection,
    mount: &Connection,
    store: SweepStore<'_>,
    old_folder_path: &str,
    new_folder_path: &str,
    chat_id: Option<&str>,
) -> usize {
    sweep_chat_documents_after_folder_move_with(
        main,
        DbSweepLookups { main, mount },
        store,
        old_folder_path,
        new_folder_path,
        chat_id,
    )
}

/// [`sweep_chat_documents_after_folder_move`] over an explicit lookup seam.
pub fn sweep_chat_documents_after_folder_move_with<L: SweepLookups>(
    main: &Connection,
    lookups: L,
    store: SweepStore<'_>,
    old_folder_path: &str,
    new_folder_path: &str,
    chat_id: Option<&str>,
) -> usize {
    let result: Result<usize, crate::db::DbError> = (|| {
        let repo = crate::db::chat_documents::ChatDocumentsRepository::new(main);
        let mut matcher = SameStoreMatcher::new(store, lookups);
        let old_prefix = if old_folder_path.ends_with('/') {
            old_folder_path.to_string()
        } else {
            format!("{old_folder_path}/")
        };
        let new_prefix = if new_folder_path.ends_with('/') {
            new_folder_path.to_string()
        } else {
            format!("{new_folder_path}/")
        };
        let mut rows_updated = 0usize;
        for row in repo.find_under_folder(old_folder_path) {
            if !matcher.matches(&row) {
                continue;
            }
            // JS `slice(oldPrefix.length)` — UTF-16 units; the prefix is a
            // byte-prefix of the path (startsWith), so the byte slice agrees.
            let new_file_path = format!("{new_prefix}{}", &row.file_path[old_prefix.len()..]);
            let patch = crate::db::chat_documents::CdUpdate {
                display_title: Some(basename_of(&new_file_path)),
                file_path: Some(new_file_path),
                updated_at: crate::clock::now_iso(),
                ..Default::default()
            };
            if repo.update(&row.id, &patch)? {
                rows_updated += 1;
            }
        }
        Ok(rows_updated)
    })();
    match result {
        Ok(rows_updated) => {
            if rows_updated > 0 {
                tracing::info!(
                    target: "quilltap::documents",
                    chatId = chat_id,
                    scope = store.scope,
                    mountPoint = store.mount_point_field(),
                    mountPointId = store.mount_point_id.unwrap_or("null"),
                    from = old_folder_path,
                    to = new_folder_path,
                    rowsUpdated = rows_updated as u64,
                    "Swept chat_documents after a folder move"
                );
            }
            rows_updated
        }
        Err(e) => {
            tracing::warn!(
                target: "quilltap::documents",
                chatId = chat_id,
                scope = store.scope,
                mountPoint = store.mount_point_field(),
                from = old_folder_path,
                to = new_folder_path,
                error = %crate::db::fallback::error_text(&e),
                "Failed to sweep chat_documents after a folder move"
            );
            0
        }
    }
}
