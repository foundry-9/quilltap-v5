//! Tier-2 differential (P4.D331, v4 `e4a60ad9e` + `a495faa26`, at the
//! `322a5488e` pin): the Log's repository and its one write door —
//! `quilltap_core::db::log_entries` against v4's REAL `LogEntriesRepository`
//! and `writeLogEntry`, over the same llm-logs fixture.
//!
//! Both sides copy the fixture `build-log-entries-fixture.ts` seeded through
//! v4's real repository (so the table carries v4's three indexes), run the
//! committed spec's op list in order on the copy, and record per op what it
//! answered (or the error text), every log line it emitted, and — after a
//! mutating op — the whole table with `hex(message)`. The regexp rows run
//! through the `regexp()` scalar registered on the llm-logs connection.
//!
//! Lines are compared as `LEVEL message key=value …` (v5's tracing target
//! dropped; an object field rides as `<key>Json`). Ids the spec did not pin
//! are `<minted>`, their stamps `<now>`, on both sides.
//!
//! Measured at the pin (the lane record has the runs): the write door's clip
//! of a surrogate pair at unit 4000 STORES `EF BF BD` on v4's side — the same
//! bytes v5's lossy UTF-16 truncate writes (the dump rows agree) — but the
//! entry v4 hands back still holds the LONE high surrogate (a JS string can);
//! a Rust `String` cannot, so v5's echo is U+FFFD: [`LONE_SURROGATE_CLIP`].
//! A `NaN` retention window throws `Invalid time value` OUT of
//! `cleanupOldEntries` (the cutoff is rendered before `withRawDb`).
//!
//! Generate the fixture + oracle (Node 24, from the pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-log-entries-fixture.db PATH=$N:$PATH \
//!     npx tsx $V5W/harness/oracle/fixtures/build-log-entries-fixture.ts
//!   TZ=America/Chicago QT_FIXTURE_LOG_ENTRIES=/tmp/qt-log-entries-fixture.db PATH=$N:$PATH \
//!     npx tsx $V5W/harness/oracle/cases/log-entries-tier2.ts \
//!     > /tmp/oracle-log-entries-tier2.ndjson
//! Run:
//!   QT_ORACLE_LOG_ENTRIES=/tmp/oracle-log-entries-tier2.ndjson \
//!   QT_FIXTURE_LOG_ENTRIES=/tmp/qt-log-entries-fixture.db \
//!     cargo test -p quilltap-harness --test log_entries_tier2_equivalence -- --nocapture
//!
//! Skips (does not fail) when either env var is unset — the standing gated
//! discipline.

use std::collections::HashSet;
use std::path::PathBuf;

use quilltap_core::db::fallback::error_text;
use quilltap_core::db::log_entries::{
    write_log_entry, LogEntriesRepository, LogEntryCreateOptions, LogEntryFilter, LogEntryRow,
    WriteLogEntryParams,
};
use quilltap_core::db::table_shape::Partition;
use quilltap_core::db::Writer;
use quilltap_core::test_support::captured_with;
use serde::Deserialize;
use serde_json::{json, Value};

/// `(op, v4's outWellFormed, v5's)` — the ruled divergence: v4's returned
/// entry keeps the lone surrogate its UTF-16 clip left; v5's is well-formed
/// (U+FFFD, the bytes BOTH sides store). Observable only in an echo of the
/// written entry (`log_create_entry`'s `{entry}`), never in the table.
const LONE_SURROGATE_CLIP: &[(&str, bool, bool)] = &[("write-clip-astral-split", false, true)];

const SPEC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../harness/oracle/fixtures/log-entries-tier2.json"
);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    tz: String,
    now_iso: String,
    seed: Vec<Value>,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Op {
    id: String,
    kind: String,
    #[serde(default)]
    filter: Option<Value>,
    #[serde(default)]
    pattern: String,
    #[serde(default)]
    flags: String,
    #[serde(default)]
    limit: i64,
    #[serde(default)]
    message: Option<Value>,
    #[serde(default)]
    chat_id: String,
    #[serde(default)]
    character_id: Option<String>,
    #[serde(default)]
    options: Option<Value>,
    #[serde(default)]
    retention_days: Option<Value>,
}

fn expand(m: &Value) -> String {
    match m {
        Value::String(s) => s.clone(),
        Value::Object(o) => {
            let unit = o["repeat"].as_str().unwrap();
            let n = o["count"].as_u64().unwrap() as usize;
            format!(
                "{}{}",
                unit.repeat(n),
                o.get("then").and_then(Value::as_str).unwrap_or("")
            )
        }
        other => panic!("bad message {other}"),
    }
}

/// v5's capture line (`LEVEL target message k=v …`) as the oracle renders it.
fn line(captured: &str, minted: &[String]) -> String {
    let mut parts = captured.splitn(3, ' ');
    let level = parts.next().unwrap();
    let _target = parts.next().unwrap();
    let rest = parts.next().unwrap_or("");
    let mut out = format!("{level} {rest}");
    for id in minted {
        out = out.replace(id.as_str(), "<minted>");
    }
    out
}

struct Normalizer {
    pinned: HashSet<String>,
}

impl Normalizer {
    fn entry(&self, e: &LogEntryRow) -> Value {
        let minted = !self.pinned.contains(&e.id);
        json!({
            "id": if minted { "<minted>" } else { e.id.as_str() },
            "message": e.message,
            "chatId": e.chat_id,
            "characterId": e.character_id,
            "createdAt": if minted { "<now>" } else { e.created_at.as_str() },
            "updatedAt": if minted { "<now>" } else { e.updated_at.as_str() },
        })
    }

    fn dump(&self, conn: &rusqlite::Connection) -> Value {
        let mut stmt = conn
            .prepare(
                r#"SELECT "id", "chatId", "characterId", "createdAt", "updatedAt", hex("message") FROM log_entries"#,
            )
            .unwrap();
        let mut rows: Vec<(bool, String, String, Value)> = stmt
            .query_map([], |r| {
                let id: String = r.get(0)?;
                let minted = !self.pinned.contains(&id);
                let hex: String = r.get(5)?;
                let shown = if minted {
                    "<minted>".to_string()
                } else {
                    id.clone()
                };
                let value = json!({
                    "id": shown,
                    "chatId": r.get::<_, String>(1)?,
                    "characterId": r.get::<_, Option<String>>(2)?,
                    "createdAt": if minted { "<now>".to_string() } else { r.get::<_, String>(3)? },
                    "updatedAt": if minted { "<now>".to_string() } else { r.get::<_, String>(4)? },
                    "messageHex": hex,
                });
                Ok((minted, hex, shown, value))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        rows.sort_by(|a, b| (a.0, &a.1, &a.2).cmp(&(b.0, &b.1, &b.2)));
        Value::Array(rows.into_iter().map(|r| r.3).collect())
    }
}

/// The oracle's `settleMintedRuns`: a run of rows minted "now" ties on
/// `createdAt` up to clock noise, so it is sorted by message bytes.
fn settle_minted_runs(out: Vec<Value>) -> Value {
    let mut result = Vec::new();
    let mut run: Vec<Value> = Vec::new();
    let flush = |run: &mut Vec<Value>, result: &mut Vec<Value>| {
        run.sort_by(|a, b| a["message"].as_str().cmp(&b["message"].as_str()));
        result.append(run);
    };
    for e in out {
        if e["createdAt"] == "<now>" {
            run.push(e);
        } else {
            flush(&mut run, &mut result);
            result.push(e);
        }
    }
    flush(&mut run, &mut result);
    Value::Array(result)
}

fn filter_of(v: &Value) -> LogEntryFilter {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    LogEntryFilter {
        chat_id: s("chatId"),
        character_id: s("characterId"),
        from: s("from"),
        to: s("to"),
        limit: v["limit"].as_i64().unwrap(),
    }
}

#[test]
fn every_op_answers_logs_and_leaves_the_table_as_v4() {
    let (Ok(oracle), Ok(fixture)) = (
        std::env::var("QT_ORACLE_LOG_ENTRIES"),
        std::env::var("QT_FIXTURE_LOG_ENTRIES"),
    ) else {
        eprintln!("SKIP: QT_ORACLE_LOG_ENTRIES / QT_FIXTURE_LOG_ENTRIES unset");
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(SPEC).unwrap()).unwrap();
    let rows: Vec<Value> = std::fs::read_to_string(&oracle)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), spec.ops.len(), "one oracle row per op");

    let dir = tempfile::tempdir().unwrap();
    let work: PathBuf = dir.path().join("llm-logs-work.db");
    std::fs::copy(&fixture, &work).unwrap();
    let writer =
        Writer::open_writable_in(Partition::LlmLogs, &work, &spec.test_pepper_base64).unwrap();
    let conn = writer.connection();
    let repo = LogEntriesRepository::new(conn);

    let mut pinned: HashSet<String> = spec
        .seed
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    for op in &spec.ops {
        if let Some(id) = op
            .options
            .as_ref()
            .and_then(|o| o["id"].as_str())
            .filter(|s| !s.is_empty())
        {
            pinned.insert(id.to_string());
        }
    }
    let norm = Normalizer { pinned };
    let now_ms = quilltap_core::clock::iso_to_ms(&spec.now_iso).expect("nowIso");

    let mut failures = Vec::new();
    for (op, want) in spec.ops.iter().zip(&rows) {
        assert_eq!(want["op"], op.id.as_str());
        let mut minted: Vec<String> = Vec::new();
        let mut got = serde_json::Map::new();
        let (_, lines) = captured_with(|| match op.kind.as_str() {
            "find" => {
                let found = repo.find_entries(&filter_of(op.filter.as_ref().unwrap()));
                got.insert(
                    "out".into(),
                    settle_minted_runs(found.iter().map(|e| norm.entry(e)).collect()),
                );
            }
            "search" => match repo.search_messages(&op.pattern, &op.flags, op.limit) {
                Ok(found) => {
                    got.insert(
                        "out".into(),
                        settle_minted_runs(found.iter().map(|e| norm.entry(e)).collect()),
                    );
                }
                Err(e) => {
                    got.insert("error".into(), json!(error_text(&e)));
                }
            },
            "create" => {
                let options = op.options.as_ref().map(|o| LogEntryCreateOptions {
                    id: o["id"].as_str().map(str::to_string),
                    created_at: o["createdAt"].as_str().map(str::to_string),
                    updated_at: o["updatedAt"].as_str().map(str::to_string),
                });
                match repo.create(
                    &expand(op.message.as_ref().unwrap()),
                    &op.chat_id,
                    op.character_id.as_deref(),
                    options.as_ref(),
                ) {
                    Ok(row) => {
                        if !norm.pinned.contains(&row.id) {
                            minted.push(row.id.clone());
                        }
                        got.insert("out".into(), norm.entry(&row));
                    }
                    Err(e) => {
                        got.insert("error".into(), json!(error_text(&e)));
                    }
                }
            }
            "write" => match write_log_entry(
                conn,
                WriteLogEntryParams {
                    message: &expand(op.message.as_ref().unwrap()),
                    chat_id: &op.chat_id,
                    character_id: op.character_id.as_deref(),
                },
            ) {
                Ok(row) => {
                    if !norm.pinned.contains(&row.id) {
                        minted.push(row.id.clone());
                    }
                    got.insert("out".into(), norm.entry(&row));
                    let hex: String = row.message.bytes().map(|b| format!("{b:02X}")).collect();
                    got.insert("outHex".into(), json!(hex));
                    // A Rust `String` is always well-formed.
                    got.insert("outWellFormed".into(), json!(true));
                }
                Err(e) => {
                    got.insert("error".into(), json!(error_text(&e)));
                }
            },
            "cleanup" => {
                let days = match op.retention_days.as_ref().unwrap() {
                    Value::String(s) if s == "NaN" => f64::NAN,
                    v => v.as_f64().unwrap(),
                };
                match repo.cleanup_old_entries(days, now_ms, &spec.tz) {
                    Ok(n) => {
                        got.insert("out".into(), json!(n));
                    }
                    Err(e) => {
                        got.insert("error".into(), json!(error_text(&e)));
                    }
                }
            }
            other => panic!("unknown op kind {other}"),
        });
        let lines: Vec<String> = lines.iter().map(|l| line(l, &minted)).collect();
        got.insert("lines".into(), json!(lines));
        if matches!(op.kind.as_str(), "create" | "write" | "cleanup") {
            got.insert("dump".into(), norm.dump(conn));
        }
        if let Some(&(_, v4, v5)) = LONE_SURROGATE_CLIP.iter().find(|r| r.0 == op.id) {
            assert_eq!(
                (
                    want["outWellFormed"].as_bool(),
                    got["outWellFormed"].as_bool()
                ),
                (Some(v4), Some(v5)),
                "{}: the LONE_SURROGATE_CLIP row",
                op.id
            );
        } else if want.get("outWellFormed") != got.get("outWellFormed") {
            failures.push(format!("{} — outWellFormed", op.id));
        }
        for key in ["out", "error", "outHex", "lines", "dump"] {
            let (w, g) = (want.get(key), got.get(key));
            if w != g {
                failures.push(format!(
                    "{} — {key}:\n  v4 {}\n  v5 {}",
                    op.id,
                    serde_json::to_string(&w).unwrap(),
                    serde_json::to_string(&g).unwrap()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} disagreements:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("log_entries_tier2: {} ops agree", spec.ops.len());
}
