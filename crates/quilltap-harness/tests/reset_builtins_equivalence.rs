//! P4.4u4 unit 2 (tier 2) differential: v4's `handleResetBuiltins` (driven via the
//! collection route) vs the Rust service
//! (`quilltap_core::services::quilltap_import::reset::reset_builtins`).
//!
//! Both sides start from the SAME empty fixtures, PRE-SEED Lorian + Riya + both
//! avatars (the reset's precondition), then reset: cascade-delete the built-ins,
//! re-import with the seed-id → preserved-id remap, reseed the avatars. Every id
//! is minted on both sides, so the post-reset `characters` / `memories` dumps are
//! remapped to first-seen tokens (FKs verify by relationship) and timestamps
//! placeholdered.
//!
//! Diffed: the RESULT SHAPE (deletedCharacterCount / remappedIdCount /
//! preserved+postReset present / postReset != preserved [fresh mint] /
//! importedCharacters+Memories), the post-state counts (2 chars, 42 memories, 2
//! avatar links, 2 chars with defaultImageId), and the normalized characters +
//! memories row state. (The cascade delete, the import, and the avatar seed are
//! each byte-proven by their own differentials — this pins the composition + the
//! `replaceMappedIdsRecursively` / `findBuiltinCharacterIds` helpers + the result
//! shape.)
//!
//! Generate the oracle (jest, see the .test.ts header) then run:
//!   QT_ORACLE_RESET_BUILTINS=/tmp/oracle-reset-builtins.ndjson \
//!   QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
//!   QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
//!     cargo test -p quilltap-harness --test reset_builtins_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::SINGLE_USER_ID;
use quilltap_core::services::quilltap_import::reset::reset_builtins;
use quilltap_core::services::quilltap_import::seed::seed_sample_content;
use quilltap_host::HostImageCodec;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

struct TableSpec {
    table: &'static str,
    oracle_key: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "characters",
        oracle_key: "characters",
        order_by: "name",
        id_columns: &["id", "characterDocumentMountPointId", "defaultImageId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
    TableSpec {
        table: "memories",
        oracle_key: "memories",
        order_by: "content",
        id_columns: &["id", "characterId", "aboutCharacterId"],
        ts_columns: &["createdAt", "updatedAt"],
    },
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/qtap-import-tier2.json")
}

fn normalize(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        let rows = dumps[i]
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .unwrap_or_else(|| panic!("{}: no rows", spec.table));
        for row in rows.iter_mut() {
            let obj = row.as_object_mut().unwrap();
            for col in spec.id_columns {
                if let Some(Value::String(raw)) = obj.get(*col) {
                    let next = format!("ID_{}", id_map.len());
                    let token = id_map.entry(raw.clone()).or_insert(next).clone();
                    obj.insert((*col).to_string(), Value::String(token));
                }
            }
            for col in spec.ts_columns {
                if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                    obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
                }
            }
        }
    }
}

#[test]
fn reset_builtins_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_RESET_BUILTINS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_RESET_BUILTINS (see header).");
            return;
        }
    };
    let (main_fixture, mount_fixture) = match (
        std::env::var("QT_FIXTURE_QTAPIMPORT_MAIN"),
        std::env::var("QT_FIXTURE_QTAPIMPORT_MOUNT"),
    ) {
        (Ok(a), Ok(b)) => (a, b),
        _ => {
            eprintln!("SKIP: set QT_FIXTURE_QTAPIMPORT_MAIN/MOUNT (header).");
            return;
        }
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .expect("read oracle")
            .lines()
            .find(|l| !l.trim().is_empty())
            .expect("one line"),
    )
    .expect("parse oracle");

    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-reset-main-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-reset-mount-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).expect("copy main");
    std::fs::copy(&mount_fixture, &mount_work).expect("copy mount");

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64).expect("open main");
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64).expect("open mount");

    // PRE-SEED: Lorian + Riya + both avatars (gate passes on the empty DB).
    let pre = seed_sample_content(main.connection(), mount.connection(), &HostImageCodec);
    assert_eq!(pre.imported_characters, 2, "pre-seed imported 2 characters");
    assert_eq!(pre.avatars_written, 2, "pre-seed wrote 2 avatars");

    // The op under test.
    let result = reset_builtins(
        main.connection(),
        mount.connection(),
        &HostImageCodec,
        SINGLE_USER_ID,
    )
    .expect("reset_builtins");

    // Result shape vs the oracle.
    let ores = &oracle["result"];
    assert_eq!(
        result.deleted_character_ids.len() as i64,
        ores["deletedCharacterCount"].as_i64().unwrap(),
        "deletedCharacterCount"
    );
    assert_eq!(result.deleted_character_ids.len(), 2);
    assert_eq!(
        result.remapped_id_count as i64,
        ores["remappedIdCount"].as_i64().unwrap(),
        "remappedIdCount"
    );
    assert_eq!(result.remapped_id_count, 2);
    // preserved + postReset both present; postReset differs from preserved (fresh mint).
    let both_present = |pairs: &[(String, Option<String>)]| pairs.iter().all(|(_, v)| v.is_some());
    assert!(
        both_present(&result.preserved_ids),
        "preserved both present"
    );
    assert!(
        both_present(&result.post_reset_ids),
        "postReset both present"
    );
    let differs = result
        .preserved_ids
        .iter()
        .zip(&result.post_reset_ids)
        .all(|((_, pre), (_, post))| pre != post);
    assert!(differs, "postReset must differ from preserved (fresh mint)");
    assert_eq!(ores["postResetDiffersFromPreserved"], Value::Bool(true));
    assert_eq!(
        result.import.imported.characters as i64,
        ores["importedCharacters"].as_i64().unwrap()
    );
    assert_eq!(
        result.import.imported.memories as i64,
        ores["importedMemories"].as_i64().unwrap()
    );

    // Post-state counts.
    let char_count: i64 = main
        .connection()
        .query_row("SELECT count(*) FROM characters", [], |r| r.get(0))
        .unwrap();
    let mem_count: i64 = main
        .connection()
        .query_row("SELECT count(*) FROM memories", [], |r| r.get(0))
        .unwrap();
    let avatar_links: i64 = mount
        .connection()
        .query_row(
            "SELECT count(*) FROM doc_mount_file_links WHERE relativePath = 'images/avatar.webp'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let chars_with_avatar: i64 = main
        .connection()
        .query_row(
            "SELECT count(*) FROM characters WHERE defaultImageId IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let ops = &oracle["postState"];
    assert_eq!(char_count, ops["characterCount"].as_i64().unwrap(), "chars");
    assert_eq!(char_count, 2);
    assert_eq!(mem_count, ops["memoryCount"].as_i64().unwrap(), "memories");
    assert_eq!(mem_count, 42);
    assert_eq!(
        avatar_links,
        ops["avatarLinkCount"].as_i64().unwrap(),
        "avatar links"
    );
    assert_eq!(avatar_links, 2);
    assert_eq!(
        chars_with_avatar,
        ops["charsWithDefaultImage"].as_i64().unwrap(),
        "chars with avatar"
    );

    // Normalized characters + memories row diff.
    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            main.dump_table_json(s.table, s.order_by)
                .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
        })
        .collect();
    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|s| oracle.get(s.oracle_key).cloned().expect("oracle table"))
        .collect();
    normalize(&mut got);
    normalize(&mut want);
    for (i, s) in TABLES.iter().enumerate() {
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{}: columns",
            s.table
        );
        assert_eq!(
            got[i]["rows"], want[i]["rows"],
            "{}: rows diverged\n  rust:   {}\n  oracle: {}",
            s.table, got[i]["rows"], want[i]["rows"]
        );
    }

    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    eprintln!("OK: reset_builtins matched oracle (result shape + post-state chars/memories).");
}
