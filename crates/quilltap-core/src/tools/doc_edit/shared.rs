//! Shared context, addressing, and access-control helpers for the doc-edit tool
//! handlers — v4 `lib/tools/handlers/doc-edit/shared.ts`.
//!
//! These are used across every handler group (text, markdown, file management,
//! document UI, blob, photo). All are **synchronous** and take the two writer
//! connections (`main` + `mount`) directly: the doc-edit dispatcher runs each
//! handler inside a single [`crate::db::runtime::Db::write`] closure (the wardrobe
//! precedent), so a handler both reads (`chats`/`characters` on main, the store on
//! mount) and writes (the store on mount) on the same RW connections.
//!
//! ## Seamed side effects (documented no-ops, matching the oracle mocks)
//!
//! v4's handlers post Librarian announcements (`postLibrarian*` +
//! `resolveActorOrigin` + `contentHiddenFromCharacters`) and fire a background
//! re-index (`triggerReindexIfNeeded`). Both are fire-and-forget side effects that
//! never appear in the tool result, so — like the wave-3 whisper-posting seams and
//! the standing reindex deferral — the port omits them. Since P4.32 (the ruled
//! reindex un-mock, 2026-08-04) the differential oracle mocks the librarian
//! writers and `triggerReindexIfNeeded` ONLY — `@/lib/doc-edit/reindex-file`
//! runs REAL, so v4's own chunk-on-write executes in the oracle and the
//! `chunkCount` it bumps is positively asserted (equal AND nonzero on pinned
//! written paths) by the `doc_text`/`doc_fm` differentials, not excluded.
//!
//! **v4 `40319484` chained `reindexLinkGroupSiblings` onto that same
//! `triggerReindexIfNeeded` promise** (`shared.ts:474`), so it inherits the seam.
//! Where v4 actually gets the effect for a DATABASE-backed doc-edit write, v5
//! gets it too: this file's write dispatch lands bytes through
//! `database_store::write_database_document`, which runs the chunk pass AND the
//! group pass itself (v4 calls it from both places; the pass is idempotent).
//! **Loudly deferred:** for a FILESYSTEM/obsidian doc-edit write the seam is the
//! only path in v4, and v5 has no reindex there at all — so a hard-linked
//! filesystem sibling's chunk set is not rebuilt by a doc-edit tool write. That
//! is the standing reindex deferral's blast radius growing by one case, not a new
//! seam; it closes when the doc-edit reindex is ported.
//!
//! ## The host-filesystem branches (P4.6bg)
//!
//! The legacy filesystem/obsidian mount branches (`mountType != 'database'`), the
//! `general` scope, and the legacy project fallback reach the host disk. They run
//! whenever the resolver produced a real `absolute_path` — i.e. the host supplied a
//! `files_dir` (v4 `getFilesDir()`), threaded through
//! [`DocEditToolContext::files_dir`]. With `files_dir: None` the resolver returns
//! [`crate::doc_edit::path_resolver::ResolveError::FsSeam`] before any tool touches
//! disk, preserving the historic refusal. The read/write dispatch
//! ([`read_file_with_mtime`] / [`write_file_with_mtime_check`]) and the
//! enumeration walks (`text.rs`) branch on `mount_type` accordingly.

use std::collections::HashSet;

use rusqlite::Connection;
use serde_json::Value;

use crate::db::database_store::{self, DbStoreErrorCode, ReadDoc, StoreError};
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::tiered_mount_pool::{
    flatten_tier_pool, resolve_tiered_mount_pool, FlattenScope, TierContext, TierResolveOptions,
};
use crate::db::{characters_read, chats_read, projects};
use crate::doc_edit::path_resolver::{PathResolutionContext, ResolveError, ResolvedPath};
use crate::doc_edit::qtap_uri::parse_qtap_uri;
use crate::doc_edit::DocEditScope;
use crate::services::librarian_notifications::{LibrarianActorOrigin, LibrarianScope};
use crate::tools::llm_number::llm_number;

/// Context required for doc-edit tool execution — v4 `DocEditToolContext`.
#[derive(Clone, Debug)]
pub struct DocEditToolContext {
    pub chat_id: String,
    pub user_id: String,
    pub project_id: Option<String>,
    pub character_id: Option<String>,
    /// Operator "look everywhere" override (Document-Mode / Brahma Console
    /// surfaces only — never a character tool call). Passed straight to the path
    /// resolver's `operator_override`.
    pub operator_override: bool,
    /// The host files directory (v4 `getFilesDir()` = `<base>/files`). `Some`
    /// makes the `general`/legacy-project/fs-mount branches reach the host disk;
    /// `None` preserves the historic FsSeam refusal (P4.6bg S2). Threaded straight
    /// into [`crate::doc_edit::path_resolver::resolve_doc_edit_path`].
    pub files_dir: Option<std::path::PathBuf>,
}

/// Map a resolved [`DocEditScope`] to the announcement's [`LibrarianScope`]. v4's
/// write handlers pass the raw scope string cast to
/// `'project' | 'document_store' | 'general'`; the port collapses `group` to
/// `document_store` upstream ([`scope_from_str`]), matching the doc-save corpus
/// (group-store writes never reach a `change:{diff}` announcement here).
pub fn librarian_scope_from(scope: DocEditScope) -> LibrarianScope {
    match scope {
        DocEditScope::Project => LibrarianScope::Project,
        DocEditScope::General => LibrarianScope::General,
        DocEditScope::DocumentStore => LibrarianScope::DocumentStore,
    }
}

/// v4 `resolveActorOrigin` (`shared.ts:44`): who effected a doc write, so the
/// Librarian announcement can name them. Prefers the acting character's name; with
/// no `characterId` (operator / Document-Mode surfaces) or a failed lookup, falls
/// back to user attribution. `name` is a slim column (not vault-overlaid), so the
/// raw read matches v4's overlaid `findById().name`.
pub fn resolve_actor_origin(main: &Connection, ctx: &DocEditToolContext) -> LibrarianActorOrigin {
    let Some(cid) = ctx.character_id.as_deref() else {
        return LibrarianActorOrigin::ByUser;
    };
    if let Ok(Some(character)) = characters_read::find_by_id_raw(main, cid) {
        if let Some(name) = character.get("name").and_then(Value::as_str) {
            if !name.is_empty() {
                return LibrarianActorOrigin::ByCharacter {
                    character_name: name.to_string(),
                };
            }
        }
    }
    LibrarianActorOrigin::ByUser
}

/// Synchronous sibling of
/// [`crate::services::librarian_notifications::document_hidden_from_characters`]:
/// true when the indexed document at `(mount_point_id, relative_path)` is
/// `character_read:false`, so a Librarian announcement naming it must be suppressed
/// (the operator-override path can reach a hidden doc; the read gate blocks
/// characters). False for missing ids/paths, a missing link row, or any lookup
/// error — an announcement is never blocked by a transient fault. Takes the `mount`
/// connection directly (the handlers run inside the write closure).
pub fn document_hidden_from_characters(
    mount: &Connection,
    mount_point_id: Option<&str>,
    relative_path: &str,
) -> bool {
    let Some(mp) = mount_point_id else {
        return false;
    };
    if mp.is_empty() || relative_path.is_empty() {
        return false;
    }
    match DocMountFileLinksRepository::new(mount).find_by_mount_point_and_path(mp, relative_path) {
        Ok(Some(link)) => !link.allow_character_read,
        _ => false,
    }
}

/// Map a raw `scope` string to [`DocEditScope`] (v4 casts the string; an
/// unrecognized value falls back to `document_store`, matching v4's default).
pub fn scope_from_str(scope: Option<&str>, default: DocEditScope) -> DocEditScope {
    match scope {
        Some("project") => DocEditScope::Project,
        Some("general") => DocEditScope::General,
        Some("document_store") | Some("group") => DocEditScope::DocumentStore,
        Some(_) => DocEditScope::DocumentStore,
        None => default,
    }
}

/// The addressing fields a doc tool resolves before touching a store, after a
/// `qtap://` URI (if any) has been projected onto `scope` / `mount_point` /
/// `path` — v4's `QtapAddressableInput` post-`applyQtapUriToInput`.
#[derive(Clone, Debug, Default)]
pub struct Addressing {
    pub scope: Option<String>,
    pub mount_point: Option<String>,
    pub path: Option<String>,
    /// Heading text carried by the URI fragment (heading tools apply it).
    pub uri_heading: Option<String>,
    /// Heading level carried by the URI fragment.
    pub uri_level: Option<u8>,
}

/// v4 `applyQtapUriToInput`: when a `uri` is present, parse it and project its
/// parts onto `scope`/`mount_point`/`path` (the URI wins over any stale
/// scope/mount_point/path), surfacing the fragment heading/level for heading
/// tools. Idempotent — no `uri` returns the inputs unchanged. A malformed URI
/// returns the parse error message (surfaces as a clean tool error).
pub fn apply_qtap_uri(
    scope: Option<String>,
    mount_point: Option<String>,
    path: Option<String>,
    uri: Option<&str>,
) -> Result<Addressing, String> {
    match uri {
        None => Ok(Addressing {
            scope,
            mount_point,
            path,
            uri_heading: None,
            uri_level: None,
        }),
        Some(uri) => {
            let parts = parse_qtap_uri(uri).map_err(|e| e.message)?;
            Ok(Addressing {
                scope: Some(parts.scope.as_str().to_string()),
                mount_point: parts.mount_point,
                path: Some(parts.path),
                uri_heading: parts.heading,
                uri_level: parts.level,
            })
        }
    }
}

/// v4 `isTextFile` (`path-resolver.ts:797`): a fixed allow-list of text
/// extensions, matched case-insensitively against `path.extname` (which is `''`
/// for a dot-leading basename with no other dot).
pub fn is_text_file(file_path: &str) -> bool {
    const ALLOWED: &[&str] = &[
        ".md",
        ".markdown",
        ".txt",
        ".json",
        ".jsonl",
        ".ndjson",
        ".yaml",
        ".yml",
        ".xml",
        ".html",
        ".htm",
        ".css",
        ".js",
        ".ts",
        ".jsx",
        ".tsx",
        ".py",
        ".rb",
        ".sh",
        ".bash",
        ".zsh",
        ".csv",
        ".toml",
        ".ini",
        ".cfg",
        ".conf",
        ".log",
        ".env",
        ".gitignore",
        ".editorconfig",
    ];
    let ext = extname(file_path).to_ascii_lowercase();
    ALLOWED.contains(&ext.as_str())
}

/// Node `path.extname` for a POSIX-ish path: the substring from the last `.` in
/// the final path segment, or `""` when the segment has no dot or is a dotfile
/// (the only dot at index 0).
pub fn extname(p: &str) -> String {
    let base = p.rsplit('/').next().unwrap_or(p);
    match base.rfind('.') {
        None | Some(0) => String::new(),
        Some(i) => base[i..].to_string(),
    }
}

/// Node `path.basename` for a POSIX path (used for display titles).
pub fn basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// Flatten a [`ResolveError`] into the message v4's caught `PathResolutionError`
/// surfaces (`err.message`). `FsSeam` never occurs on the database-backed corpus.
pub fn resolve_error_message(e: ResolveError) -> String {
    match e {
        ResolveError::Path { message, .. } => message,
        ResolveError::FsSeam => {
            "This location is served from the host filesystem, which is unavailable here."
                .to_string()
        }
    }
}

// ============================================================================
// Cross-character vault visibility (Shared Vaults + systemTransparency)
// ============================================================================

/// v4 `collectPeerCharacterIdsForReads`: the character ids of other present
/// participants whose vaults are readable — only when the chat has
/// `allowCrossCharacterVaultReads`. Empty when the flag is off, the chat is
/// missing, or the acting character is the only present participant. DB errors
/// are swallowed to `[]` (v4's `safeQuery` returns null → no peers).
pub fn collect_peer_character_ids_for_reads(
    main: &Connection,
    ctx: &DocEditToolContext,
) -> Vec<String> {
    if ctx.chat_id.is_empty() {
        return Vec::new();
    }
    let Ok(Some(chat)) = chats_read::find_by_id(main, &ctx.chat_id) else {
        return Vec::new();
    };
    if chat
        .get("allowCrossCharacterVaultReads")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Vec::new();
    }
    let acting = ctx.character_id.as_deref();
    let mut peers: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    if let Some(participants) = chat.get("participants").and_then(Value::as_array) {
        for p in participants {
            let Some(cid) = p.get("characterId").and_then(Value::as_str) else {
                continue;
            };
            if Some(cid) == acting {
                continue;
            }
            let status = p.get("status").and_then(Value::as_str).unwrap_or("");
            if !is_participant_present(status) {
                continue;
            }
            if seen.insert(cid.to_string()) {
                peers.push(cid.to_string());
            }
        }
    }
    peers
}

/// v4 `isParticipantPresent` (`chat.types.ts:444`): present == `active` | `silent`.
fn is_participant_present(status: &str) -> bool {
    status == "active" || status == "silent"
}

/// v4 `actingCharacterIsOpaqueToVaults`: a character with `systemTransparency !==
/// true` accepts the covenant of trust — every character vault (own + peers') is
/// hidden from doc_* tools. Defaults to **opaque** when there is no character or
/// the lookup fails, so a transient error never grants access.
pub fn acting_character_is_opaque_to_vaults(main: &Connection, ctx: &DocEditToolContext) -> bool {
    let Some(cid) = ctx.character_id.as_deref() else {
        return false;
    };
    match characters_read::find_by_id_raw(main, cid) {
        Ok(Some(character)) => {
            character.get("systemTransparency").and_then(Value::as_bool) != Some(true)
        }
        // Lookup failed or missing → opaque (fail closed).
        _ => true,
    }
}

/// v4 `assertWriteDoesNotTargetPeerVault`: when cross-character reads are enabled,
/// a write whose `mount_point` names a peer participant's vault (by id or name)
/// raises the dedicated read-only error before resolution runs. `Err(message)` is
/// the thrown `PathResolutionError` message.
pub fn assert_write_does_not_target_peer_vault(
    main: &Connection,
    mount: &Connection,
    mount_point_hint: Option<&str>,
    peer_character_ids: &[String],
) -> Result<(), String> {
    let Some(hint) = mount_point_hint else {
        return Ok(());
    };
    if peer_character_ids.is_empty() {
        return Ok(());
    }
    let needle = hint.to_lowercase();
    for peer_id in peer_character_ids {
        let Ok(Some(peer)) = characters_read::find_by_id_raw(main, peer_id) else {
            continue;
        };
        let Some(mp_id) = peer
            .get("characterDocumentMountPointId")
            .and_then(Value::as_str)
        else {
            continue;
        };
        let peer_name = peer.get("name").and_then(Value::as_str).unwrap_or("");
        if mp_id == hint {
            return Err(format!(
                "{peer_name}'s vault is read-only in this chat. Cross-character vault sharing permits reads only."
            ));
        }
        if let Ok(Some(mp)) = DocMountPointsRepository::new(mount).find_by_id_for_docedit(mp_id) {
            if mp.name.to_lowercase() == needle {
                return Err(format!(
                    "{peer_name}'s vault is read-only in this chat. Cross-character vault sharing permits reads only."
                ));
            }
        }
    }
    Ok(())
}

// ============================================================================
// Per-document policy gates (character_read / character_write)
//
// These enforce the `allowCharacterRead` / `allowCharacterWrite` flags on the
// link row. They govern CHARACTERS only — the operator (`operator_override`) is
// never restricted, so every gate is a no-op then. Existence-leak rule: a
// `character_read:false` file yields the SAME not-found error as a missing file.
// A DB read failure is treated as "no link" (gate passes), matching v4's
// `safeQuery` null.
// ============================================================================

/// v4 `assertCharacterMayRead`: not-found-style error when the acting character
/// may not read the resolved document (`allowCharacterRead == false`). No-op for
/// the operator, for non-mount scopes, and for paths with no link row yet.
pub fn assert_character_may_read(
    mount: &Connection,
    resolved: &ResolvedPath,
    ctx: &DocEditToolContext,
) -> Result<(), String> {
    if ctx.operator_override {
        return Ok(());
    }
    let Some(mp_id) = resolved.mount_point_id.as_deref() else {
        return Ok(());
    };
    if let Ok(Some(link)) = DocMountFileLinksRepository::new(mount)
        .find_by_mount_point_and_path(mp_id, &resolved.relative_path)
    {
        if !link.allow_character_read {
            // Mirror the "missing file" shape so existence isn't leaked.
            return Err(format!("File not found: {}", resolved.relative_path));
        }
    }
    Ok(())
}

/// v4 `assertCharacterMayWrite`: checks the read gate first (a
/// `character_read:false` file reports not-found — don't leak existence), then
/// the write gate. No-op for the operator and non-mount scopes; a path with no
/// link row (a brand-new file) is allowed.
pub fn assert_character_may_write(
    mount: &Connection,
    resolved: &ResolvedPath,
    ctx: &DocEditToolContext,
) -> Result<(), String> {
    if ctx.operator_override {
        return Ok(());
    }
    let Some(mp_id) = resolved.mount_point_id.as_deref() else {
        return Ok(());
    };
    let Ok(Some(link)) = DocMountFileLinksRepository::new(mount)
        .find_by_mount_point_and_path(mp_id, &resolved.relative_path)
    else {
        return Ok(()); // creating a new file — no policy row to honour yet
    };
    if !link.allow_character_read {
        return Err(format!("File not found: {}", resolved.relative_path));
    }
    if !link.allow_character_write {
        return Err(format!(
            "This document is read-only to characters: {}",
            resolved.relative_path
        ));
    }
    Ok(())
}

/// v4 `getCharacterBlockedReadPaths`: the set of lowercased `relativePath`s in a
/// mount hidden from characters by `allowCharacterRead == false`. Empty for the
/// operator or when nothing is blocked. Used by doc_list_files / doc_grep.
pub fn get_character_blocked_read_paths(
    mount: &Connection,
    mount_point_id: &str,
    ctx: &DocEditToolContext,
) -> HashSet<String> {
    let mut blocked = HashSet::new();
    if ctx.operator_override {
        return blocked;
    }
    if let Ok(links) =
        DocMountFileLinksRepository::new(mount).find_by_mount_point_id(mount_point_id)
    {
        for link in links {
            if !link.allow_character_read {
                blocked.insert(link.relative_path.to_lowercase());
            }
        }
    }
    blocked
}

/// v4 `assertFolderHasNoWriteProtectedDescendants`: fail a folder delete/move when
/// any document under `folderRelativePath` is `character_write:false` (or
/// `character_read:false`), naming the protected document — a character can't
/// relocate/delete a protected file by operating on its parent. No-op for the
/// operator and non-mount scopes.
pub fn assert_folder_has_no_write_protected_descendants(
    mount: &Connection,
    resolved: &ResolvedPath,
    ctx: &DocEditToolContext,
) -> Result<(), String> {
    if ctx.operator_override {
        return Ok(());
    }
    let Some(mp_id) = resolved.mount_point_id.as_deref() else {
        return Ok(());
    };
    let Ok(links) = DocMountFileLinksRepository::new(mount).find_by_mount_point_id(mp_id) else {
        return Ok(());
    };
    // POSIX prefix with a trailing slash so "Notes" doesn't match "Notes-x/y.md".
    let folder = resolved
        .relative_path
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    let prefix = if folder.is_empty() {
        String::new()
    } else {
        format!("{}/", folder.to_lowercase())
    };
    for link in links {
        let rel = link.relative_path.to_lowercase();
        let in_folder = prefix.is_empty() || rel.starts_with(&prefix);
        if !in_folder {
            continue;
        }
        if !link.allow_character_write || !link.allow_character_read {
            return Err(format!(
                "This folder contains a protected document ({}) that characters may not move or delete. The operation was cancelled.",
                link.relative_path
            ));
        }
    }
    Ok(())
}

// ============================================================================
// Resolution-context builders
// ============================================================================

/// v4 `buildReadResolutionContext`: when the chat has cross-character reads
/// enabled, the vaults of other present participants join `characterIds` so the
/// resolver admits them. An opaque acting character sees only project stores.
pub fn build_read_resolution_context(
    main: &Connection,
    addressing: &Addressing,
    ctx: &DocEditToolContext,
) -> PathResolutionContext {
    if acting_character_is_opaque_to_vaults(main, ctx) {
        return PathResolutionContext {
            project_id: ctx.project_id.clone(),
            character_id: None,
            character_ids: Vec::new(),
            mount_point: addressing.mount_point.clone(),
            operator_override: ctx.operator_override,
        };
    }
    let peers = collect_peer_character_ids_for_reads(main, ctx);
    PathResolutionContext {
        project_id: ctx.project_id.clone(),
        character_id: ctx.character_id.clone(),
        character_ids: peers,
        mount_point: addressing.mount_point.clone(),
        operator_override: ctx.operator_override,
    }
}

/// v4 `buildWriteResolutionContext`: peer vaults are never admitted; a write that
/// targets a peer's vault (by name or id) raises the read-only error first.
pub fn build_write_resolution_context(
    main: &Connection,
    mount: &Connection,
    addressing: &Addressing,
    ctx: &DocEditToolContext,
) -> Result<PathResolutionContext, String> {
    if acting_character_is_opaque_to_vaults(main, ctx) {
        return Ok(PathResolutionContext {
            project_id: ctx.project_id.clone(),
            character_id: None,
            character_ids: Vec::new(),
            mount_point: addressing.mount_point.clone(),
            operator_override: ctx.operator_override,
        });
    }
    let peers = collect_peer_character_ids_for_reads(main, ctx);
    assert_write_does_not_target_peer_vault(
        main,
        mount,
        addressing.mount_point.as_deref(),
        &peers,
    )?;
    Ok(PathResolutionContext {
        project_id: ctx.project_id.clone(),
        character_id: ctx.character_id.clone(),
        character_ids: Vec::new(),
        mount_point: addressing.mount_point.clone(),
        operator_override: ctx.operator_override,
    })
}

// ============================================================================
// Mount enumeration
// ============================================================================

/// v4 `OfficialProjectMount` — the canonical `scope: 'project'` store.
#[derive(Clone, Debug)]
pub struct OfficialProjectMount {
    pub id: String,
    pub name: String,
    pub base_path: String,
    pub mount_type: String,
}

/// v4 `resolveOfficialProjectMount`: the project's `officialMountPointId` store,
/// or `None` when the project has no official mount, it is missing, or disabled.
pub fn resolve_official_project_mount(
    main: &Connection,
    mount: &Connection,
    project_id: Option<&str>,
) -> Option<OfficialProjectMount> {
    let project_id = project_id?;
    // `find_official_mount_point_id_raw` → Result<Option<row-found>Option<nullable>>.
    let mp_id = match projects::find_official_mount_point_id_raw(main, project_id) {
        Ok(Some(Some(id))) => id,
        _ => return None,
    };
    let row = DocMountPointsRepository::new(mount)
        .find_by_id_for_docedit(&mp_id)
        .ok()??;
    if !row.enabled {
        return None;
    }
    Some(OfficialProjectMount {
        id: row.id,
        name: row.name,
        base_path: row.base_path,
        mount_type: row.mount_type,
    })
}

/// v4 `AccessibleMountPoint` (`path-resolver.ts:741`).
#[derive(Clone, Debug)]
pub struct AccessibleMountPoint {
    pub id: String,
    pub name: String,
    pub base_path: String,
    pub mount_type: String,
}

/// v4 `getAccessibleMountPoints` over `collectAccessibleMountPointIds`: every
/// store the chat context can reach (responding + participant vaults +
/// project-linked stores + Quilltap General), deduped and filtered to enabled
/// mounts. Composed from the ported tiered-mount-pool resolver.
pub fn get_accessible_mount_points(
    main: &Connection,
    mount: &Connection,
    project_id: Option<&str>,
    character_id: Option<&str>,
    extra_character_ids: &[String],
) -> Vec<AccessibleMountPoint> {
    let tier_ctx = TierContext {
        user_id: None,
        character_id: character_id.map(str::to_string),
        character_mount_point_id: None,
        character_ids: if extra_character_ids.is_empty() {
            None
        } else {
            Some(extra_character_ids.to_vec())
        },
        project_id: project_id.map(str::to_string),
    };
    let opts = TierResolveOptions {
        require_ownership: false,
        include_participants: true,
    };
    let pool = resolve_tiered_mount_pool(main, mount, &tier_ctx, &opts);
    let ids = flatten_tier_pool(&pool, FlattenScope::All, true);
    let repo = DocMountPointsRepository::new(mount);
    let mut out = Vec::new();
    for id in ids {
        if let Ok(Some(row)) = repo.find_by_id_for_docedit(&id) {
            if row.enabled {
                out.push(AccessibleMountPoint {
                    id: row.id,
                    name: row.name,
                    base_path: row.base_path,
                    mount_type: row.mount_type,
                });
            }
        }
    }
    out
}

// ============================================================================
// Read / write dispatch (database-backed via the store; fs-backed via host disk)
// ============================================================================

/// Flatten a [`StoreError`] to the message v4 surfaces.
pub fn store_error_message(e: StoreError) -> String {
    match e {
        StoreError::Store(s) => s.message,
        StoreError::Db(d) => d.to_string(),
    }
}

/// v4 `readFileWithMtime` (`path-resolver.ts:622`): a database-backed
/// `ResolvedPath` reads the bytes from `doc_mount_documents` (a NOT_FOUND is
/// re-shaped to v4's `ENOENT: database document not found at <rel>`, the analogue
/// of `fs.readFile`'s ENOENT); a **filesystem-backed** path (`mountType !=
/// 'database'` — an fs/obsidian mount, the `general` scope, or the legacy project
/// fallback) reads the bytes off the host disk + `fs.stat`s it. The on-disk branch
/// only runs when the resolver produced a real `absolute_path` (the host supplied
/// a `files_dir`); with `files_dir: None` the resolver returned the FsSeam refusal
/// before reaching here.
pub fn read_file_with_mtime(
    mount: &Connection,
    resolved: &ResolvedPath,
) -> Result<ReadDoc, String> {
    if resolved.mount_type.as_deref() != Some("database") {
        return read_fs_file_with_mtime(&resolved.absolute_path);
    }
    let mp = resolved
        .mount_point_id
        .as_deref()
        .ok_or("Database-backed ResolvedPath is missing mountPointId")?;
    match database_store::read_database_document(mount, mp, &resolved.relative_path) {
        Ok(doc) => Ok(doc),
        Err(StoreError::Store(s)) if s.code == DbStoreErrorCode::NotFound => Err(format!(
            "ENOENT: database document not found at {}",
            resolved.relative_path
        )),
        Err(e) => Err(store_error_message(e)),
    }
}

/// v4 `writeFileWithMtimeCheck` (`path-resolver.ts:677`): a database-backed
/// `ResolvedPath` lands the bytes via the store primitive; a filesystem-backed
/// path `mkdir -p`s the parent and writes the bytes to disk. Either way returns
/// the new mtime (ms). The `expected_mtime` guard is not ported (reindex/mtime
/// seam — see the module header).
pub fn write_file_with_mtime_check(
    mount: &Connection,
    resolved: &ResolvedPath,
    content: &str,
) -> Result<i64, String> {
    if resolved.mount_type.as_deref() != Some("database") {
        return write_fs_file_with_mtime_check(&resolved.absolute_path, content, None);
    }
    let mp = resolved
        .mount_point_id
        .as_deref()
        .ok_or("Database-backed ResolvedPath is missing mountPointId")?;
    database_store::write_database_document(mount, mp, &resolved.relative_path, content)
        .map_err(store_error_message)
}

/// The host-disk arm of [`read_file_with_mtime`] — v4's `fs.readFile` +
/// `fs.stat` (`path-resolver.ts:649`). `size` is the on-disk **byte** length
/// (`stats.size`), NOT the UTF-16 length a database store reports. Shared with the
/// operator Document-Mode surface (`crate::documents`).
pub(crate) fn read_fs_file_with_mtime(absolute_path: &str) -> Result<ReadDoc, String> {
    let content = std::fs::read_to_string(absolute_path)
        .map_err(|e| fs_io_error(absolute_path, "open", &e))?;
    let meta =
        std::fs::metadata(absolute_path).map_err(|e| fs_io_error(absolute_path, "stat", &e))?;
    Ok(ReadDoc {
        content,
        mtime_ms: metadata_mtime_ms(&meta),
        size: meta.len() as i64,
    })
}

/// The message v4's `writeFileWithMtimeCheck` fs branch throws on an mtime
/// conflict (`path-resolver.ts:706`) — distinct from the database store's
/// "Document was modified…" wording. The operator route keys on the
/// "mtime mismatch" substring to map it to a 409.
pub(crate) const FS_MTIME_MISMATCH_MESSAGE: &str =
    "File was modified by another process (mtime mismatch). Please reload and try again.";

/// The host-disk arm of [`write_file_with_mtime_check`] — v4's optional
/// `expectedMtime` guard, then `fs.mkdir(parent, { recursive: true })`,
/// `fs.writeFile`, and `fs.stat` (`path-resolver.ts:694-731`). Also reused by
/// `doc_open_document`'s new-blank path and the operator Document-Mode surface.
/// When `expected_mtime` is `Some` and an existing file's mtime disagrees, returns
/// [`FS_MTIME_MISMATCH_MESSAGE`].
pub(crate) fn write_fs_file_with_mtime_check(
    absolute_path: &str,
    content: &str,
    expected_mtime: Option<i64>,
) -> Result<i64, String> {
    let path = std::path::Path::new(absolute_path);
    if let Some(expected) = expected_mtime {
        // v4: stat; a mismatch is a conflict; a missing file (ENOENT) is fine — it's
        // a create. Any other stat error bubbles.
        match std::fs::metadata(path) {
            Ok(meta) => {
                if metadata_mtime_ms(&meta) != expected {
                    return Err(FS_MTIME_MISMATCH_MESSAGE.to_string());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(fs_io_error(absolute_path, "stat", &e)),
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| fs_io_error(absolute_path, "mkdir", &e))?;
    }
    std::fs::write(path, content).map_err(|e| fs_io_error(absolute_path, "open", &e))?;
    let meta = std::fs::metadata(path).map_err(|e| fs_io_error(absolute_path, "stat", &e))?;
    Ok(metadata_mtime_ms(&meta))
}

/// `stats.mtime.getTime()` — milliseconds since the Unix epoch.
pub(crate) fn metadata_mtime_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Shape a host-disk I/O failure like the Node error the caller would otherwise
/// surface. A missing path mirrors Node's `ENOENT: no such file or directory,
/// <syscall> '<path>'` (the only shape any tool branch inspects); every other
/// error falls through to the OS string. These paths are not differential-covered
/// (the corpus exercises the success arms + the resolver-level escape errors).
pub(crate) fn fs_io_error(path: &str, syscall: &str, e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => {
            format!("ENOENT: no such file or directory, {syscall} '{path}'")
        }
        _ => e.to_string(),
    }
}

// ============================================================================
// Small JSON-arg accessors (the tool args are a `serde_json::Value` object)
// ============================================================================

/// Owned `String` for `args[key]` when it is a JSON string.
pub fn arg_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Borrowed `&str` for `args[key]` when it is a JSON string.
pub fn arg_str_ref<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

/// `args[key]` as a bool (only a real JSON boolean; absent/other → `None`).
pub fn arg_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

/// `args[key]` as an i64 (JS `number` truncated to integer, matching how the
/// offset/limit/level fields are consumed).
pub fn arg_i64(args: &Value, key: &str) -> Option<i64> {
    // Every key read through this helper (`offset`/`limit` on doc_read_file,
    // `max_results`/`context_lines` on doc_grep, `level` on the heading tools) is
    // an `llmNumber(...)` field in v4, so the lenient conversion belongs here
    // rather than at each site: a model that quoted its number gets the read it
    // asked for, and everything else falls through to be rejected on its merits.
    args.get(key).and_then(|raw| llm_number(raw).as_i64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extname_matches_node() {
        assert_eq!(extname("notes.md"), ".md");
        assert_eq!(extname("a/b/c.JSON"), ".JSON");
        assert_eq!(extname(".gitignore"), "");
        assert_eq!(extname("noext"), "");
        assert_eq!(extname("archive.tar.gz"), ".gz");
        assert_eq!(extname("trailing."), ".");
    }

    #[test]
    fn is_text_file_allow_list() {
        assert!(is_text_file("x.md"));
        assert!(is_text_file("X.MARKDOWN"));
        assert!(is_text_file("data/x.json"));
        assert!(is_text_file("x.yaml"));
        assert!(!is_text_file("x.png"));
        assert!(!is_text_file("x.pdf"));
        assert!(!is_text_file("noext"));
    }

    #[test]
    fn scope_from_str_defaults() {
        assert_eq!(
            scope_from_str(Some("project"), DocEditScope::DocumentStore),
            DocEditScope::Project
        );
        assert_eq!(
            scope_from_str(Some("general"), DocEditScope::DocumentStore),
            DocEditScope::General
        );
        assert_eq!(
            scope_from_str(Some("group"), DocEditScope::DocumentStore),
            DocEditScope::DocumentStore
        );
        assert_eq!(
            scope_from_str(Some("bogus"), DocEditScope::DocumentStore),
            DocEditScope::DocumentStore
        );
        assert_eq!(
            scope_from_str(None, DocEditScope::Project),
            DocEditScope::Project
        );
    }
}
