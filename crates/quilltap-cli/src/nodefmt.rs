//! Node/V8 output-formatting primitives the CLI reproduces byte-for-byte:
//! JS number rendering, `util.inspect` string quoting (the `console.table`
//! cell form), `JSON.stringify(x, null, 2)`, `path.join`-style joining, and
//! the launcher's `formatBytes`.
//!
//! Corpus constraints (documented seams, all held by the differential
//! fixtures): numbers stay inside the range where V8's and ryu's shortest
//! representations agree (|x| in [1e-4, 1e21) or integer-valued); table
//! strings avoid ` `/` ` and C1 controls; blobs stay out of
//! table-rendered SQL results.

use serde_json::{Map, Value};

/// Render an `f64` the way JS `String(n)` / `JSON.stringify(n)` does for the
/// values that occur in CLI output: `-0` renders `-0` only in the inspect
/// (String) form, integer-valued doubles render bare, fractional values take
/// ryu's shortest form (== V8's inside the corpus range).
pub fn js_num_string(f: f64) -> String {
    if f == 0.0 && f.is_sign_negative() {
        return "-0".to_string();
    }
    serde_json::to_string(&js_number_to_json(f)).unwrap_or_else(|_| "null".to_string())
}

/// The JSON `Value` form of a JS number (the core `db::js_number_to_json`
/// rule, reproduced locally so this lane leaves core untouched): an
/// integer-valued finite double in i64 range collapses to a JSON integer,
/// exactly as `JSON.stringify` renders `9.0` as `9`.
pub fn js_number_to_json(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
        Value::from(f as i64)
    } else {
        Value::from(f)
    }
}

/// A SQLite cell as better-sqlite3 hands it to JS: INTEGER goes through a
/// (lossy past 2^53) double conversion, REAL is a double, TEXT a string,
/// NULL null, BLOB a Node `Buffer` (JSON form `{"type":"Buffer","data":[…]}`).
pub fn cell_to_js_value(v: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => js_number_to_json(i as f64),
        ValueRef::Real(f) => js_number_to_json(f),
        ValueRef::Text(b) => Value::from(String::from_utf8_lossy(b).into_owned()),
        ValueRef::Blob(b) => {
            let mut m = Map::new();
            m.insert("type".into(), Value::from("Buffer"));
            m.insert(
                "data".into(),
                Value::from(b.iter().map(|&x| Value::from(x)).collect::<Vec<_>>()),
            );
            Value::Object(m)
        }
    }
}

/// `JSON.stringify(value, null, 2)` — serde_json's 2-space pretty printer over
/// an insertion-ordered `Value` is byte-identical for the shapes the CLI
/// emits (the ported mime-registry precedent).
pub fn json_stringify_pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string())
}

/// `util.inspect` string quoting as it appears in `console.table` cells:
/// prefer `'`, fall to `"` when the string holds a `'`, fall to a backtick
/// when it holds both (unless it also holds a backtick or `${`, in which case
/// single quotes with `\'` escapes). Escapes match Node's `strEscape` for the
/// common set: backslash, the chosen quote, and C0 controls.
pub fn inspect_quote(s: &str) -> String {
    let quote = if !s.contains('\'') {
        '\''
    } else if !s.contains('"') {
        '"'
    } else if !s.contains('`') && !s.contains("${") {
        '`'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\x08' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\x0c' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Node `path.join(a, b)` for the POSIX shapes the CLI composes (data dirs,
/// lock paths): join with `/` then normalize `.`/`..`/`//` without anchoring
/// a relative path to the cwd.
pub fn node_join(a: &str, b: &str) -> String {
    node_normalize(&format!("{a}/{b}"))
}

/// Node `path.normalize` (POSIX): resolve `.` and `..` textually, collapse
/// duplicate separators, drop the trailing separator. A relative path stays
/// relative (leading `..` segments survive).
pub fn node_normalize(p: &str) -> String {
    let absolute = p.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|s| *s != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            s => parts.push(s),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

/// Node `path.resolve(p)` for one argument: an absolute path is normalized in
/// place; a relative one is normalized against the process cwd (which is what
/// v4's `path.resolve(outPath)` does).
pub fn node_resolve(p: &str) -> String {
    if p.starts_with('/') {
        return node_normalize(p);
    }
    let cwd = std::env::current_dir()
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "/".to_string());
    node_join(&cwd, p)
}

/// JS `String#length` — UTF-16 code units, not chars or bytes.
pub fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// JS `String#slice(0, n)` — counts UTF-16 code units. A cut landing inside a
/// surrogate pair leaves JS holding a lone surrogate, which Node turns into
/// U+FFFD on the way to a UTF-8 stream; `from_utf16_lossy` is that same
/// substitution.
pub fn slice_utf16(s: &str, n: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().take(n).collect();
    String::from_utf16_lossy(&units)
}

/// JS `parseInt(s, 10)`: prefix-parse an optionally-signed decimal integer;
/// `None` for NaN (no leading digits / missing value).
pub fn js_parse_int(s: Option<&str>) -> Option<i64> {
    let s = s?.trim();
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => (-1i64, r),
        None => (1, s.strip_prefix('+').unwrap_or(s)),
    };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<i64>().ok().map(|n| sign * n)
}

/// The launcher's `formatBytes` (docs-commands.js flavor — `toFixed` via the
/// ported JS kernel).
pub fn format_bytes(n: f64) -> String {
    if n < 1024.0 {
        return format!("{} B", js_num_string(n));
    }
    if n < 1024.0 * 1024.0 {
        return format!("{} KB", quilltap_core::jsnum::to_fixed(n / 1024.0, 1));
    }
    if n < 1024.0 * 1024.0 * 1024.0 {
        return format!(
            "{} MB",
            quilltap_core::jsnum::to_fixed(n / 1024.0 / 1024.0, 1)
        );
    }
    format!(
        "{} GB",
        quilltap_core::jsnum::to_fixed(n / 1024.0 / 1024.0 / 1024.0, 2)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_render_like_js() {
        assert_eq!(js_num_string(3.0), "3");
        assert_eq!(js_num_string(3.5), "3.5");
        assert_eq!(js_num_string(-0.0), "-0");
        assert_eq!(js_num_string(0.1), "0.1");
        assert_eq!(js_num_string(9007199254740992.0), "9007199254740992");
    }

    #[test]
    fn inspect_quoting_matches_probed_node() {
        assert_eq!(inspect_quote("plain"), "'plain'");
        assert_eq!(inspect_quote("it's"), "\"it's\"");
        assert_eq!(inspect_quote("say \"hi\""), "'say \"hi\"'");
        assert_eq!(inspect_quote("both ' and \""), "`both ' and \"`");
        assert_eq!(inspect_quote("tab\there"), "'tab\\there'");
        assert_eq!(inspect_quote("line\nbreak"), "'line\\nbreak'");
        assert_eq!(inspect_quote("back\\slash"), "'back\\\\slash'");
        assert_eq!(inspect_quote("`tick`"), "'`tick`'");
        assert_eq!(
            inspect_quote("mixed `t` ' \" all"),
            "'mixed `t` \\' \" all'"
        );
        assert_eq!(inspect_quote(""), "''");
    }

    #[test]
    fn node_paths() {
        assert_eq!(node_join("/foo/", "data"), "/foo/data");
        assert_eq!(node_join("/foo//bar", "data"), "/foo/bar/data");
        assert_eq!(node_join("rel/x", "data"), "rel/x/data");
        assert_eq!(node_normalize("/a/b/../c/./d/"), "/a/c/d");
    }

    #[test]
    fn bytes_format() {
        assert_eq!(format_bytes(0.0), "0 B");
        assert_eq!(format_bytes(100.0), "100 B");
        assert_eq!(format_bytes(2048.0), "2.0 KB");
        assert_eq!(format_bytes(1536.0), "1.5 KB");
        assert_eq!(format_bytes(5.5 * 1024.0 * 1024.0), "5.5 MB");
        assert_eq!(format_bytes(2.25 * 1024.0 * 1024.0 * 1024.0), "2.25 GB");
    }
}
