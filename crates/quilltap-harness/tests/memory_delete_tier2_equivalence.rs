//! Tier-2 differential test: the memory deletion chokepoint (v4
//! `lib/memory/memory-gate.ts` `deleteMemoryWithUnlink` /
//! `deleteMemoriesWithUnlinkBatch`), ported as
//! `MemoriesRepository::delete_with_unlink` / `delete_many_with_unlink`.
//!
//! Both sides run the SAME delete sequence (`memory-delete-tier2.json`) on a fresh
//! copy of the pre-seeded graph fixture (nine memories cross-linked through
//! `relatedMemoryIds` across two characters), then the `memories` table is dumped
//! canonically and the post-op state is asserted identical. This proves the
//! neighbour-unlink scan, the character-scoped rewrites, the LIKE pre-filter, the
//! idempotent missing-row branch, the empty-batch branch, and the by-character
//! `bulkDelete` grouping.
//!
//! NORMALIZATION: sentinel-aware minted-timestamp placeholder. The seed pins every
//! id + createdAt + updatedAt to the seed sentinel; only a neighbour that gets
//! unlinked has its `updatedAt` minted (by `updateForCharacter`), so a non-sentinel
//! `updatedAt` is collapsed to `<ts>` on BOTH dumps — a row left at the sentinel
//! proves it was NOT touched (a stray bump would diverge).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-mem-delete-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memory-delete-fixture.ts
//!   QT_FIXTURE_MEMDEL=/tmp/qt-mem-delete-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-delete-tier2.ts > /tmp/oracle-mem-delete.ndjson
//! Run:
//!   QT_ORACLE_MEMDEL=/tmp/oracle-mem-delete.ndjson \
//!   QT_FIXTURE_MEMDEL=/tmp/qt-mem-delete-fixture.db \
//!     cargo test -p quilltap-harness --test memory_delete_tier2_equivalence

use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "seedTimestamp")]
    seed_timestamp: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "deleteMemoryWithUnlink")]
    DeleteMemoryWithUnlink { id: String },
    #[serde(rename = "deleteMemoriesWithUnlinkBatch")]
    DeleteMemoriesWithUnlinkBatch { ids: Vec<String> },
}

/// Collapse a non-sentinel `updatedAt` to `<ts>`, in place (sentinel = seed value).
fn normalize(dump: &mut Value, sentinel: &str, label: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));
        let bump = matches!(obj.get("updatedAt"), Some(Value::String(s)) if s != sentinel);
        if bump {
            obj.insert("updatedAt".to_string(), Value::String("<ts>".to_string()));
        }
    }
}

#[test]
fn memory_delete_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_MEMDEL") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MEMDEL to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_MEMDEL") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MEMDEL to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/memory-delete-tier2.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let mut oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    let work = std::env::temp_dir().join(format!("qt-mem-delete-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.memories();
        for op in &spec.ops {
            match op {
                Op::DeleteMemoryWithUnlink { id } => {
                    repo.delete_with_unlink(id).expect("deleteMemoryWithUnlink");
                }
                Op::DeleteMemoriesWithUnlinkBatch { ids } => {
                    repo.delete_many_with_unlink(ids)
                        .expect("deleteMemoriesWithUnlinkBatch");
                }
            }
        }
    }

    let mut got = writer
        .dump_table_json("memories", "id")
        .expect("dump memories");

    let _ = std::fs::remove_file(&work);

    normalize(&mut got, &spec.seed_timestamp, "rust");
    normalize(&mut oracle, &spec.seed_timestamp, "oracle");

    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert_eq!(n, 6, "expected six surviving rows (3 of 9 deleted)");
    eprintln!("OK: memory-delete tier-2 matched oracle ({n} rows).");
}
