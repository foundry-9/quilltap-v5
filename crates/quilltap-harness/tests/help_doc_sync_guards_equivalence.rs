//! P4.d6 differential — the help-doc sync's PRUNE GUARD (v4 `551f090b`
//! `syncHelpDocs`, the `files.length === 0` early return, WIDENED by bug 18
//! (`13ddc5ee`) with the blank-content refusal).
//!
//! The prune is the difference between "delete rows whose file is gone" and
//! "empty the table", so its exact boundary is pinned against v4's REAL code.
//!
//! Three scenarios, each over a FRESH copy of the help-sync fixture DB and the
//! committed tree at `fixtures/help-sync-guards/<scenario>/`, walked through the
//! production host walker:
//!   - `missing-dir`      — no help/ at all → no sync, no prune;
//!   - `no-markdown`      — help/ with zero .md → no sync, no prune;
//!   - `only-empty-files` — help/ with ONE whitespace-only .md. The walk is
//!     non-empty (past the `files.is_empty()` guard) but the file contributes no
//!     `pathsOnDisk` entry — historically that emptied the table. Bug 18 added a
//!     second guard: `pathsOnDisk.size === 0 && existingDocs.length > 0` refuses
//!     the prune, so the rows survive (oracle-confirmed: `deleted: 0`, all three
//!     rows left). v5 made the same refusal in this convergence.
//!
//! Generate (Node 24, from the v4 checkout), AFTER building the fixture:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this tree>
//!   cd ~/source/quilltap-server
//!   : > /tmp/oracle-help-sync-guards.ndjson
//!   for s in missing-dir no-markdown only-empty-files; do
//!     QT_FIXTURE_HELP_MAIN=/tmp/qt-help-sync-main.db QT_HELP_SYNC_SCENARIO=$s \
//!       $N/node --import tsx $V5/harness/oracle/cases/help-sync-guards.ts \
//!       >> /tmp/oracle-help-sync-guards.ndjson
//!   done
//! Run:
//!   QT_ORACLE_HELP_SYNC_GUARDS=/tmp/oracle-help-sync-guards.ndjson \
//!   QT_FIXTURE_HELP_MAIN=/tmp/qt-help-sync-main.db \
//!     cargo test -p quilltap-harness --test help_doc_sync_guards_equivalence

use serde::Deserialize;
use serde_json::json;

use quilltap_core::db::runtime::Db;
use quilltap_core::services::help_doc_sync::sync_help_docs;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

#[derive(Deserialize)]
struct GuardCase {
    scenario: String,
    #[serde(rename = "totalOnDisk")]
    total_on_disk: usize,
    deleted: usize,
    failed: usize,
    #[serde(rename = "helpDocIds")]
    help_doc_ids: Vec<String>,
}

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

#[test]
fn help_doc_sync_guards_match_oracle() {
    let (oracle_path, fixture_main) = match (
        std::env::var("QT_ORACLE_HELP_SYNC_GUARDS"),
        std::env::var("QT_FIXTURE_HELP_MAIN"),
    ) {
        (Ok(o), Ok(f)) => (o, f),
        _ => {
            eprintln!(
                "skipping help_doc_sync_guards_match_oracle: set QT_ORACLE_HELP_SYNC_GUARDS + \
                 QT_FIXTURE_HELP_MAIN to run the differential"
            );
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("help-sync.json")).expect("spec"),
    )
    .expect("parse spec");

    let oracle_text = std::fs::read_to_string(&oracle_path).expect("oracle ndjson");
    let cases: Vec<GuardCase> = oracle_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert_eq!(
        cases.len(),
        3,
        "expected all three guard scenarios in the oracle — regenerate {oracle_path}"
    );

    for case in &cases {
        // Fresh copy per scenario: `only-empty-files` empties the table.
        let tmp = tempfile::tempdir().expect("tempdir");
        let work_main = tmp.path().join("help-sync-main.db");
        std::fs::copy(&fixture_main, &work_main).expect("copy fixture");
        let db = Db::open_main(&work_main, &spec.test_pepper_base64).expect("open db");

        let scenario_root = fixtures_dir().join("help-sync-guards").join(&case.scenario);
        let files = quilltap_host::files_store::load_help_source_files(&scenario_root);

        let result = db
            .write_blocking(move |ws| Ok(sync_help_docs(ws.main().connection(), &files)))
            .expect("sync");

        let ids: Vec<String> = db
            .read_main(|c| {
                let mut stmt = c
                    .prepare("SELECT id FROM help_docs ORDER BY id ASC")
                    .map_err(quilltap_core::db::DbError::from)?;
                let mapped = stmt
                    .query_map([], |r| r.get::<_, String>(0))
                    .map_err(quilltap_core::db::DbError::from)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(quilltap_core::db::DbError::from)?;
                Ok(mapped)
            })
            .expect("dump ids");

        let rust = json!({
            "scenario": case.scenario,
            "totalOnDisk": result.total_on_disk,
            "deleted": result.deleted,
            "failed": result.failed,
            "helpDocIds": ids,
        });
        let oracle = json!({
            "scenario": case.scenario,
            "totalOnDisk": case.total_on_disk,
            "deleted": case.deleted,
            "failed": case.failed,
            "helpDocIds": case.help_doc_ids,
        });
        assert_eq!(
            rust, oracle,
            "guard scenario {} diverged\nrust:   {rust}\noracle: {oracle}",
            case.scenario
        );

        eprintln!(
            "guard {}: totalOnDisk={} deleted={} rowsLeft={}",
            case.scenario,
            result.total_on_disk,
            result.deleted,
            ids.len()
        );
    }

    // Pin the two halves of the boundary explicitly, so a future regeneration
    // that quietly loses a scenario cannot pass as agreement.
    let by_name = |n: &str| cases.iter().find(|c| c.scenario == n).expect("scenario");
    assert_eq!(
        by_name("missing-dir").deleted,
        0,
        "a missing help/ must NEVER prune — this is the wipe guard"
    );
    assert_eq!(
        by_name("no-markdown").deleted,
        0,
        "a help/ with no Markdown must NEVER prune — this is the wipe guard"
    );
    assert_eq!(
        by_name("only-empty-files").deleted,
        0,
        "bug 18: a help/ whose only Markdown is whitespace-only must NOT prune — \
         `pathsOnDisk.size === 0 && existingDocs.length > 0` refuses the wipe. If this \
         ever flips back to a nonzero delete, v4 reverted its bug-18 guard."
    );
    assert!(
        !by_name("only-empty-files").help_doc_ids.is_empty(),
        "bug 18: the rows must survive an all-blank help/ — the table stays populated."
    );
}
