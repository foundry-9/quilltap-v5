//! Tier-2 differential (DB state): the fictional-clock anchor backfill —
//! [`quilltap_core::db::fictional_clock_anchor_repair`] vs v4's REAL migration
//! `anchorFictionalClockBaseMigration.run()` (P4.d18; v4 `e3a9654f`).
//!
//! Both sides provision a fresh instance (v4 through `initializeDatabase` +
//! `ensureCollection('chats', …)`, v5 through `provision_fresh_instance` — the
//! D23 re-dump of that same `generateDDL` output), plant the shared spec's
//! `chats` rows, run the pass twice, and dump `(id, createdAt, timestampConfig)`
//! ordered by id. Exact equality on every field, including the raw
//! `timestampConfig` TEXT — key order and all, because that column is JSON the
//! backfill re-serializes and v4's spread keeps an already-present-but-falsy
//! `fictionalBaseRealTime` in its original slot.
//!
//! The clock is pinned on both sides (the spec's `nowMs`), so even the two rows
//! that fall back to "now" are diffed on their exact value.
//!
//! Generate the oracle (Node 24; TZ=UTC):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd /tmp/qt-v4-pin-231be14c
//!   TZ=UTC $N/node --import tsx \
//!     ~/source/quilltap-v5/harness/oracle/cases/fictional-clock-anchor.ts \
//!     > /tmp/oracle-fictional-clock-anchor.ndjson
//! Run:
//!   QT_ORACLE_FICTIONAL_CLOCK_ANCHOR=/tmp/oracle-fictional-clock-anchor.ndjson \
//!     cargo test -p quilltap-harness --test fictional_clock_anchor_equivalence -- --nocapture

use quilltap_core::db::fictional_clock_anchor_repair::anchor_fictional_clock_bases;
use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use serde_json::Value;

/// The same test pepper every provisioning fixture uses.
const TEST_PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";

fn spec() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../harness/oracle/fixtures/fictional-clock-anchor-spec.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("read spec")).expect("parse spec")
}

#[test]
fn fictional_clock_anchor_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_FICTIONAL_CLOCK_ANCHOR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_FICTIONAL_CLOCK_ANCHOR to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("cannot read {oracle_path}: {e}"))
            .lines()
            .find(|l| !l.trim().is_empty())
            .expect("oracle file is empty"),
    )
    .expect("parse oracle line");

    let spec = spec();
    let now_ms = spec["nowMs"].as_i64().expect("spec nowMs");
    let chats = spec["chats"].as_array().expect("spec chats");
    assert!(
        chats.len() >= 13,
        "the spec lost rows — every guard arm of v4's migration must stay tabled ({} left)",
        chats.len()
    );

    // --- v5: a fresh instance, the spec's rows, the pass twice ---
    let scratch = tempfile::tempdir().unwrap();
    let data = scratch.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, TEST_PEPPER).expect("provision");
    let main = Writer::open_writable(&data.join("quilltap.db"), TEST_PEPPER).unwrap();
    let conn = main.connection();

    for chat in chats {
        conn.execute(
            r#"INSERT INTO "chats" ("id","userId","title","createdAt","updatedAt","timestampConfig")
                 VALUES (?1,?2,?3,?4,?5,?6)"#,
            rusqlite::params![
                chat["id"].as_str().unwrap(),
                "ffffffff-ffff-ffff-ffff-ffffffffffff",
                chat["label"].as_str().unwrap(),
                chat["createdAt"].as_str(),
                "2024-03-04T05:06:07.000Z",
                chat["timestampConfig"].as_str(),
            ],
        )
        .expect("plant chat row");
    }

    let anchored = anchor_fictional_clock_bases(conn, now_ms).expect("repair");
    // v4's own `needsAnchor` guard is the idempotence marker — no marker row.
    let anchored_again = anchor_fictional_clock_bases(conn, now_ms).expect("repair again");

    // --- diff ---
    assert_eq!(
        anchored as i64,
        oracle["itemsAffected"].as_i64().unwrap(),
        "rows anchored on the first pass"
    );
    assert_eq!(
        anchored_again as i64,
        oracle["secondItemsAffected"].as_i64().unwrap(),
        "rows anchored on the second pass (must be 0 — the pass is idempotent)"
    );
    assert!(
        oracle["shouldRun"].as_bool().unwrap(),
        "v4's shouldRun gate must OPEN on this spec, or the oracle proves nothing"
    );
    assert!(
        !oracle["shouldRunAfter"].as_bool().unwrap(),
        "v4's shouldRun gate must CLOSE once the backfill has run"
    );
    assert!(anchored > 0, "the spec must exercise the stamping path");

    let mut stmt = conn
        .prepare(r#"SELECT "id","createdAt","timestampConfig" FROM "chats" ORDER BY "id""#)
        .unwrap();
    let got: Vec<(String, String, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    let want = oracle["rows"].as_array().expect("oracle rows");
    assert_eq!(got.len(), want.len(), "row count");
    for (row, expected) in got.iter().zip(want) {
        let (id, created_at, config) = row;
        assert_eq!(
            id.as_str(),
            expected["id"].as_str().unwrap(),
            "row id order"
        );
        assert_eq!(
            created_at.as_str(),
            expected["createdAt"].as_str().unwrap(),
            "createdAt must not be touched ({id})"
        );
        assert_eq!(
            config.as_deref(),
            expected["timestampConfig"].as_str(),
            "timestampConfig for {id}"
        );
    }

    eprintln!(
        "OK: fictional-clock-anchor matched oracle ({} rows, {anchored} anchored, second pass {anchored_again}).",
        got.len()
    );
}
