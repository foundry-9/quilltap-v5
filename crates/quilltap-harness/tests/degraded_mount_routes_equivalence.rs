//! P4.166 (dogfood #156 / #157) differential — the DEGRADED MOUNT INDEX at the
//! route surface: the api fns behind v4's routes vs v4's REAL route handlers
//! (`harness/oracle/cases/degraded-mount-routes.test.ts`), both over a FRESH
//! copy of the committed groups-projects pair whose mount-index file is
//! GARBAGE, so each side boots it degraded (v4's client flags
//! `__quilltapMountIndexDegraded`; v5's `Db::open` leaves the partition
//! `Degraded` — P4.159).
//!
//! Each `route` row compares the status, the body (v4's wire body — an error
//! renders `{error}` plus the store-unavailable / validation riders the web
//! edge merges; a success the response's `data`, minted `updatedAt` blanked)
//! and every line in the COMPARED set (both sides, in order): the overlay's
//! quiet `Dedicated database unavailable` DEBUG, v4's context-middleware ERROR
//! for a store-unavailable 503 (v5's [`CoreError::log_store_unavailable`] — the
//! edge's call, made here with the route's bracket), and the route families'
//! own `[Projects v1]` / `[Characters v1]` / `[Groups v1]` /
//! `[WardrobeTransfers v1]` lines. Lines OUTSIDE the set are v4 residue this
//! family does not claim (the repository fallback ERRORs of v4's folder /
//! mount-point repos, the General tier's folder ensure, the middleware's
//! `Unhandled route error` — v5 logs no such line), listed per row by the
//! oracle and recorded in P4.166's lane record.
//!
//! The `guard` rows: `requireMountIndexDb()`'s throw (the text every C4 job
//! failure and every neutral refusal carries) against
//! `db::fallback::error_text(PartitionUnavailable(MountIndex))`, and #157's
//! `Failed to log LLM call` on a GARBAGE llm-logs file against
//! `services::llm_logging::log_llm_call` over a degraded llm-logs partition.
//!
//! Generate the oracle (see the .ts header), then:
//!   QT_ORACLE_DEGRADED_MOUNT_ROUTES=/tmp/oracle-degraded-mount-routes.ndjson \
//!     cargo test -p quilltap-harness --test degraded_mount_routes_equivalence -- --nocapture
//!
//! Skips (does not fail) when the env var is unset.

use std::path::PathBuf;

use quilltap_core::api::inventory_route_factory::{self, CollectionAction};
use quilltap_core::api::types::{ErrorKind, InventoryContainer, InventoryScope, Response};
use quilltap_core::api::{characters, chat_outfits, custom_tools, groups, projects, wardrobe};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde_json::{json, Value};

const KAPPA: &str = "a3000000-0000-4000-8000-000000000003";
const MISSING_PROJECT: &str = "a3000000-0000-4000-8000-0000000000ff";
const GAMMA: &str = "a2000000-0000-4000-8000-000000000001";
const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
const CLEO: &str = "a1000000-0000-4000-8000-000000000003";
const MISSING_CHARACTER: &str = "a1000000-0000-4000-8000-0000000000ff";
const CHAT_A: &str = "c1000000-0000-4000-8000-000000000001";
const TAG: &str = "aa000000-0000-4000-8000-000000000001";
/// The item the planted history row names — no pool can hold it.
const STALE_ITEM: &str = "ab000000-0000-4000-8000-000000000001";

/// The workbench family's `gated` definition (`workbench-route-cases.json`),
/// as the oracle sends it.
fn gated_definition() -> Value {
    json!({
        "name": "gated",
        "title": "The Gated Table",
        "description": "A table that consults the sheet.",
        "roll": { "min": 0.8, "max": 0.8 },
        "outcomes": [
            { "when": { "gt": 0.5, "metadata": { "hasKey": { "eq": true } } }, "message": "The lock turns for {{value}}.", "state": "success" },
            { "when": true, "message": "The lock holds.", "state": "failure" }
        ]
    })
}

/// v4's garbage — the oracle writes the same bytes.
const GARBAGE: &str = "this is not a database";

/// Rows measured at the pin and NOT claimed by this family (each recorded in
/// P4.166's lane record): v4's character delete SUCCEEDS on a degraded mount
/// (its vault clean-up is fail-soft) where v5's delete needs the partition.
///
/// And the merge (156.c): v4 completes it on a degraded mount — a vault-less
/// arrival is compiled, dressed from an empty pool and given its arrival
/// defaults, then INFO `[ChatMerge] Merged character into chat` — where v5's
/// per-character read drops the character on the unavailable partition and
/// v5 logs none of v4's `[ChatMerge]` INFO lines at all (Tier-3 deferral).
const UNCLAIMED_ROWS: &[&str] = &["character_delete", "chat_merge_vaultless"];

/// Rows whose DEBUG-level READ lines (the overlay's quiet `Dedicated database
/// unavailable; answering with the fallback`, a `[WearablePool]` cycle-check
/// load) are not claimed — every INFO / WARN / ERROR compared line still is
/// (recorded in P4.166's lane record): the character update's
/// broken-vault-safe `readCharacterVaultPropertiesForWrite` makes two reads
/// before its write throws, where v5 refuses the write before any read (same
/// 500) — and so does every other neutral-500 row (the General / character
/// tier creates read their folders before the first write throws); the
/// transfers GET's batched list overlays make one read per overlay file per
/// entity batch, where v5's degraded arm reads nothing (same 200).
const READS_BEFORE_THROW_UNCLAIMED: &[&str] = &[
    "character_update",
    "general_wardrobe_create",
    "character_wardrobe_create",
    "character_inventory_create",
    "transfers_destinations",
];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn spec() -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/groups-projects.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn rows() -> Option<Vec<Value>> {
    let Ok(path) = std::env::var("QT_ORACLE_DEGRADED_MOUNT_ROUTES") else {
        eprintln!("SKIP: QT_ORACLE_DEGRADED_MOUNT_ROUTES unset");
        return None;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    // 30 route rows + 2 guard rows: a truncated regen must not pass vacuously.
    assert_eq!(rows.len(), 32, "oracle row count");
    Some(rows)
}

/// The tests of this binary share [`lines`]' one buffer — each holds this
/// for its whole run.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A PROCESS-GLOBAL capture: every swapped route runs inside `Db::write`, on
/// the WRITER thread, which the thread-scoped `captured_with` cannot see
/// (memory note `capture-rig-writer-thread`).
fn lines() -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
    static BUF: std::sync::OnceLock<std::sync::Arc<std::sync::Mutex<Vec<String>>>> =
        std::sync::OnceLock::new();
    BUF.get_or_init(|| {
        use tracing_subscriber::layer::SubscriberExt;
        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry()
            .with(quilltap_core::test_support::CaptureLayer(buf.clone()));
        tracing::subscriber::set_global_default(subscriber).expect("one global capture per binary");
        buf
    })
    .clone()
}

/// A fresh copy of the pair with the mount index GARBAGE (and, with `llm`,
/// a garbage llm-logs sibling too); `plant` runs on the main copy first.
fn degraded_db(pepper: &str, plant: Option<&str>, llm: bool) -> (Db, tempfile::TempDir) {
    let scratch = tempfile::Builder::new()
        .prefix("qt-degraded-mount-")
        .tempdir()
        .unwrap();
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    let llm_logs = scratch.path().join("llm.db");
    std::fs::copy(fixtures_dir().join("groups-projects-main.db"), &main).unwrap();
    if let Some(sql) = plant {
        let w = quilltap_core::db::Writer::open_writable(&main, pepper).unwrap();
        w.connection().execute_batch(sql).unwrap();
    }
    std::fs::write(&mount, GARBAGE).unwrap();
    if llm {
        std::fs::write(&llm_logs, GARBAGE).unwrap();
    }
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: llm.then_some(llm_logs),
        },
        pepper,
    )
    .expect("open db");
    (db, scratch)
}

fn status_of(r: &Response) -> i64 {
    match r {
        Response::Error(e) => match e.kind {
            ErrorKind::BadRequest => 400,
            ErrorKind::Unauthorized => 401,
            ErrorKind::Forbidden => 403,
            ErrorKind::NotFound => 404,
            ErrorKind::Conflict => 409,
            ErrorKind::Unprocessable => 422,
            ErrorKind::Locked | ErrorKind::Unavailable => 503,
            ErrorKind::Internal => 500,
        },
        _ => 200,
    }
}

/// v4's wire body: an error is `{error}` + the riders the web edge merges
/// (`dispatch::merge_error_wire`); a success is the response's `data`.
fn body_of(r: &Response) -> Value {
    match r {
        Response::Error(e) => {
            let mut body = json!({ "error": e.message });
            for rider in [e.unavailable_wire_body(), e.validation_wire_body()]
                .into_iter()
                .flatten()
            {
                if let Value::Object(m) = rider {
                    for (k, v) in m {
                        body[k] = v;
                    }
                }
            }
            body
        }
        other => {
            let v = serde_json::to_value(other).unwrap();
            v.get("data").cloned().unwrap_or(Value::Null)
        }
    }
}

/// Minted `updatedAt` / `createdAt` (a success echo) blanked on both sides.
fn blank_minted(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (k, val) in m.iter_mut() {
                if k == "updatedAt" && val.is_string() {
                    *val = json!("<minted>");
                } else {
                    blank_minted(val);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(blank_minted),
        _ => {}
    }
}

/// Is this line one the family compares? (See the module doc.) `text` is the
/// message, or a rendered line that STARTS with its message.
fn compared(text: &str) -> bool {
    const MIDDLEWARE: [&str; 3] = [
        "] Project document store unavailable",
        "] Group document store unavailable",
        "] Character vault unavailable",
    ];
    text.starts_with("Dedicated database unavailable; answering with the fallback")
        || (text.starts_with("[/")
            || text.starts_with("[GET ")
            || text.starts_with("[PUT ")
            || text.starts_with("[POST ")
            || text.starts_with("[DELETE "))
            && MIDDLEWARE.iter().any(|m| text.contains(m))
        || text.starts_with("[Projects v1]")
        || text.starts_with("[Groups v1]")
        || text.starts_with("[Characters v1]")
        || text.starts_with("[WardrobeTransfers v1]")
        || text.starts_with("[Chats v1]")
        || text.starts_with("[WearablePool]")
        || text.starts_with("Failed to log LLM call")
}

/// v4's line as `LEVEL message k=v … error=<text>` (the `error` context lifted
/// by the oracle rendered LAST, as v5 renders a trailing `error` field).
fn v4_line(line: &Value) -> String {
    let mut out = format!(
        "{} {}",
        line["level"].as_str().unwrap().to_uppercase(),
        line["message"].as_str().unwrap()
    );
    for kv in line["fields"].as_array().unwrap() {
        out.push_str(&format!(
            " {}={}",
            kv[0].as_str().unwrap(),
            kv[1].as_str().unwrap()
        ));
    }
    if let Some(e) = line["error"].as_str() {
        out.push_str(&format!(" error={e}"));
    }
    out
}

/// v5's captured line (`LEVEL target message k=v …`) as `LEVEL message k=v …`:
/// the target dropped, `errorJson` (a context-string error) decoded into
/// `error`.
fn v5_line(line: &str) -> (String, String) {
    let (level, rest) = line.split_once(' ').unwrap();
    let (_target, rest) = rest.split_once(' ').unwrap();
    let mut out = format!("{level} {rest}");
    if let Some(at) = out.find(" errorJson=") {
        let raw = &out[at + " errorJson=".len()..];
        let text: String = serde_json::from_str(raw).unwrap_or_else(|_| raw.to_string());
        out = format!("{} error={text}", &out[..at]);
    }
    (rest.to_string(), out)
}

fn v4_compared(row: &Value) -> Vec<String> {
    row["logs"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|l| compared(l["message"].as_str().unwrap()))
                .map(v4_line)
                .collect()
        })
        .unwrap_or_default()
}

fn v5_compared(captured: &[String]) -> Vec<String> {
    captured
        .iter()
        .filter_map(|l| {
            let (rest, rendered) = v5_line(l);
            compared(&rest).then_some(rendered)
        })
        .collect()
}

/// Run one route row on a fresh degraded copy, returning the response and
/// every line it logged (the middleware ERROR included, logged as the web
/// edge does with the route's bracket).
async fn drive(row: &Value, pepper: &str, user_id: &str) -> (Response, Vec<String>) {
    let name = row["name"].as_str().unwrap();
    // The row's plant, echoed by the oracle — the SAME SQL both sides.
    let (db, _dir) = degraded_db(pepper, row["plant"].as_str(), false);
    lines().lock().unwrap().clear();
    let (route, r) = match name {
        "project_add_chat" => (
            format!("POST /api/v1/projects/{KAPPA}"),
            projects::project_chat_add(&db, KAPPA, CHAT_A).await,
        ),
        "project_add_chat_missing" => (
            format!("POST /api/v1/projects/{MISSING_PROJECT}"),
            projects::project_chat_add(&db, MISSING_PROJECT, CHAT_A).await,
        ),
        "group_update" => (
            format!("PUT /api/v1/groups/{GAMMA}"),
            groups::group_update(&db, GAMMA, json!({ "name": "Gamma II" })).await,
        ),
        "character_favorite" => (
            format!("POST /api/v1/characters/{ARIA}"),
            characters::character_favorite(&db, user_id, ARIA).await,
        ),
        "character_favorite_vaultless" => (
            format!("POST /api/v1/characters/{CLEO}"),
            characters::character_favorite(&db, user_id, CLEO).await,
        ),
        "character_favorite_missing" => (
            format!("POST /api/v1/characters/{MISSING_CHARACTER}"),
            characters::character_favorite(&db, user_id, MISSING_CHARACTER).await,
        ),
        "character_update" => (
            format!("PUT /api/v1/characters/{ARIA}"),
            characters::character_update(&db, user_id, ARIA, json!({ "title": "The Bold" })).await,
        ),
        "character_add_tag" => (
            format!("POST /api/v1/characters/{ARIA}"),
            characters::character_add_tag(&db, user_id, ARIA, TAG).await,
        ),
        "character_prompt_create" => (
            format!("POST /api/v1/characters/{ARIA}/prompts"),
            characters::character_prompt_create(&db, user_id, ARIA, "Terse", "Be brief.", false)
                .await,
        ),
        "project_delete" => (
            format!("DELETE /api/v1/projects/{KAPPA}"),
            projects::project_delete(&db, KAPPA).await,
        ),
        "group_add_member" => (
            format!("POST /api/v1/groups/{GAMMA}"),
            groups::group_member_add(&db, GAMMA, ARIA).await,
        ),
        "project_wardrobe_list" => (
            format!("GET /api/v1/projects/{KAPPA}/wardrobe"),
            projects::project_wardrobe_list(&db, KAPPA, false).await,
        ),
        "general_wardrobe_list" => (
            "GET /api/v1/wardrobe".to_string(),
            wardrobe::wardrobe_list(&db, false).await,
        ),
        "general_wardrobe_create" => (
            "POST /api/v1/wardrobe".to_string(),
            wardrobe::wardrobe_create(&db, json!({ "title": "Rain Boots", "types": ["footwear"] }))
                .await,
        ),
        "project_inventory_create" => (
            format!("POST /api/v1/projects/{KAPPA}/inventory"),
            inventory_route_factory::collection_action(
                &db,
                CollectionAction::Create {
                    project_id: None,
                    chat_id: None,
                    body: json!({ "name": "Brass Lantern" }),
                },
                &InventoryContainer {
                    scope: InventoryScope::Project,
                    id: Some(KAPPA.to_string()),
                },
                user_id,
            )
            .await,
        ),
        "character_inventory_create" => (
            format!("POST /api/v1/characters/{ARIA}/inventory"),
            inventory_route_factory::collection_action(
                &db,
                CollectionAction::Create {
                    project_id: None,
                    chat_id: None,
                    body: json!({ "name": "Brass Lantern" }),
                },
                &InventoryContainer {
                    scope: InventoryScope::Character,
                    id: Some(ARIA.to_string()),
                },
                user_id,
            )
            .await,
        ),
        "general_inventory_list" => (
            "GET /api/v1/inventory".to_string(),
            inventory_route_factory::collection_action(
                &db,
                CollectionAction::List {
                    include_archived: false,
                    shared: false,
                    project_id: None,
                    chat_id: None,
                },
                &InventoryContainer {
                    scope: InventoryScope::General,
                    id: None,
                },
                user_id,
            )
            .await,
        ),
        "group_wardrobe_create" => (
            format!("POST /api/v1/groups/{GAMMA}/wardrobe"),
            groups::group_wardrobe_create(
                &db,
                GAMMA,
                json!({ "title": "Rain Boots", "types": ["footwear"] }),
            )
            .await,
        ),
        "character_wardrobe_create" => (
            format!("POST /api/v1/characters/{ARIA}/wardrobe"),
            characters::character_wardrobe_create(
                &db,
                user_id,
                ARIA,
                json!({ "title": "Rain Boots", "types": ["footwear"] }),
            )
            .await,
        ),
        "project_scenarios_list" => (
            format!("GET /api/v1/projects/{KAPPA}/scenarios"),
            projects::project_scenario_list(&db, KAPPA, false, user_id).await,
        ),
        "project_create" => (
            "POST /api/v1/projects".to_string(),
            projects::project_create(&db, json!({ "name": "Ember" })).await,
        ),
        "character_create" => (
            "POST /api/v1/characters".to_string(),
            characters::character_create(&db, user_id, json!({ "name": "Fen" })).await,
        ),
        "chat_wardrobe_history" => (
            format!("GET /api/v1/chats/{CHAT_A}"),
            chat_outfits::chat_wardrobe_history_get(&db, CHAT_A, Some(ARIA)),
        ),
        "chat_equip_wear" => (
            format!("POST /api/v1/chats/{CHAT_A}"),
            chat_outfits::chat_equip(
                &db,
                user_id,
                CHAT_A,
                json!({ "characterId": ARIA, "mode": "wear", "itemId": STALE_ITEM }),
            )
            .await,
        ),
        "chat_wardrobe_rollback" => (
            format!("POST /api/v1/chats/{CHAT_A}"),
            chat_outfits::chat_wardrobe_rollback(
                &db,
                user_id,
                CHAT_A,
                json!({ "characterId": ARIA, "index": 0 }),
            )
            .await,
        ),
        "workbench_audit_seated" | "workbench_audit_seated_vaultless" => {
            let seated = if name == "workbench_audit_seated" {
                ARIA
            } else {
                CLEO
            };
            (
                "POST /api/v1/custom-tools".to_string(),
                custom_tools::custom_tool_audit(
                    &db,
                    &gated_definition(),
                    None,
                    Some(&json!({ "characterId": seated })),
                    None,
                    None,
                    None,
                    0,
                ),
            )
        }
        "transfers_destinations" => (
            "GET /api/v1/wardrobe/transfers".to_string(),
            wardrobe::wardrobe_transfer_destinations(&db, user_id),
        ),
        other => panic!("no v5 driver for row {other}"),
    };
    if let Response::Error(e) = &r {
        e.log_store_unavailable(&route);
    }
    let captured = lines().lock().unwrap().clone();
    (r, captured)
}

#[tokio::test]
async fn degraded_mount_routes_match_oracle() {
    let _serial = SERIAL.lock().await;
    let _ = lines();
    let Some(rows) = rows() else { return };
    let spec = spec();
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let user_id = spec["userId"].as_str().unwrap().to_string();
    let mut failures = Vec::new();
    for row in rows.iter().filter(|r| r["kind"] == "route") {
        let name = row["name"].as_str().unwrap();
        if UNCLAIMED_ROWS.contains(&name) {
            continue;
        }
        let (r, captured) = drive(row, &pepper, &user_id).await;
        let (status, mut body) = (status_of(&r), body_of(&r));
        let mut want_body = row["body"].clone();
        blank_minted(&mut body);
        blank_minted(&mut want_body);
        if status != row["status"].as_i64().unwrap() || body != want_body {
            failures.push(format!(
                "{name}: v5 {status} {body}\n      v4 {} {want_body}",
                row["status"]
            ));
        }
        let (mut want, mut got) = (v4_compared(row), v5_compared(&captured));
        if READS_BEFORE_THROW_UNCLAIMED.contains(&name) {
            want.retain(|l| !l.starts_with("DEBUG "));
            got.retain(|l| !l.starts_with("DEBUG "));
        }
        if want != got {
            failures.push(format!(
                "{name} lines:\n      v4 {want:#?}\n      v5 {got:#?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} row(s) diverge:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// The guard sentence every neutral refusal and every C4 job failure carries.
#[test]
fn guard_sentence_is_v4s() {
    let Some(rows) = rows() else { return };
    let row = rows
        .iter()
        .find(|r| r["name"] == "mount_index_guard")
        .unwrap();
    assert_eq!(
        quilltap_core::db::fallback::error_text(&quilltap_core::db::DbError::PartitionUnavailable(
            quilltap_core::write_partition::WriteDbTarget::MountIndex
        )),
        row["error"].as_str().unwrap()
    );
}

/// Dogfood #157: `Failed to log LLM call` on a DEGRADED llm-logs partition —
/// v4's line (keys, order, the guard sentence as a context string) against
/// `log_llm_call`'s over the same garbage sibling. v4's two repository ERRORs
/// before it (`Error creating entity`, `Error creating LLM log`) are outside
/// the compared set (v5's llm-logs repository logs neither — recorded).
#[tokio::test]
async fn failed_llm_log_line_is_v4s() {
    let _serial = SERIAL.lock().await;
    use quilltap_core::services::llm_logging::{
        log_llm_call, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage, LogResponse,
    };
    let _ = lines();
    let Some(rows) = rows() else { return };
    let row = rows
        .iter()
        .find(|r| r["name"] == "llm_log_degraded")
        .unwrap();
    let spec = spec();
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let user_id = spec["userId"].as_str().unwrap();
    let (db, _dir) = degraded_db(pepper, None, true);
    let params = LogLlmCallParams {
        user_id: user_id.to_string(),
        log_type: "CHAT_MESSAGE".to_string(),
        message_id: None,
        chat_id: None,
        character_id: None,
        provider: "OPENAI".to_string(),
        model_name: "gpt-4o-mini".to_string(),
        connection_profile_id: None,
        image_profile_id: None,
        request: LogRequest {
            messages: vec![LogRequestMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
                attachments: None,
            }],
            ..LogRequest::default()
        },
        response: LogResponse {
            content: "Hi".to_string(),
            ..LogResponse::default()
        },
        usage: None,
        cache_usage: None,
        raw_provider_usage: None,
        request_hashes: None,
        duration_ms: None,
    };
    lines().lock().unwrap().clear();
    let written = log_llm_call(&db, params, &LogContext::none()).await;
    let captured = lines().lock().unwrap().clone();
    assert_eq!(written, None, "v4 answers null");
    assert_eq!(row["returned"], Value::Null);
    assert_eq!(v5_compared(&captured), v4_compared(row));
    assert_eq!(v4_compared(row).len(), 1, "exactly one compared line");
}
