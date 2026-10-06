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
//! - **(i) code** — in `code_only(production_zone(src))` with whitespace
//!   removed, every `consumed_at` presence test (`.is_none(` / `.is_some(` /
//!   `.is_none_or(` / `.is_some_and(`, through any `.as_ref()` /
//!   `.as_deref()` chain, and `== None` / `!= None`). The allow-list IS the set
//!   of files: the home, plus the two CORRECT open checks that mirror v4's own
//!   open checks (they ask "was it ever delivered?", not "is it in force?").
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

use std::path::PathBuf;

use regex::Regex;
use source_census::{code_only, production_zone, rust_sources, string_literals};

/// `(path from the repo root, presence tests, why)`.
const ALLOWED: &[(&str, usize, &str)] = &[
    (
        "crates/quilltap-core/src/db/chat_informs.rs",
        1,
        "the HOME — `is_inform_in_force` (`row.permanent || \
         row.consumed_at.is_none()`), v4 `isInformInForce`.",
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

fn code_presence_tests() -> Regex {
    Regex::new(r"consumed_at(\.as_(ref|deref)\(\))*(\.is_(none|some)(_or|_and)?\(|[!=]=None)")
        .unwrap()
}

fn sql_null_tests() -> Regex {
    Regex::new(r#"(?i)\bconsumedAt\\?"?\s+IS\s+(NOT\s+)?NULL\b"#).unwrap()
}

/// Count the code arm's matches in one production zone.
fn code_count(zone: &str) -> usize {
    let code: String = code_only(zone)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    code_presence_tests().find_iter(&code).count()
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
    let body: String = home_code[start..body_end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(
        code_presence_tests().find_iter(&body).count(),
        1,
        "the home file's one presence test must be inside is_inform_in_force"
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
                .filter(|l| re.is_match(l))
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

/// The matchers themselves, so a regex edit cannot quietly blind the census.
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
        assert_eq!(code_presence_tests().find_iter(hit).count(), 1, "{hit}");
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
    ] {
        assert!(sql_null_tests().is_match(hit), "{hit}");
    }
    for miss in [
        "UPDATE chat_informs SET consumedAt = ?1",
        "SELECT consumedAt, recordMessageId FROM chat_informs WHERE recordMessageId IS NULL",
    ] {
        assert!(!sql_null_tests().is_match(miss), "{miss}");
    }
}
