//! File-management doc-edit handlers — v4
//! `lib/tools/handlers/doc-edit/file-management-handlers.ts`: `doc_move_file`,
//! `doc_copy_file`, `doc_delete_file`, `doc_create_folder`, `doc_delete_folder`,
//! `doc_move_folder`.
//!
//! Both the **database-backed** mount branch (`mount_type == "database"`) and the
//! **filesystem** (`fs.*`) branches are ported. A filesystem-backed resolved path
//! only occurs when the resolver produced a real `absolute_path` — i.e. the host
//! supplied a `files_dir` (P4.6bg); with `files_dir: None` the resolver's
//! [`crate::doc_edit::path_resolver::ResolveError::FsSeam`] surfaces before any
//! handler runs, so the historic refusal is preserved for the database-only corpus.
//!
//! Each handler does its OWN existence checks with v4's byte-exact error strings
//! BEFORE the store mutation. The composed store primitives
//! ([`crate::db::database_store`]) also carry byte-faithful messages, but v4's
//! handlers pre-empt them with their own (path-interpolated) strings on the paths
//! the LLM sees, so this port reproduces the handler-level strings directly.
//!
//! ## Side effects
//!
//! - Librarian announcements (W4.6c — now live): the move/copy/delete/folder-created
//!   /folder-deleted announcements are built inside the write closure and threaded
//!   out via [`DocEditToolResult::pending_librarian_announcement`]
//!   ([`super::PendingLibrarianAnnouncement`]); the executor posts them after the
//!   closure returns (the G6 write-announcement precedent).
//! - The store refresh (P4.D380, round ruling R-2): every write, copy and
//!   disk move goes through the document core's ONE writer / mover, which runs
//!   `after_document_write` in the tool's own write closure
//!   (`Refresh::Now` — `TOOL_REFRESH_IN_CLOSURE`); the old
//!   `triggerReindexIfNeeded` seam is gone.
//! - The `chat_documents` sweeps (P4.D380, v4 `30a3b4cb7`): `doc_move_file` and
//!   both `doc_move_folder` branches call the core's
//!   `sweep_chat_documents_after_move` / `_after_folder_move`, so every open
//!   copy of a moved file — in any chat, under any address that reaches the same
//!   store — follows it. (These were "verified no-ops" while the corpus seeded no
//!   open rows; v4's own sweep was a literal-address rename that the same-store
//!   matcher replaced.)

use crate::inventory::lint_scope::TitleLintScope;
use rusqlite::Connection;
use serde_json::{json, Value};

use super::shared::{
    apply_qtap_uri, arg_str, assert_character_may_read, assert_character_may_write,
    assert_folder_has_no_write_protected_descendants, basename, build_read_resolution_context,
    build_write_resolution_context, document_hidden_from_characters, is_text_file,
    librarian_scope_from, published_mount_point, resolve_actor_origin, resolve_error_message,
    scope_from_str, store_error_message, Addressing, DocEditToolContext,
};
use super::DocEditToolResult;
use crate::db::database_store::{self, database_document_exists, database_folder_exists};
use crate::doc_edit::path_resolver::{resolve_doc_edit_path, ResolvedPath};
use crate::doc_edit::qtap_uri::parse_qtap_uri;
use crate::doc_edit::uri_producers::uri_for_resolved_path;
use crate::doc_edit::DocEditScope;
use crate::services::librarian_notifications::{
    LibrarianCopyAnnouncement, LibrarianDeleteAnnouncement, LibrarianFolderCreatedAnnouncement,
    LibrarianFolderDeletedAnnouncement, LibrarianMoveAnnouncement,
};

/// POSIX `path.posix.basename` for a forward-slash path (copy's source basename).
fn posix_basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// POSIX `path.posix.join(dir, name)` for the copy dir-append (both already
/// forward-slash). A trailing slash on `dir` is collapsed; empty `dir` yields
/// `name`.
fn posix_join(dir: &str, name: &str) -> String {
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// The doc-store URI for a resolved path (no heading/level fragment for file-mgmt
/// results).
fn uri_for(mount: &Connection, resolved: &ResolvedPath) -> String {
    uri_for_resolved_path(mount, resolved, None, None)
}

// ============================================================================
// doc_move_file
// ============================================================================

pub fn handle_move_file(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = apply_qtap_uri(
        arg_str(args, "scope"),
        arg_str(args, "mount_point"),
        arg_str(args, "path"),
        args.get("uri").and_then(Value::as_str),
    )?;
    let Some(path) = addressing.path.clone() else {
        return Ok(DocEditToolResult::fail(
            "A `path` (or `uri`) for the source is required.",
        ));
    };
    let new_path = arg_str(args, "new_path");
    let scope = scope_from_str(addressing.scope.as_deref(), DocEditScope::DocumentStore);

    let write_context = build_write_resolution_context(main, mount, &addressing, ctx)?;
    let resolved_source = resolve_doc_edit_path(
        main,
        mount,
        scope,
        Some(&path),
        &write_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;
    let resolved_dest = resolve_doc_edit_path(
        main,
        mount,
        scope,
        new_path.as_deref(),
        &write_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;

    // A move removes the source path — gate it by character_write. The destination
    // is gated too in case it would overwrite a protected file (a no-op when the
    // destination doesn't exist yet).
    assert_character_may_write(mount, &resolved_source, ctx)?;
    assert_character_may_write(mount, &resolved_dest, ctx)?;

    // Capture the source's read-policy BEFORE the move relocates its link, so a
    // character_read:false document isn't announced to characters (operator path).
    let hidden_from_characters = document_hidden_from_characters(
        mount,
        resolved_source.mount_point_id.as_deref(),
        &resolved_source.relative_path,
    );

    // P4.D380 (v4 `a3d53eae1`): "The core checks the source and the
    // destination, moves the link row or the file, and refreshes a moved file
    // on disk."
    use crate::documents::core::{rename_document, Refresh, RenameOutcome};
    let np = new_path.clone().unwrap_or_default();
    match rename_document(
        main,
        mount,
        &resolved_source,
        &resolved_dest,
        Refresh::Now { main, mount },
    )
    .map_err(|e| e.message())?
    {
        RenameOutcome::Moved => {}
        RenameOutcome::SourceMissing => {
            return Ok(DocEditToolResult::fail(format!(
                "Source file not found: {path}"
            )))
        }
        RenameOutcome::SourceNotAFile => {
            return Ok(DocEditToolResult::fail(format!(
            "Path is a folder, not a file: {path}. Use doc_move_folder to rename or move folders."
        )))
        }
        RenameOutcome::DestinationTaken => {
            return Ok(DocEditToolResult::fail(format!(
                "Destination already exists: {np}. Move will not overwrite existing files."
            )))
        }
    }

    // "Keep any chat's Document Mode pointer on the file, under the store name
    // the rows were recorded with (never a caller's `self`, bug 256)." (v4
    // `30a3b4cb7`.)
    let published =
        published_mount_point(Some(&resolved_source), addressing.mount_point.as_deref());
    crate::documents::core::sweep_chat_documents_after_move(
        main,
        mount,
        crate::documents::core::SweepStore {
            scope: scope.as_str(),
            mount_point: published.as_deref().map(Some),
            mount_point_id: resolved_source.mount_point_id.as_deref(),
        },
        &path,
        &np,
        None,
        Some(&ctx.chat_id),
    );

    tracing::info!(
        target: "quilltap::doc_edit",
        from = %path,
        to = %np,
        scope = scope.as_str(),
        mountType = resolved_source.mount_type.as_deref(),
        "Moved document"
    );

    let moved_uri = uri_for(mount, &resolved_dest);
    let announcement = LibrarianMoveAnnouncement {
        chat_id: ctx.chat_id.clone(),
        old_display_title: basename(&path).to_string(),
        new_display_title: basename(&np).to_string(),
        old_uri: uri_for(mount, &resolved_source),
        new_uri: moved_uri.clone(),
        scope: librarian_scope_from(scope),
        mount_point: published,
        origin: resolve_actor_origin(main, ctx),
        is_folder: false,
        hidden_from_characters,
    };
    Ok(DocEditToolResult::ok(
        json!({
            "success": true,
            "old_path": path,
            "new_path": np,
            "uri": moved_uri,
        }),
        format!("Moved: {path} → {np}"),
    )
    .with_librarian_announcement(announcement))
}

// ============================================================================
// doc_copy_file
// ============================================================================

/// v4 `resolveCopyDestination` (database-mount branch): if the resolved
/// destination points at an existing folder, append the source basename (posix
/// join); otherwise use the resolved relative path. The trim/`.`/`/`/`./` early
/// return yields the bare source basename.
fn resolve_copy_destination(
    mount: &Connection,
    resolved_dest: &ResolvedPath,
    dest_rel_input: &str,
    source_basename: &str,
) -> Result<String, String> {
    let normalised = dest_rel_input.trim();
    if normalised.is_empty() || normalised == "." || normalised == "/" || normalised == "./" {
        return Ok(source_basename.to_string());
    }
    let mut dest_is_directory = false;
    if resolved_dest.mount_type.as_deref() == Some("database") {
        if let Some(mp) = resolved_dest.mount_point_id.as_deref() {
            dest_is_directory = database_folder_exists(mount, mp, &resolved_dest.relative_path)
                .map_err(|e| e.to_string())?;
        }
    } else if !resolved_dest.absolute_path.is_empty() {
        // Filesystem dest: v4 `fs.stat(...).isDirectory()`, false on a stat error.
        dest_is_directory = fs_stat_is_dir(&resolved_dest.absolute_path) == Some(true);
    }
    if dest_is_directory {
        Ok(posix_join(&resolved_dest.relative_path, source_basename))
    } else {
        Ok(resolved_dest.relative_path.clone())
    }
}

pub fn handle_copy_file(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    // A qtap:// source_uri/dest_uri supersedes the mount_point/path pair. Copy is a
    // document-store-only operation, so reject project/general scopes.
    let mut source_mount_point = arg_str(args, "source_mount_point");
    let mut source_path = arg_str(args, "source_path");
    let mut dest_mount_point = arg_str(args, "dest_mount_point");
    let mut dest_path = arg_str(args, "dest_path");

    if let Some(source_uri) = args.get("source_uri").and_then(Value::as_str) {
        let p = parse_qtap_uri(source_uri).map_err(|e| e.message)?;
        if p.scope != DocEditScope::DocumentStore {
            return Ok(DocEditToolResult::fail(
                "doc_copy_file addresses document stores only; source_uri must name a store (or \"self\").",
            ));
        }
        source_mount_point = p.mount_point;
        source_path = Some(p.path);
    }
    if let Some(dest_uri) = args.get("dest_uri").and_then(Value::as_str) {
        let p = parse_qtap_uri(dest_uri).map_err(|e| e.message)?;
        if p.scope != DocEditScope::DocumentStore {
            return Ok(DocEditToolResult::fail(
                "doc_copy_file addresses document stores only; dest_uri must name a store (or \"self\").",
            ));
        }
        dest_mount_point = p.mount_point;
        dest_path = Some(p.path);
    }
    let (Some(source_mount_point), Some(source_path)) =
        (source_mount_point.clone(), source_path.clone())
    else {
        return Ok(DocEditToolResult::fail(
            "Provide source_uri, or both source_mount_point and source_path.",
        ));
    };
    let (Some(dest_mount_point), Some(dest_path)) = (dest_mount_point.clone(), dest_path.clone())
    else {
        return Ok(DocEditToolResult::fail(
            "Provide dest_uri, or both dest_mount_point and dest_path.",
        ));
    };

    // Text-only guard, to mirror doc_write_file / doc_read_file semantics.
    if !is_text_file(&source_path) {
        return Ok(DocEditToolResult::fail(
            "doc_copy_file supports text files only. For binary assets (images, PDFs, etc.), use the blob family of tools.",
        ));
    }

    // Resolve source (read context — peer vaults may be readable in shared chats)
    // and destination (write context — peer vaults are never writable).
    let read_addressing = Addressing {
        mount_point: Some(source_mount_point.clone()),
        ..Default::default()
    };
    let write_addressing = Addressing {
        mount_point: Some(dest_mount_point.clone()),
        ..Default::default()
    };
    let read_context = build_read_resolution_context(main, &read_addressing, ctx);
    let write_context = build_write_resolution_context(main, mount, &write_addressing, ctx)?;

    let resolved_source = resolve_doc_edit_path(
        main,
        mount,
        DocEditScope::DocumentStore,
        Some(&source_path),
        &read_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;
    // You can't copy what you can't read — gate the source by character_read.
    assert_character_may_read(mount, &resolved_source, ctx)?;

    // Initial destination resolve against the raw dest_path; re-resolved below once
    // we know whether to append the source basename.
    let dest_rel_input = dest_path.clone();
    let initial_dest_rel = if dest_rel_input.trim().is_empty() || dest_rel_input.trim() == "/" {
        ".".to_string()
    } else {
        dest_rel_input.clone()
    };
    let initial_resolved_dest = resolve_doc_edit_path(
        main,
        mount,
        DocEditScope::DocumentStore,
        Some(&initial_dest_rel),
        &write_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;

    // Same-store guard: compare by mountPointId (authoritative after lookup collapses
    // name-vs-id inputs).
    if let (Some(src_mp), Some(dst_mp)) = (
        resolved_source.mount_point_id.as_deref(),
        initial_resolved_dest.mount_point_id.as_deref(),
    ) {
        if src_mp == dst_mp {
            let label = resolved_source
                .mount_point_name
                .as_deref()
                .unwrap_or(src_mp);
            return Ok(DocEditToolResult::fail(format!(
                "doc_copy_file requires source and destination to be different document stores. Both resolved to \"{label}\"."
            )));
        }
    }

    let source_basename = posix_basename(&source_path).to_string();
    let final_dest_rel = resolve_copy_destination(
        mount,
        &initial_resolved_dest,
        &dest_rel_input,
        &source_basename,
    )?;

    // Re-resolve with the final relative path so downstream checks + write see the
    // true destination.
    let resolved_dest = resolve_doc_edit_path(
        main,
        mount,
        DocEditScope::DocumentStore,
        Some(&final_dest_rel),
        &write_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;
    // Gate the destination by character_write (no-op when it doesn't exist yet).
    assert_character_may_write(mount, &resolved_dest, ctx)?;

    // P4.D380 (v4 `a3d53eae1`): the core's `statDocument` answers both checks.
    // "The source must be a file (an upload counts — readDocument reads it)."
    use crate::documents::core::{read_document, stat_document, write_document, DocKind, Refresh};
    if stat_document(mount, &resolved_source).map_err(|e| e.message())? == Some(DocKind::Folder) {
        return Ok(DocEditToolResult::fail(format!(
            "Source path is a folder, not a file: {source_path}."
        )));
    }
    // Refuse to overwrite — match doc_move_file semantics.
    match stat_document(mount, &resolved_dest).map_err(|e| e.message())? {
        Some(DocKind::Folder) => {
            return Ok(DocEditToolResult::fail(format!(
                "Destination path {final_dest_rel} resolves to a folder, not a file location."
            )))
        }
        Some(DocKind::File) => {
            return Ok(DocEditToolResult::fail(format!(
                "Destination already exists: {final_dest_rel}. Copy will not overwrite existing files; delete it first if you want to replace it."
            )))
        }
        None => {}
    }

    let content = match read_document(mount, &resolved_source) {
        Ok(r) => r.content,
        Err(e) if e.is_enoent() => {
            return Ok(DocEditToolResult::fail(format!(
                "Source file not found: {source_path}"
            )))
        }
        Err(e) => return Err(e.message()),
    };
    // "The core creates parent folders (disk) or folder rows (database), then
    // refreshes the destination store."
    let mtime = write_document(
        main,
        mount,
        &resolved_dest,
        &content,
        None,
        Refresh::Now { main, mount },
    )
    .map_err(|e| e.message())?;

    let copied_uri = uri_for(mount, &resolved_dest);
    let src_label = resolved_source
        .mount_point_name
        .clone()
        .unwrap_or_else(|| source_mount_point.clone());
    let dst_label = resolved_dest
        .mount_point_name
        .clone()
        .unwrap_or_else(|| dest_mount_point.clone());

    let announcement = LibrarianCopyAnnouncement {
        chat_id: ctx.chat_id.clone(),
        source_display_title: source_basename.clone(),
        dest_display_title: posix_basename(&final_dest_rel).to_string(),
        source_mount_point: src_label.clone(),
        dest_mount_point: dst_label.clone(),
        source_uri: uri_for(mount, &resolved_source),
        dest_uri: copied_uri.clone(),
        origin: resolve_actor_origin(main, ctx),
        // Gate by the source's read-policy: a character_read:false source must not
        // be named in an announcement to characters (operator path).
        hidden_from_characters: document_hidden_from_characters(
            mount,
            resolved_source.mount_point_id.as_deref(),
            &resolved_source.relative_path,
        ),
    };

    Ok(DocEditToolResult::ok(
        json!({
            "success": true,
            "source_mount_point": src_label,
            "source_path": source_path,
            "dest_mount_point": dst_label,
            "dest_path": final_dest_rel,
            "uri": copied_uri,
            "mtime": mtime,
        }),
        format!("Copied: {src_label}:{source_path} → {dst_label}:{final_dest_rel}"),
    )
    .with_librarian_announcement(announcement))
}

// ============================================================================
// doc_delete_file
// ============================================================================

pub fn handle_delete_file(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = apply_qtap_uri(
        arg_str(args, "scope"),
        arg_str(args, "mount_point"),
        arg_str(args, "path"),
        args.get("uri").and_then(Value::as_str),
    )?;
    let Some(path) = addressing.path.clone() else {
        return Ok(DocEditToolResult::fail("A `path` or a `uri` is required."));
    };
    let scope = scope_from_str(addressing.scope.as_deref(), DocEditScope::DocumentStore);
    let write_context = build_write_resolution_context(main, mount, &addressing, ctx)?;
    let resolved = resolve_doc_edit_path(
        main,
        mount,
        scope,
        Some(&path),
        &write_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;
    assert_character_may_write(mount, &resolved, ctx)?;

    // Capture the read-policy BEFORE the delete removes the link row, so a
    // character_read:false document isn't announced to characters (operator path).
    let hidden_from_characters = document_hidden_from_characters(
        mount,
        resolved.mount_point_id.as_deref(),
        &resolved.relative_path,
    );

    // P4.D380 (v4 `a3d53eae1`): the core's `deleteDocument` — the database link
    // (an upload included) or the disk file; `not-a-file` means a folder.
    match crate::documents::core::delete_document(main, mount, &resolved)
        .map_err(|e| e.message())?
    {
        crate::documents::core::DeleteOutcome::Deleted => {}
        crate::documents::core::DeleteOutcome::NotFound => {
            return Ok(DocEditToolResult::fail(format!("File not found: {path}")))
        }
        crate::documents::core::DeleteOutcome::NotAFile => {
            return Ok(DocEditToolResult::fail(format!(
                "Path is a folder, not a file: {path}. Use doc_delete_folder to remove folders."
            )))
        }
    }

    tracing::info!(
        target: "quilltap::doc_edit",
        path = %path,
        scope = scope.as_str(),
        mountType = resolved.mount_type.as_deref(),
        "Deleted document"
    );

    let deleted_uri = uri_for(mount, &resolved);
    let announcement = LibrarianDeleteAnnouncement {
        chat_id: ctx.chat_id.clone(),
        display_title: basename(&path).to_string(),
        file_path: path.clone(),
        scope: librarian_scope_from(scope),
        mount_point: published_mount_point(Some(&resolved), addressing.mount_point.as_deref()),
        origin: resolve_actor_origin(main, ctx),
        hidden_from_characters,
    };
    Ok(DocEditToolResult::ok(
        json!({ "success": true, "path": path, "uri": deleted_uri }),
        format!("Deleted file: {path}"),
    )
    .with_librarian_announcement(announcement))
}

// ============================================================================
// doc_create_folder
// ============================================================================

pub fn handle_create_folder(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = apply_qtap_uri(
        arg_str(args, "scope"),
        arg_str(args, "mount_point"),
        arg_str(args, "path"),
        args.get("uri").and_then(Value::as_str),
    )?;
    let Some(path) = addressing.path.clone() else {
        return Ok(DocEditToolResult::fail("A `path` or a `uri` is required."));
    };
    let scope = scope_from_str(addressing.scope.as_deref(), DocEditScope::DocumentStore);
    let write_context = build_write_resolution_context(main, mount, &addressing, ctx)?;
    let resolved = resolve_doc_edit_path(
        main,
        mount,
        scope,
        Some(&path),
        &write_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;

    if resolved.mount_type.as_deref() == Some("database") {
        if let Some(mp) = resolved.mount_point_id.as_deref() {
            // v4 wraps createDatabaseFolder in a try/catch, returning the raw store
            // error message on failure.
            match database_store::create_database_folder(mount, mp, &resolved.relative_path) {
                Ok(_) => {
                    let uri = uri_for(mount, &resolved);
                    let announcement = LibrarianFolderCreatedAnnouncement {
                        chat_id: ctx.chat_id.clone(),
                        folder_path: path.clone(),
                        scope: librarian_scope_from(scope),
                        mount_point: published_mount_point(
                            Some(&resolved),
                            addressing.mount_point.as_deref(),
                        ),
                        origin: resolve_actor_origin(main, ctx),
                    };
                    return Ok(DocEditToolResult::ok(
                        json!({ "success": true, "path": path, "uri": uri }),
                        format!("Created folder: {path}"),
                    )
                    .with_librarian_announcement(announcement));
                }
                Err(e) => {
                    return Ok(DocEditToolResult::fail(e.to_string()));
                }
            }
        }
    }

    // Filesystem mount (v4 `file-management-handlers.ts:654-680`): `mkdir -p` (v4
    // creates the directory itself, recursive + idempotent — no existence check).
    std::fs::create_dir_all(&resolved.absolute_path)
        .map_err(|e| super::shared::fs_io_error(&resolved.absolute_path, "mkdir", &e))?;
    let uri = uri_for(mount, &resolved);
    let announcement = LibrarianFolderCreatedAnnouncement {
        chat_id: ctx.chat_id.clone(),
        folder_path: path.clone(),
        scope: librarian_scope_from(scope),
        mount_point: published_mount_point(Some(&resolved), addressing.mount_point.as_deref()),
        origin: resolve_actor_origin(main, ctx),
    };
    Ok(DocEditToolResult::ok(
        json!({ "success": true, "path": path, "uri": uri }),
        format!("Created folder: {path}"),
    )
    .with_librarian_announcement(announcement))
}

// ============================================================================
// doc_delete_folder
// ============================================================================

pub fn handle_delete_folder(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = apply_qtap_uri(
        arg_str(args, "scope"),
        arg_str(args, "mount_point"),
        arg_str(args, "path"),
        args.get("uri").and_then(Value::as_str),
    )?;
    let Some(path) = addressing.path.clone() else {
        return Ok(DocEditToolResult::fail("A `path` or a `uri` is required."));
    };
    let scope = scope_from_str(addressing.scope.as_deref(), DocEditScope::DocumentStore);
    let write_context = build_write_resolution_context(main, mount, &addressing, ctx)?;
    let resolved = resolve_doc_edit_path(
        main,
        mount,
        scope,
        Some(&path),
        &write_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;
    // A character may not delete a folder that holds a protected document.
    assert_folder_has_no_write_protected_descendants(mount, &resolved, ctx)?;

    if resolved.mount_type.as_deref() == Some("database") {
        if let Some(mp) = resolved.mount_point_id.as_deref() {
            match database_store::delete_database_folder(mount, mp, &resolved.relative_path) {
                Ok(()) => {
                    let uri = uri_for(mount, &resolved);
                    let announcement = LibrarianFolderDeletedAnnouncement {
                        chat_id: ctx.chat_id.clone(),
                        folder_path: path.clone(),
                        scope: librarian_scope_from(scope),
                        mount_point: published_mount_point(
                            Some(&resolved),
                            addressing.mount_point.as_deref(),
                        ),
                        origin: resolve_actor_origin(main, ctx),
                    };
                    return Ok(DocEditToolResult::ok(
                        json!({ "success": true, "path": path, "uri": uri }),
                        format!("Deleted folder: {path}"),
                    )
                    .with_librarian_announcement(announcement));
                }
                Err(e) => {
                    let error_msg = store_error_message(e);
                    // v4 dispatches on the message text (the store errors carry the
                    // v4 strings). NOT_EMPTY → the enriched message + formattedText.
                    if error_msg.contains("not empty") {
                        return Ok(DocEditToolResult::fail_with_formatted(
                            format!("Folder is not empty: {path}. Only empty folders can be deleted. Use doc_list_files to see the contents."),
                            format!("Error: Folder \"{path}\" is not empty. Only empty folders can be deleted."),
                        ));
                    }
                    if error_msg.to_lowercase().contains("not found")
                        && database_document_exists(mount, mp, &resolved.relative_path)
                            .map_err(|e| e.to_string())?
                    {
                        return Ok(DocEditToolResult::fail(format!(
                            "Path is a file, not a folder: {path}. Use doc_delete_file to delete files."
                        )));
                    }
                    return Ok(DocEditToolResult::fail(error_msg));
                }
            }
        }
    }

    // Filesystem mount (v4 `file-management-handlers.ts:733-779`): verify it's an
    // empty directory, then `rmdir`. The "not empty" strings differ from the
    // database branch (they carry the entry count).
    match fs_stat_is_dir(&resolved.absolute_path) {
        Some(true) => {}
        Some(false) => {
            return Ok(DocEditToolResult::fail(format!(
                "Path is not a folder: {path}"
            )))
        }
        None => return Ok(DocEditToolResult::fail(format!("Folder not found: {path}"))),
    }
    let entry_count = std::fs::read_dir(&resolved.absolute_path)
        .map_err(|e| super::shared::fs_io_error(&resolved.absolute_path, "scandir", &e))?
        .count();
    if entry_count > 0 {
        let plural = if entry_count == 1 { "" } else { "s" };
        return Ok(DocEditToolResult::fail_with_formatted(
            format!("Folder is not empty: {path} (contains {entry_count} item{plural}). Only empty folders can be deleted. Use doc_list_files to see the contents."),
            format!("Error: Folder \"{path}\" is not empty ({entry_count} item{plural}). Only empty folders can be deleted."),
        ));
    }
    std::fs::remove_dir(&resolved.absolute_path)
        .map_err(|e| super::shared::fs_io_error(&resolved.absolute_path, "rmdir", &e))?;
    let uri = uri_for(mount, &resolved);
    let announcement = LibrarianFolderDeletedAnnouncement {
        chat_id: ctx.chat_id.clone(),
        folder_path: path.clone(),
        scope: librarian_scope_from(scope),
        mount_point: published_mount_point(Some(&resolved), addressing.mount_point.as_deref()),
        origin: resolve_actor_origin(main, ctx),
    };
    Ok(DocEditToolResult::ok(
        json!({ "success": true, "path": path, "uri": uri }),
        format!("Deleted folder: {path}"),
    )
    .with_librarian_announcement(announcement))
}

// ============================================================================
// doc_move_folder
// ============================================================================

pub fn handle_move_folder(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = apply_qtap_uri(
        arg_str(args, "scope"),
        arg_str(args, "mount_point"),
        arg_str(args, "path"),
        args.get("uri").and_then(Value::as_str),
    )?;
    let Some(path) = addressing.path.clone() else {
        return Ok(DocEditToolResult::fail(
            "A `path` (or `uri`) for the source folder is required.",
        ));
    };
    let new_path = arg_str(args, "new_path");
    let scope = scope_from_str(addressing.scope.as_deref(), DocEditScope::DocumentStore);

    let write_context = build_write_resolution_context(main, mount, &addressing, ctx)?;
    let resolved_source = resolve_doc_edit_path(
        main,
        mount,
        scope,
        Some(&path),
        &write_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;
    let resolved_dest = resolve_doc_edit_path(
        main,
        mount,
        scope,
        new_path.as_deref(),
        &write_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;

    // Moving a folder relocates everything under it — a character may not move a
    // folder that holds a protected document.
    assert_folder_has_no_write_protected_descendants(mount, &resolved_source, ctx)?;

    if resolved_source.mount_type.as_deref() == Some("database") {
        if let Some(mp) = resolved_source.mount_point_id.as_deref() {
            match database_store::move_database_folder(
                mount,
                mp,
                &resolved_source.relative_path,
                &resolved_dest.relative_path,
                TitleLintScope::Unlinted { main },
            ) {
                Ok(()) => {
                    // v4 `30a3b4cb7`: every chat's Document Mode row under the folder follows,
                    // matched by the store it reaches (the case-SENSITIVE prefix — R-6).
                    crate::documents::core::sweep_chat_documents_after_folder_move(
                        main,
                        mount,
                        crate::documents::core::SweepStore {
                            scope: scope.as_str(),
                            mount_point: published_mount_point(
                                Some(&resolved_source),
                                addressing.mount_point.as_deref(),
                            )
                            .as_deref()
                            .map(Some),
                            mount_point_id: resolved_source.mount_point_id.as_deref(),
                        },
                        &path,
                        new_path.as_deref().unwrap_or_default(),
                        Some(&ctx.chat_id),
                    );
                    let moved_uri = uri_for(mount, &resolved_dest);
                    let np = new_path.clone().unwrap_or_default();
                    // v4's folder-move site passes NO `hiddenFromCharacters`
                    // (→ falsy); the file-move site does. Match: `false` here.
                    let announcement = LibrarianMoveAnnouncement {
                        chat_id: ctx.chat_id.clone(),
                        old_display_title: basename(&path).to_string(),
                        new_display_title: basename(&np).to_string(),
                        old_uri: uri_for(mount, &resolved_source),
                        new_uri: moved_uri.clone(),
                        scope: librarian_scope_from(scope),
                        mount_point: published_mount_point(
                            Some(&resolved_source),
                            addressing.mount_point.as_deref(),
                        ),
                        origin: resolve_actor_origin(main, ctx),
                        is_folder: true,
                        hidden_from_characters: false,
                    };
                    return Ok(DocEditToolResult::ok(
                        json!({
                            "success": true,
                            "old_path": path,
                            "new_path": np,
                            "uri": moved_uri,
                        }),
                        format!("Moved folder: {path} → {np}"),
                    )
                    .with_librarian_announcement(announcement));
                }
                Err(e) => {
                    let error_msg = store_error_message(e);
                    if error_msg.to_lowercase().contains("not found")
                        && database_document_exists(mount, mp, &resolved_source.relative_path)
                            .map_err(|e| e.to_string())?
                    {
                        return Ok(DocEditToolResult::fail(format!(
                            "Source path is a file, not a folder: {path}. Use doc_move_file to rename or move files."
                        )));
                    }
                    return Ok(DocEditToolResult::fail(error_msg));
                }
            }
        }
    }

    // Filesystem mount (v4 `file-management-handlers.ts:845-908`): stat the source
    // directory, refuse an existing dest, `mkdir -p` the parent, then `fs.rename`.
    match fs_stat_is_dir(&resolved_source.absolute_path) {
        Some(true) => {}
        Some(false) => {
            return Ok(DocEditToolResult::fail(format!(
                "Source path is not a folder: {path}"
            )))
        }
        None => {
            return Ok(DocEditToolResult::fail(format!(
                "Source folder not found: {path}"
            )))
        }
    }
    let np = new_path.clone().unwrap_or_default();
    let folder_case_only_rename = resolved_source.absolute_path != resolved_dest.absolute_path
        && resolved_source.absolute_path.to_lowercase()
            == resolved_dest.absolute_path.to_lowercase();
    if !folder_case_only_rename && fs_path_exists(&resolved_dest.absolute_path) {
        return Ok(DocEditToolResult::fail(format!(
            "Destination already exists: {np}. Move will not overwrite existing folders."
        )));
    }
    if let Some(parent) = std::path::Path::new(&resolved_dest.absolute_path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| super::shared::fs_io_error(&resolved_dest.absolute_path, "mkdir", &e))?;
    }
    std::fs::rename(&resolved_source.absolute_path, &resolved_dest.absolute_path)
        .map_err(|e| super::shared::fs_io_error(&resolved_dest.absolute_path, "rename", &e))?;
    // v4 `30a3b4cb7`: every chat's Document Mode row under the folder follows,
    // matched by the store it reaches (the case-SENSITIVE prefix — R-6).
    crate::documents::core::sweep_chat_documents_after_folder_move(
        main,
        mount,
        crate::documents::core::SweepStore {
            scope: scope.as_str(),
            mount_point: published_mount_point(
                Some(&resolved_source),
                addressing.mount_point.as_deref(),
            )
            .as_deref()
            .map(Some),
            mount_point_id: resolved_source.mount_point_id.as_deref(),
        },
        &path,
        new_path.as_deref().unwrap_or_default(),
        Some(&ctx.chat_id),
    );
    let moved_uri = uri_for(mount, &resolved_dest);
    // v4's folder-move site passes NO `hiddenFromCharacters` (→ falsy).
    let announcement = LibrarianMoveAnnouncement {
        chat_id: ctx.chat_id.clone(),
        old_display_title: basename(&path).to_string(),
        new_display_title: basename(&np).to_string(),
        old_uri: uri_for(mount, &resolved_source),
        new_uri: moved_uri.clone(),
        scope: librarian_scope_from(scope),
        mount_point: published_mount_point(
            Some(&resolved_source),
            addressing.mount_point.as_deref(),
        ),
        origin: resolve_actor_origin(main, ctx),
        is_folder: true,
        hidden_from_characters: false,
    };
    Ok(DocEditToolResult::ok(
        json!({
            "success": true,
            "old_path": path,
            "new_path": np,
            "uri": moved_uri,
        }),
        format!("Moved folder: {path} → {np}"),
    )
    .with_librarian_announcement(announcement))
}

// ============================================================================
// Filesystem helpers (v4 `fs.stat` / `fs.access` probes)
// ============================================================================

// P4.D380 (R-4): `fs_stat_is_file` (`metadata().ok()` — an EACCES read as
// "not found") RETIRED; the file sites probe through the core's
// `stat_document` → `stat_resolved_file`. `fs_path_exists` stays for
// `doc_move_folder`, whose filesystem branch v4 still probes with `fs.access`
// (unchanged at the pin).

/// v4 `fs.stat(p).isDirectory()` — `Some(bool)` on a hit, `None` on a stat miss.
fn fs_stat_is_dir(absolute_path: &str) -> Option<bool> {
    std::fs::metadata(absolute_path).ok().map(|m| m.is_dir())
}

/// v4 `fs.access(p)` (F_OK) — true when the path resolves to an existing entry.
fn fs_path_exists(absolute_path: &str) -> bool {
    std::path::Path::new(absolute_path).exists()
}
