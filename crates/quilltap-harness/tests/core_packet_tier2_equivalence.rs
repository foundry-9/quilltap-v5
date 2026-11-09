//! Tier-2 (real-DB) differential (P4.D378 item 12): the Core whisper's packet —
//! `quilltap_core::services::core_whisper::assemble_core_packet` vs v4's REAL
//! `assembleCorePacket` (`lib/services/aurora-notifications/core-whisper.ts`)
//! at `624d0d25c`, whose group tier walks the shared `groupMountsFor`
//! (v4 `0c47f4539`: membership order, a store shared by two groups credited to
//! the first, the character's own vault filtered out, an ABSENT group row listed
//! with name `''` — sorting first).
//!
//! Both sides READ a COPY of the SAME per-run pair built by
//! `build-core-packet-fixture.ts` (`core-packet.json`: one character per row)
//! and compare, per row, the packet's `files[]` (`path`, `body`, `sourceLabel`
//! — `null` when absent) and `approxTokens` — or `null` — as BYTES
//! (`serde_json::to_string`; key order is part of the contract). Rows:
//!   - `vault_only`, `two_groups_alpha_order`, `shared_store_first_group`,
//!     `group_store_is_the_vault`, `no_core_files` (`null`),
//!     `missing_character` (`null`);
//!   - `orphan_membership_with_links` — a membership + link for a group id
//!     with NO `groups` row (v4 lists it, label `''`, first);
//!   - `rf_group_hydrated_read_fails_raw_whole` (R-F, item 13) — the group's
//!     `properties.json` is malformed: the HYDRATED `groups.findById` fails on
//!     BOTH sides (probed in-case, compared byte-for-byte, and self-checked on
//!     v4's side), the raw row is whole and names the group;
//!   - the order's three "sound shapes", measured equal to the baseline loop:
//!     `sound_vault_in_group_a_only` (A = [vault]),
//!     `sound_a_vault_x_b_x` (A = [vault, X], B = [X]),
//!     `sound_a_x_b_x_vault` (A = [X], B = [X, vault]) — the sound shapes'
//!     groups have their official pointer NULLed so their stores are exactly
//!     their links.
//!
//! Every row is checked; the red set is printed whole, then the test FAILS if
//! any row differs.
//!
//! Build the pair + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -f /tmp/qt-corepacket-main.db /tmp/qt-corepacket-mount.db
//!   QT_FIXTURE_COREPACKET_MAIN=/tmp/qt-corepacket-main.db \
//!   QT_FIXTURE_COREPACKET_MOUNT=/tmp/qt-corepacket-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-core-packet-fixture.ts
//!   QT_FIXTURE_COREPACKET_MAIN=/tmp/qt-corepacket-main.db \
//!   QT_FIXTURE_COREPACKET_MOUNT=/tmp/qt-corepacket-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/core-packet.ts > /tmp/oracle-core-packet.ndjson
//! Run:
//!   cd $V5W
//!   QT_ORACLE_COREPACKET=/tmp/oracle-core-packet.ndjson \
//!   QT_FIXTURE_COREPACKET_MAIN=/tmp/qt-corepacket-main.db \
//!   QT_FIXTURE_COREPACKET_MOUNT=/tmp/qt-corepacket-mount.db \
//!     cargo test -p quilltap-harness --test core_packet_tier2_equivalence

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::core_whisper::assemble_core_packet;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    cases: Vec<CaseSpec>,
}

#[derive(Deserialize)]
struct CaseSpec {
    name: String,
    #[serde(rename = "characterId")]
    character_id: String,
    budget: i64,
    #[serde(rename = "hydratedProbeGroupIds")]
    hydrated_probe_group_ids: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct OracleRow {
    name: String,
    packet: Value,
    probe: Option<Value>,
}

fn bytes(v: &Value) -> String {
    serde_json::to_string(v).expect("serialize")
}

#[test]
fn core_packet_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_COREPACKET") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_COREPACKET to the oracle NDJSON (see header).");
            return;
        }
    };
    let (fx_main, fx_mount) = match (
        std::env::var("QT_FIXTURE_COREPACKET_MAIN"),
        std::env::var("QT_FIXTURE_COREPACKET_MOUNT"),
    ) {
        (Ok(a), Ok(b)) => (a, b),
        _ => {
            eprintln!("SKIP: set QT_FIXTURE_COREPACKET_MAIN / _MOUNT (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/core-packet.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_rows: Vec<OracleRow> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle line"))
        .collect();
    assert_eq!(
        oracle_rows.len(),
        spec.cases.len(),
        "oracle row count vs case count"
    );

    // Read a private COPY of the pair (a scratch dir removed on drop).
    let scratch = tempfile::Builder::new()
        .prefix("qt-corepacket-rust-")
        .tempdir()
        .expect("tempdir");
    let work_main = scratch.path().join("main.db");
    let work_mount = scratch.path().join("mount.db");
    std::fs::copy(&fx_main, &work_main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&fx_mount, &work_mount).unwrap_or_else(|e| panic!("copy mount: {e}"));
    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    let mut failures: Vec<String> = Vec::new();
    for (case, oracle) in spec.cases.iter().zip(oracle_rows.iter()) {
        assert_eq!(case.name, oracle.name, "case name alignment");
        let mut case_failures: Vec<String> = Vec::new();

        // R-F: the hydrated read FAILS, the raw row is whole — v5's probe vs
        // v4's, byte-for-byte, after checking v4's plant took.
        if let Some(ids) = &case.hydrated_probe_group_ids {
            let want = oracle
                .probe
                .clone()
                .unwrap_or_else(|| panic!("row {}: oracle carries no probe", case.name));
            for p in want.as_array().expect("probe array") {
                assert_eq!(
                    p["hydratedThrew"],
                    Value::Bool(true),
                    "row {}: v4 plant",
                    case.name
                );
                assert!(p["rawName"].is_string(), "row {}: v4 raw row", case.name);
            }
            let got = Value::Array(
                ids.iter()
                    .map(|group_id| {
                        let hydrated_threw = db
                            .read_main(|main| {
                                db.read_mount_index(|mount| {
                                    Ok(quilltap_core::db::groups::GroupsRepository::new(
                                        main, mount,
                                    )
                                    .find_by_id(group_id)
                                    .is_err())
                                })
                            })
                            .expect("hydrated probe");
                        let raw = db
                            .read_main(|main| {
                                quilltap_core::db::groups::find_name_and_official_mount_point_id_raw(
                                    main, group_id,
                                )
                            })
                            .expect("raw probe");
                        json!({
                            "groupId": group_id,
                            "hydratedThrew": hydrated_threw,
                            "rawName": raw.as_ref().map(|(n, _)| n.clone()),
                            "rawHasOfficialStore": raw.as_ref().is_some_and(|(_, o)| o.is_some()),
                        })
                    })
                    .collect(),
            );
            if bytes(&got) != bytes(&want) {
                case_failures.push(format!("R-F probe\n GOT: {got}\n EXP: {want}"));
            }
        }

        let got = match assemble_core_packet(&db, &case.character_id, case.budget) {
            None => Value::Null,
            Some(p) => json!({
                "files": p.files.iter().map(|f| json!({
                    "path": f.path,
                    "body": f.body,
                    "sourceLabel": f.source_label,
                })).collect::<Vec<_>>(),
                "approxTokens": p.approx_tokens,
            }),
        };
        if bytes(&got) != bytes(&oracle.packet) {
            case_failures.push(format!(
                "packet\n GOT: {}\n EXP: {}",
                bytes(&got),
                bytes(&oracle.packet)
            ));
        }

        if case_failures.is_empty() {
            eprintln!("OK {}", case.name);
        } else {
            eprintln!("RED {}", case.name);
            for f in &case_failures {
                failures.push(format!("row {}: {f}", case.name));
            }
        }
    }
    eprintln!(
        "core_packet_tier2_equivalence: {} rows, {} failure(s)",
        spec.cases.len(),
        failures.len()
    );
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}
