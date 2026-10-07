//! P4.D248 (v4 `e5c6bd0c0`, bug 176) — `/health`'s `structure` service at the
//! web edge: the boot's structural table pass (PHASE 3.1, `quilltap-host`)
//! recorded on the `Host`, reported by `health_parts` exactly as v4's
//! `checkStructuralHealth` + `getOverallStatus` + `getStatusCode` do
//! (`app/api/health/route.ts:107-159`): `healthy` (200) when nothing was
//! recorded; `degraded` (503) with v4's pluralized message and the problems
//! otherwise — the `json` / `fileStorage` services unchanged beside it, and the
//! same `version` / `timestamp` / `uptime` keys the healthy arm carries.
//!
//! The 503 is v4's, ported unchanged (RULED R1 at planning); v4's own UI never
//! reads it, and v5's SPA learns the same carve-out in `interpretHealth` (the
//! P4.D248 ↔ P4.D247 shared contract, an `apps/web` edit this lane does not
//! make). The Tauri `health` command relays `health_parts` verbatim, so the
//! direct `health_parts` assert below is its core too.
//!
//! Red-first (P4.D248's lane record): on `main` the unplanted arm failed on the
//! missing `structure` key and the planted arms answered 200 `healthy`.
//!
//! Run:
//!   cargo test -p quilltap-web --test health_structure

mod common;

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use serde_json::{json, Value};

const MOUNT: &str = "quilltap-mount-index.db";

/// A provisioned Fresh instance (v4's full `generateDDL` shape — every
/// structural table present), with each `sql` run on the mount index COPY.
fn planted_instance(plants: &[&str]) -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, common::TEST_PEPPER).expect("provision");
    let w = Writer::open_writable(&data.join(MOUNT), common::TEST_PEPPER).unwrap();
    for sql in plants {
        w.connection()
            .execute_batch(sql)
            .unwrap_or_else(|e| panic!("plant {sql:?}: {e}"));
    }
    base
}

async fn get_health(plants: &[&str]) -> (u16, Value) {
    let base = planted_instance(plants);
    let (addr, state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let resp = reqwest::get(format!("http://{addr}/health")).await.unwrap();
    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap();

    // The Tauri `health` command's core answers the same status and body.
    let (direct_status, direct_body) = quilltap_web::health::health_parts(&state).await;
    assert_eq!(direct_status.as_u16(), status, "health_parts status");
    assert_eq!(
        direct_body["services"], body["services"],
        "health_parts services"
    );
    drop(base);
    (status, body)
}

fn assert_other_services_healthy(body: &Value) {
    assert_eq!(
        body["services"]["json"],
        json!({ "status": "healthy", "message": "Database engine is operational" })
    );
    assert_eq!(
        body["services"]["fileStorage"],
        json!({ "status": "healthy", "message": "Local file storage operational", "mode": "local" })
    );
    assert!(body["version"].is_string(), "version: {body}");
    assert!(body["timestamp"].is_string(), "timestamp: {body}");
    assert!(body["uptime"].is_number(), "uptime: {body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sound_instance_reports_structure_healthy() {
    let (status, body) = get_health(&[]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "healthy");
    assert_eq!(
        body["services"]["structure"],
        json!({ "status": "healthy", "message": "All structural tables verified" })
    );
    assert_other_services_healthy(&body);
    // v4's service order: json, fileStorage, structure.
    let keys: Vec<&String> = body["services"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["json", "fileStorage", "structure"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn one_damaged_table_answers_503_degraded() {
    let (status, body) = get_health(&[
        "ALTER TABLE doc_mount_chunks RENAME COLUMN headingContext TO headingContext_x",
    ])
    .await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["status"], "degraded");
    assert_eq!(
        body["services"]["structure"],
        json!({
            "status": "degraded",
            "message": "1 damaged table; reads through it answer empty",
            "problems": ["table doc_mount_chunks is missing column headingContext"],
        })
    );
    // v4's key order inside the service: status, message, problems.
    let keys: Vec<&String> = body["services"]["structure"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(keys, ["status", "message", "problems"]);
    assert_other_services_healthy(&body);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_damaged_tables_pluralize() {
    let (status, body) = get_health(&[
        "ALTER TABLE doc_mount_chunks RENAME COLUMN headingContext TO headingContext_x",
        "ALTER TABLE doc_mount_file_links RENAME COLUMN description TO description_x",
    ])
    .await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(
        body["services"]["structure"],
        json!({
            "status": "degraded",
            "message": "2 damaged tables; reads through them answer empty",
            // Container order (`docMountFileLinks` before `docMountChunks`),
            // not plant order.
            "problems": [
                "table doc_mount_file_links is missing column description",
                "table doc_mount_chunks is missing column headingContext",
            ],
        })
    );
}

// ============================================================================
// P4.159 (dogfood #150) — a DEGRADED sibling reaches `/health` through the
// structural pass alone (v4 `94fbb1ae3`: `app/api/health/route.ts` has no
// direct degraded-database check; `verifyStructure` answers `<label> database
// unavailable: <guard sentence>` per repository of that partition). Red-first
// (P4.159's lane record): on unported `main` both arms answered 503
// `unhealthy` with `startupPhase: "failed"` — the boot itself died.
// ============================================================================

/// A fresh instance whose `file` sibling is replaced by the walk's C3 garbage.
async fn get_health_with_garbage(file: &str) -> (u16, Value) {
    let base = planted_instance(&[]);
    let path = base.path().join("data").join(file);
    let garbage: Vec<u8> = (0..4608usize)
        .map(|i| ((i * 131 + 17) & 255) as u8)
        .collect();
    std::fs::write(&path, garbage).unwrap();
    let (addr, state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let resp = reqwest::get(format!("http://{addr}/health")).await.unwrap();
    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap();
    let (direct_status, direct_body) = quilltap_web::health::health_parts(&state).await;
    assert_eq!(direct_status.as_u16(), status, "health_parts status");
    assert_eq!(
        direct_body["services"], body["services"],
        "health_parts services"
    );
    drop(base);
    (status, body)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_degraded_mount_index_answers_503_with_nine_problems() {
    let (status, body) = get_health_with_garbage(MOUNT).await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["status"], "degraded");
    assert_eq!(
        body["services"]["structure"],
        json!({
            "status": "degraded",
            "message": "9 damaged tables; reads through them answer empty",
            "problems": vec!["mount index database unavailable: Mount index database is in degraded mode"; 9],
        })
    );
    assert_other_services_healthy(&body);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_degraded_llm_logs_file_answers_503_with_one_problem() {
    let (status, body) = get_health_with_garbage("quilltap-llm-logs.db").await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["status"], "degraded");
    assert_eq!(
        body["services"]["structure"],
        json!({
            "status": "degraded",
            "message": "1 damaged table; reads through it answer empty",
            "problems": ["LLM logs database unavailable: LLM logs database is in degraded mode"],
        })
    );
    assert_other_services_healthy(&body);
}
