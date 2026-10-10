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
//! ## P4.D266 — housekeeping DEMOTES (v4 `d58548051`, pin `01a83539d`)
//!
//! Every former deletion is now a demotion to the cold tier (`demoted` /
//! `demotedIds` / `coldCount` in the result, `tier = 'cold'` in the dump); the
//! merge pass is retired (`mergeSimilar: true` accepted and ignored — H2/H4).
//! Four new characters: H5 (cold rows excluded from the cap + `coldCount`; a
//! `needsHousekeeping` whose all-tier count would trip the 80% check but whose
//! HOT count does not), H6 (CONSOLIDATED digests protected over the cap), H7 (a
//! zero-hot, cold-only character — no early return — swept by the retention
//! pass after a `setRetention` op writes `memoryConsolidation.coldRetentionDays`
//! through v4's REAL `setMemoryConsolidationSettings` / v5's C1 setter: the
//! superseded AUTO row AND the superseded CONSOLIDATED digest go —
//! `RETENTION_DELETES_SUPERSEDED_DIGEST`, faithful to v4 at `01a83539d`; a v4
//! fix flips this row — while MANUAL, unsuperseded and FUTURE-`updatedAt`
//! rows stay; a dry run first), H8 (the only work is retention:
//! `needsHousekeeping` false before the setting, true after). The results are
//! compared BYTE-wise in v4's key order; the fixture's `instance_settings`
//! table comes from v4's own `create-instance-settings-table` migration (M2).
//!
//! NORMALIZATION: sentinel-aware minted-`updatedAt` placeholder on the
//! `memories` / `vector_indices` dumps (chokepoint neighbour scrub / store
//! flush mint), and in the RESULTS the age/inactive month numbers inside detail
//! reasons are placeholdered (`<m> months`) — they derive from each side's own
//! wall clock. Counts, id lists (and their order), actions, and the
//! percent/similarity numbers are compared byte-exact.
//!
//! ## P4.d27 — the merge pass's dimension skip (v4 `7391404e`)
//!
//! v4 now skips a stored vector whose width differs from the store's, quietly and
//! once, instead of handing `search` a query it rejects with a warning on every
//! call. **It is outcome-neutral**: `search` already returns `[]` for a
//! dimension-mismatched query on BOTH sides, so no merge decision moves — the
//! change is log volume and a saved scan. The corpus therefore carries a
//! four-component entry in a three-component store (H4's `a4000005`, whose first
//! three components make it a 95%-cosine near-duplicate of `a4000001`) not to
//! observe the skip but to PIN the neutrality: `deletedIds`/`mergedIds` are
//! identical with and without it, measured. Invert the skip onto CONFORMING
//! entries and the merges stop, which is what makes this family sensitive to the
//! condition even though it cannot see the skip itself.
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
    #[serde(rename = "seedMemories")]
    seed_memories: Vec<Value>,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    kind: String,
    #[serde(rename = "characterId", default)]
    character_id: String,
    #[serde(default)]
    options: Value,
    /// `setRetention` (P4.D266): `memoryConsolidation.coldRetentionDays`.
    #[serde(default)]
    days: Option<i64>,
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
        count_source: None,
    }
}

/// v4's `HousekeepingResult` in ITS key order (`d58548051`: `deleted, demoted,
/// merged, kept, totalBefore, totalAfter, coldCount, capUsed, deletedIds,
/// demotedIds, mergedIds, details`) — the comparison is BYTE-wise, so the order
/// is part of the comparand. A detail's `summary` is omitted when absent (the
/// retention rows carry none).
fn result_to_json(r: &HousekeepingResult) -> Value {
    let details: Vec<Value> = r
        .details
        .iter()
        .map(|d| {
            let mut o = serde_json::Map::new();
            o.insert("memoryId".into(), json!(d.memory_id));
            o.insert("action".into(), json!(d.action));
            o.insert("reason".into(), json!(d.reason));
            if let Some(summary) = &d.summary {
                o.insert("summary".into(), json!(summary));
            }
            Value::Object(o)
        })
        .collect();
    let mut o = serde_json::Map::new();
    o.insert("deleted".into(), json!(r.deleted));
    o.insert("demoted".into(), json!(r.demoted));
    o.insert("merged".into(), json!(r.merged));
    o.insert("kept".into(), json!(r.kept));
    o.insert("totalBefore".into(), json!(r.total_before));
    o.insert("totalAfter".into(), json!(r.total_after));
    o.insert("coldCount".into(), json!(r.cold_count));
    o.insert("capUsed".into(), json!(r.cap_used));
    o.insert("deletedIds".into(), json!(r.deleted_ids));
    o.insert("demotedIds".into(), json!(r.demoted_ids));
    o.insert("mergedIds".into(), json!(r.merged_ids));
    o.insert("details".into(), Value::Array(details));
    Value::Object(o)
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

/// Collapse a non-sentinel `updatedAt` to `<ts>`, in place. The sentinels are
/// the seed timestamp plus every seed's own pinned `updatedAt` (P4.D266: the
/// retention corpus pins a FUTURE one).
fn normalize_dump(dump: &mut Value, sentinels: &[String], label: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));
        let bump = matches!(obj.get("updatedAt"), Some(Value::String(s)) if !sentinels.contains(s));
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

    let work_scratch = tempfile::Builder::new()
        .prefix("qt-mem-housekeeping-rust-")
        .tempdir()
        .expect("tempdir");
    let work = work_scratch.path().join("work.db");
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
            "setRetention" => {
                let days = op.days.expect("setRetention carries days");
                db.write(move |w| {
                    quilltap_core::db::instance_settings::set_memory_consolidation_settings(
                        w.main().connection(),
                        &json!({ "coldRetentionDays": days }),
                    )
                })
                .await
                .unwrap_or_else(|e| panic!("op[{i}] setRetention: {e:?}"));
                got_results.push(Value::Null);
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

    // Results: months placeholdered on both sides, everything else exact.
    let mut got_results = Value::Array(got_results);
    let mut want_results =
        oracle_line(&oracle_text, |v| v.get("results").is_some(), "results")["results"].clone();
    normalize_results(&mut got_results);
    normalize_results(&mut want_results);
    // BYTE-wise (a `preserve_order` map's `==` is order-blind).
    let (got_s, want_s) = (got_results.to_string(), want_results.to_string());
    assert_eq!(
        got_s, want_s,
        "per-op results diverged\n  rust:   {got_s}\n  oracle: {want_s}"
    );
    // Silent-stale guard: the rebuilt corpus really demotes and really deletes
    // through the retention sweep.
    assert!(
        want_s.contains("\"demoted\":"),
        "no `demoted` key — a stale oracle?"
    );
    assert!(
        want_s.contains("retention window"),
        "no retention delete in the oracle — the setting never landed?"
    );

    let mut sentinels: Vec<String> = vec![spec.seed_timestamp.clone()];
    for seed in &spec.seed_memories {
        if let Some(u) = seed.get("updatedAt").and_then(Value::as_str) {
            sentinels.push(u.to_string());
        }
    }

    // Dumps: sentinel-aware minted-updatedAt placeholder.
    for (i, t) in TABLES.iter().enumerate() {
        let mut want = oracle_line(
            &oracle_text,
            |v| v.get("table").and_then(Value::as_str) == Some(t),
            t,
        );
        normalize_dump(&mut got_dumps[i], &sentinels, &format!("rust {t}"));
        normalize_dump(&mut want, &sentinels, &format!("oracle {t}"));
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

    // Sanity (fresh-date corpus). P4.D266 (`d58548051`): housekeeping DEMOTES,
    // so the twenty pre-round seeds all survive (H1's / H3's former deletions
    // are now `tier = 'cold'` rows, and the retired merge pass folds nothing).
    // The fifteen new seeds lose exactly the two expired superseded cold rows
    // of H7 (AUTO + the CONSOLIDATED digest — RETENTION_DELETES_SUPERSEDED_DIGEST)
    // and H8's one: 20 + 15 − 3 = 32 memories; 12 + 3 − 2 = 13 entries (H7's
    // two deleted rows carried vectors).
    //
    // ⚠ These are HAND COUNTS after a passing row diff, so they check the CORPUS,
    // not the port (`harness-corpus-shape-constants-rot`).
    assert_eq!(
        got_dumps[0]["rows"].as_array().unwrap().len(),
        32,
        "memories survivors"
    );
    assert_eq!(
        got_dumps[2]["rows"].as_array().unwrap().len(),
        13,
        "vector_entries survivors"
    );

    eprintln!("OK: memory-housekeeping tier-2 matched oracle (results + 3 tables).");
}
