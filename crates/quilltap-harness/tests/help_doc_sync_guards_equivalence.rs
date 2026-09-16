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
//! ## This family READS its own fixture variable (P4.93)
//!
//! `QT_FIXTURE_HELP_SYNC_GUARDS_MAIN`, not the sync family's
//! `QT_FIXTURE_HELP_MAIN`. Both families used to read the one name while their
//! recipes wrote two different files, so whichever value the gate's env block
//! happened to carry is the file BOTH families opened — and neither could say
//! which. They passed on each other's fixture because ONE builder
//! (`fixtures/build-help-sync-fixture.ts`) reads ONE spec (`fixtures/
//! help-sync.json`), so the two `.db`s are logically identical; the collision
//! was latent, not yet a red. The shared BUILDER still takes its OUT path from
//! `QT_FIXTURE_HELP_MAIN` — there it is a write destination, not a family
//! selector, and the builder is not this lane's to edit.
//!
//! The six-field per-scenario compare below is the other half: it used to carry
//! no content channel at all (`helpDocIds` alone — `SELECT id`), so no fixture
//! swap could ever redden it. It now carries `helpDocContentHashes` in the same
//! id order, which is the value the sync's whole update/skip decision turns on.
//!
//! Generate (Node 24, from the v4 checkout). The fixture is built by the
//! recipe's own first stage into a FAMILY-SPECIFIC path — this family used to
//! read the sync family's `/tmp/qt-help-sync-main.db` without building it,
//! which is the staging-dependency defect P4.47 (C)'s driver class flags
//! (flagged there, repaired here per the round's Shared contract rule 3):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this tree>
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_HELP_MAIN=/tmp/qt-help-sync-guards-main.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-help-sync-fixture.ts
//!   : > /tmp/oracle-help-sync-guards.ndjson
//!   for s in missing-dir no-markdown only-empty-files; do
//!     QT_FIXTURE_HELP_SYNC_GUARDS_MAIN=/tmp/qt-help-sync-guards-main.db QT_HELP_SYNC_SCENARIO=$s \
//!       $N/node --import tsx $V5/harness/oracle/cases/help-sync-guards.ts \
//!       >> /tmp/oracle-help-sync-guards.ndjson
//!   done
//! Run:
//!   QT_ORACLE_HELP_SYNC_GUARDS=/tmp/oracle-help-sync-guards.ndjson \
//!   QT_FIXTURE_HELP_SYNC_GUARDS_MAIN=/tmp/qt-help-sync-guards-main.db \
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

#[derive(Deserialize, Debug)]
struct GuardCase {
    scenario: String,
    #[serde(rename = "totalOnDisk")]
    total_on_disk: usize,
    deleted: usize,
    failed: usize,
    /// P4.D77 — a refused prune must leave the section chunks alone too.
    #[serde(rename = "chunksWritten")]
    chunks_written: usize,
    #[serde(rename = "helpDocIds")]
    help_doc_ids: Vec<String>,
    /// P4.93 — the family's ONLY content channel. `helpDocIds` reads ids the
    /// spec pins, so it is identical across ANY fixture this builder produces;
    /// the hashes are the seeded bytes, so a fixture built from a different
    /// spec reddens here (proven by building one and running this family
    /// against the unchanged oracle — see the lane record).
    #[serde(rename = "helpDocContentHashes")]
    help_doc_content_hashes: Vec<String>,
}

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

#[test]
fn help_doc_sync_guards_match_oracle() {
    let (oracle_path, fixture_main) = match (
        std::env::var("QT_ORACLE_HELP_SYNC_GUARDS"),
        std::env::var("QT_FIXTURE_HELP_SYNC_GUARDS_MAIN"),
    ) {
        (Ok(o), Ok(f)) => (o, f),
        _ => {
            eprintln!(
                "SKIP: help_doc_sync_guards_match_oracle: set QT_ORACLE_HELP_SYNC_GUARDS + \
                 QT_FIXTURE_HELP_SYNC_GUARDS_MAIN to run the differential"
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

        let rows: Vec<(String, String)> = db
            .read_main(|c| {
                let mut stmt = c
                    .prepare("SELECT id, contentHash FROM help_docs ORDER BY id ASC")
                    .map_err(quilltap_core::db::DbError::from)?;
                let mapped = stmt
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                    .map_err(quilltap_core::db::DbError::from)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(quilltap_core::db::DbError::from)?;
                Ok(mapped)
            })
            .expect("dump ids + content hashes");
        let ids: Vec<String> = rows.iter().map(|(id, _)| id.clone()).collect();
        let hashes: Vec<String> = rows.iter().map(|(_, h)| h.clone()).collect();

        let rust = json!({
            "scenario": case.scenario,
            "totalOnDisk": result.total_on_disk,
            "deleted": result.deleted,
            "failed": result.failed,
            "chunksWritten": result.chunks_written,
            "helpDocIds": ids,
            "helpDocContentHashes": hashes,
        });
        let oracle = json!({
            "scenario": case.scenario,
            "totalOnDisk": case.total_on_disk,
            "deleted": case.deleted,
            "failed": case.failed,
            "chunksWritten": case.chunks_written,
            "helpDocIds": case.help_doc_ids,
            "helpDocContentHashes": case.help_doc_content_hashes,
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

    // P4.93 — the content channel must actually carry something in EVERY
    // scenario, or the compare above agrees about an empty list and the
    // sensitivity it was added for is vacuous (the `a-guard-whose-other-
    // conjuncts-are-false-is-untested` shape). One hash per surviving id.
    for case in &cases {
        assert_eq!(
            case.help_doc_content_hashes.len(),
            case.help_doc_ids.len(),
            "{}: one contentHash per surviving row: {case:?}",
            case.scenario
        );
        assert!(
            !case.help_doc_content_hashes.is_empty()
                && case
                    .help_doc_content_hashes
                    .iter()
                    .all(|h| !h.trim().is_empty()),
            "{}: the content channel must be populated — an empty one cannot \
             tell one fixture from another",
            case.scenario
        );
    }
}
