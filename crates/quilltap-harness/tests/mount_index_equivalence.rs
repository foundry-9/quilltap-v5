//! P4.6y mount-INDEX differential (scan / reindex / embed / semantic-search):
//! the `api::mount_files` indexing variants vs v4's REAL action-dispatch
//! handlers, over per-case FRESH copies of the committed mounts fixture. Both
//! sides' RAW eight-table dumps (+ response bodies) are normalized by the ONE
//! shared algorithm in `tests/mount_common/mod.rs`.
//!
//! Generate the oracle (Node 24, from the v4 checkout — /tmp mirror; jest
//! ignores .claude/ paths):
//!   TMPO=/tmp/qt-mount-index-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp harness/oracle/cases/mount-index.test.ts "$TMPO/cases/"
//!   cp harness/oracle/fixtures/mounts.json "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_MOUNTS_MAIN=<v5>/crates/quilltap-web/tests/fixtures/mounts-main.db \
//!   QT_FIXTURE_MOUNTS_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/mounts-mount.db \
//!   QT_MOUNTS_FS_TREE=<v5>/crates/quilltap-web/tests/fixtures/mounts-fs-tree \
//!   QT_ORACLE_OUT=/tmp/oracle-mount-index.ndjson \
//!     npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- mount-index
//! Run:
//!   QT_ORACLE_MOUNT_INDEX=/tmp/oracle-mount-index.ndjson \
//!     cargo test -p quilltap-harness --test mount_index_equivalence

#[path = "mount_common/mod.rs"]
mod mount_common;

use mount_common::*;
use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::embedding::ErasedEmbeddingProvider;
use quilltap_core::model::wire::CannedWireTransport;
use quilltap_core::services::embedding_provider::ApiEmbeddingProvider;
use serde_json::json;

// ─── the runner ──────────────────────────────────────────────────────────────

#[test]
fn mount_index_matches_oracle() {
    let Some(oracle) = load_oracle("QT_ORACLE_MOUNT_INDEX") else {
        return;
    };
    let pepper = test_pepper();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    use quilltap_core::api::mount_files as mf;

    type CaseFn = Box<dyn Fn(&Db, &tokio::runtime::Runtime) -> Response>;
    fn provider(db: &Db) -> ErasedEmbeddingProvider {
        ErasedEmbeddingProvider::new(ApiEmbeddingProvider::new(
            db.clone(),
            CannedWireTransport::new(),
        ))
    }
    let cases: Vec<(&str, CaseFn)> = vec![
        // ── scan (unit F) ──
        (
            "scan_fs",
            Box::new(|db, rt| rt.block_on(mf::mount_scan(db, MP_FS))),
        ),
        (
            "scan_obs",
            Box::new(|db, rt| rt.block_on(mf::mount_scan(db, MP_OBS))),
        ),
        (
            "scan_db",
            Box::new(|db, rt| rt.block_on(mf::mount_scan(db, MP_DB))),
        ),
        (
            "scan_404",
            Box::new(|db, rt| rt.block_on(mf::mount_scan(db, BOGUS))),
        ),
        // ── reindex / embed / semantic-search (unit E) ──
        (
            "reindex_db_default",
            Box::new(|db, rt| rt.block_on(mf::mount_reindex(db, MP_DB, None, None))),
        ),
        (
            "reindex_db_force",
            Box::new(|db, rt| rt.block_on(mf::mount_reindex(db, MP_DB, None, Some(true)))),
        ),
        (
            "reindex_db_scoped",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_reindex(
                    db,
                    MP_DB,
                    Some("notes/".to_string()),
                    Some(true),
                ))
            }),
        ),
        (
            "reindex_404",
            Box::new(|db, rt| rt.block_on(mf::mount_reindex(db, BOGUS, None, None))),
        ),
        (
            "embed_db_default",
            Box::new(|db, rt| rt.block_on(mf::mount_embed(db, MP_DB, None, None))),
        ),
        (
            "embed_db_scoped_force",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_embed(
                    db,
                    MP_DB,
                    Some("reference.txt".to_string()),
                    Some(true),
                ))
            }),
        ),
        (
            "embed_404",
            Box::new(|db, rt| rt.block_on(mf::mount_embed(db, BOGUS, None, None))),
        ),
        (
            "search_basic",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_semantic_search(
                    db,
                    &provider(db),
                    SINGLE_USER_ID,
                    json!({"query": "reference alpha", "threshold": 0, "top": 10}),
                ))
            }),
        ),
        (
            "search_scoped",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_semantic_search(
                    db,
                    &provider(db),
                    SINGLE_USER_ID,
                    json!({
                        "query": "body line",
                        "mountPointIds": [MP_DB],
                        "pathPrefix": "notes/",
                        "threshold": 0
                    }),
                ))
            }),
        ),
        (
            "search_bad_body",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_semantic_search(
                    db,
                    &provider(db),
                    SINGLE_USER_ID,
                    json!({}),
                ))
            }),
        ),
    ];

    let mut checked = 0usize;
    for (name, run) in &cases {
        let want = oracle
            .get(*name)
            .unwrap_or_else(|| panic!("no oracle row {name}"));
        let (db, scratch) = fresh_db(&pepper, name);
        let resp = run(&db, &rt);
        let (status, body) = response_to_status_body(resp);
        let tables = dump_tables(&db);
        let got = json!({ "name": name, "status": status, "body": body, "tables": tables });
        // Error cases carry a tables dump too — the refusals must leave the
        // fixture untouched on both sides.
        assert_eq!(
            norm(&got),
            norm(want),
            "case {name}\n{}",
            diff_first_line(&norm(&got), &norm(want))
        );
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);
        checked += 1;
    }

    assert_eq!(checked, 14, "expected the 14 mount-index cases");
    eprintln!("OK: mount-index matched oracle ({checked} cases).");
}
