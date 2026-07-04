//! Read-differential (W4.1d batch 3a): the tiered mount pool
//! (`db::tiered_mount_pool::resolve_tiered_mount_pool`) vs v4's REAL
//! `resolveTieredMountPool`. Both sides READ the SAME baked fixtures (two
//! characters with vaults, a group with an official and a linked store plus charA
//! membership, a project with two stores and colliding refs, and the Quilltap
//! General singleton), run the SAME resolution matrix, and compare the resolved
//! pools EXACTLY — no
//! normalization (every id is pinned/shared; the char-vault mounts are minted but
//! read identically from the shared fixture on both sides).
//!
//! Matrix: ownership gate pass/fail, the pre-resolved fast path, the participant
//! tier + self-exclusion + flag-off, the per-RESPONDING-character group tier, the
//! character>group>global dedup dropping colliding project links, and the
//! character-less pool.
//!
//! Build the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-tmp-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-tiered-mount-pool-fixture.ts
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-tmp-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/tiered-mount-pool.ts > /tmp/oracle-tmp.ndjson
//! Run:
//!   QT_ORACLE_TMP=/tmp/oracle-tmp.ndjson \
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-tmp-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-tmp-mount.db \
//!     cargo test -p quilltap-harness --test tiered_mount_pool_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::tiered_mount_pool::{
    resolve_tiered_mount_pool, TierContext, TierResolveOptions,
};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "wrongUserId")]
    wrong_user_id: String,
    #[serde(rename = "charAId")]
    char_a_id: String,
    #[serde(rename = "charBId")]
    char_b_id: String,
    #[serde(rename = "projectId")]
    project_id: String,
    #[serde(rename = "fakeMountPointId")]
    fake_mount_point_id: String,
}

#[derive(Deserialize)]
struct Row {
    id: String,
    pool: Value,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/tiered-mount-pool.json")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

#[test]
fn tiered_mount_pool_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_TMP"),
        env_or_skip("QT_FIXTURE_TMP_MAIN"),
        env_or_skip("QT_FIXTURE_TMP_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: Row = serde_json::from_str(line).expect("oracle line parses");
        oracle.insert(row.id, row.pool);
    }

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-tmp-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-tmp-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main_w = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount_w = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));
    let main = main_w.connection();
    let mount = mount_w.connection();

    let a = spec.char_a_id.clone();
    let b = spec.char_b_id.clone();
    let p = spec.project_id.clone();

    // The matrix — MUST mirror harness/oracle/cases/tiered-mount-pool.ts exactly.
    type Case = (&'static str, TierContext, TierResolveOptions);
    let cases: Vec<Case> = vec![
        (
            "basic",
            TierContext {
                character_id: Some(a.clone()),
                project_id: Some(p.clone()),
                ..Default::default()
            },
            TierResolveOptions::default(),
        ),
        (
            "ownership_pass",
            TierContext {
                user_id: Some(spec.user_id.clone()),
                character_id: Some(a.clone()),
                project_id: Some(p.clone()),
                ..Default::default()
            },
            TierResolveOptions {
                require_ownership: true,
                include_participants: false,
            },
        ),
        (
            "ownership_fail",
            TierContext {
                user_id: Some(spec.wrong_user_id.clone()),
                character_id: Some(a.clone()),
                project_id: Some(p.clone()),
                ..Default::default()
            },
            TierResolveOptions {
                require_ownership: true,
                include_participants: false,
            },
        ),
        (
            "fast_path_wins",
            TierContext {
                character_mount_point_id: Some(spec.fake_mount_point_id.clone()),
                character_id: Some(a.clone()),
                ..Default::default()
            },
            TierResolveOptions::default(),
        ),
        (
            "participants",
            TierContext {
                character_id: Some(a.clone()),
                character_ids: Some(vec![b.clone()]),
                project_id: Some(p.clone()),
                ..Default::default()
            },
            TierResolveOptions {
                require_ownership: false,
                include_participants: true,
            },
        ),
        (
            "participant_excludes_self",
            TierContext {
                character_id: Some(a.clone()),
                character_ids: Some(vec![a.clone(), b.clone()]),
                project_id: Some(p.clone()),
                ..Default::default()
            },
            TierResolveOptions {
                require_ownership: false,
                include_participants: true,
            },
        ),
        (
            "no_character",
            TierContext {
                project_id: Some(p.clone()),
                ..Default::default()
            },
            TierResolveOptions::default(),
        ),
        (
            "group_per_character_B",
            TierContext {
                character_id: Some(b.clone()),
                project_id: Some(p.clone()),
                ..Default::default()
            },
            TierResolveOptions::default(),
        ),
        (
            "participants_flag_off",
            TierContext {
                character_id: Some(a.clone()),
                character_ids: Some(vec![b.clone()]),
                ..Default::default()
            },
            TierResolveOptions::default(),
        ),
    ];

    for (id, ctx, opts) in &cases {
        let pool = resolve_tiered_mount_pool(main, mount, ctx, opts);
        let got = serde_json::to_value(&pool).unwrap();
        let want = oracle
            .get(*id)
            .unwrap_or_else(|| panic!("oracle missing case '{id}'"));
        assert_eq!(&got, want, "tiered pool mismatch for case '{id}'");
    }
    assert_eq!(cases.len(), oracle.len(), "case count mismatch");
    eprintln!(
        "tiered_mount_pool: {} cases matched the oracle.",
        cases.len()
    );
}
