//! P4.42 wire proof (NOT a differential — this lane ports no new v4 behavior).
//!
//! Two facts the two web-search DIFFERENTIALS (`web_search_wire_equivalence`,
//! `web_search_tool_equivalence`) cannot see, because they call the provider /
//! `execute_web_search` directly and never go through the production wiring:
//!
//!   1. A [`BuiltInToolRunner`] built the way the host builds it — `::new` then
//!      `.with_web_search_provider(RealWebSearchProvider)` — actually dispatches
//!      `search_web` through that provider (canned transport, no spend); and the
//!      DEFAULT runner (no provider wired) still answers v4's "not configured"
//!      bytes. This is the dogfood finding: before this lane the runner was
//!      always the default and every `search_web` refused.
//!
//!   2. The consistency pin: the tools inventory's `web_search_configured` and
//!      the runner's actual `search_web` outcome cannot disagree, because both
//!      derive from the SAME `Option<Arc<dyn WebSearchProvider>>`. Built both
//!      ways (Some / None) the inventory row's availability and the runner's
//!      outcome agree in each.
//!
//! No oracle, no env var — pure Rust wiring proof.

use std::path::PathBuf;
use std::sync::Arc;

use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::db::{chats, connection_profiles};
use quilltap_core::model::wire::{wire_key, CannedSyncWireTransport, WireResponse};
use quilltap_core::services::tool_execution::{create_tool_context, ToolCall, ToolRunner};
use quilltap_core::services::tools_inventory::tools_list;
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use quilltap_core::tools::web_search::{
    build_serper_request, NoSearchApiKeys, RealWebSearchProvider, WebSearchProvider,
};
use serde_json::{json, Value};

/// Synthetic test pepper (harness corpus; never a real key).
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "user-1";

/// The committed fixtures (harness reads them from the web crate's dir).
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn fixture_env() -> SelfInventoryEnv {
    SelfInventoryEnv {
        version: "0.0.0-fixture".to_string(),
        runtime_mode: "desktop".to_string(),
        client_shell: ClientShell::Unknown,
        mount_index_degraded: false,
        release_notes: None,
        changelog: None,
        model_info: Vec::new(),
        fallback_pricing: Vec::new(),
        registry_default_context: 8192,
    }
}

/// A fresh, EMPTY main-DB `Db` (the `search_web` handler never touches the DB, so
/// the empty schema is enough for the runner tests). `tag` disambiguates the
/// scratch dir per test.
fn fresh_db(tag: &str) -> (Db, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join(format!("{tag}-main.db"));
    // A writable open provisions the schema; drop it before the read `Db`.
    drop(Writer::open_writable(&main, PEPPER).unwrap());
    let db = Db::open(
        DbPaths {
            main,
            mount_index: None,
            llm_logs: None,
        },
        PEPPER,
    )
    .unwrap();
    (db, dir)
}

/// The Serper endpoint (`build_serper_request` default URL) — the key the canned
/// transport is registered under (base_url override is `None` here).
fn canned_serper_transport(query: &str, max_results: i64, body: &str) -> CannedSyncWireTransport {
    let req = build_serper_request(query, max_results, "k");
    CannedSyncWireTransport::new().with_raw_response(
        wire_key(&req.method, &req.url, &req.body_string()),
        WireResponse::new(200, body),
    )
}

fn search_web_call(query: &str) -> ToolCall {
    ToolCall {
        name: "search_web".to_string(),
        arguments: json!({ "query": query }),
        call_id: None,
    }
}

/// v4's "not configured" error bytes (the runner's default answer).
const NOT_CONFIGURED: &str =
    "Web search is not configured. Please add a search provider API key in Settings > API Keys.";

#[tokio::test]
async fn runner_with_provider_executes_search_web() {
    let (db, _dir) = fresh_db("with-provider");
    let ctx = create_tool_context(
        "chat-1", USER, "char-1", "pp-1", None, None, None, None, None,
    );

    // Built exactly as the host builds it: RealWebSearchProvider over the env-key
    // fallback path (serper_registered=false), wrapped in an Arc, chained in.
    let body = r#"{"organic":[{"title":"Lighthouse lore","link":"https://example.com/l","snippet":"A snippet."}]}"#;
    let provider: Arc<dyn WebSearchProvider> = Arc::new(RealWebSearchProvider::new(
        canned_serper_transport("beacons", 5, body),
        NoSearchApiKeys,
        false,
        Some("k".to_string()),
    ));
    let runner = BuiltInToolRunner::new(db, fixture_env()).with_web_search_provider(provider);

    let out = runner.run(&search_web_call("beacons"), &ctx).await;
    assert!(out.success, "the wired provider must execute the search");
    // v4's success shape: `{ formattedText, results, totalFound, query }`.
    let formatted = out
        .result
        .get("formattedText")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        formatted.contains("Found 1 search results:"),
        "formatted: {formatted}"
    );
    assert!(
        formatted.contains("Lighthouse lore"),
        "formatted: {formatted}"
    );
    assert_eq!(
        out.result.get("totalFound").and_then(Value::as_i64),
        Some(1)
    );
}

#[tokio::test]
async fn runner_without_provider_refuses_not_configured() {
    let (db, _dir) = fresh_db("no-provider");
    let ctx = create_tool_context(
        "chat-1", USER, "char-1", "pp-1", None, None, None, None, None,
    );

    // The DEFAULT runner (no `.with_web_search_provider`) — the pre-lane state.
    let runner = BuiltInToolRunner::new(db, fixture_env());
    let out = runner.run(&search_web_call("beacons"), &ctx).await;
    assert!(!out.success);
    assert_eq!(
        out.error.as_deref(),
        Some(NOT_CONFIGURED),
        "today's not-configured bytes"
    );
}

// ---------------------------------------------------------------------------
// The consistency pin (tier-1 item 5)
// ---------------------------------------------------------------------------

/// Seed a chat whose active character's connection profile ENABLES web search,
/// so the tools-inventory `search_web` gate reaches the `web_search_configured`
/// branch (rather than the profile `allowWebSearch` branch).
fn seed_web_search_chat(db_main: &std::path::Path) {
    let w = Writer::open_writable(db_main, PEPPER).unwrap();
    let conn = w.connection();

    let cp = connection_profiles::CpCreate {
        user_id: USER.to_string(),
        name: "Web-enabled".to_string(),
        provider: "OPENAI_COMPATIBLE".to_string(),
        transport: "api".to_string(),
        courier_delta_mode: false,
        api_key_id: None,
        base_url: None,
        model_name: "mock-model".to_string(),
        parameters: json!({}),
        is_default: true,
        is_cheap: false,
        allow_web_search: true,
        use_native_web_search: false,
        allow_tool_use: true,
        pseudo_tool_mode: "auto".to_string(),
        multi_character_prefill: None,
        model_class: None,
        max_context: None,
        max_tokens: None,
        is_dangerous_compatible: false,
        supports_image_upload: false,
        tags: Vec::new(),
        sort_index: 0.0,
        total_tokens: 0.0,
        total_prompt_tokens: 0.0,
        total_completion_tokens: 0.0,
        message_count: 0.0,
    };
    connection_profiles::ConnectionProfilesRepository::new(conn)
        .create(
            &cp,
            &connection_profiles::CreateOptions {
                id: "cp-1".to_string(),
                created_at: "2026-01-01T00:00:00.000Z".to_string(),
                updated_at: "2026-01-01T00:00:00.000Z".to_string(),
            },
        )
        .unwrap();

    let chat: chats::ChatCreate = serde_json::from_value(json!({
        "userId": USER,
        "title": "Web search chat",
        "participants": [{
            "id": "part-1",
            "type": "CHARACTER",
            "characterId": "char-1",
            "isActive": true,
            "connectionProfileId": "cp-1",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-01-01T00:00:00.000Z"
        }]
    }))
    .unwrap();
    chats::ChatsRepository::new(conn)
        .create(
            &chat,
            &chats::CreateOptions {
                id: "chat-1".to_string(),
                created_at: "2026-01-01T00:00:00.000Z".to_string(),
                updated_at: "2026-01-01T00:00:00.000Z".to_string(),
            },
        )
        .unwrap();
    drop(w);
}

/// The `search_web` row's `(available, unavailableReason)` from a `tools_list`
/// response.
fn search_web_row(resp: &Response) -> (bool, Option<String>) {
    let body = match resp {
        Response::ToolsInventory(v) => v,
        other => panic!("tools_list not ToolsInventory: {other:?}"),
    };
    let tools = body
        .get("tools")
        .and_then(Value::as_array)
        .expect("tools array");
    let row = tools
        .iter()
        .find(|t| t.get("id").and_then(Value::as_str) == Some("search_web"))
        .expect("search_web row");
    let available = row
        .get("available")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let reason = row
        .get("unavailableReason")
        .and_then(Value::as_str)
        .map(str::to_string);
    (available, reason)
}

#[tokio::test]
async fn web_search_advertised_iff_executable() {
    // One DB with a web-search-ENABLED chat; the only thing that varies is the
    // provider Option — the SAME source the host derives both from. The schema
    // comes from a committed fixture (test-pepper-keyed); the chat + profile the
    // inventory gate reads are inserted on top.
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("consistency-main.db");
    std::fs::copy(fixtures_dir().join("chat-admin-main.db"), &main).unwrap();
    seed_web_search_chat(&main);
    let db = Db::open(
        DbPaths {
            main,
            mount_index: None,
            llm_logs: None,
        },
        PEPPER,
    )
    .unwrap();
    let ctx = create_tool_context(
        "chat-1", USER, "char-1", "pp-1", None, None, None, None, None,
    );

    let body = r#"{"organic":[{"title":"A","link":"https://a","snippet":"s"}]}"#;
    let configured_provider: Arc<dyn WebSearchProvider> = Arc::new(RealWebSearchProvider::new(
        canned_serper_transport("q", 5, body),
        NoSearchApiKeys,
        false,
        Some("k".to_string()),
    ));

    for (label, web_search) in [
        ("configured", Some(configured_provider)),
        ("unconfigured", None),
    ] {
        // The host's exact derivation: the inventory bool IS the provider presence.
        let configured = web_search.is_some();

        // The inventory row (the advertised half) — driven with the SAME bool the
        // engine would derive.
        let resp = tools_list(&db, USER, Some("chat-1"), false, configured);
        let (available, reason) = search_web_row(&resp);
        assert_eq!(
            available, configured,
            "{label}: inventory availability must equal web_search_configured",
        );
        // Prove we're hitting the web_search_configured gate (not the profile's
        // allowWebSearch gate — that stays enabled here): the unconfigured refusal
        // is v4's "No search provider configured…" reason.
        if !configured {
            assert_eq!(
                reason.as_deref(),
                Some("No search provider configured. Please add a search provider API key in Settings > API Keys."),
                "unconfigured: the reason must be the web_search_configured one",
            );
        }

        // The runner (the executed half) — built from the SAME Option.
        let mut runner = BuiltInToolRunner::new(db.clone(), fixture_env());
        if let Some(ws) = &web_search {
            runner = runner.with_web_search_provider(Arc::clone(ws));
        }
        let out = runner.run(&search_web_call("q"), &ctx).await;
        let refused_not_configured = out.error.as_deref() == Some(NOT_CONFIGURED);
        assert_eq!(
            configured, !refused_not_configured,
            "{label}: the runner executes iff configured (never advertised-but-refusing)",
        );
    }
}
