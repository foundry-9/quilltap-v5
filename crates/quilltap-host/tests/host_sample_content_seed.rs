//! P4.4u4 unit 2: the fresh-boot sample-content seed via the HOST path. On a
//! first boot (zero-characters gate) the host seeds Lorian + Riya + 42 memories +
//! both avatars; a SECOND boot over the same instance short-circuits on the gate
//! (no duplicated characters/memories, no re-written avatars).
//!
//! The seed rides `HostConfig::seed_sample_content` (default ON since the round
//! unified; kept explicit here so the test stays self-describing). It is a REAL
//! fresh-provisioned instance booted through `assemble`.

use std::path::Path;

use quilltap_core::api::QuilltapCore as _;
use quilltap_core::api::{Request, Response};
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_host::{Host, HostConfig};

/// The test pepper `provision_fresh_instance`'s own self-test uses.
const PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";

fn seeding_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    config.seed_sample_content = true; // explicit: the seed is the surface under test
    config
}

fn count_main(db: &quilltap_core::db::runtime::Db, sql: &'static str) -> i64 {
    db.read_main(|conn| Ok(conn.query_row(sql, [], |r| r.get::<_, i64>(0))?))
        .unwrap()
}
fn count_mount(db: &quilltap_core::db::runtime::Db, sql: &'static str) -> i64 {
    db.read_mount_index(|conn| Ok(conn.query_row(sql, [], |r| r.get::<_, i64>(0))?))
        .unwrap()
}

fn assert_seeded_once(db: &quilltap_core::db::runtime::Db) {
    // Exactly the two seed characters for the single user.
    assert_eq!(
        count_main(
            db,
            "SELECT count(*) FROM characters WHERE userId = 'ffffffff-ffff-ffff-ffff-ffffffffffff'"
        ),
        2,
        "expected exactly 2 seed characters"
    );
    assert_eq!(
        count_main(db, "SELECT count(*) FROM characters WHERE name = 'Lorian'"),
        1,
        "Lorian present"
    );
    assert_eq!(
        count_main(db, "SELECT count(*) FROM characters WHERE name = 'Riya'"),
        1,
        "Riya present"
    );
    // Exactly 42 memories (not doubled).
    assert_eq!(
        count_main(db, "SELECT count(*) FROM memories"),
        42,
        "expected exactly 42 seed memories"
    );
    // Both characters have an avatar link set + a materialized avatar in the vault.
    assert_eq!(
        count_main(
            db,
            "SELECT count(*) FROM characters WHERE defaultImageId IS NOT NULL"
        ),
        2,
        "both characters have defaultImageId set"
    );
    assert_eq!(
        count_mount(
            db,
            "SELECT count(*) FROM doc_mount_file_links WHERE relativePath = 'images/avatar.webp'"
        ),
        2,
        "two avatar links (one per character vault)"
    );
    // Sanity: the wardrobe reached the vaults (8 items → 8 Wardrobe/*.md links).
    assert_eq!(
        count_mount(
            db,
            "SELECT count(*) FROM doc_mount_file_links WHERE relativePath LIKE 'Wardrobe/%'"
        ),
        8,
        "8 wardrobe .md links (4 per character)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_boot_seeds_sample_content_and_second_boot_is_a_no_op() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();

    // A fresh instance — provisioning creates empty encrypted DBs (no characters).
    provision_fresh_instance(&data, PEPPER).expect("provision");

    // First boot: assemble runs the built-in seeds + the gated sample-content seed.
    let host = Host::start(seeding_config(dir.path())).unwrap();
    let core = host.core();
    match core.dispatch(Request::Health).await {
        Response::Health(h) => assert!(h.ready, "host ready"),
        other => panic!("unexpected: {other:?}"),
    }
    let db = core.db().expect("engine ready");
    assert_seeded_once(&db);
    drop(db);

    // Tear down the first assembly (releases the instance lock) so a second boot
    // can acquire it.
    match core.dispatch(Request::Lock).await {
        Response::UnlockState(_) => {}
        other => panic!("unexpected on lock: {other:?}"),
    }
    drop(host);

    // Second boot over the SAME instance: the zero-characters gate short-circuits
    // the sample-content seed — no duplicated characters/memories, no new avatars.
    let host2 = Host::start(seeding_config(dir.path())).unwrap();
    let core2 = host2.core();
    match core2.dispatch(Request::Health).await {
        Response::Health(h) => assert!(h.ready, "host2 ready"),
        other => panic!("unexpected: {other:?}"),
    }
    let db2 = core2.db().expect("engine2 ready");
    assert_seeded_once(&db2); // unchanged — the gate held
    drop(db2);
    let _ = core2.dispatch(Request::Lock).await;
}
