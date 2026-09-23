//! The source-census LEXER, shared (P4.110).
//!
//! One home for the scanner that `compressed_column_write_sites_census.rs`
//! (P4.D203) built and `blob_write_sites_census.rs` (P4.104) copied verbatim —
//! lifted here unchanged (bar `pub`), so a fix to the lexer reaches every census
//! that stands on it. The block's own why-comments travel with it: the lexer
//! must skip strings, raw strings, char literals and both comment forms when
//! hunting an item's closing brace, and it must KEEP string literals in its
//! output, because the thing a SQL census searches for IS a literal.
//!
//! **Not every scanner in `tests/` belongs here.** `stream_watchdog_wrap_census.rs`
//! balances braces by counting characters (safe only over its curated list) and
//! `zod_issues_home_guard.rs` strips test modules its own hardened way — both
//! deliberately different, both left alone.
//!
//! Consumers: `mod source_census;` beside `mod common;` — a directory module, so
//! cargo does not build it as a test binary of its own.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub fn core_src_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the harness crate sits two levels under the repo root")
        .join("crates/quilltap-core/src")
}

pub fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// The file with every `#[cfg(test)]` item removed — and **everything else
/// kept verbatim**, string literals included.
///
/// Two properties that pull in opposite directions, and getting them mixed up
/// cost this census its most important arm:
///
/// 1. **Finding an item's boundary needs a LEXER.** The sibling census
///    (`stream_watchdog_wrap_census.rs`) balances braces by counting `{`/`}`
///    characters, which works because it only ever scans a curated file list.
///    Run over all of `crates/quilltap-core/src` the same counter panics on
///    **ten files** — `cycle_order.rs`, `select_speaker.rs`,
///    `db/chats_read.rs`, `db/fictional_clock_anchor_repair.rs`,
///    `api/generators_wizard.rs`, `generators/llm_json.rs`,
///    `services/chat_events.rs`, `services/agent_mode.rs`,
///    `services/off_scene.rs`, `services/avatar_cache.rs` — every one a test
///    module holding JSON fixture text whose braces sit inside a STRING
///    literal. So [`next_token`] skips strings, raw strings, char literals and
///    both comment forms when hunting for the closing brace.
///
/// 2. **The OUTPUT must keep string literals.** The thing this census searches
///    for — `INSERT INTO chat_messages (… content …)` — *is* a string literal.
///    An earlier draft emitted a space in place of every literal, and its
///    mutation proof duly survived: a brand-new file with an unconverted
///    `chat_messages` insert was not caught, because the insert had been
///    elided before the search ever ran.
///
/// Stripping test items is not optional the other way either: a test module's
/// seeds INSERT into `chat_messages` freely against trigger-less in-memory
/// DDL, and counting those would drown the production signal.
pub fn production_zone(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    let mut kept_from = 0usize;
    while i < src.len() {
        if src[i..].starts_with("#[cfg(test)]") {
            out.push_str(&src[kept_from..i]);
            match test_item_end(src, i) {
                Some(end) => {
                    i = end;
                    kept_from = end;
                    continue;
                }
                // Unterminated: keep the rest rather than silently dropping it.
                None => {
                    kept_from = i;
                    break;
                }
            }
        }
        // Step by TOKEN, not by char, so a `#[cfg(test)]` inside a string or a
        // comment is not mistaken for a real attribute.
        let (tok_end, _is_code) = next_token(src, i);
        i = tok_end.max(i + 1);
    }
    out.push_str(&src[kept_from.min(src.len())..]);
    out
}

/// One lexical step from `at`: `(end, is_code)`. A string, raw string, char
/// literal or comment is NOT code; everything else is.
pub fn next_token(src: &str, at: usize) -> (usize, bool) {
    let rest = &src[at..];
    if rest.starts_with("//") {
        let end = rest.find('\n').map(|n| at + n).unwrap_or(src.len());
        return (end, false);
    }
    if rest.starts_with("/*") {
        let mut depth = 0usize;
        let mut idx = 0usize;
        while idx < rest.len() {
            if rest[idx..].starts_with("/*") {
                depth += 1;
                idx += 2;
            } else if rest[idx..].starts_with("*/") {
                depth -= 1;
                idx += 2;
                if depth == 0 {
                    return (at + idx, false);
                }
            } else {
                idx += next_char_len(rest, idx);
            }
        }
        return (src.len(), false);
    }
    // Raw string: r"…", r#"…"#, br"…", br#"…"#
    let raw_prefix = if rest.starts_with("r\"") || rest.starts_with("r#") {
        Some(1usize)
    } else if rest.starts_with("br\"") || rest.starts_with("br#") {
        Some(2usize)
    } else {
        None
    };
    if let Some(prefix) = raw_prefix {
        let hashes = rest[prefix..].chars().take_while(|c| *c == '#').count();
        let open = prefix + hashes;
        if rest[open..].starts_with('"') {
            let mut close = String::from("\"");
            for _ in 0..hashes {
                close.push('#');
            }
            let body = &rest[open + 1..];
            let end = body
                .find(&close)
                .map(|n| at + open + 1 + n + close.len())
                .unwrap_or(src.len());
            return (end, false);
        }
    }
    if rest.starts_with('"') {
        let mut idx = 1usize;
        while idx < rest.len() {
            let b = rest.as_bytes()[idx];
            if b == b'\\' {
                idx += 1 + next_char_len(rest, idx + 1);
                continue;
            }
            if b == b'"' {
                return (at + idx + 1, false);
            }
            idx += next_char_len(rest, idx);
        }
        return (src.len(), false);
    }
    // A char literal, distinguished from a LIFETIME by its closing quote.
    if rest.starts_with('\'') {
        let idx = if rest.as_bytes().get(1) == Some(&b'\\') {
            // `'\n'`, `'\\'`, `'\u{1f600}'` — scan to the closing quote.
            rest[2..]
                .find('\'')
                .map(|n| 2 + n)
                .unwrap_or(rest.len().min(2))
        } else {
            1 + next_char_len(rest, 1)
        };
        if rest.as_bytes().get(idx) == Some(&b'\'') {
            return (at + idx + 1, false);
        }
        // Otherwise it is a lifetime; fall through as code.
    }
    (at + next_char_len(src, at), true)
}

pub fn next_char_len(s: &str, at: usize) -> usize {
    s[at..].chars().next().map(char::len_utf8).unwrap_or(1)
}

/// The end byte of the `#[cfg(test)]` item starting at `at` — its balanced
/// body, or the statement's `;` for a brace-less item (a `use`).
pub fn test_item_end(src: &str, at: usize) -> Option<usize> {
    let mut i = at + "#[cfg(test)]".len();
    let mut brace_start = None;
    while i < src.len() {
        let (end, is_code) = next_token(src, i);
        if is_code {
            match src.as_bytes()[i] {
                b'{' => {
                    brace_start = Some(i);
                    break;
                }
                b';' => return Some(end),
                _ => {}
            }
        }
        i = end;
    }
    let mut i = brace_start?;
    let mut depth = 0usize;
    while i < src.len() {
        let (end, is_code) = next_token(src, i);
        if is_code {
            match src.as_bytes()[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(end);
                    }
                }
                _ => {}
            }
        }
        i = end;
    }
    None
}

/// The nearest char boundary at or below `at` — a byte window over Rust source
/// full of em dashes cannot be sliced naively.
pub fn floor_boundary(s: &str, mut at: usize) -> usize {
    while at > 0 && !s.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The production zone with COMMENTS and STRING LITERALS removed — the view
/// [`codec_calls`] counts over.
///
/// Needed because the zone is verbatim (it must be, so arm (b) can read the
/// SQL), which means a prose mention of `text_to_blob()` in a why-comment
/// counts as a call. That inflated `db/avatar_rolls_collapse_heal.rs` from 2 to
/// 3 on first run, the extra being this port's own comment explaining that the
/// heal writes back through `text_to_blob()`.
pub fn code_only(zone: &str) -> String {
    let mut out = String::with_capacity(zone.len());
    let mut i = 0usize;
    while i < zone.len() {
        let (end, is_code) = next_token(zone, i);
        if is_code {
            out.push_str(&zone[i..end]);
        } else {
            out.push(' ');
        }
        i = end.max(i + 1);
    }
    out
}

/// Every string literal in the zone, body only.
///
/// This is the unit arm (b) tests, and it must be: a byte window around the
/// table name is not a statement. Two false positives proved it on first run —
/// `services/backup/collect.rs`, where `createdAt, updatedAt FROM
/// conversation_chunks` put the substring `UPDATE` in the backward window, and
/// `services/collapse_stale_chat_caches.rs`, whose genuinely-clean `UPDATE
/// chat_messages SET rawResponse = NULL …` sat 900 bytes from a comment reading
/// "(keep content)". A Rust multi-line SQL statement is ONE literal with `\`
/// continuations, so the literal is exactly the right granularity.
pub fn string_literals(zone: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < zone.len() {
        let starts_literal = zone[i..].starts_with('"')
            || zone[i..].starts_with("r\"")
            || zone[i..].starts_with("r#")
            || zone[i..].starts_with("br\"")
            || zone[i..].starts_with("br#");
        let (end, is_code) = next_token(zone, i);
        if !is_code && starts_literal && end > i {
            out.push(&zone[i..end]);
        }
        i = end.max(i + 1);
    }
    out
}

/// Word-bounded `find`: `UPDATE` must not match inside `updatedAt`.
pub fn contains_word(haystack: &str, needle: &str) -> bool {
    let mut from = 0usize;
    while let Some(at) = haystack[from..].find(needle) {
        let abs = from + at;
        let before_ok = abs == 0
            || !haystack.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && haystack.as_bytes()[abs - 1] != b'_';
        let after = abs + needle.len();
        let after_ok = match haystack.as_bytes().get(after) {
            None => true,
            Some(b) => !b.is_ascii_alphanumeric() && *b != b'_',
        };
        if before_ok && after_ok {
            return true;
        }
        from = after;
    }
    false
}
