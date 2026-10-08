//! P4.D258 tier-1 differential — `quilltap_core::services::backup::restore::
//! rows::decode_index_keyed_embedding` vs v4's REAL `decodeIndexKeyedEmbedding`
//! (`lib/backup/restore/index-keyed-embedding.ts`, new at `039f7017c` — v4 bug
//! 181, this port's own filing).
//!
//! A pre-fix full backup carries each memory embedding as
//! `JSON.stringify(Float32Array)`'s index-keyed object; the restore decodes
//! exactly that shape (canonical decimal keys `"0"…"n-1"`, no gaps, every value
//! a finite number) and leaves EVERY other value unchanged for `MemorySchema`.
//! Each row's input is placed under `embedding` in a memory object and run
//! through v5's decoder: v5 must report a decode exactly when v4 returned a
//! different value (`decoded !== input`), and leave the key byte-for-byte as v4
//! returned it — the decoded array, or the untouched input (an absent key stays
//! absent). Every object input is replayed a second time with its keys
//! REVERSED (a JSON text may carry them in any order; v4's verdict cannot
//! depend on it), so key order is covered though the oracle's own JSON emits
//! JS's ascending integer-key order.
//!
//! Red-first (P4.D258's lane record): on unported `main` the `jest_empty_object`
//! row alone was red — v5 decoded `{}` to `[]` (the 2026-10-07 ruled
//! divergence, overtaken by v4's fix: `{}` is returned unchanged and the memory
//! refused, P4.D258 R-A).
//!
//! `gap` rows are inputs that cannot cross JSON (`NaN`, a live `Float32Array`,
//! a null-prototype object) — a v5 archive is parsed JSON, so none can reach
//! the Rust decoder. Their v4 verdict (unchanged) is asserted, and their count,
//! so a corpus change is seen.
//!
//! Generate the oracle output (from the v4 checkout, or the pinned worktree
//! while v4 HEAD is past the baseline):
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/index-keyed-embedding.ts \
//!     > /tmp/oracle-index-keyed-embedding.ndjson
//! Run:
//!   QT_ORACLE_INDEX_KEYED_EMBEDDING=/tmp/oracle-index-keyed-embedding.ndjson \
//!     cargo test -p quilltap-harness --test index_keyed_embedding_equivalence

use quilltap_core::services::backup::restore::rows::decode_index_keyed_embedding;
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Deserialize)]
struct Row {
    kind: String,
    id: String,
    #[serde(default)]
    input: Option<Value>,
    #[serde(default)]
    absent: bool,
    changed: bool,
    #[serde(default)]
    out: Option<Value>,
}

/// One replay: the decoder over `{"embedding": input}` (or `{}` when absent),
/// a sibling key on each side so the decode cannot disturb the rest of the row.
fn replay(input: Option<&Value>) -> (bool, Map<String, Value>) {
    let mut m = Map::new();
    m.insert("content".into(), Value::String("kept".into()));
    if let Some(v) = input {
        m.insert("embedding".into(), v.clone());
    }
    m.insert("source".into(), Value::String("kept".into()));
    let changed = decode_index_keyed_embedding(&mut m);
    (changed, m)
}

fn reversed(v: &Value) -> Option<Value> {
    let o = v.as_object()?;
    let mut r = Map::new();
    for (k, x) in o.iter().rev() {
        r.insert(k.clone(), x.clone());
    }
    Some(Value::Object(r))
}

#[test]
fn decode_index_keyed_embedding_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_INDEX_KEYED_EMBEDDING") else {
        eprintln!("SKIP: QT_ORACLE_INDEX_KEYED_EMBEDDING unset");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    // The case emits 35 rows (32 decode + 3 gap); a truncated regen must not
    // pass vacuously.
    assert_eq!(rows.len(), 35, "oracle row count");

    let mut failures = Vec::new();
    let (mut decoded, mut unchanged, mut gaps) = (0, 0, 0);
    for row in &rows {
        match row.kind.as_str() {
            "gap" => {
                gaps += 1;
                if row.changed {
                    failures.push(format!("{}: v4 decoded a non-JSON input", row.id));
                }
                continue;
            }
            "decode" => {}
            k => panic!("{}: unknown kind {k}", row.id),
        }
        let input = if row.absent {
            None
        } else {
            Some(row.input.clone().unwrap_or(Value::Null))
        };
        let want = if row.changed {
            decoded += 1;
            row.out.clone()
        } else {
            unchanged += 1;
            input.clone()
        };
        let mut attempts = vec![("as emitted", input.clone())];
        if let Some(r) = input.as_ref().and_then(reversed) {
            attempts.push(("keys reversed", Some(r)));
        }
        for (how, attempt) in attempts {
            let (changed, m) = replay(attempt.as_ref());
            if changed != row.changed {
                failures.push(format!(
                    "{} ({how}): v5 decoded={changed}, v4 changed={}",
                    row.id, row.changed
                ));
                continue;
            }
            // An unchanged input stays exactly as it was handed in (its own
            // key order included); a decode replaces it with v4's array.
            let expect = if row.changed {
                want.clone()
            } else {
                attempt.clone()
            };
            if m.get("embedding") != expect.as_ref() {
                failures.push(format!(
                    "{} ({how}): v5 left {:?}, v4 {:?}",
                    row.id,
                    m.get("embedding"),
                    expect
                ));
            }
            let keys: Vec<&str> = m.keys().map(String::as_str).collect();
            let want_keys: &[&str] = if attempt.is_some() {
                &["content", "embedding", "source"]
            } else {
                &["content", "source"]
            };
            if keys != want_keys {
                failures.push(format!(
                    "{} ({how}): the row's keys moved: {keys:?}",
                    row.id
                ));
            }
        }
    }
    assert_eq!((decoded, unchanged, gaps), (9, 23, 3), "row kinds");
    assert!(
        failures.is_empty(),
        "{} decoder difference(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
