//! Read-differential test: the PUBLIC wardrobe READ trio + shared-archetype tier
//! (`db::wardrobe_read::{find_by_character_id, find_by_id_for_character}` +
//! `db::archetype_wardrobe`) vs v4's REAL
//! `WardrobeRepository.findByCharacterId` / `.findByIdForCharacter`.
//!
//! Both v4 and the Rust port READ the SAME baked fixtures (a character + its
//! seeded vault in a MOUNT-INDEX db, plus a provisioned "Quilltap General" store
//! whose id lives in the MAIN db's `instance_settings`) and run the SAME five
//! cases, so each result compares EXACTLY — no normalization (no clock mint on a
//! read path; every id/timestamp comes from the shared fixture).
//!
//! Cases:
//!   0. findByCharacterId(char, false)        -> active items only (archived filtered).
//!   1. findByCharacterId(char, true)         -> includes the archived item.
//!   2. findByIdForCharacter(char, composite) -> owned composite whose componentItemIds
//!      resolve to the two archetype ids (PROVES archetype seeding: slug + UUID refs land).
//!   3. findByIdForCharacter(char, archetypeA)-> NOT owned -> archetype fallback (characterId null).
//!   4. findByIdForCharacter(char, no-such)   -> null.
//!
//! Build the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WPR_MAIN=/tmp/qt-wpr-main.db QT_FIXTURE_WPR_MOUNT=/tmp/qt-wpr-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-wardrobe-public-read-fixture.ts
//!   QT_FIXTURE_WPR_MAIN=/tmp/qt-wpr-main.db QT_FIXTURE_WPR_MOUNT=/tmp/qt-wpr-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/wardrobe-public-read.ts > /tmp/oracle-wpr.ndjson
//! Run:
//!   QT_ORACLE_WPR=/tmp/oracle-wpr.ndjson \
//!   QT_FIXTURE_WPR_MAIN=/tmp/qt-wpr-main.db QT_FIXTURE_WPR_MOUNT=/tmp/qt-wpr-mount.db \
//!     cargo test -p quilltap-harness --test wardrobe_public_read_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::wardrobe_read::{find_by_character_id, find_by_id_for_character};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "compositeId")]
    composite_id: String,
    #[serde(rename = "archetypeAId")]
    archetype_a_id: String,
    #[serde(rename = "noSuchId")]
    no_such_id: String,
}

#[derive(Deserialize)]
struct Oracle {
    results: Vec<Value>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/wardrobe-public-read-tier2.json")
}

#[test]
fn wardrobe_public_read_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_WPR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_WPR to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_WPR_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_WPR_MAIN to the main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_WPR_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_WPR_MOUNT to the mount fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle");

    assert_eq!(
        oracle.results.len(),
        5,
        "expected 5 oracle results, got {}",
        oracle.results.len()
    );

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-wpr-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-wpr-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));
    let docs = mount.doc_mount_documents();
    let conn = main.connection();

    let char_id = &spec.character_id;

    // Case 0: active items only.
    let got0 = Value::Array(find_by_character_id(conn, &docs, char_id, false).expect("case0"));
    // Case 1: include archived.
    let got1 = Value::Array(find_by_character_id(conn, &docs, char_id, true).expect("case1"));
    // Case 2: owned composite by id (component refs resolved to archetype ids).
    let got2 = find_by_id_for_character(conn, &docs, char_id, &spec.composite_id, &[])
        .expect("case2")
        .unwrap_or(Value::Null);
    // Case 3: archetype fallback.
    let got3 = find_by_id_for_character(conn, &docs, char_id, &spec.archetype_a_id, &[])
        .expect("case3")
        .unwrap_or(Value::Null);
    // Case 4: unknown id -> null.
    let got4 = find_by_id_for_character(conn, &docs, char_id, &spec.no_such_id, &[])
        .expect("case4")
        .unwrap_or(Value::Null);

    let got = [got0, got1, got2, got3, got4];

    drop(main);
    drop(mount);
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

    let labels = [
        "findByCharacterId(false)",
        "findByCharacterId(true)",
        "findByIdForCharacter(composite)",
        "findByIdForCharacter(archetype)",
        "findByIdForCharacter(no-such)",
    ];
    for (i, (g, want)) in got.iter().zip(oracle.results.iter()).enumerate() {
        assert_eq!(
            g, want,
            "case {i} ({}) diverged\n  rust:   {g}\n  oracle: {want}",
            labels[i]
        );
    }

    eprintln!("OK: wardrobe-public-read matched oracle on 5 cases.");
}
