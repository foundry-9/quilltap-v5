//! Tier-2 differential test: the whole daily maintenance pass (P4.1d, v4
//! `runScheduledMaintenance`) — proving the two newly-ported repo ops
//! (`sweep_orphaned_files`, `cleanup_closed_sessions`) inside the real
//! four-sweep orchestration + the `lastMaintenanceSweepAt` stamp.
//!
//! Both sides COPY the same two-DB fixture, create the same transcript files
//! (the default-path one in their own logs dir + the pinned explicit-path
//! one), and run one maintenance pass with their own wall clock (the
//! fixture's rows are keyed in DAYS relative to build time, so minutes of
//! skew cannot flip a window). Diffed exactly: the summary counts (jobs
//! completed/dead, assets zeros, orphans swept, terminal rows/transcripts,
//! failures EMPTY), the surviving `background_jobs` (id+status) /
//! `terminal_sessions` / `doc_mount_files` rows, the transcript-file
//! outcomes, and the stamp's presence (its VALUE is wall-clock — presence +
//! parseability only).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_MAIN=/tmp/qt-maint-ops-main.db QT_FIXTURE_MOUNT=/tmp/qt-maint-ops-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-maintenance-ops-fixture.ts
//!   QT_FIXTURE_MAINT_OPS_MAIN=/tmp/qt-maint-ops-main.db \
//!   QT_FIXTURE_MAINT_OPS_MOUNT=/tmp/qt-maint-ops-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/maintenance-ops-tier2.ts > /tmp/oracle-maintenance-ops.ndjson
//! Run:
//!   QT_ORACLE_MAINT_OPS=/tmp/oracle-maintenance-ops.ndjson \
//!   QT_FIXTURE_MAINT_OPS_MAIN=/tmp/qt-maint-ops-main.db \
//!   QT_FIXTURE_MAINT_OPS_MOUNT=/tmp/qt-maint-ops-mount.db \
//!     cargo test -p quilltap-harness --test maintenance_ops_tier2_equivalence

use quilltap_core::clock::now_unix_ms;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::scheduled_maintenance::{run_scheduled_maintenance, TranscriptStore};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    explicit_transcript_path: String,
}

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/maintenance-ops.json")
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

/// The host's fs transcript store, mirrored locally (the harness does not
/// depend on quilltap-host): unlink the explicit path, else the default
/// `<dir>/<id>.log`; only a real removal counts.
struct FsStore {
    dir: std::path::PathBuf,
}
impl TranscriptStore for FsStore {
    fn unlink_transcript(&self, session_id: &str, transcript_path: Option<&str>) -> bool {
        let path = transcript_path
            .filter(|p| !p.is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| self.dir.join(format!("{session_id}.log")));
        std::fs::remove_file(&path).is_ok()
    }
}

#[test]
fn maintenance_ops_match_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_MAINT_OPS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MAINT_OPS to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_MAINT_OPS_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MAINT_OPS_MAIN (see header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_MAINT_OPS_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MAINT_OPS_MOUNT (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    // Private working copies + this side's transcript files.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-maint-ops-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-maint-ops-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let transcripts_dir = std::env::temp_dir().join(format!("qt-maint-ops-transcripts-{pid}"));
    let _ = std::fs::remove_dir_all(&transcripts_dir);
    std::fs::create_dir_all(&transcripts_dir).unwrap();
    let default_transcript = transcripts_dir.join("a0000000-0000-4000-8000-0000000000d1.log");
    std::fs::write(&default_transcript, "old default-path transcript\n").unwrap();
    let explicit = std::path::PathBuf::from(&spec.explicit_transcript_path);
    let _ = std::fs::remove_file(&explicit);
    std::fs::write(&explicit, "old explicit-path transcript\n").unwrap();

    let db = Db::open(
        DbPaths {
            main: main_work.clone(),
            mount_index: Some(mount_work.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open db: {e}"));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let store = FsStore {
        dir: transcripts_dir.clone(),
    };
    let summary = rt.block_on(run_scheduled_maintenance(&db, now_unix_ms(), &store));

    // 1) Summary equality (v4 camelCase shape).
    let got_summary = json!({
        "jobs": { "completed": summary.jobs_completed, "dead": summary.jobs_dead },
        "assets": {
            "staleChats": summary.assets_stale_chats,
            "chatsCollapsed": summary.assets_chats_collapsed,
            "filesDeleted": summary.assets_files_deleted,
        },
        "caches": {
            "staleChats": summary.caches_stale_chats,
            "chatsCollapsed": summary.caches_chats_collapsed,
            "chatRowsCleared": summary.caches_chat_rows_cleared,
            "messageRowsCleared": summary.caches_message_rows_cleared,
            "chunkEmbeddingsCleared": summary.caches_chunk_embeddings_cleared,
        },
        // `parentless_store_rows_reaped` is deliberately absent: it is P4.31's
        // v5-only sweep (step 4b), which v4's summary has no key for. It must be
        // a no-op on THIS fixture, or the row diffs below would compare v5's
        // reaped state against v4's un-reaped one — asserted just below rather
        // than assumed.
        "orphanedFilesSwept": summary.orphaned_files_swept,
        "terminals": { "rows": summary.terminal_rows, "transcripts": summary.terminal_transcripts },
        "failures": summary.failures,
    });
    assert_eq!(
        summary.parentless_store_rows_reaped, 0,
        "the maintenance fixture grew rows whose mount point is gone; P4.31's v5-only \
         sweep now reaps them here, so this family no longer compares like for like. \
         Either restore the fixture or move the coverage to store_delete_equivalence."
    );
    let want_summary = oracle_line(&oracle_text, "summary");
    assert_eq!(
        &got_summary,
        want_summary.get("summary").expect("oracle summary"),
        "summary diverged"
    );

    // 2) The surviving rows, per table.
    let got_jobs = db
        .read_main(|c| {
            let mut stmt = c.prepare("SELECT id, status FROM background_jobs ORDER BY id ASC")?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(json!({ "id": r.get::<_, String>(0)?, "status": r.get::<_, String>(1)? }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("read jobs");
    assert_eq!(
        &Value::Array(got_jobs),
        oracle_line(&oracle_text, "jobs").get("rows").unwrap(),
        "surviving jobs diverged"
    );

    let got_terminals = db
        .read_main(|c| {
            let mut stmt = c.prepare("SELECT id FROM terminal_sessions ORDER BY id ASC")?;
            let rows = stmt
                .query_map([], |r| Ok(json!({ "id": r.get::<_, String>(0)? })))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("read terminals");
    assert_eq!(
        &Value::Array(got_terminals),
        oracle_line(&oracle_text, "terminals").get("rows").unwrap(),
        "surviving terminal sessions diverged"
    );

    let got_mount = db
        .read_mount_index(|c| {
            let mut stmt = c.prepare("SELECT id FROM doc_mount_files ORDER BY id ASC")?;
            let rows = stmt
                .query_map([], |r| Ok(json!({ "id": r.get::<_, String>(0)? })))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("read mount files");
    assert_eq!(
        &Value::Array(got_mount),
        oracle_line(&oracle_text, "mount_files")
            .get("rows")
            .unwrap(),
        "orphan-sweep survivors diverged"
    );

    // 3) The stamp (presence + parseability; the value is wall-clock).
    let stamp = db
        .read_main(quilltap_core::db::instance_settings::get_last_maintenance_sweep_at)
        .expect("read stamp");
    let want_stamp = oracle_line(&oracle_text, "stamp");
    assert_eq!(
        json!({ "present": stamp.is_some(), "parseable": stamp.is_some() }),
        json!({ "present": want_stamp["present"], "parseable": want_stamp["parseable"] }),
        "stamp diverged"
    );

    // 4) The transcript-file outcomes.
    let want_tr = oracle_line(&oracle_text, "transcripts");
    assert_eq!(
        json!({ "defaultGone": !default_transcript.exists(), "explicitGone": !explicit.exists() }),
        json!({ "defaultGone": want_tr["defaultGone"], "explicitGone": want_tr["explicitGone"] }),
        "transcript outcomes diverged"
    );

    drop(db);
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    let _ = std::fs::remove_dir_all(&transcripts_dir);
    eprintln!(
        "OK: maintenance pass matched v4 (jobs {}/{}, orphans {}, terminals {}/{}).",
        summary.jobs_completed,
        summary.jobs_dead,
        summary.orphaned_files_swept,
        summary.terminal_rows,
        summary.terminal_transcripts
    );
}
