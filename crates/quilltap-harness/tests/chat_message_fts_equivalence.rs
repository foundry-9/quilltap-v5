//! Real-DB differential: v5's `db::chat_message_fts` against v4's REAL
//! `lib/database/backends/sqlite/chat-message-fts.ts` (`f45a517a9`), driven
//! step for step over the shared script
//! `harness/oracle/fixtures/chat-message-fts.json`.
//!
//! The corpus carries the reduced `chat_messages` DDL, so **neither side spells
//! the base table** — and it carries v4's own twelve test scenarios rebuilt as
//! data, plus five this port adds (the whole schema absent, an identical-text
//! rewrite, an INELIGIBLE row's content update, a rebuild crossing the 500-row
//! batch boundary, and the `TOOL` / `ASSISTANT` / NULL-role eligibility arms).
//!
//! ## What is compared, after every observed step
//!
//! `missing` · `eligible` · `indexed` · the whole `chat_messages_fts_map`
//! (by `ftsId`) · per message the STORAGE FORM and `qt_text()` of its cell ·
//! every corpus `MATCH` expression's hit list · `COUNT(*)` of the FTS5 shadow
//! table `chat_messages_fts_data` · the step's own thrown message · the rebuild
//! result and its per-batch progress callbacks. Plus `sqlite_master.sql` for
//! the five objects, per scenario — so the ON-DISK DDL text is the comparand,
//! not two transcriptions of the same intent — and the `ddl` row, which pins
//! v5's `CHAT_MESSAGE_FTS_SCHEMA_STATEMENTS` byte-for-byte against a recording
//! of v4's array.
//!
//! ## `ftsDataRows`, and why it is in the comparand
//!
//! `COUNT(*)` of the FTS5 shadow table `chat_messages_fts_data` is the ONLY
//! field that can see the `_au` trigger's `WHEN qt_text(new) IS NOT
//! qt_text(old)` guard do its job. Everything else about a re-tokenize is
//! invisible: `contentless_delete=1` means "delete rowid N, insert rowid N
//! with the same terms" leaves the same map row, the same `ftsId` and the same
//! hit lists, so without this field the guard would be a performance comment
//! rather than a tested invariant.
//!
//! MEASURED (2026-09-21, both sides): with the guard as v4 wrote it, an
//! encoding-only update — the same text rewritten as its brotli BLOB, which is
//! exactly what `compress-chat-message-text-v1` does to every row — leaves
//! `chat_messages_fts_data` at THREE rows, the trigger never having fired.
//! Swap the `WHEN` to compare raw `content` and the field MOVES, because the
//! trigger then re-tokenizes every backfilled row for nothing. That is the
//! order's named mutation proof, and it fires on this field and no other. The
//! two SQLite builds agree on the count (v4's `better-sqlite3` against v5's
//! SQLite3MC amalgamation), so it is a legitimate comparand rather than an
//! internals leak — and it is sensitive enough that changing
//! `REBUILD_BATCH_SIZE` also reddens it, which is the second proof it earns.
//!
//! ## Regenerating
//!
//! ```bash
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=/Users/csebold/source/quilltap-v5
//!   TMPO=/tmp/qt-chat-message-fts-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/chat-message-fts.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/chat-message-fts.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-chat-message-fts.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "chat-message-fts\.test\.ts$"
//!   QT_ORACLE_CHAT_MESSAGE_FTS=/tmp/oracle-chat-message-fts.ndjson \
//!     cargo test -p quilltap-harness --test chat_message_fts_equivalence -- --nocapture
//! ```
//!
//! With `QT_ORACLE_CHAT_MESSAGE_FTS` unset the differential SKIPs (and says so).

use std::collections::BTreeMap;
use std::path::PathBuf;

use quilltap_core::db::chat_message_fts::{
    chat_message_fts_eligibility_sql, chat_message_fts_object_names, count_eligible_chat_messages,
    count_indexed_chat_messages, ensure_chat_message_fts_schema, missing_chat_message_fts_objects,
    rebuild_chat_message_fts_index, CHAT_MESSAGE_FTS_SCHEMA_STATEMENTS,
};
use quilltap_core::db::text_compression::{register_qt_text, text_to_blob, TextCell};
use rusqlite::types::ValueRef;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Map, Value};

// ---------------------------------------------------------------- the corpus

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
    #[serde(default, rename = "noUdf")]
    no_udf: bool,
    #[serde(default)]
    quiet: bool,
    steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
struct Step {
    op: String,
    #[serde(default)]
    sql: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    content: Option<Content>,
    #[serde(default, rename = "type")]
    row_type: Option<String>,
    /// Absent = `'USER'`; present-and-null = a NULL role. `Option<Option<_>>`
    /// is the tri-state serde needs to tell those two apart.
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

// ---------------------------------------------------------------- the oracle

#[derive(Debug, Deserialize)]
#[serde(tag = "row", rename_all = "camelCase")]
enum OracleRow {
    Ddl {
        #[serde(rename = "objectNames")]
        object_names: Vec<String>,
        #[serde(rename = "eligibilitySql")]
        eligibility_sql: BTreeMap<String, String>,
        statements: Vec<String>,
    },
    SqliteMaster {
        scenario: String,
        objects: Value,
    },
    Step {
        scenario: String,
        step: usize,
        op: String,
        #[serde(flatten)]
        observation: Map<String, Value>,
    },
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-message-fts.json")
}

fn read_corpus() -> Corpus {
    let path = corpus_path();
    serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
    )
    .expect("parse chat-message-fts corpus")
}

// ------------------------------------------------------------- the v5 driver

/// Bind a corpus `content` spec to a SQL value, in the storage form v4 writes
/// it in — `LongBlob` through v5's own `text_to_blob`, so the compressed cell
/// the trigger sees is the cell v5 would really have written.
fn bind_content(content: &Content, long: &str) -> Box<dyn rusqlite::ToSql> {
    match content {
        Content::Text { value } => Box::new(value.clone()),
        Content::Long => Box::new(long.to_string()),
        Content::LongBlob => match text_to_blob(long) {
            TextCell::Blob(bytes) => Box::new(bytes),
            TextCell::Text(_) => {
                panic!("corpus expects LONG to compress, but text_to_blob returned Text")
            }
        },
        Content::Null => Box::new(Option::<String>::None),
    }
}

/// v5's `DbError::Sqlite` renders as `"sqlite error: <message>"`; better-sqlite3
/// throws SQLite's message bare. The comparand is WHICH observation failed and
/// WHAT SQLite said about it, not either side's error envelope, so the one v5
/// wrapper prefix is stripped here — the only normalization in this family.
fn sqlite_message(e: &quilltap_core::db::DbError) -> String {
    match e {
        quilltap_core::db::DbError::Sqlite(inner) => inner.to_string(),
        other => other.to_string(),
    }
}

fn attempt<T>(f: impl FnOnce() -> Result<T, String>) -> (Option<T>, Option<String>) {
    match f() {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(e)),
    }
}

/// Every field the oracle's `observe()` records, in v5's terms.
fn observe(conn: &Connection, scenario: &Scenario, corpus: &Corpus) -> Map<String, Value> {
    let mut obs_errors: BTreeMap<String, String> = BTreeMap::new();

    let (missing, err) =
        attempt(|| missing_chat_message_fts_objects(conn).map_err(|e| sqlite_message(&e)));
    if let Some(e) = err {
        obs_errors.insert("missing".into(), e);
    }
    let (eligible, err) =
        attempt(|| count_eligible_chat_messages(conn).map_err(|e| sqlite_message(&e)));
    if let Some(e) = err {
        obs_errors.insert("eligible".into(), e);
    }
    let (indexed, err) =
        attempt(|| count_indexed_chat_messages(conn).map_err(|e| sqlite_message(&e)));
    if let Some(e) = err {
        obs_errors.insert("indexed".into(), e);
    }

    let (map, err) = attempt(|| {
        let mut stmt = conn
            .prepare(r#"SELECT "ftsId", "messageId" FROM "chat_messages_fts_map" ORDER BY "ftsId""#)
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(json!({ "ftsId": row.get::<_, i64>(0)?, "messageId": row.get::<_, String>(1)? }))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(Value::Array(rows))
    });
    if let Some(e) = err {
        obs_errors.insert("map".into(), e);
    }

    let (contents, err) = attempt(|| {
        let sql = if scenario.no_udf {
            r#"SELECT "id", "content" AS raw, NULL AS decoded FROM "chat_messages" ORDER BY "id""#
        } else {
            r#"SELECT "id", "content" AS raw, qt_text("content") AS decoded FROM "chat_messages" ORDER BY "id""#
        };
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                let kind = match row.get_ref(1)? {
                    ValueRef::Null => "null",
                    ValueRef::Blob(_) => "blob",
                    _ => "text",
                };
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "kind": kind,
                    "decoded": row.get::<_, Option<String>>(2)?,
                }))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(Value::Array(rows))
    });
    if let Some(e) = err {
        obs_errors.insert("contents".into(), e);
    }

    // v4's own probe query, byte for byte.
    const SEARCH_SQL: &str = r#"SELECT m."id" AS id
             FROM "chat_messages_fts" f
             JOIN "chat_messages_fts_map" x ON x."ftsId" = f.rowid
             JOIN "chat_messages" m         ON m."id" = x."messageId"
            WHERE "chat_messages_fts" MATCH ?
            ORDER BY m."createdAt" DESC"#;
    let mut searches = Map::new();
    for needle in &corpus.searches {
        let (hits, err) = attempt(|| {
            let mut stmt = conn.prepare(SEARCH_SQL).map_err(|e| e.to_string())?;
            let ids = stmt
                .query_map([needle], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(Value::from(ids))
        });
        if let Some(e) = err {
            obs_errors.insert(format!("search:{needle}"), e);
        }
        searches.insert(needle.clone(), hits.unwrap_or(Value::Null));
    }

    let fts_data_rows = conn
        .query_row(
            r#"SELECT COUNT(*) AS n FROM "chat_messages_fts_data""#,
            [],
            |row| row.get::<_, i64>(0),
        )
        .ok();

    let mut out = Map::new();
    out.insert("missing".into(), to_value_opt(missing));
    out.insert("eligible".into(), to_value_opt(eligible));
    out.insert("indexed".into(), to_value_opt(indexed));
    out.insert("map".into(), map.unwrap_or(Value::Null));
    out.insert("contents".into(), contents.unwrap_or(Value::Null));
    out.insert("searches".into(), Value::Object(searches));
    out.insert("ftsDataRows".into(), to_value_opt(fts_data_rows));
    out.insert(
        "obsErrors".into(),
        Value::Object(
            obs_errors
                .into_iter()
                .map(|(k, v)| (k, Value::from(v)))
                .collect(),
        ),
    );
    out
}

fn to_value_opt<T: Into<Value>>(v: Option<T>) -> Value {
    v.map(Into::into).unwrap_or(Value::Null)
}

/// Run one step, returning `(error, rebuild, progress)` the way the oracle does.
fn run_step(
    conn: &Connection,
    step: &Step,
    long: &str,
) -> (Option<String>, Option<Value>, Option<Value>) {
    let mut rebuild = None;
    let mut progress = None;
    let outcome: Result<(), String> = (|| match step.op.as_str() {
        "ensure" => ensure_chat_message_fts_schema(conn).map_err(|e| sqlite_message(&e)),
        "exec" => conn
            .execute_batch(step.sql.as_deref().expect("exec step needs sql"))
            .map_err(|e| e.to_string()),
        "insert" => {
            let id = step.id.as_deref().expect("insert step needs id");
            let row_type = step.row_type.as_deref().unwrap_or("message");
            // Absent role = 'USER'; present-and-null = NULL (v4's helper).
            let role: Option<String> = match &step.role {
                None => Some("USER".to_string()),
                Some(inner) => inner.clone(),
            };
            let created_at = format!(
                "2026-01-01T00:00:{:0>2}.000Z",
                &id[id.len().saturating_sub(2)..]
            );
            let content = bind_content(step.content.as_ref().expect("insert needs content"), long);
            conn.execute(
                r#"INSERT INTO "chat_messages" ("id","chatId","type","role","content","createdAt") VALUES (?,?,?,?,?,?)"#,
                rusqlite::params![id, "chat-1", row_type, role, content.as_ref(), created_at],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        }
        "update" => {
            let content = bind_content(step.content.as_ref().expect("update needs content"), long);
            conn.execute(
                r#"UPDATE "chat_messages" SET "content" = ? WHERE "id" = ?"#,
                rusqlite::params![
                    content.as_ref(),
                    step.id.as_deref().expect("update needs id")
                ],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        }
        "delete" => conn
            .execute(
                r#"DELETE FROM "chat_messages" WHERE "id" = ?"#,
                rusqlite::params![step.id.as_deref().expect("delete needs id")],
            )
            .map(|_| ())
            .map_err(|e| e.to_string()),
        "rebuild" => {
            let mut calls: Vec<Value> = Vec::new();
            let result = {
                let mut cb = |scanned: i64, total: i64| calls.push(json!([scanned, total]));
                rebuild_chat_message_fts_index(conn, Some(&mut cb)).map_err(|e| sqlite_message(&e))
            };
            progress = Some(Value::Array(calls));
            let result = result?;
            // `durationMs` is a clock and is NOT a comparand.
            rebuild = Some(json!({
                "scanned": result.scanned,
                "indexed": result.indexed,
                "total": result.total,
            }));
            Ok(())
        }
        other => panic!("unknown step op: {other}"),
    })();
    (outcome.err(), rebuild, progress)
}

/// v5's `sqlite_master` rows for the five objects, in the oracle's shape.
fn sqlite_master_objects(conn: &Connection) -> Value {
    let names = chat_message_fts_object_names();
    let placeholders = (0..names.len()).map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT name, type, sql FROM sqlite_master WHERE name IN ({placeholders}) ORDER BY name"
    );
    let mut stmt = conn.prepare(&sql).expect("prepare sqlite_master");
    let rows: Vec<Value> = stmt
        .query_map(rusqlite::params_from_iter(names.iter()), |row| {
            Ok(json!({
                "name": row.get::<_, String>(0)?,
                "type": row.get::<_, String>(1)?,
                "sql": row.get::<_, Option<String>>(2)?,
            }))
        })
        .expect("query sqlite_master")
        .collect::<Result<_, _>>()
        .expect("collect sqlite_master");
    Value::Array(rows)
}

#[test]
fn chat_message_fts_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHAT_MESSAGE_FTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHAT_MESSAGE_FTS to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"));
    let rows: Vec<OracleRow> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("parse oracle row: {e}\n{l}")))
        .collect();

    let corpus = read_corpus();
    let long = corpus.long.unit.repeat(corpus.long.times);

    // ---- the `ddl` row: the byte pin on the statements themselves ---------
    let ddl = rows
        .iter()
        .find_map(|r| match r {
            OracleRow::Ddl {
                object_names,
                eligibility_sql,
                statements,
            } => Some((object_names, eligibility_sql, statements)),
            _ => None,
        })
        .expect("oracle carries no `ddl` row");
    let (object_names, eligibility_sql, statements) = ddl;
    assert_eq!(
        chat_message_fts_object_names(),
        object_names.iter().map(String::as_str).collect::<Vec<_>>(),
        "chatMessageFtsObjectNames diverged"
    );
    for (alias, expected) in eligibility_sql {
        assert_eq!(
            &chat_message_fts_eligibility_sql(alias),
            expected,
            "eligibility SQL diverged for alias {alias:?}"
        );
    }
    assert_eq!(
        statements.len(),
        CHAT_MESSAGE_FTS_SCHEMA_STATEMENTS.len(),
        "statement COUNT diverged"
    );
    for (i, expected) in statements.iter().enumerate() {
        assert_eq!(
            CHAT_MESSAGE_FTS_SCHEMA_STATEMENTS[i], expected,
            "statement {i} diverged BYTE-WISE from v4's\n  v5: {:?}\n  v4: {:?}",
            CHAT_MESSAGE_FTS_SCHEMA_STATEMENTS[i], expected
        );
    }

    // ---- the scenarios ---------------------------------------------------
    let mut compared = 0usize;
    for scenario in &corpus.scenarios {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        if !scenario.no_udf {
            register_qt_text(&conn).expect("register qt_text");
        }
        conn.execute_batch(&corpus.chat_messages_ddl)
            .expect("create chat_messages");
        ensure_chat_message_fts_schema(&conn).expect("ensure fts schema");

        let oracle_master = rows
            .iter()
            .find_map(|r| match r {
                OracleRow::SqliteMaster {
                    scenario: s,
                    objects,
                } if *s == scenario.name => Some(objects),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no sqliteMaster row for scenario {:?}", scenario.name));
        assert_eq!(
            &sqlite_master_objects(&conn),
            oracle_master,
            "scenario {:?}: sqlite_master text for the five objects diverged",
            scenario.name
        );
        compared += 1;

        for (i, step) in scenario.steps.iter().enumerate() {
            let (error, rebuild, progress) = run_step(&conn, step, &long);
            let observed = !scenario.quiet || step.op != "insert" || i == scenario.steps.len() - 1;
            if !observed {
                continue;
            }

            let oracle_obs = rows
                .iter()
                .find_map(|r| match r {
                    OracleRow::Step {
                        scenario: s,
                        step: n,
                        op,
                        observation,
                    } if *s == scenario.name && *n == i => Some((op, observation)),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no oracle step row for {:?} step {i}", scenario.name));
            let (oracle_op, oracle_observation) = oracle_obs;
            assert_eq!(oracle_op, &step.op, "{:?} step {i}: op", scenario.name);

            let mut got = observe(&conn, scenario, &corpus);
            got.insert("error".into(), to_value_opt(error));
            got.insert("rebuild".into(), rebuild.unwrap_or(Value::Null));
            got.insert("progress".into(), progress.unwrap_or(Value::Null));

            for key in [
                "error",
                "missing",
                "eligible",
                "indexed",
                "map",
                "contents",
                "searches",
                "ftsDataRows",
                "obsErrors",
                "rebuild",
                "progress",
            ] {
                let mine = got.get(key).unwrap_or(&Value::Null);
                let theirs = oracle_observation.get(key).unwrap_or(&Value::Null);
                assert_eq!(
                    mine,
                    theirs,
                    "scenario {:?} step {i} ({}): `{key}` diverged\n  rust:   {}\n  oracle: {}",
                    scenario.name,
                    step.op,
                    serde_json::to_string(mine).unwrap(),
                    serde_json::to_string(theirs).unwrap(),
                );
            }
            compared += 1;
        }
    }

    eprintln!(
        "OK: chat_message_fts matched oracle over {} scenarios ({compared} comparisons).",
        corpus.scenarios.len()
    );
}

/// The corpus must reach every behaviour the module has, or a green
/// differential proves less than it looks. Runs with no env var.
#[test]
fn corpus_covers_every_behaviour() {
    let corpus = read_corpus();
    let ops: Vec<&str> = corpus
        .scenarios
        .iter()
        .flat_map(|s| s.steps.iter().map(|st| st.op.as_str()))
        .collect();
    for op in ["ensure", "exec", "insert", "update", "delete", "rebuild"] {
        assert!(ops.contains(&op), "no corpus step exercises `{op}`");
    }
    assert!(
        corpus.scenarios.iter().any(|s| s.no_udf),
        "no scenario runs without the qt_text UDF — v4's loud-failure design is untested"
    );
    // The rebuild must cross v4's REBUILD_BATCH_SIZE, or the keyset walk's
    // second page and the repeated progress callback never run.
    assert!(
        corpus
            .scenarios
            .iter()
            .any(|s| s.steps.iter().filter(|st| st.op == "insert").count() > 500),
        "no scenario crosses the 500-row rebuild batch boundary"
    );
    // Every eligibility arm: a system event, a non-conversational role, a NULL
    // role, and a NULL content.
    let inserts: Vec<&Step> = corpus
        .scenarios
        .iter()
        .flat_map(|s| s.steps.iter())
        .filter(|st| st.op == "insert")
        .collect();
    assert!(inserts
        .iter()
        .any(|s| s.row_type.as_deref() == Some("system")));
    assert!(inserts
        .iter()
        .any(|s| s.role == Some(Some("SYSTEM".into()))));
    assert!(inserts.iter().any(|s| s.role == Some(Some("TOOL".into()))));
    assert!(inserts.iter().any(|s| s.role == Some(None)));
    assert!(inserts
        .iter()
        .any(|s| matches!(s.content, Some(Content::Null))));
    assert!(inserts
        .iter()
        .any(|s| matches!(s.content, Some(Content::LongBlob))));
}
