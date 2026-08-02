//! Tier-2 differential (P4.D30, v4 `83118077`): the Pascal definition READ path
//! — `pascal::roster::load_tools_from_mount` vs v4's real `loadToolsFromMount`,
//! over the committed `pascal-run-custom-{main,mount}.db` fixture.
//!
//! `pascal_roster_equivalence` is the shadowing/cap corpus and mocks the store
//! reads on purpose. This family mocks nothing: definition bytes come out of a
//! real encrypted mount index through the real storage dispatch, which is the
//! layer the drift moved. It is where the drift's two reachable effects show:
//!
//!  - CHAR_C's vault holds `Tools/beacon.tool.json` as a database BLOB (the
//!    fixture builder writes it through v4's real `storeMountFile` with
//!    `treatNativeTextAsDocument: false`, the way every file-storage bridge
//!    writes). The old `read_database_document` dispatch found no document row
//!    and SKIPPED it; the canonical reader loads it.
//!  - `Tools/mangled.tool.json` is the same, with one invalid UTF-8 byte in its
//!    description. v4's `bytes.toString('utf-8')` substitutes U+FFFD instead of
//!    throwing, so it loads — where v5's old `read_to_string` would have failed
//!    the file with a `could not be read` entry that v4 never produces.
//!
//! The third effect (the SOURCE_NOT_FOUND race skip) and the reader's boundary
//! refusal are not stageable from a committed fixture — neither listing can
//! produce a vanished or escaping path. Both are pinned by unit tests in
//! `pascal::roster::read_tool_file_tests`, the way v4 pins them with jest stubs.
//!
//! Generate the oracle (Node 24; the v4 checkout at the oracle baseline —
//! mirror to /tmp; jest ignores `.claude/`; pin a detached worktree only on
//! drift/dirty). Self-shielding: the fixture DBs AND the `.meta.json` sidecar
//! the oracle reads are copied to /tmp first, so the sidecar travels with the
//! copy:
//!   M=/tmp/qt-pascal-reader-mirror; mkdir -p $M/cases $M/fixtures $M/db
//!   cp <V5W>/harness/oracle/cases/pascal-definition-reader.test.ts $M/cases/
//!   cp <V5W>/harness/oracle/fixtures/pascal-run-custom.json $M/fixtures/
//!   cp <V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db $M/db/
//!   cp <V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-main.db.meta.json $M/db/
//!   cp <V5W>/crates/quilltap-web/tests/fixtures/pascal-run-custom-mount.db $M/db/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_PASCAL_MAIN=$M/db/pascal-run-custom-main.db \
//!   QT_FIXTURE_PASCAL_MOUNT=$M/db/pascal-run-custom-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-pascal-definition-reader.ndjson \
//!     npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$M/cases" -- pascal-definition-reader
//! Run:
//!   QT_ORACLE_PASCAL_DEFINITION_READER=/tmp/oracle-pascal-definition-reader.ndjson \
//!     cargo test -p quilltap-harness --test pascal_definition_reader_equivalence

use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::tiered_mount_pool::MountTier;
use quilltap_core::pascal::roster::load_tools_from_mount;
use serde_json::{json, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
/// The General store — a fixed id in the fixture builder, not a minted one.
const GENERAL_MP: &str = "93000000-0000-4000-8000-0000000000aa";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

#[derive(serde::Deserialize)]
struct Meta {
    #[serde(rename = "vaultA")]
    vault_a: String,
    #[serde(rename = "vaultB")]
    vault_b: String,
    #[serde(rename = "vaultC")]
    vault_c: String,
    #[serde(rename = "vaultD")]
    vault_d: String,
}

fn open(tag: &str) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-pascal-reader-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("pascal-run-custom-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("pascal-run-custom-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open db")
}

fn tier_of(s: &str) -> MountTier {
    match s {
        "character" => MountTier::Character,
        "participant" => MountTier::Participant,
        "group" => MountTier::Group,
        "project" => MountTier::Project,
        "global" => MountTier::Global,
        other => panic!("unknown tier {other}"),
    }
}

fn tier_str(t: MountTier) -> &'static str {
    match t {
        MountTier::Character => "character",
        MountTier::Participant => "participant",
        MountTier::Group => "group",
        MountTier::Project => "project",
        MountTier::Global => "global",
    }
}

#[test]
fn definition_reads_match_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_PASCAL_DEFINITION_READER") else {
        eprintln!(
            "SKIP pascal_definition_reader_equivalence: QT_ORACLE_PASCAL_DEFINITION_READER unset"
        );
        return;
    };
    let raw = std::fs::read_to_string(&path).expect("read oracle ndjson");

    let meta: Meta = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("pascal-run-custom-main.db.meta.json"))
            .unwrap(),
    )
    .unwrap();

    // Case name → (mountPointId, tier). Kept beside the oracle's own list so a
    // corpus that grows a case without a mapping fails loudly rather than
    // skipping the new arm in silence.
    let mount_for = |name: &str| -> (String, &'static str) {
        match name {
            "vault-a-documents" => (meta.vault_a.clone(), "character"),
            "vault-b-documents" => (meta.vault_b.clone(), "participant"),
            "vault-c-blobs" => (meta.vault_c.clone(), "participant"),
            "vault-d-broken-overlay" => (meta.vault_d.clone(), "character"),
            "general-store" => (GENERAL_MP.to_string(), "global"),
            "missing-mount" => ("00000000-0000-4000-8000-00000000dead".to_string(), "global"),
            other => panic!("case '{other}' has no mount mapping — add it"),
        }
    };

    let mut checked = 0usize;
    let mut saw_blob_case = false;
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let want: Value = serde_json::from_str(line).expect("oracle row");
        let name = want["name"].as_str().unwrap().to_string();
        let (mount_point_id, tier) = mount_for(&name);

        let db = open(&name);
        let (found, errors) = db
            .read_mount_index(|mount| {
                Ok(load_tools_from_mount(mount, &mount_point_id, tier_of(tier)))
            })
            .unwrap();

        let got = json!({
            "found": found.iter().map(|t| json!({
                "definition": serde_json::to_value(&t.definition).unwrap(),
                "tier": tier_str(t.tier),
                "mountPointId": t.mount_point_id,
                "mountName": t.mount_name,
                "definitionPath": t.definition_path,
            })).collect::<Vec<_>>(),
            "errors": errors.iter().map(|e| json!({
                "definitionPath": e.definition_path,
                "mountPointId": e.mount_point_id,
                "mountName": e.mount_name,
                "tier": tier_str(e.tier),
                "reason": e.reason,
            })).collect::<Vec<_>>(),
        });

        assert_eq!(got["found"], want["found"], "case '{name}': found");
        assert_eq!(got["errors"], want["errors"], "case '{name}': errors");

        if name == "vault-c-blobs" {
            saw_blob_case = true;
            let names: Vec<&str> = found.iter().map(|t| t.definition.name.as_str()).collect();
            // The drift's whole point: two BLOB-stored definitions beside the
            // document-stored one, all three loaded.
            assert_eq!(names, vec!["ansible", "beacon", "mangled"]);
            let mangled = found
                .iter()
                .find(|t| t.definition.name == "mangled")
                .expect("the mangled definition loaded");
            assert!(
                mangled.definition.description.contains('\u{FFFD}'),
                "the invalid UTF-8 byte must decode to U+FFFD, not fail the read"
            );
        }
        checked += 1;
    }

    assert!(
        checked >= 6,
        "corpus shrank to {checked} cases — regenerate the oracle"
    );
    assert!(
        saw_blob_case,
        "the blob-stored-definition case is missing from the corpus"
    );
    eprintln!("OK pascal_definition_reader_equivalence: {checked} cases");
}
