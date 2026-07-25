//! v4 `lib/backup/restore/json-stream.ts` — the disk-backed JSON readers the
//! extracted backup tree is parsed through.
//!
//! v4's reason for the hand-rolled scanner is a V8 constraint: `fs.readFile` +
//! `JSON.parse` both materialize the whole payload as one string, and V8 caps
//! strings at ~512 MB, so a full-history `llm-logs.json` throws
//! `ERR_STRING_TOO_LONG`. Rust has no such cap — but the scanner is ported
//! faithfully anyway, for two reasons that are behavior rather than mechanism:
//!
//!  1. its **error messages are observable** — the preview/restore route leaks
//!     `error.message` straight to the client (`system/restore/route.ts:176`),
//!     so a malformed archive's wording is part of the contract; and
//!  2. it **rejects top-level scalars** (`:115`), which a permissive
//!     `serde_json::from_str::<Vec<Value>>` would happily accept. An archive
//!     carrying `[1,2]` in a data file fails on v4 and must fail here.
//!
//! The one mechanism difference: v4 scans UTF-16 code units of a decoded string
//! chunk; this scans **bytes**. Every structural character in JSON is ASCII and
//! every UTF-8 continuation byte is ≥ 0x80, so the state machine is identical;
//! only the error sites need to decode a character back out, which they do.

use std::io::Read;
use std::path::Path;

use serde_json::Value;

/// A read error carrying v4's exact thrown message.
pub type JsonReadError = String;

/// v4 `readJsonFile` (`:20`) — plain read + parse. Used for `manifest.json`
/// only; every array goes through [`read_json_array_file`].
pub fn read_json_file(base_path: &Path, relative_path: &str) -> Result<Value, JsonReadError> {
    let file_path = join_rel(base_path, relative_path);
    let content = std::fs::read(&file_path).map_err(|e| format!("{relative_path}: {e}"))?;
    serde_json::from_slice(&content).map_err(|e| format!("{relative_path}: {e}"))
}

/// v4 `readJsonArrayFileOptional` (`:151`) — `access` first, so a MISSING file
/// yields the fallback; a present-but-malformed file still throws.
pub fn read_json_array_file_optional(
    base_path: &Path,
    relative_path: &str,
) -> Result<Vec<Value>, JsonReadError> {
    let file_path = join_rel(base_path, relative_path);
    if !file_path.exists() {
        return Ok(Vec::new());
    }
    read_json_array_file(base_path, relative_path)
}

/// v4 `readJsonArrayFile` (`:42`) — the streaming array scanner. One element is
/// parsed at a time; the file is read in 1 MB chunks (v4's `highWaterMark`).
pub fn read_json_array_file(
    base_path: &Path,
    relative_path: &str,
) -> Result<Vec<Value>, JsonReadError> {
    let file_path = join_rel(base_path, relative_path);
    // The I/O error text itself is engine-specific (v4 surfaces Node's
    // `ENOENT: … open '<abs path>'`), so it is not part of the contract — but
    // WHICH file failed is, and it is what the user needs. The relative path
    // leads; the differential's `preview_missing_required` case asserts on it.
    let file = std::fs::File::open(&file_path).map_err(|e| format!("{relative_path}: {e}"))?;
    let mut reader = std::io::BufReader::new(file);

    let mut result: Vec<Value> = Vec::new();
    let mut started = false;
    let mut finished = false;
    let mut in_element = false;
    let mut element_buf: Vec<u8> = Vec::new();
    let mut depth: i64 = 0;
    let mut in_string = false;
    let mut escape = false;

    let mut chunk = vec![0u8; 1 << 20];
    loop {
        let n = reader.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let bytes = &chunk[..n];
        for i in 0..n {
            let c = bytes[i];

            if !started {
                if c == b'[' {
                    started = true;
                } else if !is_ws(c) {
                    return Err(format!(
                        "readJsonArrayFile: expected '[' at start of {relative_path}, got {}",
                        json_stringify_char(bytes, i)
                    ));
                }
                continue;
            }

            if finished {
                if !is_ws(c) {
                    return Err(format!(
                        "readJsonArrayFile: unexpected character after array end in {relative_path}: {}",
                        json_stringify_char(bytes, i)
                    ));
                }
                continue;
            }

            if in_element {
                element_buf.push(c);

                if escape {
                    escape = false;
                    continue;
                }
                if in_string {
                    if c == b'\\' {
                        escape = true;
                    } else if c == b'"' {
                        in_string = false;
                    }
                    continue;
                }
                if c == b'"' {
                    in_string = true;
                    continue;
                }
                if c == b'{' || c == b'[' {
                    depth += 1;
                    continue;
                }
                if c == b'}' || c == b']' {
                    depth -= 1;
                    if depth == 0 {
                        result.push(serde_json::from_slice(&element_buf).map_err(|e| {
                            // v4's `JSON.parse` throw propagates verbatim; the
                            // wording is engine-specific either way, so the
                            // differential only asserts THAT it throws here.
                            format!("readJsonArrayFile: invalid element in {relative_path}: {e}")
                        })?);
                        element_buf.clear();
                        in_element = false;
                    }
                }
                continue;
            }

            // Between elements: look for the next element start or the array close.
            if c == b']' {
                finished = true;
                continue;
            }
            if c == b',' || is_ws(c) {
                continue;
            }
            if c != b'{' && c != b'[' {
                return Err(format!(
                    "readJsonArrayFile: only object/array elements supported at top level ({relative_path}), got {}",
                    json_stringify_char(bytes, i)
                ));
            }
            in_element = true;
            element_buf.clear();
            element_buf.push(c);
            depth = 1;
        }
    }

    if !started {
        return Err(format!(
            "readJsonArrayFile: empty file or no array in {relative_path}"
        ));
    }
    if !finished {
        return Err(format!(
            "readJsonArrayFile: unexpected end of input in {relative_path}"
        ));
    }

    Ok(result)
}

/// v4's `isWs` (`:55`).
fn is_ws(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b'\n' || c == b'\r'
}

/// `path.join(basePath, relativePath)` for the `/`-joined relative paths this
/// module is called with.
fn join_rel(base: &Path, rel: &str) -> std::path::PathBuf {
    let mut p = base.to_path_buf();
    for part in rel.split('/') {
        p.push(part);
    }
    p
}

/// `JSON.stringify(c)` where `c` is the single character at `bytes[i]` — the
/// three "got X" error sites. Non-ASCII is decoded back to its character (v4
/// scans code units, so it would report the same one); a byte that does not
/// start a valid sequence falls back to the replacement character, which is
/// what a lossy decode of that input would have handed v4 anyway.
fn json_stringify_char(bytes: &[u8], i: usize) -> String {
    let end = (i + 4).min(bytes.len());
    let ch = std::str::from_utf8(&bytes[i..end])
        .ok()
        .and_then(|s| s.chars().next())
        .or_else(|| {
            // A truncated multi-byte sequence at the chunk boundary.
            for take in (1..=(end - i)).rev() {
                if let Ok(s) = std::str::from_utf8(&bytes[i..i + take]) {
                    if let Some(c) = s.chars().next() {
                        return Some(c);
                    }
                }
            }
            None
        })
        .unwrap_or('\u{fffd}');
    let mut out = String::from("\"");
    match ch {
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        '\u{8}' => out.push_str("\\b"),
        '\t' => out.push_str("\\t"),
        '\n' => out.push_str("\\n"),
        '\u{c}' => out.push_str("\\f"),
        '\r' => out.push_str("\\r"),
        c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
        c => out.push(c),
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("qt-jsonstream-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn reads_objects_and_nested_arrays() {
        let d = scratch("ok");
        write(
            &d,
            "a.json",
            "[\n  {\"a\": \"]},[\\\"\", \"b\": [1, {\"c\": 2}]},\n  [1, 2]\n]\n",
        );
        let got = read_json_array_file(&d, "a.json").unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0]["a"], serde_json::json!("]},[\""));
        assert_eq!(got[1], serde_json::json!([1, 2]));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn v4_error_strings_are_reproduced() {
        let d = scratch("err");

        write(&d, "start.json", "x[]");
        assert_eq!(
            read_json_array_file(&d, "start.json").unwrap_err(),
            "readJsonArrayFile: expected '[' at start of start.json, got \"x\""
        );

        write(&d, "after.json", "[] x");
        assert_eq!(
            read_json_array_file(&d, "after.json").unwrap_err(),
            "readJsonArrayFile: unexpected character after array end in after.json: \"x\""
        );

        write(&d, "scalar.json", "[1, 2]");
        assert_eq!(
            read_json_array_file(&d, "scalar.json").unwrap_err(),
            "readJsonArrayFile: only object/array elements supported at top level (scalar.json), got \"1\""
        );

        write(&d, "empty.json", "   ");
        assert_eq!(
            read_json_array_file(&d, "empty.json").unwrap_err(),
            "readJsonArrayFile: empty file or no array in empty.json"
        );

        write(&d, "trunc.json", "[{\"a\": 1}");
        assert_eq!(
            read_json_array_file(&d, "trunc.json").unwrap_err(),
            "readJsonArrayFile: unexpected end of input in trunc.json"
        );

        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn optional_missing_yields_the_fallback_but_malformed_still_throws() {
        let d = scratch("opt");
        assert_eq!(
            read_json_array_file_optional(&d, "nope.json").unwrap(),
            Vec::<Value>::new()
        );
        write(&d, "bad.json", "nope");
        assert!(read_json_array_file_optional(&d, "bad.json").is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn control_characters_stringify_the_way_json_stringify_does() {
        assert_eq!(json_stringify_char(b"\n", 0), "\"\\n\"");
        assert_eq!(json_stringify_char(b"\x01", 0), "\"\\u0001\"");
        assert_eq!(json_stringify_char("é".as_bytes(), 0), "\"é\"");
    }
}
