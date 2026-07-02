//! Tier-2 differential test: the **housekeeping sweep** (v4
//! `lib/memory/housekeeping.ts` `runHousekeeping` / `needsHousekeeping`), ported
//! as `quilltap_core::services::housekeeping`.
//!
//! Both sides run the SAME op sequence (`memory-housekeeping-tier2.json`) on a
//! fresh copy of the pre-seeded fixture (fifteen memories across three
//! characters), then BOTH the per-op RESULTS and the final `memories` +
//! `vector_indices` + `vector_entries` state are diffed. One character banks the
//! retention pass (old-low deleted with and without a vector, recent-low /
//! recently-accessed / MANUAL / high-importance kept, and a reinforced+linked
//! memory whose blended protection score overrides the retention rule); one
//! banks the stored-vector similarity merge (a 95%-cosine near-duplicate folded
//! into the more important row, an orthogonal row untouched); one banks cap
//! enforcement (cap 3 over 5 → the two lowest-effective-weight rows go from the
//! tail) plus the dry-run (op 1 previews the same cap sweep and writes nothing —
//! proven by op 4's totalBefore). The `needsHousekeeping` ops bank both the
//! preview-driven false and the 80%-watermark true.
//!
//! NORMALIZATION: sentinel-aware minted-`updatedAt` placeholder on the
//! `memories` / `vector_indices` dumps (chokepoint neighbour scrub / store
//! flush mint), and in the RESULTS the age/inactive month numbers inside detail
//! reasons are placeholdered (`<m> months`) — they derive from each side's own
//! wall clock. Counts, id lists (and their order), actions, and the
//! percent/similarity numbers are compared byte-exact.
//!
//! ⏳ CORPUS FRESHNESS: decisions depend on wall-clock age; the corpus outcomes
//! hold while the spec's "recent" dates (2026-06-xx) are under the 6-month
//! windows (until ~2026-12). Both sides stay in agreement after that (the diff
//! is oracle-vs-Rust, not spec-pinned), but the sanity row-counts below assume
//! fresh dates — refresh the spec's recent dates when regenerating later.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   PATH=$N:$PATH QT_FIXTURE_OUT=/tmp/qt-mem-housekeeping-fixture.db \
//!     npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memory-housekeeping-fixture.ts
//!   PATH=$N:$PATH QT_FIXTURE_MEMHOUSEKEEPING=/tmp/qt-mem-housekeeping-fixture.db \
//!     npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-housekeeping-tier2.ts > /tmp/oracle-mem-housekeeping.ndjson
//! Run:
//!   QT_ORACLE_MEMHOUSEKEEPING=/tmp/oracle-mem-housekeeping.ndjson \
//!   QT_FIXTURE_MEMHOUSEKEEPING=/tmp/qt-mem-housekeeping-fixture.db \
//!     cargo test -p quilltap-harness --test memory_housekeeping_tier2_equivalence

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::Db;
use quilltap_core::services::housekeeping::{
    needs_housekeeping, run_housekeeping, HousekeepingOptions, HousekeepingResult,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "seedTimestamp")]
    seed_timestamp: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    kind: String,
    #[serde(rename = "characterId")]
    character_id: String,
    options: Value,
}

fn options_from_json(v: &Value) -> HousekeepingOptions {
    HousekeepingOptions {
        max_memories: v
            .get("maxMemories")
            .and_then(Value::as_u64)
            .map(|n| n as usize),
        max_age_months: v.get("maxAgeMonths").and_then(Value::as_f64),
        max_inactive_months: v.get("maxInactiveMonths").and_then(Value::as_f64),
        min_importance: v.get("minImportance").and_then(Value::as_f64),
        merge_similar: v.get("mergeSimilar").and_then(Value::as_bool),
        merge_threshold: v.get("mergeThreshold").and_then(Value::as_f64),
        dry_run: v.get("dryRun").and_then(Value::as_bool),
    }
}

fn result_to_json(r: &HousekeepingResult) -> Value {
    json!({
        "deleted": r.deleted,
        "merged": r.merged,
        "kept": r.kept,
        "totalBefore": r.total_before,
        "totalAfter": r.total_after,
        "capUsed": r.cap_used,
        "deletedIds": r.deleted_ids,
        "mergedIds": r.merged_ids,
        "details": r.details.iter().map(|d| json!({
            "memoryId": d.memory_id,
            "action": d.action,
            "reason": d.reason,
            "summary": d.summary,
        })).collect::<Vec<_>>(),
    })
}

/// Placeholder every "<number> months" in a reason string — the month values
/// derive from each side's own wall clock (v4 `toFixed(1)`).
fn normalize_months(s: &str) -> String {
    let parts: Vec<&str> = s.split(" months").collect();
    if parts.len() == 1 {
        return s.to_string();
    }
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i < parts.len() - 1 {
            out.push_str(part.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.'));
            out.push_str("<m> months");
        } else {
            out.push_str(part);
        }
    }
    out
}

/// Apply the months placeholder to every detail reason in a results array.
fn normalize_results(results: &mut Value) {
    let Some(arr) = results.as_array_mut() else {
        return;
    };
    for r in arr.iter_mut() {
        let Some(details) = r.get_mut("details").and_then(Value::as_array_mut) else {
            continue;
        };
        for d in details.iter_mut() {
            if let Some(Value::String(reason)) = d.get("reason") {
                let n = normalize_months(reason);
                d.as_object_mut()
                    .unwrap()
                    .insert("reason".to_string(), Value::String(n));
            }
        }
    }
}

/// Collapse a non-sentinel `updatedAt` to `<ts>`, in place (sentinel = seed value).
fn normalize_dump(dump: &mut Value, sentinel: &str, label: &str) {
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

/// Pick an NDJSON line by predicate from the multi-line oracle.
fn oracle_line(oracle_text: &str, pred: impl Fn(&Value) -> bool, what: &str) -> Value {
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle ndjson line");
        if pred(&v) {
            return v;
        }
    }
    panic!("oracle ndjson missing {what}");
}

const TABLES: &[&str] = &["memories", "vector_indices", "vector_entries"];

#[tokio::test]
async fn memory_housekeeping_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_MEMHOUSEKEEPING") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MEMHOUSEKEEPING to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_MEMHOUSEKEEPING") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MEMHOUSEKEEPING to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/memory-housekeeping-tier2.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let work = std::env::temp_dir().join(format!(
        "qt-mem-housekeeping-rust-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let db = Db::open_main(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    let mut got_results: Vec<Value> = Vec::new();
    for (i, op) in spec.ops.iter().enumerate() {
        let options = options_from_json(&op.options);
        match op.kind.as_str() {
            "runHousekeeping" => {
                let r = run_housekeeping(&db, &op.character_id, &options)
                    .await
                    .unwrap_or_else(|e| panic!("op[{i}] runHousekeeping: {e:?}"));
                got_results.push(result_to_json(&r));
            }
            "needsHousekeeping" => {
                let b = needs_housekeeping(&db, &op.character_id, &options)
                    .await
                    .unwrap_or_else(|e| panic!("op[{i}] needsHousekeeping: {e:?}"));
                got_results.push(Value::Bool(b));
            }
            other => panic!("unknown op kind {other}"),
        }
    }

    let mut got_dumps: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            db.read_main(|conn| dump_table_json_conn(conn, t, "id"))
                .unwrap_or_else(|e| panic!("dump {t}: {e:?}"))
        })
        .collect();
    drop(db);
    let _ = std::fs::remove_file(&work);

    // Results: months placeholdered on both sides, everything else exact.
    let mut got_results = Value::Array(got_results);
    let mut want_results =
        oracle_line(&oracle_text, |v| v.get("results").is_some(), "results")["results"].clone();
    normalize_results(&mut got_results);
    normalize_results(&mut want_results);
    assert_eq!(
        got_results, want_results,
        "per-op results diverged\n  rust:   {got_results}\n  oracle: {want_results}"
    );

    // Dumps: sentinel-aware minted-updatedAt placeholder.
    for (i, t) in TABLES.iter().enumerate() {
        let mut want = oracle_line(
            &oracle_text,
            |v| v.get("table").and_then(Value::as_str) == Some(t),
            t,
        );
        normalize_dump(
            &mut got_dumps[i],
            &spec.seed_timestamp,
            &format!("rust {t}"),
        );
        normalize_dump(&mut want, &spec.seed_timestamp, &format!("oracle {t}"));
        assert_eq!(
            got_dumps[i]["columns"], want["columns"],
            "{t} column set / order"
        );
        assert_eq!(
            got_dumps[i]["rows"], want["rows"],
            "{t} row state diverged\n  rust:   {}\n  oracle: {}",
            got_dumps[i]["rows"], want["rows"]
        );
    }

    // Sanity (fresh-date corpus): H1 keeps 5 of 7, H2 keeps 2 of 3, H3 keeps 3
    // of 5 → 10 memories; entries: h1-old-high + h2-keep + h2-other + a3000001.
    assert_eq!(
        got_dumps[0]["rows"].as_array().unwrap().len(),
        10,
        "memories survivors"
    );
    assert_eq!(
        got_dumps[2]["rows"].as_array().unwrap().len(),
        4,
        "vector_entries survivors"
    );

    eprintln!("OK: memory-housekeeping tier-2 matched oracle (results + 3 tables).");
}
