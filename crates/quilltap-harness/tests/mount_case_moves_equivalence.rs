//! Tier-2 differential — the database-store case-only move/rename changes of v4
//! `0a0419f5`: `move_database_document` (a case-only document rename is allowed)
//! and `move_database_folder` (dest-conflict allows the source itself; the
//! destination canonicalises under the destination PARENT's stored casing;
//! descendants + links prefix-rewrite from the source's stored path).
//!
//! Drives the ported functions over a fresh copy of the doc-mount-file-links
//! tier-2 fixture with the SAME seed + move sequence the oracle
//! (`harness/oracle/cases/mount-case-moves.ts`) drives against v4's real
//! `moveDatabaseDocument` / `moveDatabaseFolder`, then compares the deterministic
//! folder paths + link relativePaths.
//!
//! Generate the oracle (Node 24, from the v4 checkout at `d68638b4`), AFTER
//! building the shared fixture:
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-dmfl-fixture.db \
//!     $N/node --import tsx <W>/harness/oracle/fixtures/build-doc-mount-file-links-fixture.ts
//!   QT_FIXTURE_MOUNT_CASE_MOVES=/tmp/qt-dmfl-fixture.db \
//!     $N/node --import tsx <W>/harness/oracle/cases/mount-case-moves.ts \
//!     > /tmp/oracle-mount-case-moves.ndjson
//! Run:
//!   QT_ORACLE_MOUNT_CASE_MOVES=/tmp/oracle-mount-case-moves.ndjson \
//!   QT_FIXTURE_MOUNT_CASE_MOVES=/tmp/qt-dmfl-fixture.db \
//!     cargo test -p quilltap-harness --test mount_case_moves_equivalence -- --nocapture

use quilltap_core::db::database_store::{move_database_document, move_database_folder};
use quilltap_core::db::Writer;
use quilltap_core::services::mount_index::converters::RefusingTextExtractor;
use quilltap_core::services::mount_index::file_op_error::FileOpError;
use quilltap_core::services::mount_index::file_ops::{copy_file, move_file, CopyOpts};
use quilltap_core::services::mount_index::read_file::MountFileError;
use serde_json::{json, Value};

const STORE_ID: &str = "5700000e-0000-4000-8000-0000000000d5";
const TEST_PEPPER: &str = "ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=";

fn project(dump: &Value, cols: &[&str]) -> Vec<Value> {
    dump["rows"]
        .as_array()
        .expect("rows array")
        .iter()
        .map(|r| {
            let mut obj = serde_json::Map::new();
            for c in cols {
                obj.insert((*c).to_string(), r[*c].clone());
            }
            Value::Object(obj)
        })
        .collect()
}

#[test]
fn mount_case_moves_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_MOUNT_CASE_MOVES") else {
        eprintln!("SKIP: set QT_ORACLE_MOUNT_CASE_MOVES to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture) = std::env::var("QT_FIXTURE_MOUNT_CASE_MOVES") else {
        eprintln!("SKIP: set QT_FIXTURE_MOUNT_CASE_MOVES to the seed fixture .db (see header).");
        return;
    };

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .expect("read oracle")
            .trim(),
    )
    .expect("parse oracle");

    let work = std::env::temp_dir().join(format!("qt-mount-case-moves-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).expect("copy fixture");

    let writer = Writer::open_writable(&work, TEST_PEPPER).expect("open fixture copy");
    {
        let repo = writer.doc_mount_file_links();
        for (rel, content) in [
            ("notes.md", "root note"),
            ("Lore/entry.md", "lore entry"),
            ("Lore/sub/deep.md", "deep lore"),
            ("Archive/keep.md", "archived"),
            ("guard.md", "do not delete me"),
        ] {
            repo.write_database_document(STORE_ID, rel, content)
                .expect("seed write_database_document");
        }
    }

    let conn = writer.connection();
    let extractor = RefusingTextExtractor;

    // 0a. copyFile same-path guard: force-copy guard.md onto GUARD.md must be
    //     rejected BEFORE the force-overwrite branch (the source survives).
    let guard_err = copy_file(
        conn,
        &CopyOpts {
            source_mount_point_id: STORE_ID.to_string(),
            source_path: "guard.md".to_string(),
            dest_mount_point_id: STORE_ID.to_string(),
            dest_path: "GUARD.md".to_string(),
            force: true,
        },
        &extractor,
    )
    .expect_err("copy onto case-variant of self must be rejected");
    let MountFileError::FileOp(FileOpError { message, code }) = guard_err else {
        panic!("expected a FileOp error, got {guard_err:?}");
    };
    let guard = json!({ "error": message, "code": code.as_str() });

    // 0b. moveFile case-only rename (guard.md → Guard.md on the same mount).
    move_file(conn, STORE_ID, "guard.md", STORE_ID, "Guard.md", &extractor)
        .expect("move file case-only");

    // 1. case-only DOCUMENT rename (notes.md → Notes.md).
    move_database_document(conn, STORE_ID, "notes.md", "Notes.md").expect("move doc case-only");
    // 2. move Lore under Archive, addressing the parent in a different casing.
    move_database_folder(conn, STORE_ID, "Lore", "archive/Lore").expect("move folder canonicalise");
    // 3. case-only rename of Archive (now containing Lore) — allow-self + descendants.
    move_database_folder(conn, STORE_ID, "Archive", "ARCHIVE").expect("move folder case-only");

    let folders_dump = writer
        .dump_table_json("doc_mount_folders", "path")
        .expect("dump folders");
    let links_dump = writer
        .dump_table_json("doc_mount_file_links", "relativePath")
        .expect("dump links");
    let _ = std::fs::remove_file(&work);

    let got = json!({
        "guard": guard,
        "folders": project(&folders_dump, &["path"]),
        "links": project(&links_dump, &["relativePath", "fileName"]),
    });

    assert_eq!(
        got, oracle,
        "mount-case-moves diverged from v4\n  rust:   {got}\n  oracle: {oracle}"
    );
    eprintln!("mount_case_moves: matched oracle (doc case-only + folder canonicalise/allow-self)");
}
