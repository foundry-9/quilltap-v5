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
    assert_character_may_write, build_read_resolution_context, build_write_resolution_context,
    is_text_file, read_file_with_mtime, resolve_error_message, scope_from_str,
    write_file_with_mtime_check, DocEditToolContext,
};
use super::DocEditToolResult;
use crate::doc_edit::diacritics::{find_unique_match, DiacriticsMatchOptions, UniqueMatch};
use crate::doc_edit::mime_registry::{
    detect_mime_from_extension, is_json_family, is_jsonl_mime, parse_content, serialize_content,
    validate_json, JsonlLineResult, ParseResult,
};
use crate::doc_edit::path_resolver::resolve_doc_edit_path;
use crate::doc_edit::uri_producers::uri_for_resolved_path;
use crate::doc_edit::DocEditScope;

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

    let mtime = write_file_with_mtime_check(mount, &resolved, &content_to_write)?;
    // triggerReindexIfNeeded + Librarian announcement: no-op seam.
    let written_uri = uri_for_resolved_path(
        main,
        mount,
        &resolved,
        ctx.character_id.as_deref(),
        None,
        None,
    );

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
    Ok(DocEditToolResult::ok(result, formatted))
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
    Ok(DocEditToolResult::ok(result, formatted))
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
    Ok(DocEditToolResult::ok(result, formatted))
}

// --- doc_grep --- (enumeration path; filled in the follow-on pass)
pub fn handle_grep(
    _main: &Connection,
    _mount: &Connection,
    _args: &Value,
    _ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    Err("doc_grep not yet ported".into())
}

// --- doc_list_files --- (enumeration path; filled in the follow-on pass)
pub fn handle_list_files(
    _main: &Connection,
    _mount: &Connection,
    _args: &Value,
    _ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    Err("doc_list_files not yet ported".into())
}
