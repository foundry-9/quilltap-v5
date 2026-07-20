//! P4.4u3: the host assembly seeds the built-in roleplay templates + mount
//! stores on every assemble/unlock, and a pre-provisioned instance is ADOPTED
//! (drift-updated / re-pointed), never duplicated.
//!
//! The fixture is a REAL fresh-provisioned instance (`provision_fresh_instance`,
//! which already runs both seed families once); booting a `Host` over it re-runs
//! the seeders through the assemble path — so the counts prove idempotency
//! (2 templates, 3 mounts, 3 pointers), not a double-seed.

use std::path::Path;

use quilltap_core::api::QuilltapCore as _;
use quilltap_core::api::{PepperState, Request, Response};
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_host::{Host, HostConfig};

/// The test pepper `provision_fresh_instance`'s own self-test uses.
const PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";

fn hermetic_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    // Keep the periodic cadences well out of the way.
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    // This test pins the P4.4u3 built-in mount counts; the sample-content seed
    // (default ON since the P4.4u4 unification) would add the two character
    // vaults' stores on top.
    config.seed_sample_content = false;
    config
}

fn count(db: &quilltap_core::db::runtime::Db, main: bool, sql: &'static str) -> i64 {
    let f = |conn: &rusqlite::Connection| Ok(conn.query_row(sql, [], |r| r.get::<_, i64>(0))?);
    if main {
        db.read_main(f).unwrap()
    } else {
        db.read_mount_index(f).unwrap()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn assemble_seeds_and_adopts_builtins() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();

    // Fresh instance — provisioning seeds both families once.
    provision_fresh_instance(&data, PEPPER).expect("provision");

    // Booting the host re-runs the seeders through `assemble` → adopt/drift-update.
    let host = Host::start(hermetic_config(dir.path())).unwrap();
    let core = host.core();
    match core.dispatch(Request::Health).await {
        Response::Health(h) => {
            assert!(h.ready);
            assert_eq!(h.pepper_state, PepperState::NeedsVaultStorage);
        }
        other => panic!("unexpected: {other:?}"),
    }

    let db = core.db().expect("engine ready");

    // Family 1: exactly the two built-in templates (drift-updated, not doubled).
    assert_eq!(
        count(
            &db,
            true,
            "SELECT count(*) FROM roleplay_templates WHERE isBuiltIn = 1"
        ),
        2,
        "expected exactly 2 built-in roleplay templates after re-seed"
    );
    // Family 2: exactly the three mount stores (ADOPTED, not duplicated).
    assert_eq!(
        count(&db, false, "SELECT count(*) FROM doc_mount_points"),
        3,
        "expected exactly 3 built-in mount stores after adopt"
    );
    assert_eq!(
        count(
            &db,
            true,
            "SELECT count(*) FROM instance_settings WHERE key IN \
             ('generalMountPointId','userUploadsMountPointId','lanternBackgroundsMountPointId')"
        ),
        3,
        "expected the three mount-pointer settings"
    );
    // The General Scenarios folder was (re-)ensured.
    assert_eq!(
        count(
            &db,
            false,
            "SELECT count(*) FROM doc_mount_folders WHERE path = 'Scenarios'"
        ),
        1,
        "expected the General Scenarios folder"
    );
    // The general state.json companion seed (P4.d10 — v4 `f48f34dc`): exactly one
    // root state.json link on the general mount, seeded with the literal `{}`.
    let state_body = db
        .read_mount_index(|conn| {
            Ok(conn
                .query_row(
                    "SELECT d.content FROM doc_mount_file_links l \
                     JOIN doc_mount_documents d ON d.fileId = l.fileId \
                     WHERE l.relativePath = 'state.json'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .unwrap_or_default())
        })
        .unwrap();
    assert_eq!(
        state_body, "{}",
        "expected the seeded general state.json body"
    );
}
