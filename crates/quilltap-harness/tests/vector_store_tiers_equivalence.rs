//! P4.D288 (C1-3) tier-2 differential, REWRITTEN over ROWS: v4's REAL
//! `CharacterVectorStore` at the `83334b9a5` pin (`lib/embedding/vector-store.ts`
//! after `af48f6cc2` — loading `id, embedding, tier` from `memories`) against
//! `db::vector_store::CharacterVectorStore` over the SAME per-run fixture (v4's
//! own `create` stored each embedding — the quantized blob — plus raw rows: an
//! undecodable TEXT cell, an EMPTY blob, a JSON-array TEXT cell; a 3-d majority
//! with a 2-d minority; a second character whose two lengths TIE).
//!
//! Replays the oracle's op list: each `load` (+ `Read memory index rows`, the
//! undecodable-skip WARN, `Loaded vector index from memory rows` with the
//! dominant `dimensions`; `size`, `dimensions()`, `count_with_dimensions(2|3)`),
//! `search(q, k, isHotVector)` as `search_filtered(q, k, |_, cold| !cold)` and
//! the unfiltered `search` (ids and cold tags exact, scores within 1e-12; a 2-d
//! query scoring only the 2-d entries; a 4-d query → the dimension-mismatch
//! WARN and `[]`), `setTier` as `set_tier` (+ its `Re-stamped vector tiers`
//! DEBUG). Lines are compared from the message on (the target is v5's; v4's
//! ride the root logger).
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

    let ops = oracle["ops"].as_array().unwrap();
    let mut store: Option<CharacterVectorStore> = None;
    let (mut loads, mut searches, mut set_tiers) = (0usize, 0usize, 0usize);
    for (i, op) in ops.iter().enumerate() {
        match op["op"].as_str().unwrap() {
            "load" => {
                let cid = op["characterId"].as_str().unwrap();
                let (loaded, lines) = quilltap_core::test_support::captured_with(|| {
                    CharacterVectorStore::load(writer.connection(), cid)
                });
                assert_eq!(got_lines(lines), want_lines(op), "op {i}: load lines");
                assert_eq!(
                    loaded.size() as u64,
                    op["size"].as_u64().unwrap(),
                    "op {i}: size"
                );
                assert_eq!(
                    loaded.dimensions().map(|d| d as u64),
                    op["dimensions"].as_u64(),
                    "op {i}: the dominant length"
                );
                assert_eq!(
                    loaded.count_with_dimensions(2) as u64,
                    op["count2"].as_u64().unwrap()
                );
                assert_eq!(
                    loaded.count_with_dimensions(3) as u64,
                    op["count3"].as_u64().unwrap()
                );
                store = Some(loaded);
                loads += 1;
            }
            "search" => {
                let s = store.as_ref().expect("a load first");
                let q: Vec<f32> = op["q"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_f64().unwrap() as f32)
                    .collect();
                let k = op["k"].as_u64().unwrap() as usize;
                let (hits, lines) = quilltap_core::test_support::captured_with(|| {
                    if op["hotOnly"].as_bool().unwrap() {
                        s.search_filtered(&q, k, |_, cold| !cold)
                    } else {
                        s.search(&q, k)
                    }
                });
                assert_eq!(got_lines(lines), want_lines(op), "op {i}: lines");
                let want = op["hits"].as_array().unwrap();
                let got: Vec<(&str, bool)> = hits
                    .iter()
                    .map(|h| (h.id.as_str(), h.tier == Tier::Cold))
                    .collect();
                let want_pairs: Vec<(&str, bool)> = want
                    .iter()
                    .map(|h| (h["id"].as_str().unwrap(), h["cold"].as_bool().unwrap()))
                    .collect();
                assert_eq!(got, want_pairs, "op {i} ({}): ids + cold tags", op["label"]);
                for (g, w) in hits.iter().zip(want) {
                    let ws = w["score"].as_f64().unwrap();
                    assert!(
                        (g.score - ws).abs() <= 1e-12,
                        "op {i}: score {} vs {ws}",
                        g.score
                    );
                }
                searches += 1;
            }
            "setTier" => {
                let s = store.as_mut().expect("a load first");
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
                    quilltap_core::test_support::captured_with(|| s.set_tier(&ids, tier));
                assert_eq!(got_lines(lines), want_lines(op), "op {i}: setTier lines");
                set_tiers += 1;
            }
            other => panic!("unknown op {other}"),
        }
    }
    assert_eq!((loads, searches, set_tiers), (2, 7, 3), "the op list moved");
    eprintln!(
        "OK: vector store over rows matched oracle ({} ops).",
        ops.len()
    );
}
