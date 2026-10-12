//! P4.D265 tier-2 differential: v4's REAL `CharacterVectorStore` tier stamping
//! (`lib/embedding/vector-store.ts`, `d58548051`) against
//! `db::vector_store::CharacterVectorStore` over the SAME per-run fixture (v4's
//! own code built it — memories with cold rows, a cold memory with no vector,
//! five unit vectors).
//!
//! Replays the oracle's op list: `load` (+ its `Loaded vector index` DEBUG —
//! `cold` is the cold-id SET's size), `search(q, k, isHotVector)` as
//! `search_filtered(q, k, |_, cold| !cold)` and the unfiltered `search`
//! (ids exact, scores within 1e-12), `setTier` as `set_tier` (+ its
//! `Re-stamped vector tiers` DEBUG, an absent id, a no-op re-stamp). Lines are
//! compared from the message on (the target is v5's module path; v4's ride the
//! root logger).
//!
//! Regenerate + run (the case builds its own fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-vector-store-tiers.ndjson /tmp/qt-vector-store-tiers.db
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_VECTOR_STORE_TIERS=/tmp/qt-vector-store-tiers.db \
//!     $N/npx tsx $V5W/harness/oracle/cases/vector-store-tiers.ts \
//!     > /tmp/oracle-vector-store-tiers.ndjson
//!   cd $V5W
//!   QT_ORACLE_VECTOR_STORE_TIERS=/tmp/oracle-vector-store-tiers.ndjson \
//!   QT_FIXTURE_VECTOR_STORE_TIERS=/tmp/qt-vector-store-tiers.db \
//!     cargo test -p quilltap-harness --test vector_store_tiers_equivalence -- --nocapture

use quilltap_core::db::vector_store::{CharacterVectorStore, Tier};
use quilltap_core::db::Writer;
use serde_json::Value;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

/// v4's line, as `LEVEL message k=v …` (strings bare, numbers as JSON).
fn want_lines(op: &Value) -> Vec<String> {
    op["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let mut out = format!(
                "{} {}",
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap()
            );
            for (k, v) in l["fields"].as_object().unwrap() {
                let rendered = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                out.push_str(&format!(" {k}={rendered}"));
            }
            out
        })
        .collect()
}

/// v5's captured line minus its target: `LEVEL message k=v …`.
fn got_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|l| {
            let mut parts = l.splitn(3, ' ');
            let level = parts.next().unwrap().to_string();
            let _target = parts.next();
            format!("{level} {}", parts.next().unwrap_or_default())
        })
        .collect()
}

fn unit(v: [f32; 3]) -> Vec<f32> {
    // v4's `unit()` runs in doubles, then stores Float32 — the query is the
    // same Float32 rounding of the same doubles.
    let norm = v
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt();
    v.iter().map(|x| ((*x as f64) / norm) as f32).collect()
}

#[test]
fn vector_store_tiers_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_VECTOR_STORE_TIERS") else {
        eprintln!("SKIP: set QT_ORACLE_VECTOR_STORE_TIERS to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture) = std::env::var("QT_FIXTURE_VECTOR_STORE_TIERS") else {
        eprintln!("SKIP: set QT_FIXTURE_VECTOR_STORE_TIERS to the case's fixture (see header).");
        return;
    };
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("oracle line");
    let scratch = tempfile::Builder::new()
        .prefix("qt-vst-rust-")
        .tempdir()
        .unwrap();
    let work = scratch.path().join("work.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let writer = Writer::open_writable(&work, PEPPER).unwrap_or_else(|e| panic!("open: {e}"));
    let character_id = oracle["characterId"].as_str().unwrap();
    let q = unit([1.0, 0.0, 0.0]);

    let ops = oracle["ops"].as_array().unwrap();
    assert_eq!(ops[0]["op"], "load");
    let (store, lines) = quilltap_core::test_support::captured_with(|| {
        CharacterVectorStore::load(writer.connection(), character_id).expect("load")
    });
    let mut store = store;
    assert_eq!(got_lines(lines), want_lines(&ops[0]), "load: the lines");
    assert_eq!(
        store.size() as u64,
        ops[0]["size"].as_u64().unwrap(),
        "load: size"
    );

    let mut compared = 0usize;
    for (i, op) in ops.iter().enumerate().skip(1) {
        match op["op"].as_str().unwrap() {
            "search" => {
                let k = op["k"].as_u64().unwrap() as usize;
                let (hits, lines) = quilltap_core::test_support::captured_with(|| {
                    if op["hotOnly"].as_bool().unwrap() {
                        store.search_filtered(&q, k, |_, cold| !cold)
                    } else {
                        store.search(&q, k)
                    }
                });
                assert_eq!(got_lines(lines), want_lines(op), "op {i}: lines");
                let want = op["hits"].as_array().unwrap();
                let got_ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
                let want_ids: Vec<&str> = want.iter().map(|h| h["id"].as_str().unwrap()).collect();
                assert_eq!(got_ids, want_ids, "op {i} ({}): the ids", op["label"]);
                for (g, w) in hits.iter().zip(want) {
                    let ws = w["score"].as_f64().unwrap();
                    assert!(
                        (g.score - ws).abs() <= 1e-12,
                        "op {i}: score {} vs {ws}",
                        g.score
                    );
                }
                compared += 1;
            }
            "setTier" => {
                let ids: Vec<String> = op["ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect();
                let tier = match op["tier"].as_str().unwrap() {
                    "hot" => Tier::Hot,
                    _ => Tier::Cold,
                };
                let (_changed, lines) =
                    quilltap_core::test_support::captured_with(|| store.set_tier(&ids, tier));
                assert_eq!(got_lines(lines), want_lines(op), "op {i}: setTier lines");
                compared += 1;
            }
            other => panic!("unknown op {other}"),
        }
    }
    assert!(compared >= 8, "the op list shrank ({compared})");
    eprintln!("OK: vector store tiers matched oracle ({} ops).", ops.len());
}
