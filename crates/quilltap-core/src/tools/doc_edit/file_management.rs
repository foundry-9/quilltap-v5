//! File-management doc-edit handlers — v4
//! `lib/tools/handlers/doc-edit/file-management-handlers.ts`: `doc_move_file`,
//! `doc_copy_file`, `doc_delete_file`, `doc_create_folder`, `doc_delete_folder`,
//! `doc_move_folder`.
//!
//! Only the **database-backed** mount branch is ported (`mount_type == "database"`);
//! every corpus store is database-backed. The filesystem (`fs.*`) branches are the
//! [`crate::doc_edit::path_resolver::ResolveError::FsSeam`] — a `mount_type !=
//! "database"` resolved path surfaces the FsSeam message (as
//! [`super::shared::read_file_with_mtime`] does), never real disk I/O.
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
//! - `triggerReindexIfNeeded` — skipped seam (the `chunkCount` it would bump is
//!   pinned/excluded in the differential).
//! - `syncChatDocumentsAfterFileMove` / `syncChatDocumentsAfterFolderMove` (the
//!   `chat_documents` Document-Mode pointer sync) — skipped; the corpus seeds no
//!   open `chat_documents` rows, so both are verified no-ops.

use rusqlite::Connection;
use serde_json::{json, Value};

use super::shared::{
    apply_qtap_uri, arg_str, assert_character_may_read, assert_character_may_write,
    assert_folder_has_no_write_protected_descendants, basename, build_read_resolution_context,
    build_write_resolution_context, document_hidden_from_characters, is_text_file,
    librarian_scope_from, read_file_with_mtime, resolve_actor_origin, resolve_error_message,
    scope_from_str, store_error_message, write_file_with_mtime_check, Addressing,
    DocEditToolContext,
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
fn uri_for(
    main: &Connection,
    mount: &Connection,
    resolved: &ResolvedPath,
    ctx: &DocEditToolContext,
) -> String {
    uri_for_resolved_path(
        main,
        mount,
        resolved,
        ctx.character_id.as_deref(),
        None,
        None,
    )
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
    let resolved_source = resolve_doc_edit_path(main, mount, scope, Some(&path), &write_context)
        .map_err(resolve_error_message)?;
    let resolved_dest =
        resolve_doc_edit_path(main, mount, scope, new_path.as_deref(), &write_context)
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

    // Database-backed mount: route the move through the database-store module.
    if resolved_source.mount_type.as_deref() == Some("database") {
        if let Some(mp) = resolved_source.mount_point_id.as_deref() {
            if !database_document_exists(mount, mp, &resolved_source.relative_path)
                .map_err(|e| e.to_string())?
            {
                if database_folder_exists(mount, mp, &resolved_source.relative_path)
                    .map_err(|e| e.to_string())?
                {
                    return Ok(DocEditToolResult::fail(format!(
                        "Path is a folder, not a file: {path}. Use doc_move_folder to rename or move folders."
                    )));
                }
                return Ok(DocEditToolResult::fail(format!(
                    "Source file not found: {path}"
                )));
            }
            // Case-only rename is legal: the case-insensitive existence check
            // would find the source itself and report a bogus conflict.
            let case_only_rename = resolved_source.relative_path != resolved_dest.relative_path
                && resolved_source.relative_path.to_lowercase()
                    == resolved_dest.relative_path.to_lowercase();
            if !case_only_rename
                && database_document_exists(mount, mp, &resolved_dest.relative_path)
                    .map_err(|e| e.to_string())?
            {
                let np = new_path.clone().unwrap_or_default();
                return Ok(DocEditToolResult::fail(format!(
                    "Destination already exists: {np}. Move will not overwrite existing files."
                )));
            }
            database_store::move_database_document(
                mount,
                mp,
                &resolved_source.relative_path,
                &resolved_dest.relative_path,
            )
            .map_err(store_error_message)?;
            // syncChatDocumentsAfterFileMove: no-op seam (see the module header).
            let moved_uri = uri_for(main, mount, &resolved_dest, ctx);
            let np = new_path.clone().unwrap_or_default();
            let announcement = LibrarianMoveAnnouncement {
                chat_id: ctx.chat_id.clone(),
                old_display_title: basename(&path).to_string(),
                new_display_title: basename(&np).to_string(),
                old_uri: uri_for(main, mount, &resolved_source, ctx),
                new_uri: moved_uri.clone(),
                scope: librarian_scope_from(scope),
                mount_point: addressing.mount_point.clone(),
                origin: resolve_actor_origin(main, ctx),
                is_folder: false,
                hidden_from_characters,
            };
            return Ok(DocEditToolResult::ok(
                json!({
                    "success": true,
                    "old_path": path,
                    "new_path": np,
                    "uri": moved_uri,
                }),
                format!("Moved: {path} → {np}"),
            )
            .with_librarian_announcement(announcement));
        }
    }

    // Non-database mount → the host-filesystem seam.
    Err(fs_seam())
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
    }
    // FS-backed dest detection is the FsSeam (never on the corpus); leave false.
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
    )
    .map_err(resolve_error_message)?;
    // Gate the destination by character_write (no-op when it doesn't exist yet).
    assert_character_may_write(mount, &resolved_dest, ctx)?;

    // Verify source exists and is a file (not a folder).
    if resolved_source.mount_type.as_deref() == Some("database") {
        if let Some(mp) = resolved_source.mount_point_id.as_deref() {
            if !database_document_exists(mount, mp, &resolved_source.relative_path)
                .map_err(|e| e.to_string())?
            {
                if database_folder_exists(mount, mp, &resolved_source.relative_path)
                    .map_err(|e| e.to_string())?
                {
                    return Ok(DocEditToolResult::fail(format!(
                        "Source path is a folder, not a file: {source_path}."
                    )));
                }
                return Ok(DocEditToolResult::fail(format!(
                    "Source file not found: {source_path}"
                )));
            }
        }
    } else {
        return Err(fs_seam());
    }

    // Refuse to overwrite — match doc_move_file semantics.
    if resolved_dest.mount_type.as_deref() == Some("database") {
        if let Some(mp) = resolved_dest.mount_point_id.as_deref() {
            if database_document_exists(mount, mp, &resolved_dest.relative_path)
                .map_err(|e| e.to_string())?
            {
                return Ok(DocEditToolResult::fail(format!(
                    "Destination already exists: {final_dest_rel}. Copy will not overwrite existing files; delete it first if you want to replace it."
                )));
            }
            if database_folder_exists(mount, mp, &resolved_dest.relative_path)
                .map_err(|e| e.to_string())?
            {
                return Ok(DocEditToolResult::fail(format!(
                    "Destination path {final_dest_rel} resolves to a folder, not a file location."
                )));
            }
        }
    } else {
        return Err(fs_seam());
    }

    // Read the source content, write the destination.
    let content = read_file_with_mtime(mount, &resolved_source)?.content;
    let mtime = write_file_with_mtime_check(mount, &resolved_dest, &content)?;
    // triggerReindexIfNeeded: no-op seam (see the module header).

    let copied_uri = uri_for(main, mount, &resolved_dest, ctx);
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
        source_uri: uri_for(main, mount, &resolved_source, ctx),
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
    let resolved = resolve_doc_edit_path(main, mount, scope, Some(&path), &write_context)
        .map_err(resolve_error_message)?;
    assert_character_may_write(mount, &resolved, ctx)?;

    // Capture the read-policy BEFORE the delete removes the link row, so a
    // character_read:false document isn't announced to characters (operator path).
    let hidden_from_characters = document_hidden_from_characters(
        mount,
        resolved.mount_point_id.as_deref(),
        &resolved.relative_path,
    );

    if resolved.mount_type.as_deref() == Some("database") {
        if let Some(mp) = resolved.mount_point_id.as_deref() {
            let deleted =
                database_store::delete_database_document(mount, mp, &resolved.relative_path)
                    .map_err(|e| e.to_string())?;
            if !deleted {
                if database_folder_exists(mount, mp, &resolved.relative_path)
                    .map_err(|e| e.to_string())?
                {
                    return Ok(DocEditToolResult::fail(format!(
                        "Path is a folder, not a file: {path}. Use doc_delete_folder to remove folders."
                    )));
                }
                return Ok(DocEditToolResult::fail(format!("File not found: {path}")));
            }
            let deleted_uri = uri_for(main, mount, &resolved, ctx);
            let announcement = LibrarianDeleteAnnouncement {
                chat_id: ctx.chat_id.clone(),
                display_title: basename(&path).to_string(),
                file_path: path.clone(),
                scope: librarian_scope_from(scope),
                mount_point: addressing.mount_point.clone(),
                origin: resolve_actor_origin(main, ctx),
                hidden_from_characters,
            };
            return Ok(DocEditToolResult::ok(
                json!({ "success": true, "path": path, "uri": deleted_uri }),
                format!("Deleted file: {path}"),
            )
            .with_librarian_announcement(announcement));
        }
    }

    Err(fs_seam())
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
    let resolved = resolve_doc_edit_path(main, mount, scope, Some(&path), &write_context)
        .map_err(resolve_error_message)?;

    if resolved.mount_type.as_deref() == Some("database") {
        if let Some(mp) = resolved.mount_point_id.as_deref() {
            // v4 wraps createDatabaseFolder in a try/catch, returning the raw store
            // error message on failure.
            match database_store::create_database_folder(mount, mp, &resolved.relative_path) {
                Ok(_) => {
                    let uri = uri_for(main, mount, &resolved, ctx);
                    let announcement = LibrarianFolderCreatedAnnouncement {
                        chat_id: ctx.chat_id.clone(),
                        folder_path: path.clone(),
                        scope: librarian_scope_from(scope),
                        mount_point: addressing.mount_point.clone(),
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

    Err(fs_seam())
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
    let resolved = resolve_doc_edit_path(main, mount, scope, Some(&path), &write_context)
        .map_err(resolve_error_message)?;
    // A character may not delete a folder that holds a protected document.
    assert_folder_has_no_write_protected_descendants(mount, &resolved, ctx)?;

    if resolved.mount_type.as_deref() == Some("database") {
        if let Some(mp) = resolved.mount_point_id.as_deref() {
            match database_store::delete_database_folder(mount, mp, &resolved.relative_path) {
                Ok(()) => {
                    let uri = uri_for(main, mount, &resolved, ctx);
                    let announcement = LibrarianFolderDeletedAnnouncement {
                        chat_id: ctx.chat_id.clone(),
                        folder_path: path.clone(),
                        scope: librarian_scope_from(scope),
                        mount_point: addressing.mount_point.clone(),
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

    Err(fs_seam())
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
    let resolved_source = resolve_doc_edit_path(main, mount, scope, Some(&path), &write_context)
        .map_err(resolve_error_message)?;
    let resolved_dest =
        resolve_doc_edit_path(main, mount, scope, new_path.as_deref(), &write_context)
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
            ) {
                Ok(()) => {
                    // syncChatDocumentsAfterFolderMove: no-op seam (see module header).
                    let moved_uri = uri_for(main, mount, &resolved_dest, ctx);
                    let np = new_path.clone().unwrap_or_default();
                    // v4's folder-move site passes NO `hiddenFromCharacters`
                    // (→ falsy); the file-move site does. Match: `false` here.
                    let announcement = LibrarianMoveAnnouncement {
                        chat_id: ctx.chat_id.clone(),
                        old_display_title: basename(&path).to_string(),
                        new_display_title: basename(&np).to_string(),
                        old_uri: uri_for(main, mount, &resolved_source, ctx),
                        new_uri: moved_uri.clone(),
                        scope: librarian_scope_from(scope),
                        mount_point: addressing.mount_point.clone(),
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

    Err(fs_seam())
}

// ============================================================================
// Helpers
// ============================================================================

/// The FsSeam message a non-database mount surfaces (mirrors
/// [`super::shared::read_file_with_mtime`]).
fn fs_seam() -> String {
    "This location is served from the host filesystem, which is unavailable here.".to_string()
}
