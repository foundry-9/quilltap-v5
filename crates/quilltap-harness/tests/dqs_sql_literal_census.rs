//! P4.172 — the double-quoted-string-literal ("DQS") census.
//!
//! v4's driver compiles SQLite with `SQLITE_DQS=0`
//! (`better-sqlite3-multiple-ciphers/deps/defines.gypi:17`), and since P4.172
//! every v5 connection switches the same fallback off at runtime
//! (`quilltap_core::db::runtime::refuse_double_quoted_strings`). So a
//! double-quoted token in v5's SQL is an IDENTIFIER and nothing else: a
//! `"PENDING"` meant as a string value, a `json_extract(col, "$.x")`, an
//! `IN ("a", "b")` or a `DEFAULT "x"` now fails to compile where SQLite's legacy
//! fallback used to read it as `'PENDING'`. This census keeps that class out of
//! the source.
//!
//! **The rule.** Every string literal in the production zone of every
//! workspace crate (`crates/*/src`, test modules stripped — the shared
//! [`source_census`] lexer) that reads as SQL — an upper-case statement or
//! clause keyword — is scanned with its single-quoted SQL strings and `--`
//! comments removed. Every remaining `"token"` must be:
//!
//! - a name the schema DECLARES: `fresh_schema.json` and
//!   `migration_indexes.json` (v4's own DDL, all three partitions), or a name a
//!   v5 SQL literal itself declares — a `CREATE TABLE` / `INDEX` / `TRIGGER` /
//!   `VIRTUAL TABLE` name, a column definition, an `ADD COLUMN`, a `RENAME`
//!   target, an `AS` alias;
//! - a format placeholder `"{NAME}"` (a `format!` interpolation of a table /
//!   column / index constant — a VALUE is always a bound `?`, never spliced);
//! - or one of [`REVIEWED_IDENTIFIERS`], each a real identifier with its reason.
//!
//! Anything else is a finding: single-quote it (or bind it), or — if it really
//! is an identifier the census cannot see declared — add it to the reviewed
//! list with the reason.
//!
//! **The second half** of the census is behavioural (the order's R-A): the
//! whole `cargo test --workspace` runs with DQS off, so a query that leaned on
//! the fallback fails by name there. Test-only connections the harness opens
//! for its own fixtures are out of scope (they run no production SQL).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test dqs_sql_literal_census

mod source_census;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use regex::Regex;
use source_census::{contains_word, production_zone, string_literals, workspace_rust_sources};

/// Real identifiers no DDL literal declares in a shape the census reads —
/// each a column or alias v5's SQL names on purpose.
const REVIEWED_IDENTIFIERS: &[(&str, &str)] = &[
    // Columns a boot ensure ADDS through a `format!` over its `*_COLUMN`
    // constant (`ADD COLUMN "{COLUMN}"`), so no literal spells them in a
    // declaring position; every one is in `fresh_schema.json`'s history or a
    // v4 migration the ensure re-homes.
    (
        "transcriptVersion",
        "chats — chats_transcript_version_repair",
    ),
    (
        "moderationRefusalCount",
        "chats — the moderation-refusal columns' ensure",
    ),
    (
        "lastModerationRefusalAt",
        "chats — the moderation-refusal columns' ensure",
    ),
    // RETIRED columns a repair reads (and drops) on an instance v4 had not yet
    // migrated — named on purpose because they may still exist.
    (
        "autoHousekeepingSettings",
        "chat_settings — memory_maintenance_settings_move",
    ),
    (
        "impersonationVoiceRewrite",
        "chat_settings — the voice-mode repair's legacy read",
    ),
    // Columns of a derived table the same statement builds (`chats_search`'s
    // `s."ca"` / `s."mid"` over its own sub-select's aliases, written unquoted
    // there).
    (
        "ca",
        "chats_search — the matching-ids sub-select's createdAt alias",
    ),
    (
        "mid",
        "chats_search — the matching-ids sub-select's message-id alias",
    ),
];

/// Files whose literals carry SQL KEYWORDS as prose, never as SQL run against a
/// database: model-facing prompt text and a tool definition's JSON schema.
const PROSE_FILES: &[&str] = &[
    "crates/quilltap-core/src/services/brahma_console/prompt_text.rs",
    "crates/quilltap-core/src/services/scene_state_tracking/prompt_text.rs",
    "crates/quilltap-core/src/tools/definitions/data.rs",
];

/// Single literals that quote SQL for a HUMAN (help / error text), never run.
const PROSE_TOKENS: &[(&str, &str)] = &[
    // `quilltap db`'s refusal example: `quilltap db --write "UPDATE ..."`.
    ("crates/quilltap-cli/src/db_cmd.rs", "UPDATE ..."),
];

/// Upper-case words that make a literal SQL for this census. Case-SENSITIVE on
/// purpose: v5's SQL is written upper-case, and a case-insensitive match was
/// measured at the `322a5488e` follow-ups unification to flag ~30 prompt-prose
/// literals ("from", "select" in English) — a lower-case SQL literal is the
/// recorded blind spot, the full workspace gate (DQS off on every connection)
/// its backstop.
const SQL_WORDS: &[&str] = &[
    "SELECT", "INSERT", "UPDATE", "DELETE", "CREATE", "ALTER", "WHERE", "FROM", "PRAGMA",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the harness crate sits two levels under the repo root")
        .to_path_buf()
}

/// A Rust string literal's VALUE: a raw literal's body as is; a plain one's
/// with `\`-newline continuations folded and escapes resolved (only the ones
/// that matter to SQL text need to be exact — quotes, backslashes, newlines).
fn literal_value(lit: &str) -> String {
    let lit = lit.strip_prefix('b').unwrap_or(lit);
    if let Some(raw) = lit.strip_prefix('r') {
        let hashes = raw.len() - raw.trim_start_matches('#').len();
        let body = &raw[hashes + 1..raw.len() - 1 - hashes];
        return body.to_string();
    }
    let body = &lit[1..lit.len() - 1];
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\n') => {
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
            }
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn is_sql(text: &str) -> bool {
    SQL_WORDS.iter().any(|w| contains_word(text, w))
}

/// The SQL with every single-quoted string (`''` escapes included) and every
/// `--` comment blanked — what is left in double quotes is what SQLite parses
/// as a quoted identifier (or, with the fallback, a string).
fn strip_sql_strings(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                loop {
                    match chars.next() {
                        Some('\'') if chars.peek() == Some(&'\'') => {
                            chars.next();
                        }
                        Some('\'') | None => break,
                        Some(_) => {}
                    }
                }
                out.push_str("''");
            }
            '-' if chars.peek() == Some(&'-') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

fn quoted_tokens(sql: &str) -> Vec<String> {
    let re = Regex::new(r#""([^"\n]*)""#).unwrap();
    re.captures_iter(sql).map(|c| c[1].to_string()).collect()
}

/// Every name the given DDL / SQL text declares (see the module doc).
fn declared_names(sql: &str, into: &mut BTreeSet<String>) {
    let declaring = Regex::new(
        r#"(?x)
        (?: CREATE\s+(?:UNIQUE\s+|TEMP\s+|VIRTUAL\s+)?(?:TABLE|INDEX|TRIGGER)
              (?:\s+IF\s+NOT\s+EXISTS)?
          | ADD\s+COLUMN | RENAME\s+COLUMN | \bTO | \bAS )
        \s+"([^"]+)""#,
    )
    .unwrap();
    for c in declaring.captures_iter(sql) {
        into.insert(c[1].to_string());
    }
    // A column definition: `( "name" TYPE` or `, "name" TYPE`.
    let column = Regex::new(r#"[(,]\s*"([^"]+)"\s+(?:TEXT|INTEGER|REAL|BLOB|NUMERIC)\b"#).unwrap();
    for c in column.captures_iter(sql) {
        into.insert(c[1].to_string());
    }
}

/// Every quoted name in a schema dump (every string in its partition arrays).
fn schema_names(path: &Path, into: &mut BTreeSet<String>) {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    for part in ["main", "mountIndex", "llmLogs"] {
        for ddl in json[part].as_array().into_iter().flatten() {
            for t in quoted_tokens(ddl.as_str().unwrap()) {
                into.insert(t);
            }
        }
    }
}

/// One production SQL literal: where it is, and its SQL value.
struct SqlLiteral {
    file: String,
    sql: String,
}

fn production_sql_literals(root: &Path) -> Vec<SqlLiteral> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root.join("crates")).unwrap() {
        let krate = entry.unwrap().path();
        let name = krate.file_name().unwrap().to_string_lossy().to_string();
        // The harness is test code over fixtures (Tier 3 of the order); the
        // sys crate is C.
        if name == "quilltap-harness" || name == "quilltap-sqlite3mc-sys" {
            continue;
        }
        let src = krate.join("src");
        if src.is_dir() {
            workspace_rust_sources(&src, &mut files);
        }
    }
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        if PROSE_FILES.contains(&rel.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let zone = production_zone(&text);
        for lit in string_literals(&zone) {
            let sql = literal_value(lit);
            if is_sql(&sql) {
                out.push(SqlLiteral {
                    file: rel.clone(),
                    sql,
                });
            }
        }
    }
    out
}

/// The census's verdict over a literal set: every quoted token that is not a
/// known identifier, as `file: "token"`.
fn findings(literals: &[SqlLiteral], known: &BTreeSet<String>) -> Vec<String> {
    let placeholder = Regex::new(r"^\{[A-Za-z_][A-Za-z0-9_]*\}$").unwrap();
    let mut out = Vec::new();
    for lit in literals {
        for token in quoted_tokens(&strip_sql_strings(&lit.sql)) {
            if known.contains(&token)
                || placeholder.is_match(&token)
                || PROSE_TOKENS.contains(&(lit.file.as_str(), token.as_str()))
            {
                continue;
            }
            out.push(format!("{}: \"{token}\"", lit.file));
        }
    }
    out
}

fn known_identifiers(root: &Path, literals: &[SqlLiteral]) -> BTreeSet<String> {
    let mut known = BTreeSet::new();
    let provisioning = root.join("crates/quilltap-core/src/services/provisioning");
    schema_names(&provisioning.join("fresh_schema.json"), &mut known);
    schema_names(&provisioning.join("migration_indexes.json"), &mut known);
    for lit in literals {
        declared_names(&strip_sql_strings(&lit.sql), &mut known);
    }
    known.extend(
        REVIEWED_IDENTIFIERS
            .iter()
            .map(|(name, _)| name.to_string()),
    );
    known
}

#[test]
fn production_sql_quotes_only_identifiers() {
    let root = repo_root();
    let literals = production_sql_literals(&root);
    let known = known_identifiers(&root, &literals);
    let tokens: usize = literals
        .iter()
        .map(|l| quoted_tokens(&strip_sql_strings(&l.sql)).len())
        .sum();
    // Floors, so a lexer regression cannot pass by scanning nothing (measured
    // at P4.172: 1,220 SQL literals, 1,309 quoted tokens).
    assert!(
        literals.len() >= 1100,
        "{} SQL literals scanned",
        literals.len()
    );
    assert!(tokens >= 1200, "{tokens} quoted tokens scanned");
    let found = findings(&literals, &known);
    assert!(
        found.is_empty(),
        "double-quoted tokens in production SQL that are no known identifier — \
         with DQS off (v4's `SQLITE_DQS=0`) each would fail to compile if it is \
         meant as a STRING: single-quote or bind it; if it is a real identifier, \
         add it to REVIEWED_IDENTIFIERS with the reason:\n{}",
        found.join("\n")
    );
    // Every reviewed identifier still earns its place (a stale entry would
    // silently widen the allowance).
    let mut bare = BTreeSet::new();
    let provisioning = root.join("crates/quilltap-core/src/services/provisioning");
    schema_names(&provisioning.join("fresh_schema.json"), &mut bare);
    schema_names(&provisioning.join("migration_indexes.json"), &mut bare);
    for lit in &literals {
        declared_names(&strip_sql_strings(&lit.sql), &mut bare);
    }
    let used: BTreeSet<String> = literals
        .iter()
        .flat_map(|l| quoted_tokens(&strip_sql_strings(&l.sql)))
        .collect();
    for (name, why) in REVIEWED_IDENTIFIERS {
        assert!(
            !bare.contains(*name),
            "{name} ({why}) is declared now — drop the entry"
        );
        assert!(
            used.contains(*name),
            "{name} ({why}) is no longer quoted — drop the entry"
        );
    }
    eprintln!(
        "OK: {} production SQL literals, {tokens} quoted tokens, every one an identifier.",
        literals.len()
    );
}

/// The rule itself, on planted text: each fallback-reliant shape is caught,
/// each legitimate one passes.
#[test]
fn the_rule_catches_each_fallback_shape() {
    let known: BTreeSet<String> = ["chats", "status", "payload", "id"]
        .into_iter()
        .map(String::from)
        .collect();
    let lit = |sql: &str| SqlLiteral {
        file: "planted.rs".into(),
        sql: sql.into(),
    };
    let caught = |sql: &str| findings(&[lit(sql)], &known);
    assert_eq!(
        caught(r#"SELECT "id" FROM "chats" WHERE "status" = "PENDING""#),
        vec![r#"planted.rs: "PENDING""#]
    );
    assert_eq!(
        caught(r#"SELECT json_extract("payload", "$.chatId") FROM chats"#),
        vec![r#"planted.rs: "$.chatId""#]
    );
    assert_eq!(
        caught(r#"DELETE FROM chats WHERE status IN ("a", "b")"#).len(),
        2
    );
    // Single-quoted strings (JSON inside them included), comments, and a
    // format placeholder are not tokens.
    assert!(caught(r#"UPDATE "chats" SET "payload" = '{"k":"v"}' WHERE "id" = ?"#).is_empty());
    assert!(caught("SELECT \"id\" FROM chats -- a \"comment\"\n").is_empty());
    assert!(caught(r#"SELECT "{COLUMN}" FROM "chats""#).is_empty());
    assert!(caught(r#"SELECT 'it''s "fine"' FROM chats"#).is_empty());
    // A literal's value: a continuation and escaped quotes fold as rustc does.
    assert_eq!(
        literal_value("\"SELECT \\\"id\\\" \\\n     FROM chats\""),
        r#"SELECT "id" FROM chats"#
    );
    assert_eq!(literal_value(r##"r#"SELECT "id""#"##), r#"SELECT "id""#);
    // Declarations count as known.
    let mut declared = BTreeSet::new();
    declared_names(
        r#"CREATE TABLE IF NOT EXISTS "t" ("a" TEXT, "b" INTEGER); ALTER TABLE "t" ADD COLUMN "c" REAL; SELECT x AS "d""#,
        &mut declared,
    );
    assert_eq!(
        declared.into_iter().collect::<Vec<_>>(),
        vec!["a", "b", "c", "d", "t"]
    );
}
