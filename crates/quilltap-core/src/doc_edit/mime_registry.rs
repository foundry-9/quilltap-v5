//! MIME registry for document editing — v4 `lib/doc-edit/mime-registry.ts`.
//!
//! Detection, parsing, and serialization of JSON / JSONL files for the
//! `doc_read_file` / `doc_write_file` tools (YAML MIMEs are recognized for
//! consistency but never parsed here).
//!
//! ## The V8 `JSON.parse` message seam
//!
//! A `ParseFailure`/`JsonlLineResult` carries `JSON.parse`'s error TEXT, which is
//! **V8-engine-specific** (e.g. `Unexpected token 'o', "not json" is not valid
//! JSON`) and not reproducible from Rust. The port surfaces its own
//! (`serde_json`) message instead. That text reaches tool output, so it is a
//! **documented normalized seam**: the differential compares the *structure*
//! (`ok`, the parsed value, the JSONL line numbers, the `raw` line) exactly and
//! normalizes only the failure MESSAGE. The happy-path parse/serialize bytes ARE
//! byte-exact (`serde_json` pretty-print == `JSON.stringify(x, null, 2)` on the
//! corpus value space).

use serde_json::Value;

/// The recognized document MIME types (v4 `DocMimeType`).
pub const MIME_MARKDOWN: &str = "text/markdown";
pub const MIME_PLAIN: &str = "text/plain";
pub const MIME_APPLICATION_JSON: &str = "application/json";
pub const MIME_NDJSON: &str = "application/x-ndjson";
pub const MIME_APPLICATION_YAML: &str = "application/yaml";

/// Posix `path.extname` (the common subset): the substring from the last `.` in
/// the final path segment, dot included — `""` when there is no extension or the
/// only dot is the segment's first character (dotfile). Pathological all-dot
/// segments are a documented seam (the corpus uses ordinary filenames).
fn extname(file_path: &str) -> String {
    let base = file_path.rsplit('/').next().unwrap_or(file_path);
    match base.rfind('.') {
        Some(idx) if idx > 0 => base[idx..].to_string(),
        _ => String::new(),
    }
}

/// Map a file extension to a canonical MIME (v4 `detectMimeFromExtension`).
pub fn detect_mime_from_extension(file_path: &str) -> Option<&'static str> {
    match extname(file_path).to_lowercase().as_str() {
        ".json" => Some(MIME_APPLICATION_JSON),
        ".jsonl" | ".ndjson" => Some(MIME_NDJSON),
        ".md" | ".markdown" => Some(MIME_MARKDOWN),
        ".txt" => Some(MIME_PLAIN),
        ".yaml" | ".yml" => Some(MIME_APPLICATION_YAML),
        _ => None,
    }
}

/// Is this MIME one of the JSON variants (v4 `isJsonMime`)?
pub fn is_json_mime(mime: Option<&str>) -> bool {
    matches!(mime, Some("text/json") | Some("application/json"))
}

/// Is this MIME one of the JSONL variants (v4 `isJsonlMime`)?
pub fn is_jsonl_mime(mime: Option<&str>) -> bool {
    matches!(mime, Some("text/x-jsonl") | Some("application/x-ndjson"))
}

/// JSON or JSONL (v4 `isJsonFamily`)?
pub fn is_json_family(mime: Option<&str>) -> bool {
    is_json_mime(mime) || is_jsonl_mime(mime)
}

/// A parse/serialize result (v4 `ParseResult`). `Ok` carries the value; `Err`
/// carries the (V8-seam) message.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseResult {
    Ok(Value),
    Err { error: String, line: Option<usize> },
}

/// One JSONL line result (v4 `JsonlLineResult`). Serializes to `{ line, value?,
/// error?, raw }` (absent optionals omitted, matching JS `undefined`).
#[derive(Debug, Clone, PartialEq)]
pub struct JsonlLineResult {
    pub line: usize,
    pub value: Option<Value>,
    pub error: Option<String>,
    pub raw: String,
}

/// Split on `\r?\n` (v4 `content.split(/\r?\n/)`).
fn split_lines(content: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = content;
    loop {
        match rest.find('\n') {
            Some(nl) => {
                let mut seg_end = nl;
                if seg_end > 0 && rest.as_bytes()[seg_end - 1] == b'\r' {
                    seg_end -= 1;
                }
                out.push(&rest[..seg_end]);
                rest = &rest[nl + 1..];
            }
            None => {
                out.push(rest);
                break;
            }
        }
    }
    out
}

/// JS `String.prototype.trim` emptiness test (used for JSONL blank-line skipping).
fn js_trim_is_empty(s: &str) -> bool {
    crate::jsstr::js_trim(s).is_empty()
}

/// Parse content into a native value (v4 `parseContent`). JSON → single parse;
/// JSONL → per-line results (blank lines skipped), always `Ok(array)`.
pub fn parse_content(content: &str, mime: &str) -> ParseResult {
    if is_json_mime(Some(mime)) {
        return match serde_json::from_str::<Value>(content) {
            Ok(value) => ParseResult::Ok(value),
            Err(e) => ParseResult::Err {
                error: e.to_string(),
                line: None,
            },
        };
    }

    if is_jsonl_mime(Some(mime)) {
        let mut results: Vec<JsonlLineResult> = Vec::new();
        for (i, line) in split_lines(content).iter().enumerate() {
            let line_num = i + 1;
            if js_trim_is_empty(line) {
                continue;
            }
            match serde_json::from_str::<Value>(line) {
                Ok(value) => results.push(JsonlLineResult {
                    line: line_num,
                    value: Some(value),
                    error: None,
                    raw: (*line).to_string(),
                }),
                Err(e) => results.push(JsonlLineResult {
                    line: line_num,
                    value: None,
                    error: Some(e.to_string()),
                    raw: (*line).to_string(),
                }),
            }
        }
        return ParseResult::Ok(jsonl_results_to_value(&results));
    }

    ParseResult::Err {
        error: format!("MIME type {mime} is not supported for parsing"),
        line: None,
    }
}

/// Serialize a JSONL line-result array to v4's JSON shape (absent optionals
/// omitted).
pub fn jsonl_results_to_value(results: &[JsonlLineResult]) -> Value {
    Value::Array(
        results
            .iter()
            .map(|r| {
                let mut m = serde_json::Map::new();
                m.insert("line".into(), Value::from(r.line));
                if let Some(v) = &r.value {
                    m.insert("value".into(), v.clone());
                }
                if let Some(e) = &r.error {
                    m.insert("error".into(), Value::from(e.clone()));
                }
                m.insert("raw".into(), Value::from(r.raw.clone()));
                Value::Object(m)
            })
            .collect(),
    )
}

/// Serialize a native value for storage (v4 `serializeContent`). JSON →
/// `JSON.stringify(value, null, 2) + '\n'` (or compact when `pretty=false`);
/// JSONL → one compact line per array entry + trailing newline.
pub fn serialize_content(value: &Value, mime: &str, pretty: bool) -> ParseResult {
    if is_json_mime(Some(mime)) {
        let serialized = if pretty {
            serde_json::to_string_pretty(value).unwrap()
        } else {
            serde_json::to_string(value).unwrap()
        };
        return ParseResult::Ok(Value::from(format!("{serialized}\n")));
    }

    if is_jsonl_mime(Some(mime)) {
        let Some(arr) = value.as_array() else {
            return ParseResult::Err {
                error: format!("JSONL requires an array value; got {}", js_typeof(value)),
                line: None,
            };
        };
        let mut lines: Vec<String> = Vec::new();
        for item in arr {
            lines.push(serde_json::to_string(item).unwrap());
        }
        let serialized = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };
        return ParseResult::Ok(Value::from(serialized));
    }

    ParseResult::Err {
        error: format!("MIME type {mime} is not supported for serialization"),
        line: None,
    }
}

/// Serialize a raw string per v4's string branch: return as-is unless `validate`,
/// in which case parse then re-serialize canonically.
pub fn serialize_string(value: &str, mime: &str, validate: bool, pretty: bool) -> ParseResult {
    if !validate {
        return ParseResult::Ok(Value::from(value));
    }
    match parse_content(value, mime) {
        ParseResult::Ok(parsed) => serialize_content(&parsed, mime, pretty),
        err => err,
    }
}

/// Validate a raw string parses as JSON / JSONL (v4 `validateJson`). `Ok(true)`
/// on success; `Err` (with the `Line N:` prefix for JSONL) on the first failure.
pub fn validate_json(content: &str, mime: &str) -> ParseResult {
    if is_json_mime(Some(mime)) {
        return match serde_json::from_str::<Value>(content) {
            Ok(_) => ParseResult::Ok(Value::Bool(true)),
            Err(e) => ParseResult::Err {
                error: e.to_string(),
                line: None,
            },
        };
    }
    if is_jsonl_mime(Some(mime)) {
        let non_blank: Vec<&str> = split_lines(content)
            .into_iter()
            .filter(|l| !js_trim_is_empty(l))
            .collect();
        for (i, line) in non_blank.iter().enumerate() {
            if let Err(e) = serde_json::from_str::<Value>(line) {
                return ParseResult::Err {
                    error: format!("Line {}: {}", i + 1, e),
                    line: None,
                };
            }
        }
        return ParseResult::Ok(Value::Bool(true));
    }
    ParseResult::Err {
        error: format!("MIME type {mime} does not support JSON validation"),
        line: None,
    }
}

/// JS `typeof` for the serialize error message (`object` covers arrays/null too,
/// but the array branch is taken before this fires).
fn js_typeof(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Null | Value::Object(_) | Value::Array(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_extensions() {
        assert_eq!(
            detect_mime_from_extension("a.json"),
            Some(MIME_APPLICATION_JSON)
        );
        assert_eq!(detect_mime_from_extension("a.JSONL"), Some(MIME_NDJSON));
        assert_eq!(detect_mime_from_extension("dir/b.md"), Some(MIME_MARKDOWN));
        assert_eq!(detect_mime_from_extension("noext"), None);
        assert_eq!(detect_mime_from_extension(".gitignore"), None);
    }

    #[test]
    fn json_round_trip() {
        let p = parse_content("{\"a\":1}", MIME_APPLICATION_JSON);
        assert_eq!(p, ParseResult::Ok(json!({"a":1})));
        let s = serialize_content(&json!({"a":1}), MIME_APPLICATION_JSON, true);
        assert_eq!(s, ParseResult::Ok(Value::from("{\n  \"a\": 1\n}\n")));
    }
}
