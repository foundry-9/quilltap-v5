//! The `.qtap` upload READER (P4.9G4) — v4 `lib/import/ndjson-reader.ts` +
//! `lib/import/quilltap-import-stream.ts` + the route-level `loadQtapFromUpload`
//! (`app/api/v1/system/tools/route.ts:614`).
//!
//! ## The two serializations
//!
//! A `.qtap` upload is EITHER the streaming NDJSON v1 format (whose first line is
//! an `__envelope__` record carrying `"format":"qtap-ndjson"`) or the legacy
//! monolithic JSON object. [`peek_format`] sniffs the first
//! [`PEEK_BYTES`] with v4's exact regex; the legacy path is capped at
//! [`MAX_LEGACY_IMPORT_BYTES`] (v4's 450 MB V8-string headroom).
//!
//! ## Streams vs slices — the one deliberate shape change
//!
//! v4 works over a Web `ReadableStream` so a multi-gigabyte export never lands in
//! one V8 string. The Rust edge already has the upload as bytes (axum's multipart
//! yields `Bytes`), and Rust has no string-length ceiling, so the reader takes a
//! `&[u8]`. Every observable behaviour is preserved: line splitting is done at the
//! BYTE level (so a multi-byte UTF-8 sequence can't be split), blank lines and a
//! trailing `\r` are dropped, the per-line cap still fires, and the error strings
//! are v4's verbatim — those strings reach the user through the route's
//! `badRequest(err.message)`.

use serde_json::{Map, Value};

use super::QuilltapExport;

/// v4 `PEEK_BYTES` (`ndjson-reader.ts:26`).
const PEEK_BYTES: usize = 2048;

/// v4 `MAX_LINE_BYTES` (`ndjson-reader.ts:34`).
const MAX_LINE_BYTES: usize = 128 * 1024 * 1024;

/// v4 `MAX_LEGACY_IMPORT_BYTES` (`route.ts:607`).
pub const MAX_LEGACY_IMPORT_BYTES: usize = 450 * 1024 * 1024;

/// v4 `QtapFormatKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QtapFormat {
    Ndjson,
    Legacy,
}

/// v4 `peekFormat` (`ndjson-reader.ts:54`) — `/"format"\s*:\s*"qtap-ndjson"/`
/// over the first [`PEEK_BYTES`], decoded non-fatally (invalid UTF-8 is replaced,
/// never an error — matching `TextDecoder(…, {fatal:false})`).
pub fn peek_format(body: &[u8]) -> QtapFormat {
    let prefix = &body[..body.len().min(PEEK_BYTES)];
    let text = String::from_utf8_lossy(prefix);
    if has_ndjson_marker(&text) {
        QtapFormat::Ndjson
    } else {
        QtapFormat::Legacy
    }
}

/// `/"format"\s*:\s*"qtap-ndjson"/` without pulling in a regex engine: find each
/// `"format"`, skip JS `\s` whitespace, require `:`, skip whitespace, require the
/// literal `"qtap-ndjson"`.
fn has_ndjson_marker(text: &str) -> bool {
    // JS `\s`: the ASCII set plus the Unicode space separators. The bytes that
    // can appear between a JSON key and its `:` in practice are the ASCII five.
    fn skip_ws(s: &str) -> &str {
        s.trim_start_matches([' ', '\t', '\n', '\r', '\u{000b}', '\u{000c}', '\u{feff}'])
    }
    let mut rest = text;
    while let Some(i) = rest.find("\"format\"") {
        let after = skip_ws(&rest[i + "\"format\"".len()..]);
        if let Some(after) = after.strip_prefix(':') {
            if skip_ws(after).starts_with("\"qtap-ndjson\"") {
                return true;
            }
        }
        rest = &rest[i + 1..];
    }
    false
}

/// v4 `readNdjsonLines` (`ndjson-reader.ts:129`) — one parsed JSON value per
/// line. Blank lines (including a trailing newline at EOF) are skipped; a
/// trailing `\r` is stripped; a parse failure throws v4's line-accurate message.
pub fn read_ndjson_lines(body: &[u8]) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let mut line_number = 0usize;
    let mut start = 0usize;

    while start < body.len() {
        match body[start..].iter().position(|b| *b == b'\n') {
            Some(rel) => {
                let end = start + rel;
                line_number += 1;
                push_line(&body[start..end], line_number, false, &mut out)?;
                start = end + 1;
            }
            None => {
                // No terminating newline: v4 caps the accumulator, then flushes
                // the remainder as a "final line".
                if body.len() - start > MAX_LINE_BYTES {
                    return Err(format!(
                        "NDJSON line exceeded {MAX_LINE_BYTES} bytes without a newline — file is malformed"
                    ));
                }
                line_number += 1;
                push_line(&body[start..], line_number, true, &mut out)?;
                break;
            }
        }
    }
    Ok(out)
}

fn push_line(
    slice: &[u8],
    line_number: usize,
    final_line: bool,
    out: &mut Vec<Value>,
) -> Result<(), String> {
    let trimmed = match slice.last() {
        Some(b'\r') => &slice[..slice.len() - 1],
        _ => slice,
    };
    if trimmed.is_empty() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(trimmed);
    match serde_json::from_str::<Value>(&text) {
        Ok(v) => {
            out.push(v);
            Ok(())
        }
        Err(e) => {
            // v4's excerpt: >120 chars → first 117 + '...'. JS slices by UTF-16
            // code unit; we slice by char, which agrees for every realistic
            // payload and only ever changes the excerpt inside an error message.
            let excerpt = if text.chars().count() > 120 {
                format!("{}...", text.chars().take(117).collect::<String>())
            } else {
                text.to_string()
            };
            let where_ = if final_line { "final line " } else { "line " };
            Err(format!(
                "Invalid NDJSON on {where_}{line_number}: {e} (near: {excerpt})"
            ))
        }
    }
}

/// v4 `collectLegacyJson` (`ndjson-reader.ts:218`) — the whole-file string with
/// the size cap. The cap message is verbatim.
pub fn collect_legacy_json(body: &[u8], max_bytes: usize) -> Result<String, String> {
    if body.len() > max_bytes {
        return Err(format!(
            "Legacy .qtap file exceeds {max_bytes} bytes — re-export from a newer Quilltap version to get the streaming format"
        ));
    }
    Ok(String::from_utf8_lossy(body).into_owned())
}

// ============================================================================
// The reassembler (v4 `assembleExportFromStream`, quilltap-import-stream.ts:56)
// ============================================================================

struct BlobAccumulator {
    meta: Map<String, Value>,
    chunk_count: usize,
    received: Vec<Option<String>>,
}

/// v4 `assembleExportFromStream` — fold the record stream back into the legacy
/// `{manifest, data}` shape the preview/execute pipeline consumes. Unknown record
/// kinds are SKIPPED (v4 warns and counts them); a missing envelope, a bad
/// envelope format/version, or an inconsistent blob-chunk sequence throws.
pub fn assemble_export_from_stream(records: &[Value]) -> Result<QuilltapExport, String> {
    let mut manifest: Option<Value> = None;
    let mut footer_counts: Option<Value> = None;

    let mut tags: Vec<Value> = Vec::new();
    let mut connection_profiles: Vec<Value> = Vec::new();
    let mut image_profiles: Vec<Value> = Vec::new();
    let mut embedding_profiles: Vec<Value> = Vec::new();
    let mut roleplay_templates: Vec<Value> = Vec::new();
    let mut projects: Vec<Value> = Vec::new();
    let mut groups: Vec<Value> = Vec::new();
    let mut memories: Vec<Value> = Vec::new();

    // Insertion-ordered id→record maps (v4 uses a `Map` + a parallel order array).
    let mut characters: Vec<(String, Map<String, Value>)> = Vec::new();
    let mut chats: Vec<(String, Map<String, Value>)> = Vec::new();

    let mut mount_points: Vec<Value> = Vec::new();
    let mut folders: Vec<Value> = Vec::new();
    let mut documents: Vec<Value> = Vec::new();
    let mut blobs: Vec<Value> = Vec::new();
    let mut project_links: Vec<Value> = Vec::new();
    let mut conversation_annotations: Vec<Value> = Vec::new();
    let mut chat_documents: Vec<Value> = Vec::new();

    let mut blob_accumulators: Vec<(String, BlobAccumulator)> = Vec::new();

    for raw in records {
        let Some(record) = raw.as_object() else {
            // v4: "Skipping non-object NDJSON record".
            continue;
        };
        let kind = record.get("kind").and_then(Value::as_str).unwrap_or("");
        let data = || record.get("data").cloned().unwrap_or(Value::Null);

        match kind {
            "__envelope__" => {
                if manifest.is_some() {
                    // v4: "Second envelope line ignored — only the first is used".
                    continue;
                }
                let format = record.get("format").and_then(Value::as_str);
                if format != Some("qtap-ndjson") {
                    return Err(format!(
                        "Unexpected envelope format: {} (expected 'qtap-ndjson')",
                        js_string(record.get("format"))
                    ));
                }
                if record.get("version").and_then(Value::as_i64) != Some(1) {
                    return Err(format!(
                        "Unsupported NDJSON version: {} (expected 1)",
                        js_string(record.get("version"))
                    ));
                }
                manifest = Some(record.get("manifest").cloned().unwrap_or(Value::Null));
            }
            "__footer__" => footer_counts = Some(record.get("counts").cloned().unwrap_or(Value::Null)),
            "tag" => tags.push(data()),
            "connection_profile" => connection_profiles.push(data()),
            "image_profile" => image_profiles.push(data()),
            "embedding_profile" => embedding_profiles.push(data()),
            "roleplay_template" => roleplay_templates.push(data()),
            "project" => projects.push(data()),
            "group" => groups.push(data()),
            "character" => {
                let obj = data().as_object().cloned().unwrap_or_default();
                let id = obj
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                // v4 `charactersById.set(id, …)` then pushes the id — a repeated id
                // OVERWRITES the map entry but pushes the order twice.
                match characters.iter_mut().find(|(k, _)| *k == id) {
                    Some(slot) => slot.1 = obj,
                    None => characters.push((id, obj)),
                }
            }
            "wardrobe_item" => {
                let parent_id = record
                    .get("characterId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let Some(slot) = characters.iter_mut().find(|(k, _)| k == parent_id) else {
                    continue; // v4 warns + skips
                };
                slot.1
                    .entry("wardrobeItems")
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Some(arr) = slot.1.get_mut("wardrobeItems").and_then(Value::as_array_mut) {
                    arr.push(data());
                }
            }
            "character_plugin_data" => {
                let parent_id = record
                    .get("characterId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let plugin_name = record
                    .get("pluginName")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let Some(slot) = characters.iter_mut().find(|(k, _)| k == parent_id) else {
                    continue;
                };
                slot.1
                    .entry("pluginData")
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Some(o) = slot.1.get_mut("pluginData").and_then(Value::as_object_mut) {
                    o.insert(plugin_name, data());
                }
            }
            "chat" => {
                let mut obj = data().as_object().cloned().unwrap_or_default();
                let id = obj
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                obj.insert("messages".into(), Value::Array(Vec::new()));
                match chats.iter_mut().find(|(k, _)| *k == id) {
                    Some(slot) => slot.1 = obj,
                    None => chats.push((id, obj)),
                }
            }
            "chat_message" => {
                let parent_id = record
                    .get("chatId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let Some(slot) = chats.iter_mut().find(|(k, _)| k == parent_id) else {
                    continue;
                };
                if let Some(arr) = slot.1.get_mut("messages").and_then(Value::as_array_mut) {
                    arr.push(data());
                }
            }
            "memory" => memories.push(data()),
            "conversation_annotation" => conversation_annotations.push(data()),
            "chat_document" => chat_documents.push(data()),
            "doc_mount_point" => mount_points.push(data()),
            "doc_mount_folder" => folders.push(data()),
            "doc_mount_document" => documents.push(data()),
            "doc_mount_blob" => {
                let d = data();
                let obj = d.as_object().cloned().unwrap_or_default();
                let mount_point_id = str_of(&obj, "mountPointId");
                let sha256 = str_of(&obj, "sha256");
                let chunk_count = obj
                    .get("chunkCount")
                    .and_then(Value::as_u64)
                    .unwrap_or_default() as usize;
                // The meta subset v4 rebuilds, in its literal key order.
                let mut meta = Map::new();
                for key in [
                    "mountPointId",
                    "relativePath",
                    "originalFileName",
                    "originalMimeType",
                    "storedMimeType",
                    "sizeBytes",
                    "sha256",
                    "description",
                ] {
                    meta.insert(key.into(), obj.get(key).cloned().unwrap_or(Value::Null));
                }
                meta.insert("descriptionUpdatedAt".into(), nullish(&obj, "descriptionUpdatedAt"));
                meta.insert("extractedText".into(), nullish(&obj, "extractedText"));
                meta.insert(
                    "extractedTextSha256".into(),
                    nullish(&obj, "extractedTextSha256"),
                );
                meta.insert(
                    "extractionStatus".into(),
                    match obj.get("extractionStatus") {
                        Some(Value::Null) | None => Value::String("none".into()),
                        Some(v) => v.clone(),
                    },
                );
                meta.insert("extractionError".into(), nullish(&obj, "extractionError"));

                let key = format!("{mount_point_id}:{sha256}");
                let accum = BlobAccumulator {
                    meta,
                    chunk_count,
                    received: vec![None; chunk_count],
                };
                match blob_accumulators.iter_mut().find(|(k, _)| *k == key) {
                    Some(slot) => slot.1 = accum,
                    None => blob_accumulators.push((key, accum)),
                }
            }
            "doc_mount_blob_chunk" => {
                let mount_point_id = record
                    .get("mountPointId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let sha256 = record
                    .get("sha256")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let index = record.get("index").and_then(Value::as_i64).unwrap_or(-1);
                let key = format!("{mount_point_id}:{sha256}");
                let Some(pos) = blob_accumulators.iter().position(|(k, _)| *k == key) else {
                    return Err(format!(
                        "doc_mount_blob_chunk received without preceding doc_mount_blob (sha256={sha256})"
                    ));
                };
                let chunk_count = blob_accumulators[pos].1.chunk_count;
                if index < 0 || index as usize >= chunk_count {
                    return Err(format!(
                        "doc_mount_blob_chunk index {index} out of range (chunkCount={chunk_count}, sha256={sha256})"
                    ));
                }
                let idx = index as usize;
                if blob_accumulators[pos].1.received[idx].is_some() {
                    return Err(format!(
                        "Duplicate doc_mount_blob_chunk at index {idx} for sha256={sha256}"
                    ));
                }
                blob_accumulators[pos].1.received[idx] = Some(
                    record
                        .get("dataBase64")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                );
                if blob_accumulators[pos].1.received.iter().all(Option::is_some) {
                    let (_, accum) = blob_accumulators.remove(pos);
                    let mut blob = accum.meta;
                    blob.insert(
                        "dataBase64".into(),
                        Value::String(
                            accum
                                .received
                                .into_iter()
                                .map(Option::unwrap_or_default)
                                .collect::<String>(),
                        ),
                    );
                    blobs.push(Value::Object(blob));
                }
            }
            "project_doc_mount_link" => project_links.push(data()),
            // Unknown kind — v4 warns and skips.
            _ => {}
        }
    }

    let Some(manifest) = manifest else {
        return Err(
            "NDJSON export missing envelope — first line must be a __envelope__ record".to_string(),
        );
    };

    if !blob_accumulators.is_empty() {
        let pending: Vec<Value> = blob_accumulators
            .iter()
            .map(|(_, a)| {
                let mut m = Map::new();
                m.insert("sha256".into(), a.meta.get("sha256").cloned().unwrap_or(Value::Null));
                m.insert(
                    "received".into(),
                    Value::from(a.received.iter().filter(|v| v.is_some()).count()),
                );
                m.insert("expected".into(), Value::from(a.chunk_count));
                Value::Object(m)
            })
            .collect();
        return Err(format!(
            "NDJSON export truncated: {} blob(s) missing chunks — {}",
            blob_accumulators.len(),
            Value::Array(pending)
        ));
    }

    // Stitch wardrobe/plugin data back into the export shape (v4 :330-337 —
    // an EMPTY collection drops the key entirely).
    let characters: Vec<Value> = characters
        .into_iter()
        .map(|(_, mut c)| {
            if c.get("wardrobeItems")
                .and_then(Value::as_array)
                .map(|a| a.is_empty())
                .unwrap_or(false)
            {
                c.shift_remove("wardrobeItems");
            }
            if c.get("pluginData")
                .and_then(Value::as_object)
                .map(|o| o.is_empty())
                .unwrap_or(false)
            {
                c.shift_remove("pluginData");
            }
            Value::Object(c)
        })
        .collect();
    let chats: Vec<Value> = chats.into_iter().map(|(_, c)| Value::Object(c)).collect();

    // The footer counts win over the envelope's (deliberately empty) ones.
    let counts = footer_counts
        .or_else(|| manifest.get("counts").cloned())
        .unwrap_or(Value::Object(Map::new()));

    let export_type = manifest
        .get("exportType")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let data = build_export_data_for_type(
        &export_type,
        Collected {
            tags,
            connection_profiles,
            image_profiles,
            embedding_profiles,
            roleplay_templates,
            projects,
            groups,
            characters,
            chats,
            memories,
            mount_points,
            folders,
            documents,
            blobs,
            project_links,
            conversation_annotations,
            chat_documents,
        },
    )?;

    let mut m = manifest.as_object().cloned().unwrap_or_default();
    m.insert("counts".into(), counts);
    Ok(QuilltapExport {
        manifest: Value::Object(m),
        data,
    })
}

struct Collected {
    tags: Vec<Value>,
    connection_profiles: Vec<Value>,
    image_profiles: Vec<Value>,
    embedding_profiles: Vec<Value>,
    roleplay_templates: Vec<Value>,
    projects: Vec<Value>,
    groups: Vec<Value>,
    characters: Vec<Value>,
    chats: Vec<Value>,
    memories: Vec<Value>,
    mount_points: Vec<Value>,
    folders: Vec<Value>,
    documents: Vec<Value>,
    blobs: Vec<Value>,
    project_links: Vec<Value>,
    conversation_annotations: Vec<Value>,
    chat_documents: Vec<Value>,
}

/// v4 `buildExportDataForType` (`quilltap-import-stream.ts:397`) — only the
/// subset matching `exportType` lands in `data`, and the `&&` spreads DROP an
/// empty optional collection entirely.
fn build_export_data_for_type(export_type: &str, c: Collected) -> Result<Value, String> {
    let mut d = Map::new();
    match export_type {
        "characters" => {
            d.insert("characters".into(), Value::Array(c.characters));
            if !c.memories.is_empty() {
                d.insert("memories".into(), Value::Array(c.memories));
            }
        }
        "chats" => {
            d.insert("chats".into(), Value::Array(c.chats));
            if !c.memories.is_empty() {
                d.insert("memories".into(), Value::Array(c.memories));
            }
            if !c.conversation_annotations.is_empty() {
                d.insert(
                    "conversationAnnotations".into(),
                    Value::Array(c.conversation_annotations),
                );
            }
            if !c.chat_documents.is_empty() {
                d.insert("chatDocuments".into(), Value::Array(c.chat_documents));
            }
        }
        "roleplay-templates" => {
            d.insert(
                "roleplayTemplates".into(),
                Value::Array(c.roleplay_templates),
            );
        }
        "connection-profiles" => {
            d.insert(
                "connectionProfiles".into(),
                Value::Array(c.connection_profiles),
            );
        }
        "image-profiles" => {
            d.insert("imageProfiles".into(), Value::Array(c.image_profiles));
        }
        "embedding-profiles" => {
            d.insert(
                "embeddingProfiles".into(),
                Value::Array(c.embedding_profiles),
            );
        }
        "tags" => {
            d.insert("tags".into(), Value::Array(c.tags));
        }
        "projects" => {
            d.insert("projects".into(), Value::Array(c.projects));
        }
        "groups" => {
            d.insert("groups".into(), Value::Array(c.groups));
        }
        "document-stores" => {
            d.insert("mountPoints".into(), Value::Array(c.mount_points));
            d.insert("folders".into(), Value::Array(c.folders));
            d.insert("documents".into(), Value::Array(c.documents));
            d.insert("blobs".into(), Value::Array(c.blobs));
            d.insert("projectLinks".into(), Value::Array(c.project_links));
        }
        other => return Err(format!("Unknown export type in manifest: {other}")),
    }
    Ok(Value::Object(d))
}

/// v4 `loadQtapFromUpload` (`route.ts:614`) — sniff, then either reassemble the
/// NDJSON stream or `JSON.parse` the legacy monolith (whose parse failure carries
/// v4's `Invalid JSON: …` prefix and is NOT validated here; the caller validates).
pub fn load_qtap_from_upload(body: &[u8]) -> Result<QuilltapExport, String> {
    match peek_format(body) {
        QtapFormat::Ndjson => {
            let records = read_ndjson_lines(body)?;
            assemble_export_from_stream(&records)
        }
        QtapFormat::Legacy => {
            let text = collect_legacy_json(body, MAX_LEGACY_IMPORT_BYTES)?;
            let parsed: Value = serde_json::from_str(&text)
                .map_err(|e| format!("Invalid JSON: {e}"))?;
            // v4 casts without validating — the route's `validateExportFile` gate
            // runs next and is what rejects a malformed payload.
            Ok(QuilltapExport {
                manifest: parsed.get("manifest").cloned().unwrap_or(Value::Null),
                data: parsed.get("data").cloned().unwrap_or(Value::Null),
            })
        }
    }
}

fn str_of(obj: &Map<String, Value>, key: &str) -> String {
    obj.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn nullish(obj: &Map<String, Value>, key: &str) -> Value {
    match obj.get(key) {
        Some(Value::Null) | None => Value::Null,
        Some(v) => v.clone(),
    }
}

/// `String(x)` on a JSON value — the rendering v4's envelope error messages use.
fn js_string(v: Option<&Value>) -> String {
    match v {
        None => "undefined".to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sniffs_both_serializations() {
        let ndjson = br#"{"kind":"__envelope__","format":"qtap-ndjson","version":1}
{"kind":"tag","data":{}}
"#;
        assert_eq!(peek_format(ndjson), QtapFormat::Ndjson);
        let legacy = br#"{"manifest":{"format":"quilltap-export","version":"1.0"},"data":{}}"#;
        assert_eq!(peek_format(legacy), QtapFormat::Legacy);
        // The marker only counts with the regex's exact shape.
        assert_eq!(peek_format(br#"{"format" : "qtap-ndjson"}"#), QtapFormat::Ndjson);
        assert_eq!(peek_format(br#"{"formatx":"qtap-ndjson"}"#), QtapFormat::Legacy);
    }

    #[test]
    fn reads_lines_skipping_blanks_and_crlf() {
        let body = b"{\"a\":1}\r\n\n{\"b\":2}\n";
        let lines = read_ndjson_lines(body).unwrap();
        assert_eq!(lines, vec![json!({"a":1}), json!({"b":2})]);
        // A trailing line with no newline still flushes.
        let lines = read_ndjson_lines(b"{\"a\":1}\n{\"b\":2}").unwrap();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn bad_line_reports_v4s_message_shape() {
        let err = read_ndjson_lines(b"{\"a\":1}\nnot json\n").unwrap_err();
        assert!(err.starts_with("Invalid NDJSON on line 2: "), "{err}");
        assert!(err.ends_with("(near: not json)"), "{err}");
    }

    #[test]
    fn assembles_a_character_export_with_sidecars() {
        let records = vec![
            json!({"kind":"__envelope__","format":"qtap-ndjson","version":1,
                   "manifest":{"exportType":"characters","counts":{}}}),
            json!({"kind":"character","data":{"id":"c1","name":"A"}}),
            json!({"kind":"wardrobe_item","characterId":"c1","data":{"id":"w1"}}),
            json!({"kind":"character_plugin_data","characterId":"c1","pluginName":"p","data":{"x":1}}),
            json!({"kind":"memory","data":{"id":"m1"}}),
            json!({"kind":"__footer__","counts":{"characters":1,"memories":1}}),
        ];
        let out = assemble_export_from_stream(&records).unwrap();
        assert_eq!(out.manifest["counts"], json!({"characters":1,"memories":1}));
        assert_eq!(out.data["characters"][0]["wardrobeItems"][0]["id"], "w1");
        assert_eq!(out.data["characters"][0]["pluginData"]["p"]["x"], 1);
        assert_eq!(out.data["memories"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn missing_envelope_and_truncated_blobs_throw_v4s_messages() {
        let err = assemble_export_from_stream(&[json!({"kind":"tag","data":{}})]).unwrap_err();
        assert_eq!(
            err,
            "NDJSON export missing envelope — first line must be a __envelope__ record"
        );

        let records = vec![
            json!({"kind":"__envelope__","format":"qtap-ndjson","version":1,
                   "manifest":{"exportType":"document-stores","counts":{}}}),
            json!({"kind":"doc_mount_blob","data":{"mountPointId":"m","sha256":"s","chunkCount":2}}),
            json!({"kind":"doc_mount_blob_chunk","mountPointId":"m","sha256":"s","index":0,"total":2,"dataBase64":"AA=="}),
        ];
        let err = assemble_export_from_stream(&records).unwrap_err();
        assert!(err.starts_with("NDJSON export truncated: 1 blob(s) missing chunks — "), "{err}");
    }

    #[test]
    fn blob_chunks_rejoin_in_index_order() {
        let records = vec![
            json!({"kind":"__envelope__","format":"qtap-ndjson","version":1,
                   "manifest":{"exportType":"document-stores","counts":{}}}),
            json!({"kind":"doc_mount_blob","data":{"mountPointId":"m","relativePath":"a","sha256":"s","chunkCount":2}}),
            json!({"kind":"doc_mount_blob_chunk","mountPointId":"m","sha256":"s","index":1,"total":2,"dataBase64":"YmJi"}),
            json!({"kind":"doc_mount_blob_chunk","mountPointId":"m","sha256":"s","index":0,"total":2,"dataBase64":"YWFh"}),
        ];
        let out = assemble_export_from_stream(&records).unwrap();
        assert_eq!(out.data["blobs"][0]["dataBase64"], "YWFhYmJi");
        // The meta subset keeps v4's literal key order, with dataBase64 last.
        let keys: Vec<&str> = out.data["blobs"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys.last(), Some(&"dataBase64"));
        assert_eq!(keys.first(), Some(&"mountPointId"));
    }

    #[test]
    fn envelope_version_and_format_are_pinned() {
        let bad_fmt = vec![json!({"kind":"__envelope__","format":"nope","version":1})];
        assert_eq!(
            assemble_export_from_stream(&bad_fmt).unwrap_err(),
            "Unexpected envelope format: nope (expected 'qtap-ndjson')"
        );
        let bad_ver = vec![json!({"kind":"__envelope__","format":"qtap-ndjson","version":2})];
        assert_eq!(
            assemble_export_from_stream(&bad_ver).unwrap_err(),
            "Unsupported NDJSON version: 2 (expected 1)"
        );
    }
}
