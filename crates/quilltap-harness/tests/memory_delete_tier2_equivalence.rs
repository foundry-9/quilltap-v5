//! Tier-2 differential test: memory deletion, re-pointed by P4.D288 (C1-7,
//! the `83334b9a5` pin) onto the ONE retire path. v4 `af48f6cc2` deleted the
//! gate's chokepoints (`deleteMemoryWithUnlink` /
//! `deleteMemoriesWithUnlinkBatch`); each committed op
//! (`memory-delete-tier2.json`, §R.7 unchanged) now runs its ids through
//! `retireMemories(ids, 'item-delete')` — v5's
//! `services::memory_retire::retire_memories` on the writer.
//!
//! Both sides run the SAME sequence on a fresh copy of the pre-seeded graph
//! fixture (nine memories cross-linked through `relatedMemoryIds` across two
//! characters), then the `memories` table is dumped canonically and the
//! post-op state is asserted identical — the instance-wide neighbour scrub
//! (an unknown id still scrubbed), the idempotent missing-row branch, the
//! empty-batch branch, the by-holder delete grouping — and the retirement's
//! lines are compared in op order (`durationMs` normalized; an object field
//! rides as `<key>Json`, the file layer's convention).
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

    let work_scratch = tempfile::Builder::new()
        .prefix("qt-mem-delete-rust-")
        .tempdir()
        .expect("tempdir");
    let work = work_scratch.path().join("work.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    let ((), lines) = quilltap_core::test_support::captured_with(|| {
        for op in &spec.ops {
            let ids = match op {
                Op::DeleteMemoryWithUnlink { id } => vec![id.clone()],
                Op::DeleteMemoriesWithUnlinkBatch { ids } => ids.clone(),
            };
            quilltap_core::services::memory_retire::retire_memories(
                writer.connection(),
                &ids,
                quilltap_core::services::memory_retire::RetireOptions::new(
                    quilltap_core::services::memory_retire::RetireReason::ItemDelete,
                ),
            )
            .expect("retireMemories");
        }
    });

    // The retirement's lines, in op order — compared from the message on (the
    // target is v5's module path; v4's ride the root / child loggers).
    // `durationMs` is real wall-clock on both sides — normalized.
    const MESSAGES: &[&str] = &[
        "Scrubbed deleted ids from related links",
        "Nothing to retire",
        "Found digests naming doomed members",
        "Digest retires with its last member",
        "Scrubbed digest links to retired memories",
        "Entities deleted",
        "Evicted retired memories from the index",
        "Retirement rewrote an unusually large neighbour set",
        "Retired memories",
    ];
    let normalize_line = |l: &str| -> String {
        match l.find(" durationMs=") {
            Some(at) => {
                let rest = &l[at + 1..];
                let tail = rest.find(' ').map(|sp| &rest[sp..]).unwrap_or("");
                format!("{} durationMs=<n>{tail}", &l[..at])
            }
            None => l.to_string(),
        }
    };
    let got_lines: Vec<String> = lines
        .iter()
        .filter_map(|l| {
            let at = MESSAGES
                .iter()
                .filter_map(|m| l.find(&format!(" {m}")))
                .min()?;
            let level = l.split_whitespace().next().unwrap();
            Some(format!("{level} {}", normalize_line(&l[at + 1..])))
        })
        .collect();
    let want_lines: Vec<String> = oracle["lines"]
        .as_array()
        .expect("the oracle carries no `lines` — regenerate it")
        .iter()
        .map(|l| {
            let mut out = format!(
                "{} {}",
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap()
            );
            for (k, v) in l["fields"].as_object().unwrap() {
                match v {
                    Value::String(s) => out.push_str(&format!(" {k}={s}")),
                    Value::Object(_) | Value::Array(_) => out.push_str(&format!(" {k}Json={v}")),
                    other => out.push_str(&format!(" {k}={other}")),
                }
            }
            normalize_line(&out)
        })
        .collect();
    assert_eq!(got_lines, want_lines, "the retirement's lines");
    assert!(
        want_lines
            .iter()
            .any(|l| l.contains("Retired memories module=memory:retire reason=item-delete ")),
        "the closing line is compared: {want_lines:#?}"
    );

    let mut got = writer
        .dump_table_json("memories", "id")
        .expect("dump memories");

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
