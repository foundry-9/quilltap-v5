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
const ANTHROPIC_KEY: &str = "5e300000-0000-4000-8000-000000000002";
const GPT_PROFILE: &str = "5e400000-0000-4000-8000-000000000001";

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
        Response::ConnectionTest(v)
        | Response::ApiKeyTest(v)
        | Response::Models(v)
        | Response::ApiKeys(v)
        | Response::ApiKey(v) => v,
        Response::Ack(_) => json!({ "ack": true }),
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

/// P4.136: v4 `findApiKeyById` is a fallback `safeQuery` on the
/// `connection_profiles` repository — a read error logs `Error finding API key
/// by ID {collection, keyId, error}` and answers `null`, which every one of the
/// four connection-profile routes turns into a 404 `API key` (create `route.
/// ts:252`, PUT `[id]/route.ts:207`, test-connection `:360`, test-message
/// `:428`). v5 had answered 500. The models fetch (`POST /api/v1/models`) is
/// the fifth route of the same shape, found at unification; it reads the key
/// SCOPED. The plant is a COPY of the fixture whose
/// OpenAI key row's `key_value` is a BLOB — the plant both sides fail on
/// (v4's `ApiKeySchema.parse` refuses the decoded Float32Array, measured in
/// the `title_update_tier3` lifted case; v5's marshal answers
/// `InvalidColumnType`). A text `isActive` is not one: v4 coerces it.
///
/// P4.139 adds the api-keys routes' three arms (eight in all). v4's
/// `app/api/v1/api-keys/[id]/route.ts` reads PUT (`:95`) and DELETE (`:164`)
/// UNSCOPED and the `?action=test` handler (`:201`) SCOPED — exactly as v5
/// does (the file's `getUserRepositories` import at `:13` is dead) — and each
/// read is the fallback, so a corrupt key is the line and a 404, never the
/// handler's `catch` → 500 (unreachable for a read error). v5 had answered
/// `Internal "sqlite error: Invalid column type Blob …"` on all three.
#[test]
fn a_corrupt_key_row_is_v4s_logged_404_on_all_eight_key_routes() {
    let Some((db, _t)) = open_db_with_a_corrupt_openai_key() else {
        eprintln!("SKIP: set QT_FIXTURE_SETTINGS");
        return;
    };
    let rt = rt();
    let line = format!(
        "ERROR quilltap::db Error finding API key by ID collection=connection_profiles keyId={OPENAI_KEY} error=Invalid column type Blob at index: 4, name: key_value"
    );
    let want = json!({ "kind": "NotFound", "error": "API key not found" });
    type Arm<'a> = (&'static str, Box<dyn Fn() -> Response + 'a>);
    let arms: Vec<Arm<'_>> = vec![
        (
            "test-message",
            Box::new(|| {
                rt.block_on(settings::connection_test_message(
                    &db,
                    "OPENAI",
                    Some(OPENAI_KEY),
                    None,
                    "gpt-4o",
                    &json!({}),
                    &CannedCompletion("unreached".into()),
                ))
            }),
        ),
        (
            "test-connection",
            Box::new(|| {
                settings::connection_test(
                    &db,
                    "OPENAI",
                    Some(OPENAI_KEY),
                    None,
                    &CannedValidator(Ok(true)),
                )
            }),
        ),
        (
            "create",
            Box::new(|| {
                rt.block_on(settings::connection_profile_create(
                    &db,
                    USER_A,
                    &json!({
                        "name": "P4.136 corrupt-key probe",
                        "provider": "OPENAI",
                        "modelName": "gpt-4o",
                        "apiKeyId": OPENAI_KEY,
                    }),
                ))
            }),
        ),
        (
            "api-key update",
            Box::new(|| {
                rt.block_on(settings::api_key_update(
                    &db,
                    OPENAI_KEY,
                    Some("P4.139 probe"),
                    None,
                    None,
                ))
            }),
        ),
        (
            "api-key delete",
            Box::new(|| rt.block_on(settings::api_key_delete(&db, OPENAI_KEY))),
        ),
        (
            "update",
            Box::new(|| {
                rt.block_on(settings::connection_profile_update(
                    &db,
                    USER_A,
                    GPT_PROFILE,
                    &json!({ "apiKeyId": OPENAI_KEY }),
                ))
            }),
        ),
    ];
    // Every arm runs before the verdict, so a red names its whole count.
    let mut reds: Vec<String> = Vec::new();
    let mut check = |arm: &str, resp: Response, lines: Vec<String>, want_line: &str| {
        let got = body(resp);
        if got != want_line_body(arm, &want) {
            reds.push(format!("{arm}: body {got}"));
        }
        let db_lines: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains(" quilltap::db "))
            .collect();
        if db_lines != vec![want_line] {
            reds.push(format!("{arm}: lines {lines:?}"));
        }
    };
    for (arm, call) in arms {
        let (resp, lines) = quilltap_core::test_support::captured_with(call);
        check(arm, resp, lines, &line);
    }

    // The fifth arm (found at unification): `POST /api/v1/models` reads the
    // key SCOPED (v4 `models/route.ts:77-80`), so its line is the scoped home's
    // and its 404 carries v4's doubled "not found". The fetcher is never
    // reached.
    let (resp, lines) = quilltap_core::test_support::captured_with(|| {
        rt.block_on(settings::model_fetch(
            &db,
            USER_A,
            "OPENAI",
            Some(OPENAI_KEY),
            None,
            &CannedFetcher(vec![json!({ "id": "unreached" })]),
        ))
    });
    let scoped = format!(
        "ERROR quilltap::db Error finding API key by ID and user ID collection=connection_profiles keyId={OPENAI_KEY} userId={USER_A} error=Invalid column type Blob at index: 4, name: key_value"
    );
    check("models", resp, lines, &scoped);

    // The eighth arm (P4.139): `POST /api/v1/api-keys/[id]?action=test` reads
    // SCOPED (`[id]/route.ts:201`), so its line is the scoped home's; its 404
    // is the single "not found". The validator is never reached.
    let (resp, lines) = quilltap_core::test_support::captured_with(|| {
        rt.block_on(settings::api_key_test(
            &db,
            USER_A,
            OPENAI_KEY,
            None,
            &CannedValidator(Ok(true)),
        ))
    });
    check("api-key test", resp, lines, &scoped);
    assert!(
        reds.is_empty(),
        "{} red(s):\n  {}",
        reds.len(),
        reds.join("\n  ")
    );
}

/// The 404 body each arm owes: v4's `notFound('API key')`, except the models
/// fetch's `notFound('API key not found')`, which doubles the noun.
fn want_line_body(arm: &str, want: &Value) -> Value {
    if arm == "models" {
        json!({ "kind": "NotFound", "error": "API key not found not found" })
    } else {
        want.clone()
    }
}

/// P4.139: v4 `GET /api/v1/api-keys` lists `getAllApiKeys()` — the user's
/// `getApiKeysByUserId`, which DROPS a row `ApiKeySchema.safeParse` refuses
/// with its WARN (`connection-profiles.repository.ts:224-237`, a direct
/// `logger.warn`, no `collection`) and lists the rest. v5's list had failed on
/// the first bad row and answered 500 for every key the user owns.
#[test]
fn a_corrupt_key_row_is_dropped_from_the_api_key_list_with_v4s_warn() {
    let Some((db, _t)) = open_db_with_a_corrupt_openai_key() else {
        eprintln!("SKIP: set QT_FIXTURE_SETTINGS");
        return;
    };
    let (resp, lines) =
        quilltap_core::test_support::captured_with(|| settings::api_key_list(&db, USER_A));
    let v = body(resp);
    let ids: Vec<&str> = v["apiKeys"]
        .as_array()
        .unwrap_or_else(|| panic!("a 200 list, got {v}"))
        .iter()
        .map(|k| k["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec![ANTHROPIC_KEY],
        "the OpenAI key dropped, the rest listed"
    );
    assert_eq!(v["count"], json!(1));
    let db_lines: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains(" quilltap::db "))
        .collect();
    let warn = format!(
        "WARN quilltap::db API key validation failed keyId={OPENAI_KEY} userId={USER_A} error=Invalid column type Blob at index: 4, name: key_value"
    );
    assert_eq!(db_lines, vec![&warn], "{lines:?}");
}

/// The fixture copy with the OpenAI key row's `key_value` cell made a BLOB
/// (written through a writable open on the COPY, never the fixture).
fn open_db_with_a_corrupt_openai_key() -> Option<(Db, tempfile::TempDir)> {
    let fixture = std::env::var("QT_FIXTURE_SETTINGS").ok()?;
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path().join("main.db");
    std::fs::copy(&fixture, &main).unwrap();
    {
        let w = quilltap_core::db::Writer::open_writable(&main, PEPPER).unwrap();
        let n = w
            .connection()
            .execute(
                "UPDATE api_keys SET key_value = x'00000000' WHERE id = ?1",
                [OPENAI_KEY],
            )
            .unwrap();
        assert_eq!(n, 1, "the plant must land on the fixture's OpenAI key");
    }
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
