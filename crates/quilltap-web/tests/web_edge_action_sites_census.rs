//! P4.D220 Tier 2 item 9 — **every `?action=` read on a v5 REST edge goes
//! through the ONE helper.**
//!
//! v4 `ad1c4c37f` consolidated its action dispatch into one primitive
//! (`dispatchAction`) precisely because nine routes had hand-rolled their own
//! reading of `?action=` — and several of them let a bare `?action=` fall
//! through to a default that deleted, restored or uploaded. v5 had the same
//! spread: `query::action()` folded the bare action into "absent", and three
//! edges read the raw value with `query::first(…, "action")` / a
//! `first_map(…).get("action")`. After P4.D220 the only reader of the `action`
//! query key is `query.rs` (`action_param` → `dispatch_action` /
//! `dispatch_required_action`); this census keeps it that way.
//!
//! **How:** every `crates/quilltap-web/src/*.rs` except `query.rs` is passed
//! through the shared lexer-based strip (`source_census::
//! strip_comments_and_attrs` — comments and attributes gone, string literals
//! KEPT verbatim, per `a-source-census-needs-a-lexer-and-must-keep-literals`),
//! its `#[cfg(test)]` modules are removed by BRACE balance over a
//! literal-blanked copy (`an-in-file-source-census-must-strip-test-modules-by-
//! braces`), and every remaining `"action"` string literal is counted. The
//! count must equal the ALLOWED list exactly — each entry a named site that is
//! NOT a query read.
//!
//! Run:
//!   cargo test -p quilltap-web --test web_edge_action_sites_census

mod source_census;

/// The `"action"` literals that are NOT query reads, by file, with the count.
/// `system_data_routes.rs`: the `tasks-queue` POST's BODY field (`{ action:
/// 'start' | 'stop' }`, v4 `handleTasksQueueControl`), not `?action=`.
const ALLOWED: &[(&str, usize)] = &[("system_data_routes.rs", 1)];

/// Replace the inside of every string / char literal with spaces so a brace
/// in a literal cannot unbalance the test-module walk (the strip keeps
/// literals verbatim on purpose; this copy is for STRUCTURE only).
fn blank_literals(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // raw strings r"…" / r#"…"#
        if c == 'r' && i + 1 < chars.len() && (chars[i + 1] == '"' || chars[i + 1] == '#') {
            let mut j = i + 1;
            let mut hashes = 0;
            while j < chars.len() && chars[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < chars.len() && chars[j] == '"' {
                out.push('r');
                for _ in 0..hashes {
                    out.push('#');
                }
                out.push('"');
                j += 1;
                loop {
                    if j >= chars.len() {
                        break;
                    }
                    if chars[j] == '"' && (0..hashes).all(|k| chars.get(j + 1 + k) == Some(&'#')) {
                        out.push('"');
                        for _ in 0..hashes {
                            out.push('#');
                        }
                        j += 1 + hashes;
                        break;
                    }
                    out.push(if chars[j] == '\n' { '\n' } else { ' ' });
                    j += 1;
                }
                i = j;
                continue;
            }
        }
        if c == '"' {
            out.push('"');
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '"' {
                if chars[j] == '\\' {
                    out.push(' ');
                    j += 1;
                }
                out.push(if chars.get(j) == Some(&'\n') {
                    '\n'
                } else {
                    ' '
                });
                j += 1;
            }
            out.push('"');
            i = j + 1;
            continue;
        }
        if c == '\'' {
            // a char literal ('x', '\n', '\'') — not a lifetime
            let is_char = matches!(
                (chars.get(i + 1), chars.get(i + 2)),
                (Some('\\'), _) | (Some(_), Some('\''))
            );
            if is_char {
                let mut j = i + 1;
                if chars[j] == '\\' {
                    j += 2;
                }
                while j < chars.len() && chars[j] != '\'' {
                    j += 1;
                }
                // same LENGTH as the literal: quote, blanks, quote
                out.push('\'');
                for _ in i + 1..j {
                    out.push(' ');
                }
                out.push('\'');
                i = j + 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Remove every module marked `#[cfg(test)]`. The marker is rewritten to an
/// identifier-like sentinel BEFORE the strip (which drops attributes), then
/// each sentinel's following `mod … { … }` is cut by brace balance measured on
/// the literal-blanked copy (char offsets agree: blanking preserves length).
fn strip_test_modules(src: &str) -> String {
    const SENTINEL: &str = "CFGTESTSENTINEL";
    let marked = src.replace("#[cfg(test)]", &format!("{SENTINEL} "));
    let stripped = source_census::strip_comments_and_attrs(&marked);
    let chars: Vec<char> = stripped.chars().collect();
    let blank: Vec<char> = blank_literals(&stripped).chars().collect();
    assert_eq!(chars.len(), blank.len(), "blanking must preserve offsets");
    let needle: Vec<char> = SENTINEL.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if blank[i..].starts_with(&needle) {
            // the next `{` opens the module body (a `;` ends a body-less item)
            let mut j = i + needle.len();
            while j < blank.len() && blank[j] != '{' && blank[j] != ';' {
                j += 1;
            }
            if j < blank.len() && blank[j] == '{' {
                let mut depth = 0i32;
                while j < blank.len() {
                    match blank[j] {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
            }
            i = j + 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[test]
fn every_action_read_goes_through_the_one_helper() {
    let src_dir = source_census::repo_root().join("crates/quilltap-web/src");
    let mut files: Vec<_> = std::fs::read_dir(&src_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    files.sort();
    assert!(files.len() > 30, "the scan must see the whole web src tree");

    let mut found: Vec<(String, usize)> = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name == "query.rs" {
            continue;
        }
        let src = std::fs::read_to_string(path).unwrap();
        let body = strip_test_modules(&src);
        let n = body.matches("\"action\"").count();
        if n > 0 {
            found.push((name, n));
        }
    }
    let allowed: Vec<(String, usize)> = ALLOWED
        .iter()
        .map(|(f, n)| ((*f).to_string(), *n))
        .collect();
    assert_eq!(
        found, allowed,
        "a raw `\"action\"` read outside `query.rs` — route it through \
         `query::dispatch_action` / `dispatch_required_action` (v4's ONE \
         `dispatchAction`), or name the non-query site in ALLOWED"
    );
}

/// The strip itself: a `"action"` inside a comment or a test module is not a
/// read; one in live code is — and a brace inside a literal does not end the
/// test module early.
#[test]
fn the_strip_keeps_live_code_and_drops_tests_and_comments() {
    let src = r#"
fn live() { let _ = first(p, "action"); } // "action" in a comment
#[cfg(test)]
mod tests {
    fn t() { let _ = "}"; let _ = get("action"); }
}
fn after() { let _ = "action"; }
"#;
    let body = strip_test_modules(src);
    assert_eq!(body.matches("\"action\"").count(), 2, "{body}");
    assert!(
        body.contains("fn after"),
        "code after the test module survives: {body}"
    );
}
