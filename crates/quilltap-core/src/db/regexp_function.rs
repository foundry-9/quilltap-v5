//! The `regexp()` SQL function — JavaScript regular expressions, visible from
//! SQL. v4 `lib/database/backends/sqlite/regexp-function.ts` (`e4a60ad9e`, with
//! `a495faa26`'s reason strip; P4.D331).
//!
//! v4's why, carried: "SQLite parses `X REGEXP Y` but ships no implementation:
//! the operator calls a user function named `regexp(Y, X)` and fails with 'no
//! such function: REGEXP' when none is registered. This module supplies one,
//! backed by the JavaScript `RegExp` engine, so the operator works and so does
//! the call form with an optional third argument of flags." A pattern that does
//! not compile throws, which SQLite surfaces as the statement's error —
//! "callers that take a pattern from a model should compile it first
//! ([`compile_sql_regexp`]) and answer the mistake in plain words."
//!
//! **Registered ONLY on llm-logs connections** (v4 `llm-logs-client.ts:56` is
//! the one call site in `lib/`), where the `log_entries` table `log_grep`
//! searches lives — so `x REGEXP 'y'` on `main` or the mount index still fails
//! `no such function: REGEXP`, as it does in v4. The openers take the
//! partition ([`crate::db::Writer::open_writable_in`],
//! [`crate::db::runtime::open_readonly`]) and call [`register_regexp`] in their
//! `LlmLogs` arm, AFTER `qt_text` (v4's order); NOT the every-connection
//! `register_qt_text` pattern. v4's CLI registers no `regexp` either
//! (`packages/quilltap/lib/db-helpers.js:206-211`).
//!
//! **The engine is `regress`** (an ECMAScript regex engine — R-I, approved
//! 2026-10-21): lookaround, backreferences, named groups, ASCII `\d` / `\w` /
//! `\b`, V8's `.` and the `i` / `m` / `s` / `u` semantics, none of which the
//! `regex` crate has. The SUBJECT is matched as UTF-16 (`find_from_utf16`): a
//! JS string is UTF-16, so without `u` a pattern sees code units (`^.$` does
//! not match an astral character; `^..$` does). The engine's own error texts
//! are not V8's, so [`v8_message`] maps them onto V8's `REGEXP_ERROR_MESSAGES`
//! — the mapping is pinned row by row by `regexp_function_equivalence`'s
//! compile corpus, and every syntax the two engines disagree on is a named
//! `REGEXP_ENGINE_GAP` row there, never a silent approximation.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use rusqlite::functions::{Context, FunctionFlags};
use rusqlite::types::ValueRef;
use rusqlite::Connection;

/// The SQL-visible name. SQLite's `REGEXP` operator resolves to it.
pub const REGEXP_FUNCTION_NAME: &str = "regexp";

/// v4 `ALLOWED_FLAGS = /^[imsu]*$/` — "`g` and `y` are stateful and
/// meaningless for a match test." A REPEATED legal flag passes this class test
/// and then fails V8's constructor.
pub fn allowed_flags(flags: &str) -> bool {
    flags.chars().all(|c| matches!(c, 'i' | 'm' | 's' | 'u'))
}

/// v4 `CACHE_LIMIT` — "a scan uses one entry, so this is generous." A WHOLE
/// cache clear at the limit, not LRU; process-global, its state never
/// observable.
pub const CACHE_LIMIT: usize = 64;

/// v4's flags reason (an EM DASH).
pub fn unsupported_flags_reason(flags: &str) -> String {
    format!("unsupported flags \"{flags}\" — use any of i, m, s, u")
}

/// A compiled pattern (opaque).
#[derive(Clone)]
pub struct CompiledRegexp {
    regex: Arc<regress::Regex>,
    unicode: bool,
}

impl CompiledRegexp {
    /// `RegExp.prototype.test` over a JS string: its UTF-16 code units, read
    /// as UCS-2 (one unit, one character) without `u` and as UTF-16 (a
    /// surrogate pair, one character) with it — V8's two modes.
    pub fn test(&self, value: &str) -> bool {
        let units: Vec<u16> = value.encode_utf16().collect();
        if self.unicode {
            self.regex.find_from_utf16(&units, 0).next().is_some()
        } else {
            self.regex.find_from_ucs2(&units, 0).next().is_some()
        }
    }
}

static CACHE: LazyLock<Mutex<HashMap<String, CompiledRegexp>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// v4 `compileSqlRegexp(pattern, flags = '')` — "Compile a pattern the way the
/// SQL function will. Returns the RegExp, or the reason it would not compile —
/// the one statement of what the function accepts." The flags check FIRST; the
/// cache keyed `${flags}/${pattern}`; then the constructor. A syntax error's
/// reason is `/<pattern>/<flags>: <V8 message>` (V8's own `Invalid regular
/// expression:` prefix stripped — the callers add theirs); a repeated flag is
/// V8's `Invalid flags supplied to RegExp constructor '<flags>'` (a message
/// with no such prefix, kept whole).
pub fn compile_sql_regexp(pattern: &str, flags: &str) -> Result<CompiledRegexp, String> {
    if !allowed_flags(flags) {
        return Err(unsupported_flags_reason(flags));
    }
    let key = format!("{flags}/{pattern}");
    if let Some(hit) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return Ok(hit.clone());
    }
    let mut seen = Vec::new();
    for c in flags.chars() {
        if seen.contains(&c) {
            return Err(format!(
                "Invalid flags supplied to RegExp constructor '{flags}'"
            ));
        }
        seen.push(c);
    }
    if let Some(message) = v8_only_refusal(pattern, flags) {
        return Err(format!("/{pattern}/{flags}: {message}"));
    }
    // Without `u` a JS pattern is UTF-16 code units too: an astral literal is
    // TWO characters (a surrogate pair), exactly as the subject is read.
    let built = if flags.contains('u') {
        regress::Regex::with_flags(pattern, flags)
    } else {
        regress::Regex::from_unicode(pattern.encode_utf16().map(u32::from), flags)
    };
    match built {
        Ok(regex) => {
            let compiled = CompiledRegexp {
                regex: Arc::new(regex),
                unicode: flags.contains('u'),
            };
            let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
            if cache.len() >= CACHE_LIMIT {
                cache.clear();
            }
            cache.insert(key, compiled.clone());
            Ok(compiled)
        }
        Err(error) => Err(format!(
            "/{pattern}/{flags}: {}",
            v8_message(&error.text, pattern, flags)
        )),
    }
}

/// Patterns V8 refuses that the engine accepts — checked before the engine
/// (each a measured `regexp_function_equivalence` compile row).
fn v8_only_refusal(pattern: &str, flags: &str) -> Option<&'static str> {
    let unicode = flags.contains('u');
    // V8: a lookbehind cannot be quantified in any mode (`/(?<=a)*/`).
    if has_quantified_lookbehind(pattern) {
        return Some("Invalid quantifier");
    }
    // V8 (annex B off under `u`): a named reference to a group that does not
    // exist is an error whenever the pattern has ANY named group or `u` is set.
    if (unicode || pattern.contains("(?<") && !only_lookbehinds(pattern))
        && pattern.contains("\\k<")
    {
        let names = group_names(pattern);
        let mut rest = pattern;
        while let Some(at) = rest.find("\\k<") {
            let tail = &rest[at + 3..];
            if let Some(end) = tail.find('>') {
                if !names.iter().any(|n| n == &tail[..end]) {
                    return Some("Invalid named capture referenced");
                }
            }
            rest = &rest[at + 3..];
        }
    }
    None
}

fn has_quantified_lookbehind(pattern: &str) -> bool {
    for marker in ["(?<=", "(?<!"] {
        let mut from = 0;
        while let Some(at) = pattern[from..].find(marker) {
            let start = from + at;
            if let Some(close) = matching_paren(pattern, start) {
                if matches!(
                    pattern[close + 1..].chars().next(),
                    Some('*' | '+' | '?' | '{')
                ) {
                    return true;
                }
            }
            from = start + marker.len();
        }
    }
    false
}

fn matching_paren(pattern: &str, open: usize) -> Option<usize> {
    let bytes = pattern.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    let mut in_class = false;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'[' if !in_class => in_class = true,
            b']' if in_class => in_class = false,
            b'(' if !in_class => depth += 1,
            b')' if !in_class => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn only_lookbehinds(pattern: &str) -> bool {
    group_names(pattern).is_empty()
}

fn group_names(pattern: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = pattern;
    while let Some(at) = rest.find("(?<") {
        let tail = &rest[at + 3..];
        if !tail.starts_with('=') && !tail.starts_with('!') {
            if let Some(end) = tail.find('>') {
                names.push(tail[..end].to_string());
            }
        }
        rest = &rest[at + 3..];
    }
    names
}

/// The engine's error text → V8's `REGEXP_ERROR_MESSAGES` entry, by the
/// compile corpus. The engine's messages are coarser than V8's (one
/// "Unbalanced parenthesis" for both `Unterminated group` and `Unmatched ')'`),
/// so the pattern disambiguates where a row needs it.
fn v8_message(engine: &str, pattern: &str, flags: &str) -> String {
    let unicode = flags.contains('u');
    let m = match engine {
        "Unbalanced parenthesis" => {
            let opens = pattern.matches('(').count();
            let closes = pattern.matches(')').count();
            if closes > opens {
                "Unmatched ')'"
            } else {
                "Unterminated group"
            }
        }
        "Unbalanced bracket" => {
            if unicode && (pattern.starts_with(']') || pattern.starts_with('}')) {
                "Lone quantifier brackets"
            } else {
                "Unterminated character class"
            }
        }
        "Quantifier not allowed here" => {
            if pattern.starts_with("(?") {
                "Invalid quantifier"
            } else {
                "Nothing to repeat"
            }
        }
        "Invalid quantifier" => {
            if pattern.contains('}') || pattern.contains('{') {
                if pattern.ends_with('{') || pattern.contains("{,") {
                    "Incomplete quantifier"
                } else {
                    "numbers out of order in {} quantifier"
                }
            } else {
                "Nothing to repeat"
            }
        }
        "Invalid braced quantifier" => {
            if unicode && pattern.starts_with('}') {
                "Lone quantifier brackets"
            } else if pattern.contains("{,") || pattern.ends_with('{') {
                "Incomplete quantifier"
            } else {
                "numbers out of order in {} quantifier"
            }
        }
        "Duplicate capture group name" => "Duplicate capture group name",
        "Invalid property name" | "Invalid property escape" => "Invalid property name",
        "Invalid character range" => {
            if unicode && pattern.contains("\\d-") {
                "Invalid character class"
            } else {
                "Range out of order in character class"
            }
        }
        "Incomplete escape" | "Unterminated escape" => {
            if pattern.ends_with('\\') {
                "\\ at end of pattern"
            } else {
                "Invalid escape"
            }
        }
        "Invalid character escape" => {
            if pattern.starts_with("\\c") {
                "Invalid Unicode escape"
            } else if pattern.contains("\\0") {
                "Invalid decimal escape"
            } else {
                "Invalid escape"
            }
        }
        "Invalid unicode escape" => "Invalid Unicode escape",
        "Invalid atom character" => {
            if unicode && (pattern.starts_with('}') || pattern.starts_with(']')) {
                "Lone quantifier brackets"
            } else if pattern.contains(['*', '+', '?']) {
                "Nothing to repeat"
            } else {
                "Invalid group"
            }
        }
        "Invalid group modifier" => {
            if pattern.contains("(?<") {
                "Invalid capture group name"
            } else {
                "Invalid group"
            }
        }
        "Invalid token at named capture group identifier" => {
            if pattern.contains("(?<") {
                "Invalid capture group name"
            } else {
                "Invalid group"
            }
        }
        "Range values reversed, start char code is greater than end char code." => {
            "Range out of order in character class"
        }
        "Invalid named backreference syntax" => "Invalid named reference",
        "Regular expression is too deeply nested" => "Stack overflow",
        "Capture group count limit exceeded" => "Too many captures",
        "Loop count limit exceeded" => "Regular expression too large",
        other => return other.to_string(),
    };
    m.to_string()
}

/// JS `String(value)` for a SQLite operand: TEXT as is, an INTEGER / REAL as
/// its JS number text (better-sqlite3 hands both over as a JS number), a BLOB
/// as Node's `Buffer#toString()` (UTF-8, lossy) — unreachable through
/// `log_grep`, whose `message` is TEXT.
fn js_string(value: ValueRef<'_>) -> Option<String> {
    match value {
        ValueRef::Null => None,
        ValueRef::Integer(i) => Some(crate::pascal::js_value::number_to_string(i as f64)),
        ValueRef::Real(f) => Some(crate::pascal::js_value::number_to_string(f)),
        ValueRef::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Some(String::from_utf8_lossy(b).into_owned()),
    }
}

/// The failure a bad pattern raises inside the statement: its `Display` is
/// EXACTLY `Invalid regular expression: <reason>` (better-sqlite3 surfaces the
/// thrown Error's message unprefixed as the statement error).
#[derive(Debug)]
pub struct InvalidRegularExpression(pub String);

impl std::fmt::Display for InvalidRegularExpression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid regular expression: {}", self.0)
    }
}

impl std::error::Error for InvalidRegularExpression {}

/// v4 `registerRegexpFunction` — `regexp(pattern, value[, flags])`,
/// deterministic, variadic. "NULL in either of the first two arguments answers
/// NULL (no match), as SQL comparisons do." Called ONLY from the openers'
/// `LlmLogs` arm (R-1; `regexp_registration_sites` pins it).
pub fn register_regexp(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function(
        REGEXP_FUNCTION_NAME,
        -1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx: &Context<'_>| -> rusqlite::Result<Option<i64>> {
            let arg = |i: usize| {
                if i < ctx.len() {
                    ctx.get_raw(i)
                } else {
                    ValueRef::Null
                }
            };
            let (Some(pattern), Some(value)) = (js_string(arg(0)), js_string(arg(1))) else {
                return Ok(None);
            };
            let flags = js_string(arg(2)).unwrap_or_default();
            match compile_sql_regexp(&pattern, &flags) {
                Ok(compiled) => Ok(Some(i64::from(compiled.test(&value)))),
                Err(reason) => Err(rusqlite::Error::UserFunctionError(Box::new(
                    InvalidRegularExpression(reason),
                ))),
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flags_check_runs_first() {
        assert_eq!(
            compile_sql_regexp("(", "g").err().unwrap(),
            "unsupported flags \"g\" — use any of i, m, s, u"
        );
    }

    #[test]
    fn the_operator_form_resolves_to_the_function() {
        let conn = Connection::open_in_memory().unwrap();
        register_regexp(&conn).unwrap();
        let m: i64 = conn
            .query_row("SELECT 'lockpick' REGEXP 'lock(pick|smith)'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(m, 1);
    }

    #[test]
    fn a_bad_pattern_fails_the_statement_with_the_bare_sentence() {
        let conn = Connection::open_in_memory().unwrap();
        register_regexp(&conn).unwrap();
        let err = conn
            .query_row("SELECT regexp('(', 'x')", [], |r| r.get::<_, i64>(0))
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Invalid regular expression: /(/: Unterminated group"
        );
    }
}
