//! Markdown-specific doc-edit handlers — v4
//! `lib/tools/handlers/doc-edit/markdown-handlers.ts`: `doc_read_frontmatter`,
//! `doc_update_frontmatter`, `doc_read_heading`, `doc_update_heading`.
//!
//! Each write path's Librarian announcement + reindex are the documented no-op
//! seams (see [`super::shared`]).

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::shared::{
    apply_qtap_uri, arg_bool, arg_i64, arg_str, arg_str_ref, assert_character_may_read,
    assert_character_may_write, basename, build_read_resolution_context,
    build_write_resolution_context, is_text_file, librarian_scope_from, read_file_with_mtime,
    resolve_actor_origin, resolve_error_message, scope_from_str, write_file_with_mtime_check,
    Addressing, DocEditToolContext,
};
use super::DocEditToolResult;
use crate::doc_edit::markdown_parser::{
    find_heading_section, read_heading_content, replace_heading_content,
    update_frontmatter_in_content,
};
use crate::doc_edit::path_resolver::resolve_doc_edit_path;
use crate::doc_edit::unified_diff::generate_unified_diff;
use crate::doc_edit::uri_producers::uri_for_resolved_path;
use crate::doc_edit::DocEditScope;
use crate::markdown::parse_frontmatter;
use crate::services::librarian_notifications::{
    content_hidden_from_characters, LibrarianWriteAnnouncement, LibrarianWriteChange,
};

// --- doc_read_frontmatter ---

pub fn handle_read_frontmatter(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = addressing(args)?;
    let Some(path) = addressing.path.clone() else {
        return Ok(DocEditToolResult::fail("A `path` or a `uri` is required."));
    };
    let scope = scope_from_str(addressing.scope.as_deref(), DocEditScope::DocumentStore);
    let read_context = build_read_resolution_context(main, &addressing, ctx);
    let resolved = resolve_doc_edit_path(
        main,
        mount,
        scope,
        Some(&path),
        &read_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;
    assert_character_may_read(mount, &resolved, ctx)?;

    if !is_text_file(&resolved.relative_path) {
        return Ok(DocEditToolResult::fail(format!(
            "File is not a supported text format: {path}"
        )));
    }

    let uri = uri_for_resolved_path(
        main,
        mount,
        &resolved,
        ctx.character_id.as_deref(),
        None,
        None,
    );
    let content = read_file_with_mtime(mount, &resolved)?.content;
    let parsed = parse_frontmatter(&content);

    let Some(Value::Object(data)) = parsed.data else {
        return Ok(DocEditToolResult::ok(
            json!({ "frontmatter": Value::Null, "path": path, "uri": uri }),
            format!("No frontmatter found in {path}"),
        ));
    };

    // Filter to specific keys if requested (preserving the requested-key order).
    let frontmatter: Map<String, Value> = match args.get("keys").and_then(Value::as_array) {
        Some(keys) if !keys.is_empty() => {
            let mut filtered = Map::new();
            for key in keys {
                if let Some(k) = key.as_str() {
                    if let Some(v) = data.get(k) {
                        filtered.insert(k.to_string(), v.clone());
                    }
                }
            }
            filtered
        }
        _ => data,
    };

    let fm = Value::Object(frontmatter);
    let formatted = format!(
        "Frontmatter from {path}:\n\n{}",
        serde_json::to_string_pretty(&fm).unwrap_or_default()
    );
    Ok(DocEditToolResult::ok(
        json!({ "frontmatter": fm, "path": path, "uri": uri }),
        formatted,
    ))
}

// --- doc_update_frontmatter ---

pub fn handle_update_frontmatter(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = addressing(args)?;
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

    if !is_text_file(&resolved.relative_path) {
        return Ok(DocEditToolResult::fail(format!(
            "File is not a supported text format: {path}"
        )));
    }

    let content = read_file_with_mtime(mount, &resolved)?.content;
    let updates: Map<String, Value> = args
        .get("updates")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let replace_all = arg_bool(args, "replace_all").unwrap_or(false);
    let new_content = update_frontmatter_in_content(&content, &updates, replace_all);
    let mtime = write_file_with_mtime_check(mount, &resolved, &new_content)?;
    // reindex: no-op seam.
    let edited_uri = uri_for_resolved_path(
        main,
        mount,
        &resolved,
        ctx.character_id.as_deref(),
        None,
        None,
    );

    let display_title = basename(&path).to_string();
    let announcement = LibrarianWriteAnnouncement {
        chat_id: ctx.chat_id.clone(),
        uri: edited_uri.clone(),
        scope: librarian_scope_from(scope),
        mount_point: addressing.mount_point.clone(),
        origin: resolve_actor_origin(main, ctx),
        change: LibrarianWriteChange::Edited {
            diff: generate_unified_diff(&content, &new_content, &display_title),
        },
        display_title,
        hidden_from_characters: content_hidden_from_characters(&new_content),
    };

    let result = json!({
        "success": true,
        "path": path,
        "uri": edited_uri,
        "mtime": mtime,
    });
    let action = if replace_all { "Replaced" } else { "Updated" };
    let key_count = updates.len();
    let formatted = format!("{action} frontmatter in {path} ({key_count} keys, mtime: {mtime})");
    Ok(DocEditToolResult::ok(result, formatted).with_librarian_announcement(announcement))
}

// --- doc_read_heading ---

pub fn handle_read_heading(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = addressing(args)?;
    let Some(path) = addressing.path.clone() else {
        return Ok(DocEditToolResult::fail("A `path` or a `uri` is required."));
    };
    let Some(heading_arg) = heading_arg(args, &addressing) else {
        return Ok(DocEditToolResult::fail(
            "A `heading` is required (pass it directly or as a qtap:// fragment, e.g. \"...#Childhood\").",
        ));
    };
    let level_arg = level_arg(args, &addressing);
    let scope = scope_from_str(addressing.scope.as_deref(), DocEditScope::DocumentStore);
    let read_context = build_read_resolution_context(main, &addressing, ctx);
    let resolved = resolve_doc_edit_path(
        main,
        mount,
        scope,
        Some(&path),
        &read_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;
    assert_character_may_read(mount, &resolved, ctx)?;

    if !is_text_file(&resolved.relative_path) {
        return Ok(DocEditToolResult::fail(format!(
            "File is not a supported text format: {path}"
        )));
    }

    let uri = uri_for_resolved_path(
        main,
        mount,
        &resolved,
        ctx.character_id.as_deref(),
        None,
        None,
    );
    let content = read_file_with_mtime(mount, &resolved)?.content;

    // v4 wraps this in a try/catch returning `{ success:false, error }` (no
    // formattedText) — NOT the dispatcher catch.
    match find_heading_section(&content, &heading_arg, level_arg) {
        Ok(heading) => {
            let section = read_heading_content(&content, &heading);
            let result = json!({
                "content": section,
                "heading": heading.text,
                "level": heading.level,
                "path": path,
                "uri": uri,
            });
            let formatted = format!(
                "Content under \"{} {}\" in {path}:\n\n{section}",
                "#".repeat(heading.level),
                heading.text
            );
            Ok(DocEditToolResult::ok(result, formatted))
        }
        Err(e) => Ok(DocEditToolResult::fail(e.0)),
    }
}

// --- doc_update_heading ---

pub fn handle_update_heading(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = addressing(args)?;
    let Some(path) = addressing.path.clone() else {
        return Ok(DocEditToolResult::fail("A `path` or a `uri` is required."));
    };
    let Some(heading_arg) = heading_arg(args, &addressing) else {
        return Ok(DocEditToolResult::fail(
            "A `heading` is required (pass it directly or as a qtap:// fragment, e.g. \"...#Childhood\").",
        ));
    };
    let level_arg = level_arg(args, &addressing);
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

    if !is_text_file(&resolved.relative_path) {
        return Ok(DocEditToolResult::fail(format!(
            "File is not a supported text format: {path}"
        )));
    }

    let content = read_file_with_mtime(mount, &resolved)?.content;
    let new_section = arg_str(args, "content").unwrap_or_default();
    let preserve = arg_bool(args, "preserve_subheadings") != Some(false);

    match find_heading_section(&content, &heading_arg, level_arg) {
        Ok(heading) => {
            let new_content = replace_heading_content(&content, &heading, &new_section, preserve);
            let mtime = write_file_with_mtime_check(mount, &resolved, &new_content)?;
            // reindex: no-op seam.
            let edited_uri = uri_for_resolved_path(
                main,
                mount,
                &resolved,
                ctx.character_id.as_deref(),
                None,
                None,
            );
            let display_title = basename(&path).to_string();
            let announcement = LibrarianWriteAnnouncement {
                chat_id: ctx.chat_id.clone(),
                uri: edited_uri.clone(),
                scope: librarian_scope_from(scope),
                mount_point: addressing.mount_point.clone(),
                origin: resolve_actor_origin(main, ctx),
                change: LibrarianWriteChange::Edited {
                    diff: generate_unified_diff(&content, &new_content, &display_title),
                },
                display_title,
                hidden_from_characters: content_hidden_from_characters(&new_content),
            };
            let result = json!({
                "success": true,
                "path": path,
                "uri": edited_uri,
                "mtime": mtime,
            });
            let formatted = format!(
                "Updated content under \"{} {}\" in {path} (mtime: {mtime}). Note: your previous read of this file is now stale.",
                "#".repeat(heading.level),
                heading.text
            );
            Ok(DocEditToolResult::ok(result, formatted).with_librarian_announcement(announcement))
        }
        Err(e) => Ok(DocEditToolResult::fail(e.0)),
    }
}

// ---- shared arg helpers ----

/// Apply a `qtap://` URI to the addressing fields (the shared first step of every
/// markdown handler).
fn addressing(args: &Value) -> Result<Addressing, String> {
    apply_qtap_uri(
        arg_str(args, "scope"),
        arg_str(args, "mount_point"),
        arg_str(args, "path"),
        arg_str_ref(args, "uri"),
    )
}

/// v4's `input.heading ?? input.uriHeading` — an explicit heading overrides a
/// URI-fragment heading.
fn heading_arg(args: &Value, addressing: &Addressing) -> Option<String> {
    arg_str(args, "heading").or_else(|| addressing.uri_heading.clone())
}

/// v4's `input.level ?? input.uriLevel` — an explicit level overrides the
/// fragment level; returned as `Option<usize>` for `find_heading_section`.
fn level_arg(args: &Value, addressing: &Addressing) -> Option<usize> {
    arg_i64(args, "level")
        .map(|l| l as usize)
        .or_else(|| addressing.uri_level.map(usize::from))
}
