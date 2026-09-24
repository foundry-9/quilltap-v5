//! **The web crate's shared source-census helpers (P4.115 item 5 — P4.110's
//! Tier 3 item 7).** The web crate's two scanners of `api/types.rs`'s
//! `pub enum Request` — `dispatch_wrong_type_census.rs` and
//! `tri_state_edges_share_the_decoder.rs` — had each written their own
//! `repo_root`, `types_rs`, enum-body walker, top-level splitter and, worse,
//! their own and DIFFERENT strip rule. They share this module instead (a plain
//! `mod source_census;` in each, the harness crate's idiom — no test module is
//! shared ACROSS crates anywhere in the repo, and the web crate's dev-deps do
//! not include the harness; lifting onto the harness's token lexer is recorded
//! as a deferral).
//!
//! ## ONE strip rule — [`strip_comments_and_attrs`]
//!
//! The dispatch census's old `strip_noise` blanked only a LINE that STARTED
//! with `//` or `#[`, which leaves a multi-line attribute's continuation lines
//! (and their stray top-level commas) in the stream — hit at P4.D163
//! (`dispatch-census-strip-noise-and-multi-line-serde-attrs`); `types.rs` has
//! one such attribute today (`ChatUpdate.concierge_state`'s
//! `#[serde(\n default,\n …\n)]`). The tri-state census's rule — drop every
//! `//` comment to end of line and every bracket-balanced `#[...]` span — is
//! the one kept, made stricter in the one way the
//! `a-source-census-needs-a-lexer-and-must-keep-literals` note demands: it
//! SKIPS string and char literals (copying them through verbatim), so a `//`
//! or `#[` inside a literal is neither a comment nor an attribute, and a `]`
//! inside an attribute's string argument cannot end the attribute early.
//! Operating on `char`s keeps it UTF-8 safe.

#![allow(dead_code)] // each census uses the parts it needs

use std::path::PathBuf;

/// The repo root: the web crate sits two levels under it.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the web crate sits two levels under the repo root")
        .to_path_buf()
}

/// `crates/quilltap-core/src/api/types.rs`, read whole.
pub fn types_rs() -> String {
    let p = repo_root().join("crates/quilltap-core/src/api/types.rs");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Copy the string literal starting at `chars[i]` (`"` or a raw `r#*"`)
/// through to `out`; return the index just past it. `None` when `chars[i]`
/// does not open one.
fn copy_string_literal(chars: &[char], i: usize, out: &mut String) -> Option<usize> {
    // Raw string: `r"…"` / `r#"…"#` (optionally `br…`), not part of an ident.
    if chars[i] == 'r' && (i == 0 || !is_ident_char(chars[i - 1]) || chars[i - 1] == 'b') {
        let mut j = i + 1;
        let mut hashes = 0;
        while chars.get(j) == Some(&'#') {
            hashes += 1;
            j += 1;
        }
        if chars.get(j) == Some(&'"') {
            j += 1;
            loop {
                match chars.get(j) {
                    None => break,
                    Some('"') if (1..=hashes).all(|h| chars.get(j + h) == Some(&'#')) => {
                        j += 1 + hashes;
                        break;
                    }
                    _ => j += 1,
                }
            }
            out.extend(&chars[i..j]);
            return Some(j);
        }
        return None;
    }
    if chars[i] != '"' {
        return None;
    }
    let mut j = i + 1;
    while j < chars.len() {
        match chars[j] {
            '\\' => j += 2,
            '"' => {
                j += 1;
                break;
            }
            _ => j += 1,
        }
    }
    let j = j.min(chars.len());
    out.extend(&chars[i..j]);
    Some(j)
}

/// Copy a char literal (`'x'`, `'\n'`, `'\''`, `'\u{..}'`) starting at
/// `chars[i]`; `None` for a lifetime (`'a`) or anything else.
fn copy_char_literal(chars: &[char], i: usize, out: &mut String) -> Option<usize> {
    if chars[i] != '\'' {
        return None;
    }
    let end = if chars.get(i + 1) == Some(&'\\') {
        // An escape: scan to the closing quote.
        let mut j = i + 2;
        while j < chars.len() && chars[j] != '\'' {
            j += 1;
        }
        j
    } else if chars.get(i + 2) == Some(&'\'') {
        i + 2
    } else {
        return None; // a lifetime
    };
    let j = (end + 1).min(chars.len());
    out.extend(&chars[i..j]);
    Some(j)
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Remove `//` line comments (doc comments included) and bracket-balanced
/// `#[...]` / `#![...]` attribute spans, keeping string and char literals
/// verbatim (module header).
pub fn strip_comments_and_attrs(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some(j) = copy_string_literal(&chars, i, &mut out) {
            i = j;
            continue;
        }
        if let Some(j) = copy_char_literal(&chars, i, &mut out) {
            i = j;
            continue;
        }
        if chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        let attr_open = chars[i] == '#'
            && (chars.get(i + 1) == Some(&'[')
                || (chars.get(i + 1) == Some(&'!') && chars.get(i + 2) == Some(&'[')));
        if attr_open {
            // Skip to the bracket that balances the attribute's opening `[`,
            // stepping over any string literal inside it (a `]` in a
            // `rename = "…"` argument is not the attribute's end).
            let mut depth = 0i32;
            let mut sink = String::new();
            while i < chars.len() {
                if let Some(j) = copy_string_literal(&chars, i, &mut sink) {
                    i = j;
                    continue;
                }
                match chars[i] {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// The text between `pub enum Request`'s braces, from source already passed
/// through [`strip_comments_and_attrs`] (a brace in a comment would otherwise
/// unbalance the walk).
pub fn request_enum_body(stripped: &str) -> String {
    let start = stripped.find("pub enum Request").expect("the Request enum");
    let open = stripped[start..].find('{').expect("enum body") + start;
    let mut depth = 0i32;
    let mut end = open;
    for (i, ch) in stripped[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    stripped[open + 1..end].to_string()
}

/// `types.rs`'s `Request` enum body, stripped — the one entry both censuses
/// start from.
pub fn request_body() -> String {
    request_enum_body(&strip_comments_and_attrs(&types_rs()))
}

/// Split a body on top-level commas, counting only the delimiters in `open` /
/// `close`.
///
/// The call sites need DIFFERENT delimiter sets, and getting that wrong is
/// silent: the variant split counts braces alone ([`VARIANT_OPEN`] /
/// [`VARIANT_CLOSE`]), because a `[u8]` in a type would otherwise unbalance it
/// and make one variant swallow its neighbours — which is exactly what a first
/// draft of the dispatch census's parser did, reporting fields under the wrong
/// variant name. The field split additionally counts `<>` and `()`
/// ([`FIELD_OPEN`] / [`FIELD_CLOSE`]) so `Option<Vec<String>>` stays one
/// field. Neither counts `[]`.
pub fn split_top_level(body: &str, open: &str, close: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in body.chars() {
        if open.contains(ch) {
            depth += 1;
        } else if close.contains(ch) {
            depth -= 1;
        }
        if ch == ',' && depth == 0 {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    out.push(cur);
    out
}

pub const VARIANT_OPEN: &str = "{";
pub const VARIANT_CLOSE: &str = "}";
pub const FIELD_OPEN: &str = "{<(";
pub const FIELD_CLOSE: &str = "}>)";

/// A variant chunk's name (the last word before its `{`, or the unit
/// variant's name).
pub fn variant_name(variant: &str) -> Option<&str> {
    let head = variant.split('{').next().unwrap_or("").trim();
    head.trim_end_matches(',').split_whitespace().next_back()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_survive_and_multi_line_attributes_vanish() {
        let src = "#[serde(\n  default,\n  rename = \"a]b\",\n)]\nx: \"// not a comment #[x]\", // gone\ny: '[', z: &'a str";
        let got = strip_comments_and_attrs(src);
        assert_eq!(got, "\nx: \"// not a comment #[x]\", \ny: '[', z: &'a str");
    }
}
