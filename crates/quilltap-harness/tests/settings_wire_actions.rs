//! P4.6d wire-action handler tests — connection test / api-key test / test message
//! / models fetch. These compose over injected seams (the per-provider validate
//! WIRE / completion / models-fetch is v4 plugin internals, NOT ported — a host
//! seam), so this is a Rust-side composition test over the shared fixture (the
//! v4-route differential for the actual wire is deferred; the handler LOGIC —
//! key resolution, config validation, response mapping, and the models CACHE
//! effect — is what these prove). Env-gated on the settings fixture.
//!
//! This family consumes no oracle NDJSON — it composes the ported handlers over
//! injected seams — but it DOES read the shared settings fixture, and a recipe
//! must build every /tmp file it reads rather than lean on a sibling's staging
//! (P4.54 measured this family FAILING 0/4 whenever `settings_routes_
//! equivalence` had not just run). The fixture build below is the same one that
//! family's regen runs; it is idempotent, and the file is not committed.
//!
//! Build the fixture (Node 24, from the v4 checkout — see
//! `build-settings-fixture.ts`'s own header):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SETTINGS_MAIN=/tmp/qt-settings-fixture.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-settings-fixture.ts
//! Run:
//!   QT_FIXTURE_SETTINGS=/tmp/qt-settings-fixture.db \
//!     cargo test -p quilltap-harness --test settings_wire_actions

use quilltap_core::api::settings::{self, ConnectionValidator, ModelsFetcher};
use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse,
};
use serde_json::{json, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER_A: &str = "5e100000-0000-4000-8000-000000000001";
const OPENAI_KEY: &str = "5e300000-0000-4000-8000-000000000001";

fn open_db() -> Option<(Db, tempfile::TempDir)> {
    let fixture = std::env::var("QT_FIXTURE_SETTINGS").ok()?;
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path().join("main.db");
    std::fs::copy(&fixture, &main).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: None,
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open db");
    Some((db, tmp))
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

/// A canned validator (v4's `validateApiKey` boolean/error outcome).
struct CannedValidator(Result<bool, String>);
impl ConnectionValidator for CannedValidator {
    fn validate(&self, _p: &str, _k: &str, _b: Option<&str>) -> Result<bool, String> {
        self.0.clone()
    }
}

/// A canned models fetcher (v4 `getAvailableModels` + metadata merge).
struct CannedFetcher(Vec<Value>);
impl ModelsFetcher for CannedFetcher {
    fn fetch(
        &self,
        _p: &str,
        _k: &str,
        _b: Option<&str>,
    ) -> Result<(Vec<String>, Vec<Value>), String> {
        let ids = self
            .0
            .iter()
            .filter_map(|m| m.get("id").and_then(Value::as_str).map(String::from))
            .collect();
        Ok((ids, self.0.clone()))
    }
}

struct CannedCompletion(String);
impl CompletionProvider for CannedCompletion {
    fn send_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &CompletionParams,
    ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        let content = self.0.clone();
        async move {
            Ok(CompletionResponse {
                content,
                usage: None,
                finish_reason: None,
                attachment_results: None,
                cache_usage: None,
            })
        }
    }
}

fn body(resp: Response) -> Value {
    match resp {
        Response::ConnectionTest(v) | Response::ApiKeyTest(v) | Response::Models(v) => v,
        Response::Error(e) => json!({ "kind": format!("{:?}", e.kind), "error": e.message }),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn connection_test_success_and_failure() {
    let Some((db, _t)) = open_db() else {
        eprintln!("SKIP: set QT_FIXTURE_SETTINGS");
        return;
    };
    // valid → {valid:true, provider, message}.
    let v = body(settings::connection_test(
        &db,
        "OPENAI",
        Some(OPENAI_KEY),
        None,
        &CannedValidator(Ok(true)),
    ));
    assert_eq!(v["valid"], json!(true));
    assert_eq!(v["provider"], json!("OPENAI"));
    assert_eq!(v["message"], json!("Successfully connected to OPENAI"));

    // validateApiKey false → {valid:false, error:"Failed to validate connection to OpenAI"}.
    let v = body(settings::connection_test(
        &db,
        "OPENAI",
        Some(OPENAI_KEY),
        None,
        &CannedValidator(Ok(false)),
    ));
    assert_eq!(v["valid"], json!(false));
    assert_eq!(v["error"], json!("Failed to validate connection to OpenAI"));

    // Config invalid (no key on a key-requiring provider) → errors[0], validator never called.
    let v = body(settings::connection_test(
        &db,
        "OPENAI",
        None,
        None,
        &CannedValidator(Err("must not be called".into())),
    ));
    assert_eq!(v["valid"], json!(false));
    assert_eq!(v["error"], json!("OpenAI API Key is required for OPENAI"));
}

#[test]
fn api_key_test_records_usage() {
    let Some((db, _t)) = open_db() else {
        return;
    };
    let rt = rt();
    // valid → records usage (bumps lastUsed) + {valid:true, message}.
    let v = body(rt.block_on(settings::api_key_test(
        &db,
        USER_A,
        OPENAI_KEY,
        None,
        &CannedValidator(Ok(true)),
    )));
    assert_eq!(v["valid"], json!(true));
    assert_eq!(v["message"], json!("API key is valid"));
    let key = db
        .read_main(move |c| quilltap_core::db::api_keys::find_by_id(c, OPENAI_KEY))
        .unwrap()
        .unwrap();
    assert!(key.last_used.is_some(), "lastUsed bumped by record_usage");

    // invalid → {valid:false, provider} (no error, known provider).
    let v = body(rt.block_on(settings::api_key_test(
        &db,
        USER_A,
        OPENAI_KEY,
        None,
        &CannedValidator(Ok(false)),
    )));
    assert_eq!(v["valid"], json!(false));
    assert_eq!(v.get("error"), None);
}

#[test]
fn test_message_maps_response() {
    let Some((db, _t)) = open_db() else {
        return;
    };
    let rt = rt();
    let v = body(rt.block_on(settings::connection_test_message(
        &db,
        "OPENAI",
        Some(OPENAI_KEY),
        None,
        "gpt-4o",
        &json!({ "temperature": 0.5, "max_tokens": 50 }),
        &CannedCompletion("Hello there!".into()),
    )));
    assert_eq!(v["success"], json!(true));
    assert_eq!(v["modelName"], json!("gpt-4o"));
    assert_eq!(
        v["message"],
        json!("Test message successful! Model responded: \"Hello there!\"")
    );
    assert_eq!(v["responsePreview"], json!("Hello there!"));
}

#[test]
fn model_fetch_caches_rows() {
    let Some((db, _t)) = open_db() else {
        return;
    };
    let rt = rt();
    // A NEW model (not in the seeded cache) so the upsert inserts a row.
    let fetched = vec![json!({
        "id": "gpt-5-preview",
        "displayName": "GPT-5 preview",
        "contextWindow": 256000,
        "maxOutputTokens": 32000,
        "deprecated": false,
        "experimental": true,
    })];
    let v = body(rt.block_on(settings::model_fetch(
        &db,
        USER_A,
        "OPENAI",
        Some(OPENAI_KEY),
        None,
        &CannedFetcher(fetched),
    )));
    assert_eq!(v["provider"], json!("OPENAI"));
    assert_eq!(v["count"], json!(1));
    assert_eq!(v["models"], json!(["gpt-5-preview"]));

    // The cache now has the new row (3 total: 2 seeded + 1 fetched).
    let cached = db
        .read_main(|c| quilltap_core::db::provider_models::find_by_provider(c, "OPENAI"))
        .unwrap();
    assert!(
        cached
            .iter()
            .any(|m| m.get("modelId").and_then(Value::as_str) == Some("gpt-5-preview")),
        "fetched model cached"
    );
    assert_eq!(cached.len(), 3, "2 seeded + 1 fetched");
}

/// P4.D97 (v4 bug 85): the POST echo's `modelsWithInfo` rows gain
/// `supportsThinking` / `thinksByDefault` from the manifest's model catalogue
/// per exact id — and an uncatalogued id gains NOTHING (v4's
/// `staticInfo?.…` spread drops the keys with `undefined`). The GET leg is
/// untouched because the cache write never carries the facts — pinned here by
/// reading the cache back after the fetch.
#[test]
fn model_fetch_enriches_thinking_facts() {
    let Some((db, _t)) = open_db() else {
        return;
    };
    let rt = rt();
    let fetched = vec![
        json!({ "id": "deepseek-v4-flash", "displayName": "DeepSeek V4 Flash" }),
        json!({ "id": "deepseek-experimental", "displayName": "DeepSeek Experimental" }),
    ];
    let v = body(rt.block_on(settings::model_fetch(
        &db,
        USER_A,
        "DEEPSEEK",
        Some(OPENAI_KEY),
        None,
        &CannedFetcher(fetched),
    )));
    let rows = v["modelsWithInfo"].as_array().expect("modelsWithInfo");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["supportsThinking"], json!(true));
    assert_eq!(rows[0]["thinksByDefault"], json!(true));
    assert!(
        rows[1].get("supportsThinking").is_none() && rows[1].get("thinksByDefault").is_none(),
        "an uncatalogued id must gain no thinking keys, got {:?}",
        rows[1]
    );

    // The cache is fact-blind on both legs, exactly as v4's is.
    let cached = db
        .read_main(|c| quilltap_core::db::provider_models::find_by_provider(c, "DEEPSEEK"))
        .unwrap();
    assert!(
        cached
            .iter()
            .all(|m| m.get("supportsThinking").is_none() && m.get("thinksByDefault").is_none()),
        "the models cache must not carry thinking facts"
    );
}
