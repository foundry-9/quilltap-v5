//! Text-file doc-edit handlers — v4 `lib/tools/handlers/doc-edit/text-handlers.ts`:
//! `doc_read_file`, `doc_write_file`, `doc_str_replace`, `doc_insert_text`,
//! `doc_grep`, `doc_list_files`.
//!
//! Every write path's Librarian announcement + `triggerReindexIfNeeded` are the
//! documented no-op seams (see [`super::shared`]); the differential oracle mocks
//! them. The non-text blob `extractedText` read branch (a converted pdf/docx) is a
//! tracked deferral — the corpus reads only text files, so v4's fallback error is
//! reproduced directly.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::shared::{
    apply_qtap_uri, arg_bool, arg_i64, arg_str, arg_str_ref, assert_character_may_read,
    assert_character_may_write, basename, build_read_resolution_context,
    build_write_resolution_context, collect_peer_character_ids_for_reads,
    get_accessible_mount_points, get_character_blocked_read_paths, is_text_file,
    librarian_scope_from, read_file_with_mtime, resolve_actor_origin, resolve_error_message,
    resolve_official_project_mount, scope_from_str, write_file_with_mtime_check,
    DocEditToolContext,
};
use super::DocEditToolResult;
use crate::db::database_store::list_database_files;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::tiered_mount_pool::resolve_group_mount_point_ids_for_character;
use crate::doc_edit::diacritics::{
    find_all_matches, find_unique_match, DiacriticsMatchOptions, UniqueMatch,
};
use crate::doc_edit::mime_registry::{
    detect_mime_from_extension, is_json_family, is_jsonl_mime, parse_content, serialize_content,
    validate_json, JsonlLineResult, ParseResult,
};
use crate::doc_edit::path_resolver::{resolve_doc_edit_path, resolve_mount_point_ref};
use crate::doc_edit::qtap_uri::ScopedAuthority;
use crate::doc_edit::unified_diff::generate_unified_diff;
use crate::doc_edit::uri_producers::{uri_for_resolved_path, DocStoreUriResolver};
use crate::doc_edit::DocEditScope;
use crate::folder_utils::{is_automatic_image_path, is_os_cruft_name};
use crate::services::librarian_notifications::{
    content_hidden_from_characters, LibrarianWriteAnnouncement, LibrarianWriteChange,
};

/// The 1-based line number for a UTF-16 code-unit offset in `content` — v4
/// `getLineNumber` (`text-handlers.ts:66`), which iterates JS string indices
/// (UTF-16 units) up to `offset`.
fn get_line_number(content: &str, offset: usize) -> usize {
    content
        .encode_utf16()
        .take(offset)
        .filter(|&u| u == 0x0A)
        .count()
        + 1
}

/// JS `String.prototype.substring(start, end)` over UTF-16 code units. Boundaries
/// are clamped to `[0, len]` and ordered; match boundaries always fall on whole
/// code points here (the needle is whole code points), so no surrogate splits.
fn utf16_substring(s: &str, start: usize, end: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    let start = start.min(units.len());
    let end = end.clamp(start, units.len());
    String::from_utf16_lossy(&units[start..end])
}

/// JS `str.substring(0, n)` (first `n` UTF-16 units) — used for anchor previews.
fn utf16_prefix(s: &str, n: usize) -> String {
    utf16_substring(s, 0, n)
}

// --- doc_read_file ---

pub fn handle_read_file(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = apply_qtap_uri(
        arg_str(args, "scope"),
        arg_str(args, "mount_point"),
        arg_str(args, "path"),
        arg_str_ref(args, "uri"),
    )?;
    let Some(path) = addressing.path.clone() else {
        return Ok(DocEditToolResult::fail("A `path` or a `uri` is required."));
    };
    let scope = scope_from_str(addressing.scope.as_deref(), DocEditScope::DocumentStore);
    let read_context = build_read_resolution_context(main, &addressing, ctx);
    let resolved = resolve_doc_edit_path(main, mount, scope, Some(&path), &read_context)
        .map_err(resolve_error_message)?;
    assert_character_may_read(mount, &resolved, ctx)?;
    let uri = uri_for_resolved_path(
        main,
        mount,
        &resolved,
        ctx.character_id.as_deref(),
        None,
        None,
    );

    if !is_text_file(&resolved.relative_path) {
        // The converted-blob `extractedText` read branch is a tracked deferral;
        // v4's fallback (no converted blob) is this error.
        return Ok(DocEditToolResult::fail(format!(
            "File is not a supported text format: {path}"
        )));
    }

    let doc = read_file_with_mtime(mount, &resolved)?;
    let raw_content = doc.content;
    let mtime = doc.mtime_ms;
    let size = doc.size;
    let mime = detect_mime_from_extension(&resolved.relative_path);

    let offset = arg_i64(args, "offset");
    let limit = arg_i64(args, "limit");

    let mut result = Map::new();
    result.insert("path".into(), json!(path));
    result.insert("uri".into(), json!(uri));
    result.insert("mtime".into(), json!(mtime));
    // totalLines is always the FULL line count (result.truncated is always false;
    // only formattedText reflects the slice).
    let total_lines = raw_content.split('\n').count();
    result.insert("totalLines".into(), json!(total_lines));
    result.insert("truncated".into(), json!(false));
    if let Some(m) = mime {
        result.insert("mimeType".into(), json!(m));
    }

    let formatted_text: String;
    if is_json_family(mime) {
        let mime = mime.unwrap();
        result.insert("rawContent".into(), json!(raw_content));
        match parse_content(&raw_content, mime) {
            ParseResult::Ok(value) => {
                if is_jsonl_mime(Some(mime)) {
                    let lines = jsonl_lines(&value);
                    let parsed = lines.iter().any(|l| l.error.is_none());
                    let formatted_lines: Vec<String> = lines
                        .iter()
                        .map(|l| match &l.error {
                            Some(err) => format!("[L{}] PARSE ERROR: {err}", l.line),
                            None => format!(
                                "[L{}] {}",
                                l.line,
                                serde_json::to_string(l.value.as_ref().unwrap_or(&Value::Null))
                                    .unwrap_or_default()
                            ),
                        })
                        .collect();
                    formatted_text = format!(
                        "File: {path} (JSONL, {} entries)\n\n{}",
                        lines.len(),
                        formatted_lines.join("\n")
                    );
                    result.insert("content".into(), value);
                    result.insert("parsed".into(), json!(parsed));
                } else {
                    formatted_text = format!(
                        "File: {path} (JSON)\n\n{}",
                        serde_json::to_string_pretty(&value).unwrap_or_default()
                    );
                    result.insert("content".into(), value);
                    result.insert("parsed".into(), json!(true));
                }
            }
            ParseResult::Err { error, line } => {
                result.insert("content".into(), json!(raw_content));
                result.insert("parsed".into(), json!(false));
                let mut pe = Map::new();
                pe.insert("message".into(), json!(error));
                if let Some(l) = line {
                    pe.insert("line".into(), json!(l));
                }
                result.insert("parseError".into(), Value::Object(pe));
                formatted_text = format!("File: {path} — Parse Error: {error}");
            }
        }
    } else {
        let lines: Vec<&str> = raw_content.split('\n').collect();
        let (output_lines, truncated): (Vec<&str>, bool) = if offset.is_some() || limit.is_some() {
            let start_line = (offset.unwrap_or(1) - 1).max(0) as usize;
            let end_line = match limit {
                Some(l) => start_line.saturating_add(l.max(0) as usize),
                None => lines.len(),
            };
            let s = start_line.min(lines.len());
            let e = end_line.min(lines.len()).max(s);
            (lines[s..e].to_vec(), end_line < lines.len())
        } else {
            (lines.clone(), false)
        };
        let content = output_lines.join("\n");
        let line_start = offset.unwrap_or(1);
        let numbered: Vec<String> = output_lines
            .iter()
            .enumerate()
            .map(|(i, l)| format!("[L{}] {l}", line_start + i as i64))
            .collect();
        let header = format!("File: {path} ({total_lines} lines, {size} bytes)");
        let trunc_msg = if truncated {
            format!(
                "\n[Truncated — showing lines {line_start}-{} of {total_lines}]",
                line_start + output_lines.len() as i64 - 1
            )
        } else {
            String::new()
        };
        formatted_text = format!("{header}{trunc_msg}\n\n{}", numbered.join("\n"));
        result.insert("content".into(), json!(content));
    }

    Ok(DocEditToolResult::ok(Value::Object(result), formatted_text))
}

/// Interpret the `parse_content` JSONL result (an array of line-result objects)
/// as [`JsonlLineResult`]s for display.
fn jsonl_lines(value: &Value) -> Vec<JsonlLineResult> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|o| JsonlLineResult {
                    line: o.get("line").and_then(Value::as_u64).unwrap_or(0) as usize,
                    value: o.get("value").cloned(),
                    error: o.get("error").and_then(Value::as_str).map(str::to_string),
                    raw: o
                        .get("raw")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

// --- doc_write_file ---

pub fn handle_write_file(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = apply_qtap_uri(
        arg_str(args, "scope"),
        arg_str(args, "mount_point"),
        arg_str(args, "path"),
        arg_str_ref(args, "uri"),
    )?;
    let Some(path) = addressing.path.clone() else {
        return Ok(DocEditToolResult::fail("A `path` or a `uri` is required."));
    };
    let scope = scope_from_str(addressing.scope.as_deref(), DocEditScope::DocumentStore);
    let write_context = build_write_resolution_context(main, mount, &addressing, ctx)?;
    let resolved = resolve_doc_edit_path(main, mount, scope, Some(&path), &write_context)
        .map_err(resolve_error_message)?;
    assert_character_may_write(mount, &resolved, ctx)?;

    if !is_text_file(&resolved.relative_path) {
        return Ok(DocEditToolResult::fail(format!(
            "File is not a supported text format: {path}"
        )));
    }

    let mime = detect_mime_from_extension(&resolved.relative_path);
    let content_to_write: String;
    if is_json_family(mime) {
        let mime = mime.unwrap();
        match args.get("content") {
            Some(Value::String(s)) => {
                if let ParseResult::Err { error, .. } = validate_json(s, mime) {
                    let msg = if is_jsonl_mime(Some(mime)) {
                        format!("Invalid JSONL: {error}")
                    } else {
                        format!("Invalid JSON: {error}")
                    };
                    return Ok(DocEditToolResult::fail(msg));
                }
                content_to_write = s.clone();
            }
            Some(value) => match serialize_content(value, mime, true) {
                ParseResult::Ok(v) => {
                    content_to_write = v.as_str().unwrap_or_default().to_string();
                }
                ParseResult::Err { error, .. } => {
                    return Ok(DocEditToolResult::fail(format!(
                        "Cannot serialize content: {error}"
                    )));
                }
            },
            None => match serialize_content(&Value::Null, mime, true) {
                ParseResult::Ok(v) => content_to_write = v.as_str().unwrap_or_default().to_string(),
                ParseResult::Err { error, .. } => {
                    return Ok(DocEditToolResult::fail(format!(
                        "Cannot serialize content: {error}"
                    )));
                }
            },
        }
    } else {
        match args.get("content") {
            Some(Value::String(s)) => content_to_write = s.clone(),
            other => {
                let ty = js_typeof(other);
                return Ok(DocEditToolResult::fail(format!(
                    "Non-JSON files require string content; got {ty}"
                )));
            }
        }
    }

    // Capture the pre-image before writing so the announcement shows a diff
    // (existing file) or reports the new contents (creation). An unreadable/absent
    // pre-image means this is a creation (v4's best-effort try/catch → null).
    let previous_content: Option<String> = read_file_with_mtime(mount, &resolved)
        .ok()
        .map(|d| d.content);

    let mtime = write_file_with_mtime_check(mount, &resolved, &content_to_write)?;
    // triggerReindexIfNeeded: no-op seam.
    let written_uri = uri_for_resolved_path(
        main,
        mount,
        &resolved,
        ctx.character_id.as_deref(),
        None,
        None,
    );

    let display_title = basename(&path).to_string();
    let change = match &previous_content {
        None => LibrarianWriteChange::Created {
            body: content_to_write.clone(),
        },
        Some(prev) => LibrarianWriteChange::Edited {
            diff: generate_unified_diff(prev, &content_to_write, &display_title),
        },
    };
    let announcement = LibrarianWriteAnnouncement {
        chat_id: ctx.chat_id.clone(),
        display_title,
        uri: written_uri.clone(),
        scope: librarian_scope_from(scope),
        mount_point: addressing.mount_point.clone(),
        origin: resolve_actor_origin(main, ctx),
        change,
        hidden_from_characters: content_hidden_from_characters(&content_to_write),
    };

    let result = json!({
        "success": true,
        "path": path,
        "uri": written_uri,
        "mtime": mtime,
    });
    let formatted = format!(
        "File written: {path} ({} bytes, mtime: {mtime})",
        content_to_write.encode_utf16().count()
    );
    Ok(DocEditToolResult::ok(result, formatted).with_librarian_announcement(announcement))
}

/// JS `typeof value` for the non-string-content error (`object`/`number`/…).
fn js_typeof(v: Option<&Value>) -> &'static str {
    match v {
        None => "undefined",
        Some(Value::Null) => "object",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) | Some(Value::Object(_)) => "object",
    }
}

// --- doc_str_replace ---

pub fn handle_str_replace(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = apply_qtap_uri(
        arg_str(args, "scope"),
        arg_str(args, "mount_point"),
        arg_str(args, "path"),
        arg_str_ref(args, "uri"),
    )?;
    let Some(path) = addressing.path.clone() else {
        return Ok(DocEditToolResult::fail("A `path` or a `uri` is required."));
    };
    let scope = scope_from_str(addressing.scope.as_deref(), DocEditScope::DocumentStore);
    let write_context = build_write_resolution_context(main, mount, &addressing, ctx)?;
    let resolved = resolve_doc_edit_path(main, mount, scope, Some(&path), &write_context)
        .map_err(resolve_error_message)?;
    assert_character_may_write(mount, &resolved, ctx)?;

    if !is_text_file(&resolved.relative_path) {
        return Ok(DocEditToolResult::fail(format!(
            "File is not a supported text format: {path}"
        )));
    }

    let content = read_file_with_mtime(mount, &resolved)?.content;
    let find = arg_str(args, "find").unwrap_or_default();
    let replace = arg_str(args, "replace").unwrap_or_default();
    let options = DiacriticsMatchOptions {
        case_sensitive: arg_bool(args, "case_sensitive") != Some(false),
        normalize_diacritics: arg_bool(args, "normalize_diacritics") != Some(false),
    };
    let (index, length) = match find_unique_match(&content, &find, options) {
        UniqueMatch::Found { index, length } => (index, length),
        UniqueMatch::NotFound { count } => {
            if count == 0 {
                return Ok(DocEditToolResult::fail_with_formatted(
                    format!("Text not found in file. The exact text to find was not present in {path}. Make sure you are using the exact text from your most recent read of this file."),
                    format!("Error: Text not found in {path}. No matches for the find text. Re-read the file and use the exact text."),
                ));
            }
            return Ok(DocEditToolResult::fail_with_formatted(
                format!("Multiple matches ({count}) found in file. Include more surrounding context in the find text to make it unique."),
                format!("Error: {count} matches found in {path}. Include more surrounding context to make the match unique."),
            ));
        }
    };

    let new_content = format!(
        "{}{}{}",
        utf16_substring(&content, 0, index),
        replace,
        utf16_substring(&content, index + length, content.encode_utf16().count())
    );
    let mtime = write_file_with_mtime_check(mount, &resolved, &new_content)?;
    let line_number = get_line_number(&content, index);
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
        "line_number": line_number,
    });
    let formatted = format!(
        "Replaced text at line {line_number} in {path} (mtime: {mtime}). Note: your previous read of this file is now stale — re-read before making further edits."
    );
    Ok(DocEditToolResult::ok(result, formatted).with_librarian_announcement(announcement))
}

// --- doc_insert_text ---

pub fn handle_insert_text(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = apply_qtap_uri(
        arg_str(args, "scope"),
        arg_str(args, "mount_point"),
        arg_str(args, "path"),
        arg_str_ref(args, "uri"),
    )?;
    let Some(path) = addressing.path.clone() else {
        return Ok(DocEditToolResult::fail("A `path` or a `uri` is required."));
    };
    let scope = scope_from_str(addressing.scope.as_deref(), DocEditScope::DocumentStore);
    let write_context = build_write_resolution_context(main, mount, &addressing, ctx)?;
    let resolved = resolve_doc_edit_path(main, mount, scope, Some(&path), &write_context)
        .map_err(resolve_error_message)?;
    assert_character_may_write(mount, &resolved, ctx)?;

    if !is_text_file(&resolved.relative_path) {
        return Ok(DocEditToolResult::fail(format!(
            "File is not a supported text format: {path}"
        )));
    }

    let content = read_file_with_mtime(mount, &resolved)?.content;
    let insert_content = arg_str(args, "content").unwrap_or_default();
    let position = args.get("position").cloned().unwrap_or(Value::Null);
    let at = position.get("at").and_then(Value::as_str);
    let content_len = content.encode_utf16().count();

    let (insert_offset, description): (usize, String) = match at {
        Some("start") => (0, "start of file".to_string()),
        Some("end") => (content_len, "end of file".to_string()),
        _ => {
            let before = position.get("before").and_then(Value::as_str);
            let after = position.get("after").and_then(Value::as_str);
            let Some(anchor) = before.or(after) else {
                return Ok(DocEditToolResult::fail(
                    "Position must specify before, after, or at",
                ));
            };
            let options = DiacriticsMatchOptions {
                case_sensitive: true,
                normalize_diacritics: arg_bool(args, "normalize_diacritics") != Some(false),
            };
            let (index, length) = match find_unique_match(&content, anchor, options) {
                UniqueMatch::Found { index, length } => (index, length),
                UniqueMatch::NotFound { count } => {
                    if count == 0 {
                        return Ok(DocEditToolResult::fail(
                            "Anchor text not found in file. Make sure you are using exact text from your most recent read.",
                        ));
                    }
                    return Ok(DocEditToolResult::fail(format!(
                        "Multiple matches ({count}) for anchor text. Include more context to make it unique."
                    )));
                }
            };
            let anchor_u16 = anchor.encode_utf16().count();
            let ellipsis = if anchor_u16 > 40 { "..." } else { "" };
            let preview = utf16_prefix(anchor, 40);
            if before.is_some() {
                (index, format!("before \"{preview}{ellipsis}\""))
            } else {
                (index + length, format!("after \"{preview}{ellipsis}\""))
            }
        }
    };

    let new_content = format!(
        "{}{}{}",
        utf16_substring(&content, 0, insert_offset),
        insert_content,
        utf16_substring(&content, insert_offset, content_len)
    );
    let mtime = write_file_with_mtime_check(mount, &resolved, &new_content)?;
    let line_number = get_line_number(&new_content, insert_offset);
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
        "line_number": line_number,
    });
    let formatted = format!(
        "Inserted text at {description} (line {line_number}) in {path} (mtime: {mtime}). Note: your previous read of this file is now stale."
    );
    Ok(DocEditToolResult::ok(result, formatted).with_librarian_announcement(announcement))
}

// --- doc_grep ---
//
// Only database-backed stores are searched (the filesystem `walkAndSearch` and
// the legacy-project fs fallback are the FsSeam — every corpus store is
// database-backed, and an official project mount is already enumerated by
// `get_accessible_mount_points`, so the fs branch never runs). `is_regex` uses
// the Rust `regex` crate (JS-only regex features are a documented seam); the
// default (diacritics, case-insensitive) path is byte-faithful.

/// The compiled grep matcher: v4's diacritics path (`findAllMatches`) vs its
/// per-line RegExp path.
enum GrepMatcher {
    Diacritics { query: String, case_sensitive: bool },
    Regex(regex::Regex),
}

pub fn handle_grep(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let Some(project_id) = ctx.project_id.clone() else {
        return Ok(DocEditToolResult::fail("Grep requires a project context"));
    };
    let addressing = apply_qtap_uri(
        arg_str(args, "scope"),
        arg_str(args, "mount_point"),
        arg_str(args, "path"),
        arg_str_ref(args, "uri"),
    )?;
    let resolver = DocStoreUriResolver::build(main, mount, ctx.character_id.as_deref());

    let query = arg_str(args, "query").unwrap_or_default();
    let is_regex = arg_bool(args, "is_regex").unwrap_or(false);
    let case_sensitive = arg_bool(args, "case_sensitive"); // None = default
    let normalize = arg_bool(args, "normalize_diacritics") != Some(false);
    let max_results = arg_i64(args, "max_results").unwrap_or(100).clamp(0, 500) as usize;
    let context_lines = arg_i64(args, "context_lines").unwrap_or(0).max(0) as usize;
    let path_filter = addressing.path.clone();

    let matcher = if normalize && !is_regex {
        // v4: `caseSensitive: input.case_sensitive ?? false` (default insensitive).
        GrepMatcher::Diacritics {
            query: query.clone(),
            case_sensitive: case_sensitive.unwrap_or(false),
        }
    } else {
        // v4 builds `new RegExp(query|escaped, case_sensitive ? 'g' : 'gi')`.
        let pattern = if is_regex {
            query.clone()
        } else {
            regex::escape(&query)
        };
        let insensitive = case_sensitive != Some(true);
        match regex::RegexBuilder::new(&pattern)
            .case_insensitive(insensitive)
            .build()
        {
            Ok(re) => GrepMatcher::Regex(re),
            Err(e) => {
                return Ok(DocEditToolResult::fail(format!(
                    "Invalid regex pattern: {e}"
                )))
            }
        }
    };

    let mount_point_filter = addressing
        .mount_point
        .as_deref()
        .map(|mp| resolve_mount_point_ref(main, mp, ctx.character_id.as_deref()));
    let peer_ids = collect_peer_character_ids_for_reads(main, ctx);
    let mount_points = get_accessible_mount_points(
        main,
        mount,
        Some(&project_id),
        ctx.character_id.as_deref(),
        &peer_ids,
    );
    let docs_repo = DocMountDocumentsRepository::new(mount);

    let mut matches: Vec<Value> = Vec::new();
    for mp in &mount_points {
        if matches.len() >= max_results {
            break;
        }
        if let Some(f) = &mount_point_filter {
            if !mp.name.eq_ignore_ascii_case(f) && mp.id != *f {
                continue;
            }
        }
        if mp.mount_type != "database" {
            continue; // FsSeam
        }
        let blocked = get_character_blocked_read_paths(mount, &mp.id, ctx);
        let docs = docs_repo
            .find_all_by_mount_point_id(&mp.id)
            .map_err(|e| e.to_string())?;
        for (rel, content) in docs {
            if matches.len() >= max_results {
                break;
            }
            if let Some(p) = &path_filter {
                if !rel.starts_with(p) {
                    continue;
                }
            }
            if !blocked.is_empty() && blocked.contains(&rel.to_lowercase()) {
                continue;
            }
            let uri = resolver.uri_for_mount(&mp.name, &mp.id, &rel);
            grep_search_content(
                &content,
                &rel,
                &mp.name,
                &uri,
                &matcher,
                context_lines,
                max_results,
                &mut matches,
            );
        }
    }

    let total = matches.len();
    let result = json!({ "matches": matches, "total_matches": total });
    if total == 0 {
        return Ok(DocEditToolResult::ok(
            result,
            format!("No matches found for \"{query}\""),
        ));
    }
    let formatted_matches: Vec<String> = matches
        .iter()
        .map(|m| {
            let mp = m.get("mount_point").and_then(Value::as_str);
            let prefix = mp.map(|n| format!("[{n}] ")).unwrap_or_default();
            let path = m.get("path").and_then(Value::as_str).unwrap_or("");
            let line = m.get("line_number").and_then(Value::as_i64).unwrap_or(0);
            let text = m.get("match").and_then(Value::as_str).unwrap_or("");
            let mut out = format!("{prefix}{path}:{line}: {text}");
            if let Some(before) = m.get("context_before").and_then(Value::as_array) {
                if !before.is_empty() {
                    let cb: Vec<String> = before
                        .iter()
                        .map(|l| format!("  {}", l.as_str().unwrap_or("")))
                        .collect();
                    out = format!("{}\n{out}", cb.join("\n"));
                }
            }
            if let Some(after) = m.get("context_after").and_then(Value::as_array) {
                if !after.is_empty() {
                    let ca: Vec<String> = after
                        .iter()
                        .map(|l| format!("  {}", l.as_str().unwrap_or("")))
                        .collect();
                    out = format!("{out}\n{}", ca.join("\n"));
                }
            }
            out
        })
        .collect();
    let formatted = format!(
        "Found {total} matches for \"{query}\":\n\n{}",
        formatted_matches.join("\n\n")
    );
    Ok(DocEditToolResult::ok(result, formatted))
}

/// Append the grep matches from one document's `content` (v4's `searchContent`).
#[allow(clippy::too_many_arguments)]
fn grep_search_content(
    content: &str,
    display_path: &str,
    mount_point_name: &str,
    uri: &str,
    matcher: &GrepMatcher,
    context_lines: usize,
    max_results: usize,
    matches: &mut Vec<Value>,
) {
    if matches.len() >= max_results {
        return;
    }
    let lines: Vec<&str> = content.split('\n').collect();
    // The 0-based line index of each match, in order (diacritics: one per match;
    // regex: one per matching line).
    let line_idxs: Vec<usize> = match matcher {
        GrepMatcher::Diacritics {
            query,
            case_sensitive,
        } => find_all_matches(
            content,
            query,
            DiacriticsMatchOptions {
                case_sensitive: *case_sensitive,
                normalize_diacritics: true,
            },
        )
        .iter()
        .map(|(idx, _)| get_line_number(content, *idx) - 1)
        .collect(),
        GrepMatcher::Regex(re) => lines
            .iter()
            .enumerate()
            .filter(|(_, l)| re.is_match(l))
            .map(|(i, _)| i)
            .collect(),
    };
    for line_idx in line_idxs {
        if matches.len() >= max_results {
            break;
        }
        let mut m = Map::new();
        m.insert("path".into(), json!(display_path));
        m.insert("mount_point".into(), json!(mount_point_name));
        m.insert("uri".into(), json!(uri));
        m.insert("line_number".into(), json!(line_idx + 1));
        m.insert(
            "match".into(),
            json!(lines.get(line_idx).copied().unwrap_or("")),
        );
        if context_lines > 0 {
            let before: Vec<&str> =
                lines[line_idx.saturating_sub(context_lines)..line_idx].to_vec();
            let after_end = (line_idx + 1 + context_lines).min(lines.len());
            let after: Vec<&str> = lines[(line_idx + 1).min(lines.len())..after_end].to_vec();
            m.insert("context_before".into(), json!(before));
            m.insert("context_after".into(), json!(after));
        }
        matches.push(Value::Object(m));
    }
}

// --- doc_list_files ---
//
// Database-backed stores + the official project mount only (the fs `_general` /
// legacy-project walks are the FsSeam).

pub fn handle_list_files(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let Some(project_id) = ctx.project_id.clone() else {
        return Ok(DocEditToolResult::fail(
            "List files requires a project context",
        ));
    };
    // A qtap:// URI projects onto scope / mount_point / folder.
    let (scope, mount_point, folder) = if let Some(uri) = arg_str_ref(args, "uri") {
        let parts = crate::doc_edit::qtap_uri::parse_qtap_uri(uri).map_err(|e| e.message)?;
        let f = if parts.path.is_empty() {
            arg_str(args, "folder")
        } else {
            Some(parts.path)
        };
        (
            Some(parts.scope.as_str().to_string()),
            parts.mount_point.or_else(|| arg_str(args, "mount_point")),
            f,
        )
    } else {
        (
            arg_str(args, "scope"),
            arg_str(args, "mount_point"),
            arg_str(args, "folder"),
        )
    };
    let resolver = DocStoreUriResolver::build(main, mount, ctx.character_id.as_deref());
    let pattern = arg_str(args, "pattern");
    let include_auto = arg_bool(args, "includeAutomaticImages").unwrap_or(false);

    let include_doc_store = scope.is_none()
        || scope.as_deref() == Some("document_store")
        || scope.as_deref() == Some("group");
    let include_project = scope.is_none() || scope.as_deref() == Some("project");

    let group_mount_ids: std::collections::HashSet<String> = ctx
        .character_id
        .as_deref()
        .map(|cid| resolve_group_mount_point_ids_for_character(main, mount, cid))
        .unwrap_or_default()
        .into_iter()
        .collect();

    let mut files: Vec<Value> = Vec::new();

    if include_doc_store {
        let mount_point_filter = mount_point
            .as_deref()
            .map(|mp| resolve_mount_point_ref(main, mp, ctx.character_id.as_deref()));
        let peer_ids = collect_peer_character_ids_for_reads(main, ctx);
        let mount_points = get_accessible_mount_points(
            main,
            mount,
            Some(&project_id),
            ctx.character_id.as_deref(),
            &peer_ids,
        );
        for mp in &mount_points {
            if let Some(f) = &mount_point_filter {
                if !mp.name.eq_ignore_ascii_case(f) && mp.id != *f {
                    continue;
                }
            }
            let is_group = group_mount_ids.contains(&mp.id);
            if scope.as_deref() == Some("group") && !is_group {
                continue;
            }
            let mp_scope = if is_group { "group" } else { "document_store" };
            if mp.mount_type != "database" {
                continue; // FsSeam
            }
            let blocked = get_character_blocked_read_paths(mount, &mp.id, ctx);
            let entries =
                list_database_files(mount, &mp.id, folder.as_deref()).map_err(|e| e.to_string())?;
            for e in entries {
                if let Some(pat) = &pattern {
                    if !matches_glob(&e.file_name, pat) {
                        continue;
                    }
                }
                if e.kind != "folder"
                    && !blocked.is_empty()
                    && blocked.contains(&e.relative_path.to_lowercase())
                {
                    continue;
                }
                let uri = resolver.uri_for_mount(&mp.name, &mp.id, &e.relative_path);
                files.push(file_info(
                    &e.relative_path,
                    Some(&mp.name),
                    &uri,
                    mp_scope,
                    e.file_size_bytes,
                    crate::clock::iso_to_ms(&e.last_modified).unwrap_or(0),
                    &e.kind,
                ));
            }
        }
    }

    if include_project {
        if let Some(official) = resolve_official_project_mount(main, mount, Some(&project_id)) {
            // The document-store branch already enumerated this mount when listing
            // every scope; only re-emit when scope was explicitly 'project'.
            if scope.as_deref() == Some("project") && official.mount_type == "database" {
                let blocked = get_character_blocked_read_paths(mount, &official.id, ctx);
                let entries = list_database_files(mount, &official.id, folder.as_deref())
                    .map_err(|e| e.to_string())?;
                for e in entries {
                    if let Some(pat) = &pattern {
                        if !matches_glob(&e.file_name, pat) {
                            continue;
                        }
                    }
                    if e.kind != "folder"
                        && !blocked.is_empty()
                        && blocked.contains(&e.relative_path.to_lowercase())
                    {
                        continue;
                    }
                    let uri = resolver.uri_for_scope(ScopedAuthority::Project, &e.relative_path);
                    files.push(file_info(
                        &e.relative_path,
                        Some(&official.name),
                        &uri,
                        "project",
                        e.file_size_bytes,
                        crate::clock::iso_to_ms(&e.last_modified).unwrap_or(0),
                        &e.kind,
                    ));
                }
            }
        }
        // No official mount → the legacy fs project walk is the FsSeam (skipped).
    }
    // General scope → fs `_general` walk is the FsSeam (skipped).

    // Post-filter: strip OS cruft; strip auto-images unless opted in.
    files.retain(|f| {
        let path = f.get("path").and_then(Value::as_str).unwrap_or("");
        let base = path.rsplit('/').next().unwrap_or("");
        if is_os_cruft_name(base) {
            return false;
        }
        if !include_auto && is_automatic_image_path(path) {
            return false;
        }
        true
    });

    let total = files.len();
    let result = json!({ "files": files, "total": total });
    if total == 0 {
        return Ok(DocEditToolResult::ok(result, "No files found.".to_string()));
    }
    let formatted_lines: Vec<String> = files
        .iter()
        .map(|f| {
            let mp = f.get("mount_point").and_then(Value::as_str);
            let sc = f.get("scope").and_then(Value::as_str).unwrap_or("");
            let prefix = mp
                .map(|n| format!("[{n}] "))
                .unwrap_or_else(|| format!("[{sc}] "));
            let path = f.get("path").and_then(Value::as_str).unwrap_or("");
            if f.get("kind").and_then(Value::as_str) == Some("folder") {
                return format!("{prefix}[folder] {path}");
            }
            let size = f.get("size").and_then(Value::as_i64).unwrap_or(0);
            let size_str = if size < 1024 {
                format!("{size}B")
            } else if size < 1_048_576 {
                format!("{:.1}KB", size as f64 / 1024.0)
            } else {
                format!("{:.1}MB", size as f64 / 1_048_576.0)
            };
            format!("{prefix}{path}  ({size_str})")
        })
        .collect();
    let formatted = format!("{total} files:\n\n{}", formatted_lines.join("\n"));
    Ok(DocEditToolResult::ok(result, formatted))
}

/// v4's `matchesGlob`: `*.ext` suffix match, else exact filename match. An empty
/// pattern matches everything.
fn matches_glob(filename: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return filename.ends_with(suffix);
    }
    filename == pattern
}

/// Build a `DocFileInfo` object (v4's push shape) in schema order.
fn file_info(
    path: &str,
    mount_point: Option<&str>,
    uri: &str,
    scope: &str,
    size: i64,
    modified: i64,
    kind: &str,
) -> Value {
    let mut m = Map::new();
    m.insert("path".into(), json!(path));
    if let Some(mp) = mount_point {
        m.insert("mount_point".into(), json!(mp));
    }
    m.insert("uri".into(), json!(uri));
    m.insert("scope".into(), json!(scope));
    m.insert("size".into(), json!(size));
    m.insert("modified".into(), json!(modified));
    m.insert("kind".into(), json!(kind));
    Value::Object(m)
}
