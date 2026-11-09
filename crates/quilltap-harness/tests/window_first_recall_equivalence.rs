//! P4.D375 tier-2 differential: the window-first dated search (v4 4.12.1
//! `6121f59b9`, the `624d0d25c` pin) — `memories_read::find_ids_in_event_window`,
//! recall's `gather` (the vector arm's index re-search over the window's ids,
//! the text arm's newest-first window rows) and `search_recall` on the tool
//! path (the ≤ 14-day floor relaxation) and the prompt path — against v4's REAL
//! `findIdsInEventWindow` / `gather` / `searchRecall`
//! (`harness/oracle/cases/window-first-recall.ts`), from the SAME starting
//! file the oracle wrote.
//!
//! Per op: the RESULT (ids exact, scores / salience at 1e-12, `via` exact), the
//! TRACE (byte-exact JSON) and the LINES (`Found memories in event window`,
//! `Error finding memories in event window`, `[Memory] Window-first gather` —
//! message and fields in v4's order, an object as its `…Json` key; the silence
//! legs are the undated ops, which log none).
//!
//! **`LEXICAL_WINDOW_VS_DATE_PARSE` (a v4-SHARED behaviour, NOT a v5
//! divergence — both sides answer the same; R-3):** the SQL gather compares
//! `COALESCE(occurredAt, createdAt)` as TEXT while `score` reads `Date.parse`.
//! The table below records, per planted event-time shape against the one-day
//! UTC window `2026-11-03`, whether the SQL window gathers the row and whether
//! `Date.parse` places it in the window — and the test asserts both columns
//! from the oracle, so the table cannot go stale silently. A row the two
//! disagree on is a v4 filing CANDIDATE (draft only; the human's go-ahead
//! first). Retires when v4 compares event times chronologically.
//!
//! Regenerate + run:
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -rf /tmp/qt-window-first-recall /tmp/oracle-window-first-recall.ndjson
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_WFR_DIR=/tmp/qt-window-first-recall \
//!     $N/npx tsx $V5W/harness/oracle/cases/window-first-recall.ts \
//!     > /tmp/oracle-window-first-recall.ndjson
//!   cd $V5W
//!   QT_ORACLE_WINDOW_FIRST_RECALL=/tmp/oracle-window-first-recall.ndjson \
//!   QT_FIXTURE_WFR_DIR=/tmp/qt-window-first-recall \
//!     cargo test -p quilltap-harness --test window_first_recall_equivalence -- --nocapture

use std::collections::HashSet;

use quilltap_core::db::memories_read;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::embedding::EmbeddingResult;
use quilltap_core::recall_tags::TimeWindow;
use quilltap_core::services::recall::context::RecallContextInput;
use quilltap_core::services::recall::gather::{gather, GatherOptions};
use quilltap_core::services::recall::query::RecallQuery;
use quilltap_core::services::recall::search::{search_recall, RecallSearchOptions};
use quilltap_core::test_support::global_capture;
use serde_json::{json, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const NOW_MS: f64 = 1_793_800_800_000.0; // 2026-11-04T14:00:00.000Z

/// The messages this family compares, and the target each rides in v5.
const LINES: &[(&str, &str)] = &[
    ("Found memories in event window", "quilltap::db"),
    ("Error finding memories in event window", "quilltap::db"),
    ("[Memory] Window-first gather", "quilltap::memories"),
];

/// `LEXICAL_WINDOW_VS_DATE_PARSE` — (memory number, shape, gathered by the SQL
/// window?, in the window under `Date.parse`?). Every row is asserted from the
/// oracle (the first `findIds` op for the SQL column; the planted value
/// through v5's V8 `Date.parse` twin for the other).
const LEXICAL_WINDOW_VS_DATE_PARSE: &[(u32, &str, bool, bool)] = &[
    (
        300,
        "2026-11-03 (date-only, the window's first day)",
        false,
        true,
    ),
    (301, "2026-11-03T01:00:00+05:00 (offset)", true, false),
    (
        302,
        "2026-11-03T23:59:59Z (no millis, the window's last second)",
        false,
        true,
    ),
    (303, "soon (unparseable)", false, false),
    (
        304,
        "2026-11-03T12:61:00.000Z (invalid minute)",
        true,
        false,
    ),
];

fn mid(n: u32) -> String {
    format!("d3750000-0000-4000-8000-{n:012}")
}

fn window_of(v: &Value) -> Option<TimeWindow> {
    v.as_object().map(|o| TimeWindow {
        from: o["from"].as_str().unwrap().to_string(),
        to: o["to"].as_str().unwrap().to_string(),
    })
}

fn query_of(window: Option<TimeWindow>, embedded: bool) -> RecallQuery {
    RecallQuery {
        text: "pool yesterday".to_string(),
        embedding: embedded.then(|| EmbeddingResult {
            embedding: vec![1.0, 0.0, 0.0],
            model: "probe".to_string(),
            dimensions: 3,
            provider: "OPENAI".to_string(),
        }),
        window,
        ..Default::default()
    }
}

fn close(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() <= 1e-12
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| close(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|((k, p), (l, q))| k == l && close(p, q))
        }
        _ => a == b,
    }
}

/// v4's captured line, rendered as v5's capture renders it.
fn want_lines(op: &Value) -> Vec<String> {
    op["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let message = l["message"].as_str().unwrap();
            let target = LINES.iter().find(|(m, _)| *m == message).unwrap().1;
            let mut s = format!(
                "{} {target} {message}",
                l["level"].as_str().unwrap().to_uppercase()
            );
            for (k, v) in l["fields"].as_object().unwrap() {
                match v {
                    Value::String(t) => s.push_str(&format!(" {k}={t}")),
                    Value::Object(_) | Value::Array(_) => {
                        s.push_str(&format!(" {k}Json={}", serde_json::to_string(v).unwrap()))
                    }
                    other => s.push_str(&format!(" {k}={other}")),
                }
            }
            s
        })
        .collect()
}

fn got_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .filter(|l| LINES.iter().any(|(m, t)| l.contains(&format!(" {t} {m}"))))
        .collect()
}

async fn run_op(
    db: &Db,
    character_id: &str,
    excluded: &HashSet<String>,
    op: &Value,
) -> (Value, Value) {
    let input = &op["input"];
    match op["op"].as_str().unwrap() {
        "findIds" => {
            let w = window_of(&input["window"]).unwrap();
            let hot = input["hotOnly"].as_bool().unwrap();
            let cid = character_id.to_string();
            let mut ids = db
                .read_main(move |c| memories_read::find_ids_in_event_window(c, &cid, &w, hot))
                .unwrap();
            ids.sort();
            (json!(ids), json!([]))
        }
        "gather" => {
            let opts = GatherOptions {
                limit: input["limit"].as_u64().unwrap() as usize,
                include_cold: input["includeCold"].as_bool().unwrap(),
                exclude_ids: input["exclude"]
                    .as_bool()
                    .unwrap()
                    .then(|| excluded.clone()),
                force_text: input["forceText"].as_bool().unwrap(),
                ..Default::default()
            };
            let q = query_of(
                window_of(&input["window"]),
                input["embedded"].as_bool().unwrap(),
            );
            let mut trace = Vec::new();
            let out = gather(db, character_id, &q, &opts, &mut trace, None)
                .await
                .unwrap();
            let hits: Vec<Value> = out
                .hits
                .iter()
                .map(|h| json!({ "id": h.id, "score": h.score, "via": h.via }))
                .collect();
            (
                json!({ "strategy": out.strategy.as_str(), "hits": hits }),
                serde_json::to_value(&trace).unwrap(),
            )
        }
        "search" => {
            let opts = RecallSearchOptions {
                limit: Some(input["limit"].as_u64().unwrap() as usize),
                include_cold: input["includeCold"].as_bool().unwrap(),
                literal_phrase: true,
                force_text: input["forceText"].as_bool().unwrap(),
                now_ms: NOW_MS,
                recall_context: input["recallContext"]
                    .as_bool()
                    .unwrap()
                    .then(RecallContextInput::default),
                ..Default::default()
            };
            let q = query_of(
                window_of(&input["window"]),
                input["embedded"].as_bool().unwrap(),
            );
            let mut trace = Vec::new();
            let out = search_recall(db, character_id, &q, &opts, &mut trace, None)
                .await
                .unwrap();
            let candidates: Vec<Value> = out
                .candidates
                .iter()
                .map(|c| json!({ "id": c.memory["id"], "score": c.score, "salience": c.salience }))
                .collect();
            (
                json!({ "strategy": out.strategy.as_str(), "candidates": candidates }),
                serde_json::to_value(&trace).unwrap(),
            )
        }
        other => panic!("no Rust script for op {other}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn window_first_recall_matches_v4() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_WINDOW_FIRST_RECALL") else {
        eprintln!("SKIP: set QT_ORACLE_WINDOW_FIRST_RECALL to the oracle NDJSON (see header).");
        return;
    };
    let Ok(dir) = std::env::var("QT_FIXTURE_WFR_DIR") else {
        eprintln!("SKIP: set QT_FIXTURE_WFR_DIR to the case's dir (see header).");
        return;
    };
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("oracle line");
    assert_eq!(
        NOW_MS,
        quilltap_core::memory_scoring::date_ms("2026-11-04T14:00:00.000Z")
    );
    global_capture::install();
    let character_id = oracle["characterId"].as_str().unwrap().to_string();
    let excluded: HashSet<String> = oracle["excluded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let scratch = tempfile::tempdir().unwrap();
    let work = scratch.path().join("main.db");
    std::fs::copy(std::path::Path::new(&dir).join("base.db"), &work).expect("copy base.db");
    let mut db = Db::open_main(&work, PEPPER).expect("open");

    let ops = oracle["ops"].as_array().unwrap();
    // LEXICAL_WINDOW_VS_DATE_PARSE: both columns, from the oracle.
    let first_day: Vec<&str> = ops[0]["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        ops[0]["input"]["window"]["from"],
        "2026-11-03T00:00:00.000Z"
    );
    let (from, to) = (
        quilltap_core::memory_scoring::date_ms("2026-11-03T00:00:00.000Z"),
        quilltap_core::memory_scoring::date_ms("2026-11-03T23:59:59.999Z"),
    );
    for (n, shape, gathered, parsed_in) in LEXICAL_WINDOW_VS_DATE_PARSE {
        assert_eq!(
            first_day.contains(&mid(*n).as_str()),
            *gathered,
            "LEXICAL_WINDOW_VS_DATE_PARSE {n} ({shape}): the SQL column moved"
        );
        let raw = shape.split(' ').next().unwrap();
        let t = quilltap_core::memory_scoring::date_ms(raw);
        assert_eq!(
            t.is_finite() && t >= from && t <= to,
            *parsed_in,
            "LEXICAL_WINDOW_VS_DATE_PARSE {n} ({shape}): the Date.parse column moved"
        );
    }

    let mut failures = Vec::new();
    let mut window_lines = 0usize;
    let mut relaxed = 0usize;
    let mut via_window = 0usize;
    for (i, op) in ops.iter().enumerate() {
        let kind = op["op"].as_str().unwrap();
        if kind == "plant" {
            let sql = op["input"].as_str().unwrap().to_string();
            drop(db);
            let w = quilltap_core::db::Writer::open_writable(&work, PEPPER).expect("writable");
            w.connection().execute_batch(&sql).expect("plant");
            drop(w);
            db = Db::open_main(&work, PEPPER).expect("reopen");
            continue;
        }
        let ((got, got_trace), lines) =
            global_capture::capture_async(run_op(&db, &character_id, &excluded, op)).await;
        let got_lines = got_lines(lines);
        let want_lines = want_lines(op);
        window_lines += want_lines
            .iter()
            .filter(|l| l.contains("Window-first gather"))
            .count();
        let want = &op["result"];
        if !close(&got, want) {
            failures.push(format!("op {i} ({kind}): result\n  v5 {got}\n  v4 {want}"));
        }
        let want_trace = &op["trace"];
        relaxed += want_trace.to_string().matches("floorRelaxed").count();
        via_window += want.to_string().matches("\"via\":\"window\"").count();
        if serde_json::to_string(&got_trace).unwrap() != serde_json::to_string(want_trace).unwrap()
        {
            failures.push(format!(
                "op {i} ({kind}): trace\n  v5 {got_trace}\n  v4 {want_trace}"
            ));
        }
        if got_lines != want_lines {
            failures.push(format!(
                "op {i} ({kind}): lines\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
    }
    // The corpus reaches every arm it claims to.
    assert!(
        window_lines >= 5,
        "Window-first gather lines met: {window_lines}"
    );
    assert!(relaxed >= 3, "floorRelaxed rows met: {relaxed}");
    assert!(via_window >= 5, "via window hits met: {via_window}");
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("OK: window-first recall matched v4 over {} ops.", ops.len());
}
