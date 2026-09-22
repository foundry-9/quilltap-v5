//! Real-DB differential: v5's `db::chat_message_fts_reconcile` against v4's
//! REAL `lib/startup/reconcile-chat-message-fts.ts` (`f45a517a9`), over the
//! shared corpus `harness/oracle/fixtures/chat-message-fts-reconcile.json`.
//!
//! Seven damaged states — v4's own five plus an index AHEAD of the transcript
//! (a stale map row the delete trigger never saw, which the count comparison
//! catches in the other direction) and an eligibility mix, so `eligible` is
//! not simply the row count. For each:
//!
//!  - the whole result (`restored` / `eligible` / `indexed` / `rebuilt`);
//!  - **every log line, in order**, with its LEVEL, its message and its
//!    context values — five of v4's six lines are reachable here, each with a
//!    silence leg (the scenarios where it must NOT fire);
//!  - the post-pass state: `missing`, both counts, every map row, and every
//!    corpus `MATCH` expression's hit list.
//!
//! `durationMs` is a clock and is NOT compared (v4's own oracle rows show it
//! flipping between 0 and 1 across scenarios).
//!
//! ## v4's sixth line has no v5 twin, by construction
//!
//! v4 opens `const db = getRawDatabase(); if (!db) debug('No SQLite database
//! available; skipping chat message FTS reconciliation')`. v5's reconciler
//! takes its connection as an argument and the writer task always has one, so
//! there is no v5 call that could produce that line — it is unreachable, not
//! unported. Recorded here rather than left to inference.
//!
//! ## Regenerating
//!
//! ```bash
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=/Users/csebold/source/quilltap-v5
//!   TMPO=/tmp/qt-chat-message-fts-reconcile-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/chat-message-fts-reconcile.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/chat-message-fts-reconcile.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-chat-message-fts-reconcile.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "chat-message-fts-reconcile\.test\.ts$"
//!   QT_ORACLE_CHAT_MESSAGE_FTS_RECONCILE=/tmp/oracle-chat-message-fts-reconcile.ndjson \
//!     cargo test -p quilltap-harness --test chat_message_fts_reconcile_equivalence -- --nocapture
//! ```
//!
//! With the env var unset the differential SKIPs (and says so).

use std::path::PathBuf;

use quilltap_core::db::chat_message_fts::{
    count_eligible_chat_messages, count_indexed_chat_messages, ensure_chat_message_fts_schema,
    missing_chat_message_fts_objects,
};
use quilltap_core::db::chat_message_fts_reconcile::reconcile_chat_message_fts;
use quilltap_core::db::text_compression::{register_qt_text, text_to_blob, TextCell};
use quilltap_core::test_support::captured_with;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Debug, Deserialize)]
struct Corpus {
    #[serde(rename = "chatMessagesDdl")]
    chat_messages_ddl: String,
    long: LongSpec,
    searches: Vec<String>,
    scenarios: Vec<Scenario>,
}

#[derive(Debug, Deserialize)]
struct LongSpec {
    unit: String,
    times: usize,
}

#[derive(Debug, Deserialize)]
struct Scenario {
    name: String,
    seed: Vec<Step>,
    damage: Vec<String>,
    #[serde(default, rename = "afterDamage")]
    after_damage: Vec<Step>,
}

#[derive(Debug, Deserialize)]
struct Step {
    op: String,
    id: String,
    content: Content,
    #[serde(default, rename = "type")]
    row_type: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    role: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Content {
    Text { value: String },
    Long,
    LongBlob,
    Null,
}

fn double_option<'de, D>(de: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize)]
struct OracleRow {
    scenario: String,
    result: OracleResult,
    logs: Vec<OracleLog>,
    after: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
struct OracleResult {
    restored: Vec<String>,
    eligible: i64,
    indexed: i64,
    rebuilt: bool,
}

#[derive(Debug, Deserialize)]
struct OracleLog {
    level: String,
    message: String,
    context: Value,
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-message-fts-reconcile.json")
}

fn read_corpus() -> Corpus {
    let path = corpus_path();
    serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
    )
    .expect("parse chat-message-fts-reconcile corpus")
}

fn seed(conn: &Connection, step: &Step, long: &str) {
    assert_eq!(step.op, "insert", "unsupported seed op");
    let role: Option<String> = match &step.role {
        None => Some("USER".to_string()),
        Some(inner) => inner.clone(),
    };
    let created_at = format!(
        "2026-01-01T00:00:{:0>2}.000Z",
        &step.id[step.id.len().saturating_sub(2)..]
    );
    let content: Box<dyn rusqlite::ToSql> = match &step.content {
        Content::Text { value } => Box::new(value.clone()),
        Content::Long => Box::new(long.to_string()),
        Content::LongBlob => match text_to_blob(long) {
            TextCell::Blob(bytes) => Box::new(bytes),
            TextCell::Text(_) => panic!("corpus expects LONG to compress"),
        },
        Content::Null => Box::new(Option::<String>::None),
    };
    conn.execute(
        r#"INSERT INTO "chat_messages" ("id","chatId","type","role","content","createdAt") VALUES (?,?,?,?,?,?)"#,
        rusqlite::params![
            step.id,
            "chat-1",
            step.row_type.as_deref().unwrap_or("message"),
            role,
            content.as_ref(),
            created_at
        ],
    )
    .expect("seed insert");
}

/// The oracle's `after` block, in v5's terms. Every field is `null` where the
/// call throws, exactly as the oracle's `attempt()` records it.
fn after_state(conn: &Connection, corpus: &Corpus) -> Map<String, Value> {
    let missing = missing_chat_message_fts_objects(conn)
        .map(Value::from)
        .unwrap_or(Value::Null);
    let eligible = count_eligible_chat_messages(conn)
        .map(Value::from)
        .unwrap_or(Value::Null);
    let indexed = count_indexed_chat_messages(conn)
        .map(Value::from)
        .unwrap_or(Value::Null);

    let map = (|| -> Option<Value> {
        let mut stmt = conn
            .prepare(r#"SELECT "ftsId", "messageId" FROM "chat_messages_fts_map" ORDER BY "ftsId""#)
            .ok()?;
        let rows = stmt
            .query_map([], |row| {
                Ok(json!({ "ftsId": row.get::<_, i64>(0)?, "messageId": row.get::<_, String>(1)? }))
            })
            .ok()?
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        Some(Value::Array(rows))
    })()
    .unwrap_or(Value::Null);

    const SEARCH_SQL: &str = r#"SELECT m."id" AS id
                           FROM "chat_messages_fts" f
                           JOIN "chat_messages_fts_map" x ON x."ftsId" = f.rowid
                           JOIN "chat_messages" m         ON m."id" = x."messageId"
                          WHERE "chat_messages_fts" MATCH ?
                          ORDER BY m."createdAt" DESC"#;
    let mut searches = Map::new();
    for needle in &corpus.searches {
        let hits = (|| -> Option<Value> {
            let mut stmt = conn.prepare(SEARCH_SQL).ok()?;
            let ids = stmt
                .query_map([needle], |row| row.get::<_, String>(0))
                .ok()?
                .collect::<Result<Vec<String>, _>>()
                .ok()?;
            Some(Value::from(ids))
        })()
        .unwrap_or(Value::Null);
        searches.insert(needle.clone(), hits);
    }

    let mut out = Map::new();
    out.insert("missing".into(), missing);
    out.insert("eligible".into(), eligible);
    out.insert("indexed".into(), indexed);
    out.insert("map".into(), map);
    out.insert("searches".into(), Value::Object(searches));
    out
}

/// v5's rendered log line for one of v4's lines: level, then the message, then
/// every context value. `durationMs` is excluded (a clock).
fn assert_line_matches(line: &str, oracle: &OracleLog, scenario: &str, i: usize) {
    let want_level = oracle.level.to_uppercase();
    assert!(
        line.starts_with(&format!("{want_level} quilltap::boot")),
        "{scenario}: log {i} level/target\n  want: {want_level} quilltap::boot…\n  got:  {line}"
    );
    // The capture layer renders the format-args message through its `Debug`,
    // which for `format_args!` is the formatted text WITHOUT quotes — so the
    // needle is the bare message, not a quoted one.
    assert!(
        line.contains(&oracle.message),
        "{scenario}: log {i} message\n  want: {:?}\n  got:  {line}",
        oracle.message
    );
    assert!(
        line.contains("context=startup.chat-message-fts-reconcile"),
        "{scenario}: log {i} is missing the service context\n  got: {line}"
    );
    if let Value::Object(ctx) = &oracle.context {
        for (key, value) in ctx {
            if key == "durationMs" {
                continue; // a clock
            }
            let needle = match value {
                // v4 logs `missing` as an array; v5 serializes it under the
                // `…Json` file-layer convention, so the rendered capture line
                // carries the JSON text.
                Value::Array(_) => format!("{key}Json={}", serde_json::to_string(value).unwrap()),
                // ⚠ ONE NAMED DIVERGENCE, not a normalization of convenience.
                // v4 logs `err.message` — SQLite's bare sentence. v5 renders a
                // `DbError` through its `Display`, whose `Sqlite` variant is
                // documented to carry the `sqlite error: ` prefix, and
                // `error = %e` on a `DbError` is the established convention at
                // every other v5 log site (eight in `db/` alone). Changing it
                // HERE would make this one line inconsistent with the rest of
                // the tree, so the prefix is allowed and the comparand is
                // v4's sentence as a SUFFIX — the envelope may prefix it, but
                // v4's words must be there verbatim and nothing may follow
                // them. The divergence is real and visible in `combined.log`;
                // it is recorded in the lane record rather than papered over.
                Value::String(s) if key == "error" => {
                    assert!(
                        line.contains("error=") && line.ends_with(s.as_str()),
                        "{scenario}: log {i}'s `error` must carry v4's sentence \
                         verbatim at the end of the field\n  want suffix: {s:?}\n  got: {line}"
                    );
                    continue;
                }
                Value::String(s) => format!("{key}={s}"),
                other => format!("{key}={other}"),
            };
            assert!(
                line.contains(&needle),
                "{scenario}: log {i} is missing context `{needle}`\n  got: {line}"
            );
        }
    }
}

#[test]
fn chat_message_fts_reconcile_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHAT_MESSAGE_FTS_RECONCILE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_CHAT_MESSAGE_FTS_RECONCILE to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"));
    let oracle: Vec<OracleRow> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("parse oracle row: {e}\n{l}")))
        .collect();

    let corpus = read_corpus();
    let long = corpus.long.unit.repeat(corpus.long.times);
    assert_eq!(
        oracle.len(),
        corpus.scenarios.len(),
        "oracle row count vs corpus scenario count — regenerate the oracle"
    );

    for (i, scenario) in corpus.scenarios.iter().enumerate() {
        let want = &oracle[i];
        assert_eq!(
            want.scenario, scenario.name,
            "scenario {i}: oracle out of step with the corpus"
        );

        let conn = Connection::open_in_memory().expect("open in-memory db");
        register_qt_text(&conn).expect("register qt_text");
        conn.execute_batch(&corpus.chat_messages_ddl)
            .expect("create chat_messages");
        ensure_chat_message_fts_schema(&conn).expect("ensure fts schema");
        for step in &scenario.seed {
            seed(&conn, step, &long);
        }
        for sql in &scenario.damage {
            conn.execute_batch(sql).expect("damage");
        }
        for step in &scenario.after_damage {
            seed(&conn, step, &long);
        }

        let (result, lines) = captured_with(|| reconcile_chat_message_fts(&conn));

        assert_eq!(
            result.restored, want.result.restored,
            "{}: restored diverged",
            scenario.name
        );
        assert_eq!(
            result.eligible, want.result.eligible,
            "{}: eligible diverged",
            scenario.name
        );
        assert_eq!(
            result.indexed, want.result.indexed,
            "{}: indexed diverged",
            scenario.name
        );
        assert_eq!(
            result.rebuilt, want.result.rebuilt,
            "{}: rebuilt diverged",
            scenario.name
        );

        // Every line v4 emitted, in order — and NOTHING ELSE (the silence leg
        // for the four lines this scenario must not produce).
        let ours: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("context=startup.chat-message-fts-reconcile"))
            .collect();
        assert_eq!(
            ours.len(),
            want.logs.len(),
            "{}: log COUNT diverged\n  rust:   {:#?}\n  oracle: {:#?}",
            scenario.name,
            ours,
            want.logs.iter().map(|l| &l.message).collect::<Vec<_>>()
        );
        for (n, oracle_log) in want.logs.iter().enumerate() {
            assert_line_matches(ours[n], oracle_log, &scenario.name, n);
        }

        let got_after = after_state(&conn, &corpus);
        for key in ["missing", "eligible", "indexed", "map", "searches"] {
            assert_eq!(
                got_after.get(key).unwrap_or(&Value::Null),
                want.after.get(key).unwrap_or(&Value::Null),
                "{}: after.{key} diverged",
                scenario.name
            );
        }
    }

    eprintln!(
        "OK: chat_message_fts_reconcile matched oracle over {} scenarios.",
        corpus.scenarios.len()
    );
}

/// The corpus must reach each of v4's five REACHABLE lines and leave each one
/// silent somewhere, or the capture pins above prove less than they look.
/// Runs with no env var.
#[test]
fn corpus_reaches_and_silences_every_reachable_line() {
    let corpus = read_corpus();
    let long = corpus.long.unit.repeat(corpus.long.times);

    let mut fired: Vec<Vec<String>> = Vec::new();
    for scenario in &corpus.scenarios {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        register_qt_text(&conn).expect("register qt_text");
        conn.execute_batch(&corpus.chat_messages_ddl)
            .expect("create chat_messages");
        ensure_chat_message_fts_schema(&conn).expect("ensure fts schema");
        for step in &scenario.seed {
            seed(&conn, step, &long);
        }
        for sql in &scenario.damage {
            conn.execute_batch(sql).expect("damage");
        }
        for step in &scenario.after_damage {
            seed(&conn, step, &long);
        }
        let (_, lines) = captured_with(|| reconcile_chat_message_fts(&conn));
        fired.push(lines);
    }

    for message in [
        "Message search index objects were missing; recreating",
        "Message search index counts",
        "Message search index is out of step with the transcript; rebuilding",
        "Message search index rebuilt",
        "Chat message FTS reconciliation failed; search may be degraded",
    ] {
        assert!(
            fired
                .iter()
                .any(|ls| ls.iter().any(|l| l.contains(message))),
            "no corpus scenario emits {message:?}"
        );
        assert!(
            fired
                .iter()
                .any(|ls| !ls.iter().any(|l| l.contains(message))),
            "every corpus scenario emits {message:?} — it has no silence leg"
        );
    }
}
