//! Tier-2 differential test: the scheduled danger-classification scan (P4.1d,
//! v4 `lib/background-jobs/scheduled-danger-scan.ts` `runScheduledDangerScan`).
//!
//! Both sides COPY the same seed fixture and run one scan pass; the Rust
//! port's `(usersProcessed, chatsEnqueued)` + the all-users-OFF pre-check +
//! the resulting `background_jobs` dump must match v4 byte-for-byte in the
//! minted-values form (job ids → first-seen tokens in payload order; the
//! minted `createdAt`/`updatedAt`/`scheduledAt` → `<ts>`; everything else —
//! payload bytes, priority −2, maxAttempts, status — diffed exactly). The
//! fixture banks the whole gate matrix: exempt chat type / operator-waved-off
//! / sticky-dangerous / safe-not-grown skips, never-classified + grown-safe
//! hits, the summary vs >50 vs ≤50 enqueue branches, the controlledBy filter
//! plus participant-profile-first-then-fallback resolution, the mode-OFF
//! user, and the no-resolvable-profile chat.
//!
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-danger-scan.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-danger-scan-fixture.ts
//!   QT_FIXTURE_DANGER_SCAN=/tmp/qt-danger-scan.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/danger-scan-tier2.ts > /tmp/oracle-danger-scan.ndjson
//! Run:
//!   QT_ORACLE_DANGER_SCAN=/tmp/oracle-danger-scan.ndjson \
//!   QT_FIXTURE_DANGER_SCAN=/tmp/qt-danger-scan.db \
//!     cargo test -p quilltap-harness --test danger_scan_tier2_equivalence
//!
//! Worktree gotcha: tsx oracles ignore `.claude/` worktree paths — run them
//! from a main-checkout copy (or /tmp) per the standing recipe.

use std::collections::HashMap;

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::Db;
use quilltap_core::services::danger_scan::{any_user_danger_enabled, run_scheduled_danger_scan};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    expected: Expected,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    users_processed: usize,
    chats_enqueued: usize,
}

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/danger-scan.json")
}

fn oracle_line(text: &str, kind: &str) -> Value {
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        if v.get("kind").and_then(Value::as_str) == Some(kind) {
            return v;
        }
    }
    panic!("oracle ndjson missing kind {kind}");
}

/// Minted-values normalization over a `background_jobs` dump (rows already
/// ordered by payload): the minted `id` collapses to a first-seen token, the
/// three minted timestamps to `<ts>`. Everything else stays exact.
fn normalize_jobs(rows: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("row object");
        if let Some(Value::String(raw)) = obj.get("id") {
            let next = format!("ID_{}", id_map.len());
            let token = id_map.entry(raw.clone()).or_insert(next).clone();
            obj.insert("id".into(), Value::String(token));
        }
        for col in ["createdAt", "updatedAt", "scheduledAt"] {
            if obj.get(col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert(col.into(), Value::String("<ts>".into()));
            }
        }
    }
}

#[test]
fn danger_scan_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_DANGER_SCAN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_DANGER_SCAN to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_DANGER_SCAN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_DANGER_SCAN to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    // Copy the fixture (the scan writes).
    let work = std::env::temp_dir().join(format!("qt-danger-scan-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let db = Db::open_main(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // 1) The all-users-OFF pre-check agrees with v4's recomputation.
    let any_enabled = rt
        .block_on(any_user_danger_enabled(&db))
        .expect("pre-check");
    let want_precheck = oracle_line(&oracle_text, "precheck");
    assert_eq!(
        Value::Bool(any_enabled),
        want_precheck["anyEnabled"],
        "pre-check diverged"
    );

    // 2) The sweep result.
    let (users_processed, chats_enqueued) = rt
        .block_on(run_scheduled_danger_scan(&db))
        .expect("run_scheduled_danger_scan");
    let want_result = oracle_line(&oracle_text, "result");
    assert_eq!(
        serde_json::json!({ "usersProcessed": users_processed, "chatsEnqueued": chats_enqueued }),
        serde_json::json!({
            "usersProcessed": want_result["usersProcessed"],
            "chatsEnqueued": want_result["chatsEnqueued"],
        }),
        "result diverged"
    );
    // The spec's banked expectation (guards the corpus itself).
    assert_eq!(users_processed, spec.expected.users_processed);
    assert_eq!(chats_enqueued, spec.expected.chats_enqueued);

    // 3) The background_jobs dump, minted-values form.
    let mut got_dump = db
        .read_main(|conn| dump_table_json_conn(conn, "background_jobs", "payload"))
        .expect("dump background_jobs");
    let got_rows = got_dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("rows");
    normalize_jobs(got_rows);

    let want = oracle_line(&oracle_text, "jobs");
    let mut want_rows: Vec<Value> = want
        .get("rows")
        .and_then(Value::as_array)
        .expect("oracle rows")
        .clone();
    normalize_jobs(&mut want_rows);

    assert_eq!(
        got_rows.len(),
        want_rows.len(),
        "row count diverged: got {} want {}",
        got_rows.len(),
        want_rows.len()
    );
    for (i, (got, want)) in got_rows.iter().zip(want_rows.iter()).enumerate() {
        assert_eq!(got, want, "background_jobs row {i} diverged");
    }

    drop(db);
    let _ = std::fs::remove_file(&work);
    eprintln!(
        "OK: danger scan matched v4 ({} users, {} enqueued, {} job rows).",
        users_processed,
        chats_enqueued,
        got_rows.len()
    );
}
