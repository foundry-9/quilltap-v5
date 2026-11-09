//! P4.6y refresh-parity differential (the P4.6w seam-wire proof): a standalone
//! `write-document` to a document_store mount must leave the mount index in
//! v4's post-refresh state — chunks written, link rollups set, stats
//! refreshed. v4 runs `writeDatabaseDocument`'s inline reindex + the
//! fire-and-forget `scheduleDocumentStoreRefresh` (drained by the oracle); v5
//! runs the write + the PRODUCTION `DbMountRefreshScheduler` (the live host
//! wiring — a spawned writer job), drained here by polling for the chunk.
//!
//! Generate the oracle (Node 24, from the v4 checkout — /tmp mirror):
//!   TMPO=/tmp/qt-mount-refresh-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp harness/oracle/cases/mount-refresh.test.ts "$TMPO/cases/"
//!   cp harness/oracle/fixtures/documents-web.json "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_DOC_MAIN=<v5>/crates/quilltap-web/tests/fixtures/documents-main.db \
//!   QT_FIXTURE_DOC_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/documents-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-mount-refresh.ndjson \
//!     npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- mount-refresh
//! Run:
//!   QT_ORACLE_MOUNT_REFRESH=/tmp/oracle-mount-refresh.ndjson \
//!     cargo test -p quilltap-harness --test mount_refresh_equivalence

#[path = "mount_common/mod.rs"]
mod mount_common;

use std::path::PathBuf;
use std::sync::Arc;

use mount_common::{diff_first_line, load_oracle, norm, response_to_status_body, rows_to_json};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::mount_index::refresh::DbMountRefreshScheduler;
use serde_json::{json, Map, Value};

const GEN: &str = "d0000000-0000-4000-8000-00000000000e";

const DUMP: &[(&str, &str)] = &[
    (
        "points",
        "SELECT id, name, mountType, storeType, fileCount, chunkCount, totalSizeBytes \
         FROM doc_mount_points ORDER BY id",
    ),
    (
        "links",
        "SELECT fileId, mountPointId, relativePath, fileName, conversionStatus, plainTextLength, \
         chunkCount, extractionStatus FROM doc_mount_file_links ORDER BY mountPointId, relativePath",
    ),
    (
        "documents",
        "SELECT content, contentSha256, plainTextLength FROM doc_mount_documents ORDER BY contentSha256",
    ),
    (
        "chunks",
        "SELECT c.chunkIndex, c.content, c.tokenCount, c.headingContext, \
         (CASE WHEN c.embedding IS NULL THEN NULL ELSE length(c.embedding) END) AS embeddingLength, \
         l.relativePath AS sortPath \
         FROM doc_mount_chunks c LEFT JOIN doc_mount_file_links l ON l.id = c.linkId \
         ORDER BY c.mountPointId, sortPath, c.chunkIndex",
    ),
    (
        "folders",
        "SELECT mountPointId, parentId IS NULL AS isRoot, name, path FROM doc_mount_folders \
         ORDER BY mountPointId, path",
    ),
];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

#[test]
fn refresh_after_standalone_write_matches_oracle() {
    let Some(oracle) = load_oracle("QT_ORACLE_MOUNT_REFRESH") else {
        return;
    };
    let want = oracle
        .get("refresh_after_standalone_write")
        .expect("oracle row");

    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/documents-web.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap();

    let scratch = std::env::temp_dir().join(format!("qt-mrefresh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("documents-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("documents-mount.db"), &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        pepper,
    )
    .expect("open db");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    // The PRODUCTION scheduler — the exact object host.rs now wires.
    let scheduler = Arc::new(DbMountRefreshScheduler::new(db.clone()));
    let resp = rt.block_on(quilltap_core::api::documents::document_write(
        &db,
        json!({
            "filePath": "notes/refresh-case.md",
            "scope": "document_store",
            "mountPoint": GEN,
            "content": "# Refresh case\n\nThe chunks must exist after this write.\n",
        }),
        Some(scheduler),
        None,
    ));
    let (status, body) = response_to_status_body_doc(resp);

    // Drain the fire-and-forget: poll for the refresh-case chunk (the spawned
    // writer job lands strictly after the write commits).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let n: i64 = db
            .read_mount_index(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM doc_mount_chunks c \
                     JOIN doc_mount_file_links l ON l.id = c.linkId \
                     WHERE l.relativePath = 'notes/refresh-case.md'",
                    [],
                    |row| row.get(0),
                )
                .map_err(Into::into)
            })
            .unwrap();
        if n > 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "refresh chunk never landed"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    // Barrier: a no-op write drains anything still queued behind the refresh.
    db.write_blocking(|_| Ok(())).unwrap();

    let mut tables = Map::new();
    for (name, sql) in DUMP {
        let sql = sql.to_string();
        let rows = db
            .read_mount_index(move |conn| Ok(rows_to_json(conn, &sql)))
            .unwrap();
        tables.insert((*name).to_string(), Value::Array(rows));
    }
    let got = json!({
        "name": "refresh_after_standalone_write",
        "status": status,
        "body": body,
        "tables": Value::Object(tables),
    });
    assert_eq!(
        norm(&got),
        norm(want),
        "refresh parity\n{}",
        diff_first_line(&norm(&got), &norm(want))
    );
    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);
    eprintln!("OK: refresh-after-write matched oracle (the seam wire is live).");
}

/// The documents family rides `Response::Document`; map like the mount family.
fn response_to_status_body_doc(r: quilltap_core::api::types::Response) -> (i64, Value) {
    use quilltap_core::api::types::Response;
    match r {
        Response::Document(v) => (200, v),
        other => response_to_status_body(other),
    }
}

// === P4.D380 === (v4 `a3d53eae1`, #133, round ruling R-2): a Document Mode
// save with `scope: project` into the project's DATABASE official store
// refreshes that store (the chokepoint is gated on `mountPointId`, not `scope`)
// — the production scheduler, drained by polling the store's stats.
#[test]
fn p4d380_refresh_after_project_scope_write_matches_oracle() {
    let Some(oracle) = load_oracle("QT_ORACLE_MOUNT_REFRESH") else {
        return;
    };
    let want = oracle
        .get("p4d380_refresh_after_project_scope_write")
        .expect("oracle row");
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/documents-web.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let scratch = std::env::temp_dir().join(format!("qt-mrefresh-p-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("documents-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("documents-mount.db"), &mount).unwrap();
    {
        let w = quilltap_core::db::Writer::open_writable(&main, pepper).unwrap();
        w.connection()
            .execute_batch(
                "UPDATE chats SET projectId = 'b1000000-0000-4000-8000-000000000001' \
                 WHERE id = 'c1000000-0000-4000-8000-00000000000b'",
            )
            .unwrap();
    }
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        pepper,
    )
    .expect("open db");
    let atelier_before: f64 = db
        .read_mount_index(|c| {
            c.query_row(
                "SELECT chunkCount FROM doc_mount_points WHERE id = '6eed765d-9452-4729-9398-7705adec4413'",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let scheduler = Arc::new(DbMountRefreshScheduler::new(db.clone()));
    let resp = rt.block_on(quilltap_core::api::documents::chat_document_write(
        &db,
        "c1000000-0000-4000-8000-00000000000b",
        json!({
            "filePath": "brief.md",
            "scope": "project",
            "content": "# Brief\n\nA project-scope save must refresh its store.\n",
        }),
        Some(scheduler),
        None,
    ));
    let (status, body) = response_to_status_body_doc(resp);
    // Drain: the refreshed stats land strictly after the write commits.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let now: f64 = db
            .read_mount_index(|c| {
                c.query_row(
                    "SELECT chunkCount FROM doc_mount_points WHERE id = '6eed765d-9452-4729-9398-7705adec4413'",
                    [],
                    |r| r.get(0),
                )
                .map_err(Into::into)
            })
            .unwrap();
        if now != atelier_before {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "#133: the project-scope save never refreshed its store's stats"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    db.write_blocking(|_| Ok(())).unwrap();
    let mut tables = Map::new();
    for (name, sql) in DUMP {
        let sql = sql.to_string();
        let rows = db
            .read_mount_index(move |conn| Ok(rows_to_json(conn, &sql)))
            .unwrap();
        tables.insert((*name).to_string(), Value::Array(rows));
    }
    let got = json!({
        "name": "p4d380_refresh_after_project_scope_write",
        "status": status,
        "body": body,
        "tables": Value::Object(tables),
    });
    assert_eq!(
        norm(&got),
        norm(want),
        "#133 refresh parity\n{}",
        diff_first_line(&norm(&got), &norm(want))
    );
    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);
}
// === end P4.D380 ===
