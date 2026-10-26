//! Tier-1 differential (P4.D331, v4 `e4a60ad9e` + `a495faa26`, at the
//! `322a5488e` pin): the `regexp()` SQL scalar — `quilltap_core::db::
//! regexp_function` against v4's REAL `compileSqlRegexp` and
//! `registerRegexpFunction` on an in-memory better-sqlite3.
//!
//! Row kinds (`harness/oracle/cases/regexp-function.ts`):
//!   - `compile` — `compile_sql_regexp(pattern, flags)`'s answer, the reason
//!     byte-for-byte (`/<pattern>/<flags>: <V8 message>`, v4's flags
//!     sentence, or V8's repeated-flag constructor message);
//!   - `match` — the SAME SQL + bound params on a bare `Connection` with
//!     [`register_regexp`]: the one column's value, or the statement's error
//!     text (`Invalid regular expression: …`, surfaced bare).
//!
//! Any row the engine (`regress`, R-I) cannot answer V8-identically is a named
//! [`REGEXP_ENGINE_GAP`] row — both answers written down, so a fix on either
//! side trips it. The table started EMPTY and grew only from measurement.
//!
//! Generate the oracle (Node 24, from the pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC PATH=$N:$PATH npx tsx $V5W/harness/oracle/cases/regexp-function.ts \
//!     > /tmp/oracle-regexp-function.ndjson
//! Run:
//!   QT_ORACLE_REGEXP=/tmp/oracle-regexp-function.ndjson \
//!     cargo test -p quilltap-harness --test regexp_function_equivalence -- --nocapture
//!
//! Skips (does not fail) when the env var is unset — the standing gated
//! discipline.

use quilltap_core::db::fallback::error_text;
use quilltap_core::db::regexp_function::{compile_sql_regexp, register_regexp};
use quilltap_core::db::DbError;
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;

/// `(row id, v4's answer, v5's answer)` — a syntax the engine cannot answer
/// V8-identically. Measured, never assumed (P4.D331's lane record has the
/// run that found each). Neither touches a syntax `help/the-log.md`
/// advertises (lookbehind, backreferences — both rows agree).
const REGEXP_ENGINE_GAP: &[(&str, &str, &str)] = &[
    // V8's non-`u` `i` canonicalizes by `toUpperCase` but KEEPS a non-ASCII
    // character whose upper case is ASCII (ES `Canonicalize`, step 3.g), so
    // `/^s$/i` does not match U+017F `ſ`; `regress` folds it to `s`. With `u`
    // both match (the `icase-long-s-u` row agrees).
    ("icase-long-s", "out: 0", "out: 1"),
    // Annex B: without `u`, `\u` not followed by four hex digits is an
    // identity escape for `u`, so `/^\u{2}$/` is `u` twice and matches `uu`;
    // `regress` reads the braces as a code-point escape.
    ("unicode-escape-no-u", "out: 1", "out: 0"),
];

#[derive(Deserialize)]
struct Row {
    kind: String,
    id: String,
    #[serde(default)]
    pattern: String,
    #[serde(default)]
    flags: String,
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    sql: String,
    #[serde(default)]
    params: Vec<Value>,
    #[serde(default)]
    out: Option<Value>,
    #[serde(default)]
    error: Option<String>,
}

fn oracle() -> Option<Vec<Row>> {
    let path = std::env::var("QT_ORACLE_REGEXP").ok()?;
    let text = std::fs::read_to_string(&path).expect("read the oracle");
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("an oracle row"))
        .collect();
    assert!(!rows.is_empty(), "the oracle at {path} is empty");
    Some(rows)
}

fn bind(p: &Value) -> SqlValue {
    match p {
        Value::Null => SqlValue::Null,
        Value::String(s) => SqlValue::Text(s.clone()),
        Value::Object(o) => {
            let n = o["num"].as_f64().expect("a number param");
            if n.fract() == 0.0 {
                SqlValue::Integer(n as i64)
            } else {
                SqlValue::Real(n)
            }
        }
        other => panic!("unsupported param {other}"),
    }
}

/// One row's answer, rendered the same way on both sides.
fn v4_answer(row: &Row) -> String {
    match row.kind.as_str() {
        "compile" => match &row.reason {
            None if row.ok => "ok".to_string(),
            Some(r) => format!("refused: {r}"),
            None => panic!("{}: a refused compile row without a reason", row.id),
        },
        _ => match (&row.error, &row.out) {
            (Some(e), _) => format!("error: {e}"),
            (None, Some(v)) => format!("out: {v}"),
            (None, None) => "out: null".to_string(),
        },
    }
}

fn v5_answer(row: &Row) -> String {
    match row.kind.as_str() {
        "compile" => match compile_sql_regexp(&row.pattern, &row.flags) {
            Ok(_) => "ok".to_string(),
            Err(reason) => format!("refused: {reason}"),
        },
        _ => {
            let conn = Connection::open_in_memory().unwrap();
            register_regexp(&conn).unwrap();
            let params: Vec<SqlValue> = row.params.iter().map(bind).collect();
            let got = conn.query_row(&row.sql, rusqlite::params_from_iter(params), |r| {
                r.get::<_, SqlValue>(0)
            });
            match got {
                Ok(SqlValue::Integer(i)) => format!("out: {i}"),
                Ok(SqlValue::Null) => "out: null".to_string(),
                Ok(other) => format!("out: {other:?}"),
                Err(e) => format!("error: {}", error_text(&DbError::Sqlite(e))),
            }
        }
    }
}

#[test]
fn every_compile_and_match_row_answers_as_v4() {
    let Some(rows) = oracle() else {
        eprintln!("SKIP: QT_ORACLE_REGEXP unset");
        return;
    };
    let compiles = rows.iter().filter(|r| r.kind == "compile").count();
    let matches = rows.iter().filter(|r| r.kind == "match").count();
    assert!(compiles > 40 && matches > 40, "{compiles} / {matches}");
    let mut mismatches = Vec::new();
    let mut gaps_seen = Vec::new();
    for row in &rows {
        let (v4, v5) = (v4_answer(row), v5_answer(row));
        if let Some(gap) = REGEXP_ENGINE_GAP.iter().find(|g| g.0 == row.id) {
            // A ruled gap: both answers exactly as recorded, both ways.
            assert_eq!((v4.as_str(), v5.as_str()), (gap.1, gap.2), "{}", row.id);
            gaps_seen.push(row.id.as_str());
            continue;
        }
        if v4 != v5 {
            mismatches.push(format!("  ({:?}, {v4:?}, {v5:?}),", row.id));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} rows disagree:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    for gap in REGEXP_ENGINE_GAP {
        assert!(gaps_seen.contains(&gap.0), "stale gap row {}", gap.0);
    }
    eprintln!(
        "regexp_function: {compiles} compile + {matches} match rows, {} gap rows",
        REGEXP_ENGINE_GAP.len()
    );
}
