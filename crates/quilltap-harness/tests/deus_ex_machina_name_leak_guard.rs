//! The name-leak guard's SOURCE half (P4.D372 item 3; v4
//! `__tests__/unit/lib/chat/established-fact-leaks.test.ts`'s `UI_NAME`).
//!
//! "Deus Ex Machina" is the Salon's label for established facts and nothing
//! else; every word a model reads says *established fact*. v4 enforces it by
//! scanning its module's exported strings and every frame's output. This census
//! scans every STRING LITERAL in every `.rs` file under `crates/*/src` for v4's
//! `/deus\s+ex\s+machina/i`. Comments and doc comments may name the feature
//! (v4's own module doc does); a literal may not, because a literal is what
//! reaches a prompt. The allow-list is EMPTY.
//!
//! The behavioural half — a seat line and a machine fact walked through every
//! model-facing path, the right frame present and the name absent — is
//! `quilltap_core::established_fact::leak_tests`.
//!
//! The lexer is deliberately small: line and (nested) block comments, `"…"`
//! with escapes, raw `r#"…"#`, byte strings, char literals versus lifetimes.
//! A literal's simple escapes (`\n`, `\t`, `\r`, `\u{…}`) are decoded before the
//! match, so `"Deus\nEx Machina"` counts. The self-tests below prove the
//! detector fires on a planted literal and stays quiet on a comment.
//!
//! Run: `cargo test -p quilltap-harness --test deus_ex_machina_name_leak_guard`

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("harness crate sits two levels under the repo root")
        .to_path_buf()
}

fn ui_name() -> regex::Regex {
    regex::Regex::new(r"(?i)deus\s+ex\s+machina").unwrap()
}

/// Decode the simple escapes of a non-raw literal body.
fn decode(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut it = body.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('u') => {
                let mut hex = String::new();
                if it.peek() == Some(&'{') {
                    it.next();
                    for h in it.by_ref() {
                        if h == '}' {
                            break;
                        }
                        hex.push(h);
                    }
                }
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            // A line continuation (`\` + newline + indentation) is one join.
            Some('\n') => {
                while it.peek().is_some_and(|c| c.is_whitespace()) {
                    it.next();
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// Every string literal in `src`, decoded, with its 1-based line.
fn string_literals(src: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    let n = chars.len();
    let count_lines = |s: &[char]| s.iter().filter(|c| **c == '\n').count();
    while i < n {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        // Line comment.
        if c == '/' && i + 1 < n && chars[i + 1] == '/' {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // Block comment (nested).
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            let mut depth = 1;
            i += 2;
            while i < n && depth > 0 {
                if chars[i] == '\n' {
                    line += 1;
                }
                if chars[i] == '/' && i + 1 < n && chars[i + 1] == '*' {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && i + 1 < n && chars[i + 1] == '/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        // Raw string: r"…", r#"…"#, br#"…"#.
        let raw_start = if c == 'r' {
            Some(i + 1)
        } else if c == 'b' && i + 1 < n && chars[i + 1] == 'r' {
            Some(i + 2)
        } else {
            None
        };
        let ident_before = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        if let (Some(mut j), false) = (raw_start, ident_before) {
            let mut hashes = 0;
            while j < n && chars[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < n && chars[j] == '"' {
                let start = j + 1;
                let mut k = start;
                'scan: while k < n {
                    if chars[k] == '"' {
                        let mut h = 0;
                        while h < hashes && k + 1 + h < n && chars[k + 1 + h] == '#' {
                            h += 1;
                        }
                        if h == hashes {
                            break 'scan;
                        }
                    }
                    k += 1;
                }
                let body: String = chars[start..k.min(n)].iter().collect();
                out.push((line, body));
                line += count_lines(&chars[start..k.min(n)]);
                i = k + 1 + hashes;
                continue;
            }
        }
        // Ordinary (or byte) string.
        if c == '"' {
            let start = i + 1;
            let mut k = start;
            while k < n && chars[k] != '"' {
                if chars[k] == '\\' {
                    k += 1;
                }
                k += 1;
            }
            let body: String = chars[start..k.min(n)].iter().collect();
            out.push((line, decode(&body)));
            line += count_lines(&chars[start..k.min(n)]);
            i = k + 1;
            continue;
        }
        // Char literal versus lifetime: `'x'`, `'\n'`, `'\u{..}'`; else a lifetime.
        if c == '\'' {
            if i + 1 < n && chars[i + 1] == '\\' {
                let mut k = i + 3; // past the backslash AND the escaped char (`'\''`)
                while k < n && chars[k] != '\'' {
                    k += 1;
                }
                i = k + 1;
                continue;
            }
            if i + 2 < n && chars[i + 2] == '\'' {
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let crates = root.join("crates");
    for krate in std::fs::read_dir(&crates).expect("read crates/") {
        let src = krate.expect("crate entry").path().join("src");
        if src.is_dir() {
            walk(&src, &mut out);
        }
    }
    out.sort();
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("read dir") {
        let p = e.expect("entry").path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The allow-list — EMPTY (v4's has no exception either).
const ALLOWED: &[&str] = &[];

#[test]
fn no_string_literal_under_crates_src_names_the_feature() {
    let root = repo_root();
    let re = ui_name();
    let files = rust_sources(&root);
    assert!(
        files.len() > 500,
        "scanned only {} files — the walk is broken",
        files.len()
    );
    let mut literals = 0usize;
    let mut hits = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap_or_else(|e| panic!("read {f:?}: {e}"));
        for (line, lit) in string_literals(&text) {
            literals += 1;
            if re.is_match(&lit) {
                let rel = f.strip_prefix(&root).unwrap().display().to_string();
                if !ALLOWED.contains(&rel.as_str()) {
                    hits.push(format!("{rel}:{line}: {lit:?}"));
                }
            }
        }
    }
    eprintln!(
        "deus_ex_machina_name_leak_guard: {} files, {literals} string literals scanned",
        files.len()
    );
    assert!(
        literals > 10_000,
        "only {literals} literals — the lexer is broken"
    );
    assert!(
        hits.is_empty(),
        "the UI name reaches a string literal (a model could read it):\n{}",
        hits.join("\n")
    );
}

#[test]
fn the_detector_fires_on_a_planted_literal_and_ignores_comments() {
    let re = ui_name();
    let hit = |src: &str| {
        string_literals(src)
            .iter()
            .filter(|(_, l)| re.is_match(l))
            .count()
    };
    // Mutation proof: the label planted in a const is caught, in every literal form.
    assert_eq!(hit("const X: &str = \"Deus Ex Machina\";"), 1);
    assert_eq!(hit("const X: &str = r#\"deus ex machina\"#;"), 1);
    assert_eq!(hit("const X: &str = \"Deus\\nEx Machina\";"), 1);
    assert_eq!(hit("const X: &str = \"Deus \\\n    Ex Machina\";"), 1);
    // Comments and doc comments may name it.
    assert_eq!(
        hit("// Deus Ex Machina\n/// Deus Ex Machina\n/* Deus Ex Machina */ fn f() {}"),
        0
    );
    // Lifetimes and char literals do not derail the lexer.
    assert_eq!(hit("fn f<'a>(x: &'a str) -> char { let q = '\"'; 'x' } const Y: &str = \"deus ex machina\";"), 1);
    // A regex spelling of the pattern (as the leak tests carry) is not the name.
    assert_eq!(hit(r#"const R: &str = r"(?i)deus\s+ex\s+machina";"#), 0);
    assert_eq!(
        hit("let q = '\\''; const Z: &str = \"Deus Ex Machina\";"),
        1
    );
}
