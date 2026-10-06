//! The chat-informs IN-FORCE census (P4.151 B4 — replacing the narrow in-file
//! test P4.D249 landed in `db/chat_informs.rs`'s test module).
//!
//! **v4 asks "is this inform in force?" in ONE place.** `isInformInForce`
//! (`lib/schemas/chat-inform.types.ts:83-84`, `permanent || !consumedAt`)
//! is what the repository's pending read, batch read and delete use
//! (`chat-informs.repository.ts:94,139,277`); `52d6e7ecd` closed the drift
//! class where a read open-coded `!consumedAt` and so dropped a delivered
//! STANDING row. v5's home is `db::chat_informs::is_inform_in_force`.
//!
//! The census it replaces (`db::chat_informs::tests::
//! the_in_force_predicate_is_never_open_coded`) was narrow: it scanned only
//! its own file, counted two raw substrings (`.is_none()` / `.is_some()` on
//! `consumed_at`) over the text — comments and literals included — found the
//! test module by `split("#[cfg(test)]")`, and could not see a SQL
//! `consumedAt IS NULL` at all. This one stands on the shared `source_census`
//! lexer, over every production `.rs` file of core, web, host, cli and tauri:
//!
//! - **(i) code** — over `code_only(production_zone(src))`, TOKENIZED (P4.157
//!   R-E; the regex arm it replaces saw only the method form and `== None`,
//!   so a `match`, a `matches!`, `Option::is_none(&…)` or a rebinding walked
//!   past it): every `consumed_at` presence test — the method form
//!   (`.is_none(` / `.is_some(` / `.is_none_or(` / `.is_some_and(`, through any
//!   zero-argument adapter chain), `== None` / `!= None` either side,
//!   `matches!`, the path form, a `match` scrutinee, a refutable `let` — over
//!   `consumed_at` AND every name a `let` rebinds it to. One committed evasion
//!   fixture per form (`tests/fixtures/chat_informs_in_force_evasions/`) proves
//!   the census trips. The allow-list IS the set of files with a per-file
//!   COUNT: the home, plus the two CORRECT open checks that mirror v4's own
//!   open checks (they ask "was it ever delivered?", not "is it in force?").
//!   A per-file count is a known limit (recorded again by P4.157): a second
//!   presence test added to an allowed file while one is removed nets zero.
//! - **(ii) SQL** — in `string_literals(production_zone(src))`, any literal
//!   that tests `consumedAt` against `NULL` (`consumedAt IS [NOT] NULL`, the
//!   column optionally quoted). Allowed: none — no production SQL names it
//!   today, and one that did would bypass the home entirely.
//!
//! The census keys on the PREDICATE, never on line numbers (P4.149 edits log
//! bytes beside the cancel's open check this round).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test chat_informs_in_force_census

mod source_census;

use std::collections::BTreeSet;
use std::path::PathBuf;

use regex::Regex;
use source_census::{code_only, production_zone, rust_sources, string_literals};

/// `(path from the repo root, presence tests, why)`.
const ALLOWED: &[(&str, usize, &str)] = &[
    (
        "crates/quilltap-core/src/db/chat_informs.rs",
        2,
        "the HOME — `is_inform_in_force` (`row.permanent || \
         row.consumed_at.is_none()`), v4 `isInformInForce` — plus the row's \
         JSON projection omitting an absent `consumedAt` (`if let Some(v) = \
         &r.consumed_at`, a refutable `let` the token census counts since \
         P4.157): v4's parsed row carries no `consumedAt` key when the cell is \
         NULL — a serialization question, not an in-force one.",
    ),
    (
        "crates/quilltap-core/src/services/inform_block.rs",
        1,
        "the block's `row_ids` keeps the NEVER-delivered rows for the consume \
         write (`r.consumed_at.is_none()`), v4 `inform-block.ts:137` \
         `!r.consumedAt` — a delivery question, not an in-force one.",
    ),
    (
        "crates/quilltap-core/src/api/chat_informs.rs",
        1,
        "the cancel's \"ever delivered\" (`r.consumed_at.is_some()`), v4 \
         `app/api/v1/chats/[id]/actions/inform.ts:219` — a delivery \
         question, not an in-force one.",
    ),
];

const CRATES: &[&str] = &[
    "crates/quilltap-core/src",
    "crates/quilltap-web/src",
    "crates/quilltap-host/src",
    "crates/quilltap-cli/src",
    "crates/quilltap-tauri/src",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the harness crate sits two levels under the repo root")
        .to_path_buf()
}

fn production_files() -> Vec<(String, String)> {
    let root = repo_root();
    let mut out = Vec::new();
    for dir in CRATES {
        let dir = root.join(dir);
        if !dir.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        rust_sources(&dir, &mut files);
        for f in files {
            let rel = f
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(&f).unwrap();
            out.push((rel, production_zone(&src)));
        }
    }
    out.sort();
    out
}

fn sql_null_tests() -> Regex {
    // `IS [NOT] NULL`, SQLite's postfix `ISNULL` / `NOTNULL` too.
    Regex::new(r#"(?i)\bconsumedAt\\?"?\s+(?:IS\s*(?:NOT\s*)?NULL|NOTNULL)\b"#).unwrap()
}

/// A raw source literal with Rust's `\`-newline continuations (the house
/// style for multi-line SQL) joined back into one line, so the SQL arm sees
/// `consumedAt \<newline>    IS NULL` as the one clause it is.
fn joined_literal(lit: &str) -> String {
    Regex::new(r"\\\n\s*")
        .unwrap()
        .replace_all(lit, " ")
        .into_owned()
}

/// One lexical token of the census's code view: an identifier (or a number),
/// or punctuation — `::`, `==`, `!=`, `=>`, `->` as one token each.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Ident(String),
    Punct(&'static str),
}

const PUNCT2: &[&str] = &["::", "==", "!=", "=>", "->", "&&", "||", ".."];
const PUNCT1: &[&str] = &[
    "(", ")", "{", "}", "[", "]", "<", ">", ".", ",", ";", ":", "=", "!", "&", "|", "?", "*", "+",
    "-", "/", "%", "^", "@", "#", "$", "~", "'",
];

/// Tokenize a `code_only` view (literals and comments already blanked).
fn tokens(code: &str) -> Vec<Tok> {
    let b = code.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_alphanumeric() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Ident(code[start..i].to_string()));
        } else if let Some(p) = PUNCT2.iter().find(|p| code[i..].starts_with(**p)) {
            out.push(Tok::Punct(p));
            i += 2;
        } else if let Some(p) = PUNCT1.iter().find(|p| code[i..].starts_with(**p)) {
            out.push(Tok::Punct(p));
            i += 1;
        } else {
            // Any other byte (a non-ASCII char in code is not expected) is
            // skipped whole, never split mid-UTF-8.
            i += code[i..].chars().next().map(char::len_utf8).unwrap_or(1);
        }
    }
    out
}

fn is_ident(t: Option<&Tok>, name: &str) -> bool {
    matches!(t, Some(Tok::Ident(s)) if s == name)
}
fn is_punct(t: Option<&Tok>, p: &str) -> bool {
    matches!(t, Some(Tok::Punct(q)) if *q == p)
}

const PRESENCE_METHODS: &[&str] = &["is_none", "is_some", "is_none_or", "is_some_and"];

/// Is token `i` a VALUE mention of a watched name — not a struct field
/// initializer (`consumed_at: None`) and not shorthand init (`{ consumed_at,`)?
fn is_value_mention(t: &[Tok], i: usize, watched: &BTreeSet<String>) -> bool {
    let Some(Tok::Ident(name)) = t.get(i) else {
        return false;
    };
    if !watched.contains(name) {
        return false;
    }
    let prev = i.checked_sub(1).and_then(|j| t.get(j));
    let next = t.get(i + 1);
    if is_punct(prev, ".") {
        return true; // a field access: `r.consumed_at`
    }
    if is_punct(next, ":") {
        return false; // a field initializer / pattern field
    }
    let shorthand = (is_punct(prev, "{") || is_punct(prev, ","))
        && (is_punct(next, ",") || is_punct(next, "}"));
    !shorthand
}

/// The end (exclusive) of the expression starting at `from`: the first
/// depth-0 token in `stops`, or the end.
fn expr_end(t: &[Tok], from: usize, stops: &[&str]) -> usize {
    let mut depth = 0i32;
    let mut i = from;
    while i < t.len() {
        if let Tok::Punct(p) = &t[i] {
            match *p {
                "(" | "[" => depth += 1,
                ")" | "]" => {
                    if depth == 0 {
                        return i;
                    }
                    depth -= 1;
                }
                "{" if depth == 0 && stops.contains(&"{") => return i,
                "{" => depth += 1,
                "}" => {
                    if depth == 0 {
                        return i;
                    }
                    depth -= 1;
                }
                p if depth == 0 && stops.contains(&p) => return i,
                _ => {}
            }
        } else if depth == 0 && stops.contains(&"else") && is_ident(t.get(i), "else") {
            return i;
        }
        i += 1;
    }
    t.len()
}

fn mentions_in(t: &[Tok], range: std::ops::Range<usize>, watched: &BTreeSet<String>) -> bool {
    range.into_iter().any(|i| is_value_mention(t, i, watched))
}

/// Every `let` in the token stream: `(pattern range, expression range)`.
fn lets(t: &[Tok]) -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    for i in 0..t.len() {
        if !is_ident(t.get(i), "let") {
            continue;
        }
        let eq = expr_end(t, i + 1, &["=", ";"]);
        if !is_punct(t.get(eq), "=") {
            continue;
        }
        let end = expr_end(t, eq + 1, &[";", "{", "else"]);
        out.push((i + 1..eq, eq + 1..end));
    }
    out
}

/// The watched names: `consumed_at`, plus every plain `let` binding whose
/// initializer mentions a watched name (a REBINDING — `let d =
/// r.consumed_at.as_ref();`), to a fixpoint.
fn watched_names(t: &[Tok]) -> BTreeSet<String> {
    let mut watched: BTreeSet<String> = ["consumed_at".to_string()].into();
    loop {
        let before = watched.len();
        for (pat, expr) in lets(t) {
            // A plain binding: `[mut] NAME [: TYPE]`.
            let mut k = pat.start;
            if is_ident(t.get(k), "mut") {
                k += 1;
            }
            let Some(Tok::Ident(name)) = t.get(k) else {
                continue;
            };
            let plain = k + 1 == pat.end || is_punct(t.get(k + 1), ":");
            if plain && name != "Some" && name != "None" && mentions_in(t, expr, &watched) {
                watched.insert(name.clone());
            }
        }
        if watched.len() == before {
            return watched;
        }
    }
}

/// Count every `consumed_at` presence test in one production zone, by token
/// (P4.157 R-E — the regex arm this replaces saw only the method form and
/// `== None`):
///
/// 1. the method form — a mention, any zero-argument adapter chain
///    (`.as_ref()`, `.as_deref()`, …), then `.is_none(` / `.is_some(` /
///    `.is_none_or(` / `.is_some_and(`;
/// 2. `== None` / `!= None`, either side;
/// 3. `matches!(…)` whose first argument mentions it;
/// 4. the PATH form — `Option::is_none(&…)` (any `::is_none(` and kin)
///    whose arguments mention it;
/// 5. a `match` whose scrutinee mentions it;
/// 6. a refutable `let` (`if let` / `while let` / `let … else`) whose
///    pattern names `Some` / `None` and whose initializer mentions it;
///
/// over `consumed_at` AND every name a `let` rebinds it to.
fn code_count(zone: &str) -> usize {
    let t = tokens(&code_only(zone));
    let watched = watched_names(&t);
    let mut n = 0usize;
    for i in 0..t.len() {
        if is_value_mention(&t, i, &watched) {
            // (1) the method form, through zero-arg adapters.
            let mut k = i + 1;
            while is_punct(t.get(k), ".")
                && matches!(t.get(k + 1), Some(Tok::Ident(m)) if !PRESENCE_METHODS.contains(&m.as_str()))
                && is_punct(t.get(k + 2), "(")
                && is_punct(t.get(k + 3), ")")
            {
                k += 4;
            }
            if is_punct(t.get(k), ".")
                && PRESENCE_METHODS.iter().any(|m| is_ident(t.get(k + 1), m))
                && is_punct(t.get(k + 2), "(")
            {
                n += 1;
            }
            // (2) `<mention> == None` / `!= None`.
            if (is_punct(t.get(k), "==") || is_punct(t.get(k), "!="))
                && is_ident(t.get(k + 1), "None")
            {
                n += 1;
            }
        }
        // (2) `None == <mention>` / `None != <mention>`.
        if is_ident(t.get(i), "None")
            && (is_punct(t.get(i + 1), "==") || is_punct(t.get(i + 1), "!="))
        {
            let end = expr_end(&t, i + 2, &[";", ",", "{", "&&", "||"]);
            if mentions_in(&t, i + 2..end, &watched) {
                n += 1;
            }
        }
        // (3) `matches!(<mention>, …)`.
        if is_ident(t.get(i), "matches")
            && is_punct(t.get(i + 1), "!")
            && is_punct(t.get(i + 2), "(")
            && mentions_in(&t, i + 3..expr_end(&t, i + 3, &[","]), &watched)
        {
            n += 1;
        }
        // (4) `Option::is_none(&<mention>)` — any path call of the four.
        if is_punct(t.get(i), "::")
            && PRESENCE_METHODS.iter().any(|m| is_ident(t.get(i + 1), m))
            && is_punct(t.get(i + 2), "(")
        {
            let end = expr_end(&t, i + 3, &[]);
            if mentions_in(&t, i + 3..end, &watched) {
                n += 1;
            }
        }
        // (5) `match <scrutinee mentioning it> {`.
        if is_ident(t.get(i), "match") {
            let end = expr_end(&t, i + 1, &["{", ";"]);
            if is_punct(t.get(end), "{") && mentions_in(&t, i + 1..end, &watched) {
                n += 1;
            }
        }
    }
    // (6) refutable lets.
    for (pat, expr) in lets(&t) {
        let refutable = pat
            .clone()
            .any(|k| is_ident(t.get(k), "Some") || is_ident(t.get(k), "None"));
        if refutable && mentions_in(&t, expr, &watched) {
            n += 1;
        }
    }
    n
}

#[test]
fn the_in_force_predicate_lives_in_one_home() {
    let files = production_files();
    assert!(
        files.len() > 300,
        "the walk must cover the production tree ({} files)",
        files.len()
    );
    assert!(
        files
            .iter()
            .any(|(p, _)| p.starts_with("crates/quilltap-web/src/")),
        "the web crate must be walked"
    );

    let mut got: Vec<(String, usize)> = files
        .iter()
        .map(|(p, z)| (p.clone(), code_count(z)))
        .filter(|(_, n)| *n > 0)
        .collect();
    got.sort();
    let mut want: Vec<(String, usize)> = ALLOWED
        .iter()
        .map(|(p, n, _)| (p.to_string(), *n))
        .collect();
    want.sort();
    assert_eq!(
        got, want,
        "a `consumed_at` presence test outside the allow-list open-codes the \
         in-force predicate (route it through `db::chat_informs::is_inform_in_force`) \
         — or, if it is a genuine DELIVERY question mirroring a v4 open check, \
         add it to ALLOWED with the v4 site that justifies it"
    );

    // The home's one use is inside `is_inform_in_force` itself.
    let (_, home) = files
        .iter()
        .find(|(p, _)| p == "crates/quilltap-core/src/db/chat_informs.rs")
        .expect("the home file");
    let home_code = code_only(home);
    let start = home_code
        .find("pub fn is_inform_in_force")
        .expect("the home fn");
    let body_end = start
        + home_code[start..]
            .find("\n}")
            .expect("the home fn's closing brace");
    assert_eq!(
        code_count(&home_code[start..body_end]),
        1,
        "the home's own predicate is one presence test inside is_inform_in_force"
    );
}

#[test]
fn no_production_sql_tests_consumed_at_against_null() {
    let re = sql_null_tests();
    let offenders: Vec<String> = production_files()
        .iter()
        .flat_map(|(p, z)| {
            string_literals(z)
                .into_iter()
                .filter(|l| re.is_match(&joined_literal(l)))
                .map(move |l| format!("{p}: {}", l.chars().take(160).collect::<String>()))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "a SQL literal tests consumedAt against NULL — that open-codes the \
         in-force predicate below the home (a delivered STANDING row has a \
         consumedAt and is still in force):\n{}",
        offenders.join("\n")
    );
}

/// The matchers themselves, so an edit cannot quietly blind the census.
#[test]
fn the_matchers_see_every_shape() {
    for hit in [
        "r.consumed_at.is_none()",
        "r.consumed_at.is_some()",
        "r.consumed_at.as_ref().is_some()",
        "r.consumed_at.as_deref().is_none_or(|s|s.is_empty())",
        "r.consumed_at.is_some_and(|_|true)",
        "r.consumed_at==None",
        "r.consumed_at!=None",
    ] {
        assert_eq!(code_count(&format!("fn f() {{ {hit}; }}")), 1, "{hit}");
    }
    // The forms the regex arm could not see (P4.157 R-E), each exactly once.
    for hit in [
        "None == r.consumed_at",
        "matches!(r.consumed_at, None)",
        "matches!(r.consumed_at.as_deref(), Some(\"x\"))",
        "Option::is_none(&r.consumed_at)",
        "Option::<String>::is_some(&r.consumed_at)",
        "match r.consumed_at { None => 1, Some(_) => 2 }",
        "if let Some(v) = &r.consumed_at { v.len() }",
        "let Some(v) = r.consumed_at.clone() else { return }",
        "let d = &r.consumed_at; d.is_none()",
        "let d = r.consumed_at.as_ref(); let e = d; e.is_some()",
    ] {
        assert_eq!(code_count(&format!("fn f() {{ {hit}; }}")), 1, "{hit}");
    }
    // Not presence tests: a field initializer, shorthand init, a plain read, a
    // `match` on something else, a rebinding of an initializer that only
    // NAMES the field.
    for miss in [
        "ChatInformRow { consumed_at: None, permanent: true }",
        "ChatInformRow { id, consumed_at, permanent }",
        "let at = r.consumed_at.clone()",
        "match r.permanent { true => 1, false => 2 }",
        "let row = ChatInformCreate { consumed_at: None }; row.id.is_empty()",
        "if let Some(v) = &r.record_message_id { v.len() }",
    ] {
        assert_eq!(code_count(&format!("fn f() {{ {miss}; }}")), 0, "{miss}");
    }
    assert_eq!(
        code_count("fn f(r: &R) -> bool {\n    r.consumed_at\n        .is_none()\n}"),
        1,
        "a line-broken chain"
    );
    assert_eq!(
        code_count("// r.consumed_at.is_none()\nfn f() {}"),
        0,
        "a comment is not a call"
    );
    for hit in [
        "SELECT * FROM chat_informs WHERE consumedAt IS NULL",
        "WHERE \"consumedAt\" is not null",
        "WHERE \\\"consumedAt\\\" IS NULL",
        "WHERE consumedAt ISNULL",
        "WHERE consumedAt NOTNULL",
    ] {
        assert!(sql_null_tests().is_match(hit), "{hit}");
    }
    for hit in [
        "WHERE participantId = ?1 AND consumedAt \\\n           IS NULL",
        "WHERE consumedAt IS \\\n    NOT NULL",
    ] {
        assert!(
            sql_null_tests().is_match(&joined_literal(hit)),
            "a `\\`-continued literal: {hit}"
        );
    }
    for miss in [
        "UPDATE chat_informs SET consumedAt = ?1",
        "SELECT consumedAt, recordMessageId FROM chat_informs WHERE recordMessageId IS NULL",
    ] {
        assert!(!sql_null_tests().is_match(miss), "{miss}");
    }
}

/// P4.157 R-E: one committed evasion fixture per form the old regex arm could
/// not see. Each must count EXACTLY one presence test.
#[test]
fn the_census_trips_on_every_committed_evasion() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/chat_informs_in_force_evasions");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".rs.txt"))
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "match.rs.txt",
            "matches_macro.rs.txt",
            "rebinding.rs.txt",
            "ufcs_is_none.rs.txt"
        ],
        "the four committed evasion forms"
    );
    let missed: Vec<String> = names
        .iter()
        .filter(|n| {
            let src = std::fs::read_to_string(dir.join(n)).unwrap();
            code_count(&production_zone(&src)) != 1
        })
        .cloned()
        .collect();
    assert!(missed.is_empty(), "the census misses: {missed:?}");
}
