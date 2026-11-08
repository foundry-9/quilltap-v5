//! P4.D362 — the `connection_profiles.maxImagesPerRequest` boot ensure
//! (`db::connection_profiles_max_images_repair`, the TWENTY-THIRD D23 move)
//! against v4's REAL migration `add-profile-max-images-per-request-v1` through
//! v4's REAL `MigrationRunner` narrowed to it
//! (`cases/connection-profiles-max-images-ensure.ts`).
//!
//! Five shapes over the SAME base files the oracle builds at the pin from v4's
//! generateDDL: (a) BASELINE → both append `INTEGER DEFAULT 4` + run the
//! backfill (measured `backfilled` 0); (b) NO TABLE, (c) MIGRATED,
//! (d) GENERATEDDL-CURRENT, (e) PLANTED NULL → `not needed`, the NULL stays.
//! Comparands: the report, `PRAGMA table_info`, `sqlite_master.sql`, every
//! row's `quote()` / `typeof` cap, the ledger; and on (c) / (e) v4's REAL
//! `findById` / `findAll` cap against v5's `find_by_id` / `find_all` (R-L7 —
//! measured: v4 reads a NULL cell as `4`).
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-max-images-ensure /tmp/oracle-max-images-ensure.ndjson
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-max-images-ensure \
//!     $N/npx tsx $V5W/harness/oracle/cases/connection-profiles-max-images-ensure.ts \
//!     > /tmp/oracle-max-images-ensure.ndjson
//! Run:
//!   cd $V5W
//!   QT_ORACLE_MAX_IMAGES_ENSURE=/tmp/oracle-max-images-ensure.ndjson \
//!   QT_FIXTURE_MAX_IMAGES_ENSURE_DIR=/tmp/qt-max-images-ensure \
//!     cargo test -p quilltap-harness --test connection_profiles_max_images_ensure_equivalence -- --nocapture

mod keystone_ensure_support;

use std::path::Path;

use keystone_ensure_support as kit;
use quilltap_core::db::connection_profiles;
use quilltap_core::db::connection_profiles_max_images_repair::{
    ensure_connection_profiles_max_images, MaxImagesEnsureOutcome, MIGRATION_ID,
};
use quilltap_core::db::Writer;
use serde_json::{json, Value};

const PROFILE_IDS: [&str; 3] = [
    "6f1c3a52-0000-4000-8000-0000000000b1",
    "6f1c3a52-0000-4000-8000-0000000000b2",
    "6f1c3a52-0000-4000-8000-0000000000b3",
];

fn cap(p: Option<&Value>) -> Value {
    match p {
        None => json!("<row absent>"),
        Some(p) => p
            .get("maxImagesPerRequest")
            .cloned()
            .unwrap_or(json!("<key absent>")),
    }
}

#[test]
fn the_max_images_ensure_matches_v4s_migration_in_five_shapes() {
    let (Some(oracle), Some(dir)) = (
        std::env::var_os("QT_ORACLE_MAX_IMAGES_ENSURE"),
        std::env::var_os("QT_FIXTURE_MAX_IMAGES_ENSURE_DIR"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_MAX_IMAGES_ENSURE and QT_FIXTURE_MAX_IMAGES_ENSURE_DIR (see header).");
        return;
    };
    let mut modes = Vec::new();
    for v4 in kit::read_ndjson(&oracle) {
        let mode = v4["mode"].as_str().unwrap().to_string();
        let report = kit::report(&v4);
        let runs = mode == "a";
        assert_eq!(report.len(), 1, "[{mode}] {report:?}");
        if runs {
            assert!(
                report[0].starts_with(&format!("+RAN {MIGRATION_ID} (Added maxImagesPerRequest column to connection_profiles (0 row(s) backfilled))")),
                "[{mode}] {report:?}"
            );
        } else {
            assert!(report[0].contains(" not needed"), "[{mode}] {report:?}");
        }
        assert_eq!(
            serde_json::from_value::<Vec<String>>(v4["ledger"].clone()).unwrap(),
            if runs {
                vec![MIGRATION_ID.to_string()]
            } else {
                vec![]
            },
            "[{mode}] v4's ledger"
        );
        let scratch = tempfile::tempdir().unwrap();
        let work = scratch.path().join("work.db");
        std::fs::copy(Path::new(&dir).join(format!("max-images-{mode}.db")), &work).unwrap();
        let w = Writer::open_writable(&work, kit::PEPPER).unwrap();
        let c = w.connection();
        let before = kit::table_sql(c, "connection_profiles");
        let outcome = ensure_connection_profiles_max_images(c).expect("ensure");
        assert_eq!(
            outcome,
            if runs {
                MaxImagesEnsureOutcome::Added { backfilled: 0 }
            } else {
                MaxImagesEnsureOutcome::NotNeeded
            },
            "[{mode}] the ensure's answer"
        );
        if !runs {
            assert_eq!(
                kit::table_sql(c, "connection_profiles"),
                before,
                "[{mode}] table touched"
            );
        }
        kit::assert_bytes(
            &kit::table_info(c, "connection_profiles"),
            &v4["tableInfo"],
            &format!("[{mode}] table_info"),
        );
        kit::assert_bytes(
            &json!(kit::table_sql(c, "connection_profiles")),
            &v4["sql"],
            &format!("[{mode}] table sql"),
        );
        kit::assert_bytes(
            &kit::cells(c, "connection_profiles", &["maxImagesPerRequest"]),
            &v4["rows"],
            &format!("[{mode}] cells"),
        );
        assert!(kit::ledger(c).is_empty(), "[{mode}] v5 stamped (UNSTAMPED)");
        if mode == "a" {
            let sql = v4["sql"].as_str().unwrap();
            assert!(
                sql.ends_with(", \"maxImagesPerRequest\" INTEGER DEFAULT 4)"),
                "marker {sql}"
            );
        }
        if matches!(mode.as_str(), "c" | "e") {
            let by_id = connection_profiles::find_by_id(c, PROFILE_IDS[1]).unwrap();
            let all = connection_profiles::find_all(c).unwrap();
            let got = json!({
                "findById": cap(by_id.as_ref()),
                "findAll": PROFILE_IDS.iter().map(|id| cap(all.iter().find(|p| p["id"] == *id))).collect::<Vec<_>>(),
            });
            kit::assert_bytes(&got, &v4["read"], &format!("[{mode}] read (R-L7)"));
        } else {
            assert!(v4["read"].is_null());
        }
        modes.push(mode);
    }
    assert_eq!(modes, ["a", "b", "c", "d", "e"], "every shape ran");
    eprintln!("OK: the max-images ensure matched v4 in shapes {modes:?}.");
}
