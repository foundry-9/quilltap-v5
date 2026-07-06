//! Tier-2 differential test: the stale-chat asset collapse (W4.8, v4
//! `lib/background-jobs/maintenance/collapse-stale-chat-assets.ts`).
//!
//! Both sides COPY the same two-DB seed fixture and run
//! `collapse_stale_chat_assets(now)` with the pinned `now`; the Rust port's
//! `StaleChatCollapseSummary` + the surviving `files` rows + the untouched album
//! `doc_mount_file_links` row must match v4's REAL `collapseStaleChatAssets`
//! byte-for-byte. The fixture exercises every skip-reason (current / current-sha /
//! album-or-vault-link / character-reference) plus the fresh-chat guard, so ONE
//! unprotected file is deleted and five survive. ZERO normalization — the sweep
//! mints nothing (the `files.delete` touches no timestamp column; the FsSeam
//! storage-delete is a no-op on both sides).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_MAIN=/tmp/qt-maint-main.db QT_FIXTURE_MOUNT=/tmp/qt-maint-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-maintenance-sweep-fixture.ts
//!   QT_FIXTURE_MAINT_MAIN=/tmp/qt-maint-main.db QT_FIXTURE_MAINT_MOUNT=/tmp/qt-maint-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/maintenance-sweep-tier2.ts > /tmp/oracle-maintenance-sweep.ndjson
//! Run:
//!   QT_ORACLE_MAINT=/tmp/oracle-maintenance-sweep.ndjson \
//!   QT_FIXTURE_MAINT_MAIN=/tmp/qt-maint-main.db \
//!   QT_FIXTURE_MAINT_MOUNT=/tmp/qt-maint-mount.db \
//!     cargo test -p quilltap-harness --test maintenance_sweep_tier2_equivalence

use quilltap_core::clock::iso_to_ms;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::maintenance::collapse_stale_chat_assets;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    now: String,
}

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/maintenance-sweep.json")
}

/// Parse the oracle NDJSON into a map keyed by `kind`.
fn parse_oracle(text: &str) -> std::collections::HashMap<String, Value> {
    let mut out = std::collections::HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        let kind = v.get("kind").and_then(Value::as_str).unwrap().to_string();
        out.insert(kind, v);
    }
    out
}

#[test]
fn maintenance_sweep_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_MAINT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MAINT to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_MAINT_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MAINT_MAIN to the main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_MAINT_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MAINT_MOUNT to the mount fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle = parse_oracle(
        &std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}")),
    );

    // Copy both fixtures to a private working pair (both sides mutate the copy).
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-maint-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-maint-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let db = Db::open(
        DbPaths {
            main: main_work.clone(),
            mount_index: Some(mount_work.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open db: {e}"));

    let now_ms = iso_to_ms(&spec.now).expect("parse now");

    // Run the sweep on a small tokio runtime (the port is async).
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let summary = rt
        .block_on(collapse_stale_chat_assets(&db, now_ms))
        .expect("collapse_stale_chat_assets");

    // 1) Summary equality (v4 camelCase keys).
    let got_summary = json!({
        "chatsScanned": summary.chats_scanned,
        "staleChats": summary.stale_chats,
        "chatsCollapsed": summary.chats_collapsed,
        "filesDeleted": summary.files_deleted,
        "bytesReleasedEstimate": summary.bytes_released_estimate,
    });
    let want_summary = oracle
        .get("summary")
        .and_then(|v| v.get("summary"))
        .expect("oracle summary");
    assert_eq!(&got_summary, want_summary, "summary diverged");

    // 2) Surviving `files` rows (main), ordered by id.
    let got_files = db
        .read_main(|c| {
            let mut stmt =
                c.prepare("SELECT id, source, category, linkedTo FROM files ORDER BY id ASC")?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "source": r.get::<_, String>(1)?,
                        "category": r.get::<_, String>(2)?,
                        "linkedTo": r.get::<_, String>(3)?,
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("read files");
    let want_files = oracle
        .get("files")
        .and_then(|v| v.get("rows"))
        .and_then(Value::as_array)
        .expect("oracle files rows");
    assert_eq!(
        &Value::Array(got_files),
        &Value::Array(want_files.clone()),
        "surviving files diverged"
    );

    // 3) The album `doc_mount_file_links` rows (mount-index) — untouched by the
    // sweep (it writes only to `files`).
    let got_mount = db
        .read_mount_index(|c| {
            let mut stmt = c.prepare(
                "SELECT id, relativePath, mountPointId FROM doc_mount_file_links ORDER BY id ASC",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "relativePath": r.get::<_, String>(1)?,
                        "mountPointId": r.get::<_, String>(2)?,
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("read mount links");
    let want_mount = oracle
        .get("mount")
        .and_then(|v| v.get("rows"))
        .and_then(Value::as_array)
        .expect("oracle mount rows");
    assert_eq!(
        &Value::Array(got_mount),
        &Value::Array(want_mount.clone()),
        "mount links diverged"
    );

    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    eprintln!(
        "OK: collapse_stale_chat_assets matched v4 (deleted {}, {} files survive).",
        summary.files_deleted,
        want_files.len()
    );
}
