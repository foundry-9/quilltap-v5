//! P4.D288 **contract C2** guard + differential — the `83334b9a5` round's wire.
//!
//! 1. **Shapes** (no oracle): each of the eight NEW variants decodes from its
//!    camelCase `type`; `chatSetScenario` carries the two new RAW fields
//!    (`scenarioRef`, `reload`); `memoryGenerateEmbeddings` IGNORES a
//!    `batchSize` key (Zod strips it — `af48f6cc2`).
//! 2. **The edge** (a served instance over the committed memories pair): each
//!    of the ten DELETED `type` strings answers `dispatch.rs`'s
//!    `Invalid request: unknown variant …` 400 (R-G); each of the six refusal
//!    arms answers the agreed sentence `The '<action>' action is recognized but
//!    not yet available (<owner>).` (§R.14, a 500 `Internal`).
//! 3. **The differential** (`QT_ORACLE_P4D288_WIRE`, v4's REAL route handlers at
//!    the pin over the SAME fixture — `harness/oracle/cases/
//!    p4-d288-wire-contract.test.ts`): the two FILLED maintenance-config arms
//!    (status, body bytes, the stored settings after; a Zod 400 as v4's
//!    `{error, details}` — the issue bytes compared whole) and the INFO
//!    `[Memories API] Memory maintenance settings updated` by PRESENCE (its
//!    bytes are P4.D297's pin, §R.5); and `system/tools` `memory-dedup-preview`
//!    / `memory-dedup` answering v4's own `Unknown action` envelope with the
//!    pin's lists (status + body bytes).
//!
//! Regenerate + run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-p4d288-wire-oracle
//!   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
//!   cd ~/source/quilltap-server
//!   rm -f /tmp/qt-p4d288-wire.ndjson
//!   QT_FIXTURE_MEM_MAIN=$V5W/crates/quilltap-web/tests/fixtures/memories-main.db \
//!   QT_FIXTURE_MEM_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/memories-mount.db \
//!   QT_ORACLE_OUT=/tmp/qt-p4d288-wire.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$M/cases" -- p4-d288-wire-contract
//!   cd $V5W
//!   QT_ORACLE_P4D288_WIRE=/tmp/qt-p4d288-wire.ndjson \
//!     cargo test -p quilltap-web --test p4_d288_wire_contract -- --nocapture

mod common;

use quilltap_core::api::memory_maintenance_config::{
    memory_maintenance_config_get, memory_maintenance_config_set,
};
use quilltap_core::api::types::{ErrorKind, Request, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::test_support::global_capture;
use serde_json::{json, Value};

fn decode(body: Value) -> Request {
    serde_json::from_value(body.clone()).unwrap_or_else(|e| panic!("decode {body}: {e}"))
}

/// The ten verbs R-G deleted, by their wire `type`.
const DELETED: [&str; 10] = [
    "memoryHousekeepPreview",
    "memoryHousekeep",
    "memoryHousekeepSweep",
    "memoryHousekeepingConfigGet",
    "memoryHousekeepingConfigSet",
    "memoryExtractionConcurrencyGet",
    "memoryExtractionConcurrencySet",
    "memoryConsolidate",
    "memoryDedupPreview",
    "memoryDedupRun",
];

#[test]
fn the_new_variants_decode_from_camel_case() {
    assert_eq!(
        decode(json!({"type":"memoryMaintain","options":{"characterId":"c1","dryRun":true}})),
        Request::MemoryMaintain {
            options: json!({"characterId":"c1","dryRun":true})
        }
    );
    assert_eq!(
        decode(json!({"type":"memoryMaintenanceStatus","characterId":""})),
        Request::MemoryMaintenanceStatus {
            character_id: Some(String::new())
        },
        "a present-empty characterId stays Some(\"\") (v4 reads it as all)"
    );
    assert_eq!(
        decode(json!({"type":"memoryMaintenanceStatus"})),
        Request::MemoryMaintenanceStatus { character_id: None }
    );
    assert_eq!(
        decode(json!({"type":"memoryRepair","options":{"dryRun":true}})),
        Request::MemoryRepair {
            options: json!({"dryRun":true})
        }
    );
    assert_eq!(
        decode(json!({"type":"memoryMaintenanceConfigGet"})),
        Request::MemoryMaintenanceConfigGet
    );
    assert_eq!(
        decode(json!({"type":"memoryMaintenanceConfigSet","config":{"enabled":true}})),
        Request::MemoryMaintenanceConfigSet {
            config: json!({"enabled":true})
        }
    );
    assert_eq!(
        decode(json!({"type":"scenarioEverywhere","includeArchived":true})),
        Request::ScenarioEverywhere {
            include_archived: true
        }
    );
    assert_eq!(
        decode(json!({"type":"scenarioEverywhere"})),
        Request::ScenarioEverywhere {
            include_archived: false
        }
    );
    assert_eq!(
        decode(json!({"type":"scenarioReadByRef","mountPointId":"m1","path":"Scenarios/a.md"})),
        Request::ScenarioReadByRef {
            mount_point_id: Some("m1".into()),
            path: Some("Scenarios/a.md".into())
        }
    );
    assert_eq!(
        decode(json!({"type":"scenarioReadByRef"})),
        Request::ScenarioReadByRef {
            mount_point_id: None,
            path: None
        }
    );
    assert_eq!(
        decode(json!({"type":"chatGetScenarioSource","chatId":"c1"})),
        Request::ChatGetScenarioSource {
            chat_id: "c1".into()
        }
    );
    match decode(json!({
        "type":"chatSetScenario","chatId":"c1",
        "scenarioRef":{"mountPointId":"m1","path":"Scenarios/a.md"},"reload":true
    })) {
        Request::ChatSetScenario {
            scenario_ref,
            reload,
            ..
        } => {
            assert_eq!(
                scenario_ref,
                Some(json!({"mountPointId":"m1","path":"Scenarios/a.md"}))
            );
            assert_eq!(reload, Some(Some(json!(true))));
        }
        other => panic!("chatSetScenario decoded to {other:?}"),
    }
    // P4.171 C-11: `reload` is three-state — an explicit `null` decodes
    // PRESENT (`Some(None)`, which the floor refuses as v4's 400), distinct
    // from an absent key (`None`).
    for (body, want) in [
        (
            json!({"type":"chatSetScenario","chatId":"c1","reload":null}),
            Some(None),
        ),
        (json!({"type":"chatSetScenario","chatId":"c1"}), None),
    ] {
        match decode(body) {
            Request::ChatSetScenario { reload, .. } => assert_eq!(reload, want),
            other => panic!("chatSetScenario decoded to {other:?}"),
        }
    }
    assert_eq!(
        decode(json!({"type":"memoryGenerateEmbeddings","characterId":"c1","batchSize":7})),
        Request::MemoryGenerateEmbeddings {
            character_id: "c1".into()
        },
        "a batchSize key is ignored (Zod strips it)"
    );
}

/// The memories pair, materialized as an instance dir (main + mount + a fresh
/// llm-logs partition).
fn memories_instance() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    for (from, to) in [
        ("memories-main.db", "quilltap.db"),
        ("memories-mount.db", "quilltap-mount-index.db"),
    ] {
        std::fs::copy(common::fixtures_dir().join(from), data.join(to)).unwrap();
    }
    let _ = Writer::open_writable(&data.join("quilltap-llm-logs.db"), common::TEST_PEPPER).unwrap();
    base
}

#[tokio::test(flavor = "multi_thread")]
async fn the_edge_answers_the_deleted_verbs_and_the_refusals() {
    let base = memories_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let dispatch = |body: Value| {
        let client = client.clone();
        async move {
            let resp = client
                .post(format!("http://{addr}/api/dispatch"))
                .header("content-type", "application/json")
                .body(body.to_string())
                .send()
                .await
                .unwrap();
            let status = resp.status().as_u16();
            (status, resp.json::<Value>().await.unwrap())
        }
    };
    for t in DELETED {
        let (status, body) = dispatch(json!({ "type": t })).await;
        assert_eq!(status, 400, "{t}: {body}");
        let message = body["data"]["message"].as_str().unwrap_or_default();
        assert!(
            message.starts_with(&format!("Invalid request: unknown variant `{t}`")),
            "{t}: {message}"
        );
    }
    // P4.D297 FILLED `maintain` / `maintenance-status` / `repair` (their bytes
    // are `memory_maintenance_routes_equivalence`'s) and P4.D295 FILLED
    // `scenarioEverywhere` / `scenarioReadByRef` / `chatGetScenarioSource`
    // (`scenarios_routes_equivalence` / `chat_scenario_routes_equivalence`):
    // no `not yet available` refusal row is left at the `83334b9a5`
    // unification — every verb the keystone added answers through its owner.
}

fn status_and_body(resp: &Response) -> (u16, Value) {
    match resp {
        Response::Memory(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::NotFound => 404,
                _ => 500,
            };
            let mut body = json!({ "error": e.message });
            if let Some(d) = &e.details {
                body["details"] = (**d).clone();
            }
            (status, body)
        }
        other => panic!("unexpected response {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_filled_config_arms_and_the_retired_tools_pair_match_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_P4D288_WIRE") else {
        eprintln!("SKIP: set QT_ORACLE_P4D288_WIRE to the oracle NDJSON (see header).");
        return;
    };
    let rows: Vec<Value> = std::fs::read_to_string(&path)
        .expect("oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let mut failures = Vec::new();
    let (mut config_rows, mut tools_rows) = (0usize, 0usize);
    for row in &rows {
        let name = row["name"].as_str().unwrap();
        let req = &row["request"];
        let (status, body) = if req["route"] == "memories" {
            config_rows += 1;
            let base = memories_instance();
            let data = base.path().join("data");
            {
                let w =
                    Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER).unwrap();
                w.connection()
                    .execute(
                        "DELETE FROM instance_settings WHERE key = 'memoryMaintenance'",
                        [],
                    )
                    .unwrap();
                if let Some(stored) = req["stored"].as_str() {
                    w.connection()
                        .execute(
                            "INSERT INTO instance_settings (key, value) VALUES ('memoryMaintenance', ?1)",
                            [stored],
                        )
                        .unwrap();
                }
            }
            let db = Db::open(
                DbPaths {
                    main: data.join("quilltap.db"),
                    mount_index: Some(data.join("quilltap-mount-index.db")),
                    llm_logs: None,
                },
                common::TEST_PEPPER,
            )
            .unwrap();
            let (resp, lines) = global_capture::capture_async(async {
                if req["method"] == "GET" {
                    memory_maintenance_config_get(&db)
                } else {
                    memory_maintenance_config_set(
                        &db,
                        "ffffffff-ffff-ffff-ffff-ffffffffffff",
                        req["body"].clone(),
                    )
                    .await
                }
            })
            .await;
            let stored = db
                .read_main(|c| {
                    Ok(quilltap_core::db::instance_settings::get_memory_maintenance_settings(c))
                })
                .unwrap()
                .to_json();
            if serde_json::to_string(&stored).unwrap()
                != serde_json::to_string(&row["storedAfter"]).unwrap()
            {
                failures.push(format!(
                    "{name}: stored after\n  v5 {stored}\n  v4 {}",
                    row["storedAfter"]
                ));
            }
            let want_info = row["infoTags"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t == "[Memories API] Memory maintenance settings updated");
            let got_info = lines
                .iter()
                .any(|l| l.contains("[Memories API] Memory maintenance settings updated"));
            if want_info != got_info {
                failures.push(format!(
                    "{name}: INFO presence v5 {got_info} v4 {want_info}"
                ));
            }
            status_and_body(&resp)
        } else {
            tools_rows += 1;
            let base = memories_instance();
            let (addr, _state) = common::serve_instance(base.path(), |mut c| {
                c.terminal = false;
                c
            })
            .await;
            let url = format!(
                "http://{addr}/api/v1/system/tools?action={}",
                req["action"].as_str().unwrap()
            );
            let client = reqwest::Client::new();
            let resp = if req["method"] == "GET" {
                client.get(url).send().await.unwrap()
            } else {
                client
                    .post(url)
                    .header("content-type", "application/json")
                    .body(req["body"].to_string())
                    .send()
                    .await
                    .unwrap()
            };
            let status = resp.status().as_u16();
            (status, resp.json::<Value>().await.unwrap())
        };
        if u64::from(status) != row["status"].as_u64().unwrap() {
            failures.push(format!("{name}: status v5 {status} v4 {}", row["status"]));
        }
        if serde_json::to_string(&body).unwrap() != serde_json::to_string(&row["body"]).unwrap() {
            failures.push(format!("{name}: body\n  v5 {body}\n  v4 {}", row["body"]));
        }
    }
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!((config_rows, tools_rows), (7, 2), "the corpus shrank");
    eprintln!("OK: p4_d288 wire matched v4 ({config_rows} config rows, {tools_rows} tools rows).");
}
