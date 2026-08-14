//! P4.6d settings-server differential — the Rust `api::settings::*` handlers vs
//! v4's REAL route handlers over the shared settings fixture. Consolidates the
//! work order's four DB-family differentials (`settings_chat`,
//! `connection_profiles_routes`, `api_keys_routes`, `provider_models_routes`) —
//! each case is tagged with its `family`. Extended by P4.6an
//! (`dangerousContentSettings`), P4.d3 (`data_retention`), P4.D50 (`taboo`),
//! P4.D57 (`brahma_console` — the Brahma Console turn-budget GET/PUT, seeded via
//! `seedBrahmaConsole`), and P4.D73 (`composer_settings` — the three 4.8.2
//! `chat_settings` columns `composerEmoji` / `composerUnicode` /
//! `smartTypographySettings`, incl. the four reject arms whose 400 body is the
//! whole `ZodError.message`; a stale oracle predating a family is caught by the
//! per-family `>=` count guards below).
//!
//! Both sides run each case over a FRESH copy of the committed fixture; the
//! response body (+ a post-mutation family-list refetch, observing the persisted
//! effect via the already-verified read marshaling) is diffed after normalizing
//! minted ids (UUIDs not in the fixture spec → `<newid>`) and timestamps (ISO →
//! `<ts>`). Delete/reorder/reset-sort return an ack (the contract folds v4's
//! `{success:true}` body → ack), so their body is not compared — the refetched
//! list carries the effect.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout — see the
//! `build-settings-fixture.ts` + `settings-routes.test.ts` headers):
//!   QT_FIXTURE_SETTINGS_MAIN=/tmp/qt-settings-fixture.db node --import tsx …build-settings-fixture.ts
//!   … QT_ORACLE_OUT=/tmp/oracle-settings-routes.ndjson npx jest -- settings-routes
//! Run:
//!   QT_ORACLE_SETTINGS_ROUTES=/tmp/oracle-settings-routes.ndjson \
//!   QT_FIXTURE_SETTINGS=/tmp/qt-settings-fixture.db \
//!     cargo test -p quilltap-harness --test settings_routes_equivalence

use std::collections::HashSet;
use std::path::PathBuf;

use quilltap_core::api::settings;
use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::{Db, DbPaths};
use regex::Regex;
use serde_json::Value;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/settings.json")
}

/// The fixture id set (so minted UUIDs collapse but baked ids stay, keeping
/// enrichment relationships verifiable).
fn known_ids(spec: &Value) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut push = |v: &Value| {
        if let Some(s) = v.as_str() {
            ids.insert(s.to_string());
        }
    };
    for k in ["userA", "userB", "settingsIdA", "roleplayTemplateId"] {
        push(&spec[k]);
    }
    for group in ["apiKeys", "profiles", "providerModels"] {
        if let Some(obj) = spec[group].as_object() {
            for v in obj.values() {
                push(v);
            }
        }
    }
    ids
}

fn normalize(v: &mut Value, known: &HashSet<String>, uuid_re: &Regex, ts_re: &Regex) {
    match v {
        Value::String(s) => {
            if ts_re.is_match(s) {
                *s = "<ts>".to_string();
            } else if uuid_re.is_match(s) && !known.contains(s.as_str()) {
                *s = "<newid>".to_string();
            }
        }
        Value::Array(a) => {
            for x in a {
                normalize(x, known, uuid_re, ts_re);
            }
        }
        Value::Object(o) => {
            for x in o.values_mut() {
                normalize(x, known, uuid_re, ts_re);
            }
        }
        _ => {}
    }
}

/// Run a single case's handler and return `(body, is_ack)`.
fn run_handler(rt: &tokio::runtime::Runtime, db: &Db, user: &str, req: &Value) -> (Value, bool) {
    let route = req["route"].as_str().unwrap();
    let method = req["method"].as_str().unwrap();
    let url = req["url"].as_str().unwrap();
    let param_id = req["paramId"].as_str();
    let body = &req["body"];

    let resp = match (route, method) {
        ("settingsChat", "GET") => rt.block_on(settings::chat_settings_get(db, user)),
        ("settingsChat", "PUT") => rt.block_on(settings::chat_settings_update(db, user, body)),
        ("connProfiles", "GET") => settings::connection_profile_list(db, user, false),
        ("connProfiles", "POST") => {
            if url.contains("action=reorder") {
                let mut order: Vec<(i64, String)> = body["order"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| {
                        (
                            e["sortIndex"].as_i64().unwrap(),
                            e["id"].as_str().unwrap().to_string(),
                        )
                    })
                    .collect();
                order.sort_by_key(|(i, _)| *i);
                let ids: Vec<String> = order.into_iter().map(|(_, id)| id).collect();
                rt.block_on(settings::connection_profile_reorder(db, user, &ids))
            } else if url.contains("action=reset-sort") {
                rt.block_on(settings::connection_profile_reset_sort(db, user))
            } else {
                rt.block_on(settings::connection_profile_create(db, user, body))
            }
        }
        ("connProfileItem", "PUT") => rt.block_on(settings::connection_profile_update(
            db,
            user,
            param_id.unwrap(),
            body,
        )),
        ("connProfileItem", "DELETE") => {
            rt.block_on(settings::connection_profile_delete(db, param_id.unwrap()))
        }
        ("apiKeys", "GET") => settings::api_key_list(db, user),
        ("apiKeys", "POST") => rt.block_on(settings::api_key_create(
            db,
            user,
            body["provider"].as_str().unwrap_or(""),
            body["label"].as_str().unwrap_or(""),
            body["apiKey"].as_str().unwrap_or(""),
        )),
        ("apiKeyItem", "PUT") => rt.block_on(settings::api_key_update(
            db,
            param_id.unwrap(),
            body["label"].as_str(),
            body["isActive"].as_bool(),
            body["apiKey"].as_str(),
        )),
        ("apiKeyItem", "DELETE") => rt.block_on(settings::api_key_delete(db, param_id.unwrap())),
        ("models", "GET") => {
            let provider = url
                .split("provider=")
                .nth(1)
                .map(|s| s.split('&').next().unwrap_or(s).to_string());
            settings::model_list(db, provider.as_deref())
        }
        ("dataRetention", "GET") => settings::data_retention_settings_get(db),
        ("dataRetention", "PUT") => {
            rt.block_on(settings::data_retention_settings_update(db, body.clone()))
        }
        // P4.D50. The oracle emits the raw request body, which may be a
        // NON-object (v4's `{...current, ...body}` spreads a string into
        // indexed keys and so contributes no `phrases`); `get("phrases")`
        // answers None for those, which is exactly the merge-keeps-current arm.
        ("taboo", "GET") => settings::taboo_settings_get(db),
        ("taboo", "PUT") => {
            let bag = match body.get("phrases") {
                Some(v) => serde_json::json!({ "phrases": v }),
                None => serde_json::json!({}),
            };
            rt.block_on(settings::taboo_settings_update(db, bag))
        }
        // P4.D57. Like taboo: the oracle emits the raw request body (possibly a
        // NON-object string, whose spread contributes no `maxAgentTurns`), and
        // the edge's body→bag mapping is mirrored here — a present value
        // (including an explicit `null`) rides raw so the handler's Zod-faithful
        // parse decides, an absent key keeps the current value.
        ("brahmaConsole", "GET") => settings::brahma_console_settings_get(db),
        ("brahmaConsole", "PUT") => {
            let bag = match body.get("maxAgentTurns") {
                Some(v) => serde_json::json!({ "maxAgentTurns": v }),
                None => serde_json::json!({}),
            };
            rt.block_on(settings::brahma_console_settings_update(db, bag))
        }
        other => panic!("unhandled route/method: {other:?}"),
    };
    response_to_body(resp)
}

fn response_to_body(resp: Response) -> (Value, bool) {
    match resp {
        Response::ChatSettings(v)
        | Response::ConnectionProfiles(v)
        | Response::ConnectionProfile(v)
        | Response::ApiKeys(v)
        | Response::ApiKey(v)
        | Response::DataRetention(v)
        | Response::Taboo(v)
        | Response::BrahmaConsole(v)
        | Response::Models(v) => (v, false),
        Response::Ack(_) => (serde_json::json!({}), true),
        Response::Error(e) => (serde_json::json!({ "error": e.message }), false),
        other => panic!("unexpected response variant: {other:?}"),
    }
}

#[test]
fn settings_routes_match_v4() {
    let (Some(oracle_path), Some(fixture)) = (
        env_or_skip("QT_ORACLE_SETTINGS_ROUTES"),
        env_or_skip("QT_FIXTURE_SETTINGS"),
    ) else {
        return;
    };
    let spec: Value =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read settings.json"))
            .expect("parse spec");
    let user_a = spec["userA"].as_str().unwrap().to_string();
    let user_b = spec["userB"].as_str().unwrap().to_string();
    let known = known_ids(&spec);
    let uuid_re = Regex::new(
        r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
    )
    .unwrap();
    let ts_re = Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$").unwrap();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    let oracle = std::fs::read_to_string(&oracle_path).expect("read oracle ndjson");
    let mut n = 0;
    let mut taboo_cases = 0;
    let mut brahma_console_cases = 0;
    let mut composer_settings_cases = 0;
    for line in oracle.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let name = row["name"].as_str().unwrap().to_string();
        let req = &row["req"];
        let user = if req["user"].as_str() == Some("A") {
            &user_a
        } else {
            &user_b
        };

        // Fresh fixture copy per case (mutations mint / delete).
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

        // P4.D50: seed `instance_settings['taboo']` through the real setter
        // before the case runs (the oracle does the same) — each case gets a
        // pristine fixture copy, so the merge-over-current arms need it.
        if let Some(seed) = req["seedTaboo"].as_array() {
            let phrases: Vec<String> = seed
                .iter()
                .map(|v| v.as_str().unwrap_or_default().to_string())
                .collect();
            rt.block_on(db.write(move |w| {
                quilltap_core::db::instance_settings::set_taboo_settings(
                    w.main().connection(),
                    &phrases,
                )
                .map(|_| ())
            }))
            .expect("seed taboo");
        }

        // P4.D57: seed `instance_settings['brahmaConsole']` through the real setter
        // (the oracle does the same) — the merge-over-current arms need it.
        if let Some(seed) = req["seedBrahmaConsole"].as_i64() {
            rt.block_on(db.write(move |w| {
                quilltap_core::db::instance_settings::set_brahma_console_settings(
                    w.main().connection(),
                    seed,
                )
                .map(|_| ())
            }))
            .expect("seed brahma-console");
        }

        let (mut got_body, is_ack) = run_handler(&rt, &db, user, req);

        // The `after` refetch (post-mutation family list).
        let after = req["after"].as_str();
        let got_after = after.map(|kind| {
            let r = match kind {
                "connProfiles" => settings::connection_profile_list(&db, user, false),
                "apiKeys" => settings::api_key_list(&db, user),
                "taboo" => settings::taboo_settings_get(&db),
                "brahmaConsole" => settings::brahma_console_settings_get(&db),
                _ => panic!("unknown after: {kind}"),
            };
            response_to_body(r).0
        });
        drop(db);

        let mut oracle_body = row["body"].clone();
        normalize(&mut got_body, &known, &uuid_re, &ts_re);
        normalize(&mut oracle_body, &known, &uuid_re, &ts_re);

        if !is_ack {
            assert_eq!(
                oracle_body, got_body,
                "[{name}] response body mismatch\noracle: {oracle_body}\ngot:    {got_body}"
            );
        }
        if let Some(mut got_after) = got_after {
            let mut oracle_after = row["after"].clone();
            normalize(&mut got_after, &known, &uuid_re, &ts_re);
            normalize(&mut oracle_after, &known, &uuid_re, &ts_re);
            assert_eq!(
                oracle_after, got_after,
                "[{name}] after-refetch mismatch\noracle: {oracle_after}\ngot:    {got_after}"
            );
        }
        n += 1;
        if row["family"].as_str() == Some("taboo") {
            taboo_cases += 1;
        }
        if row["family"].as_str() == Some("brahma_console") {
            brahma_console_cases += 1;
        }
        if row["family"].as_str() == Some("composer_settings") {
            composer_settings_cases += 1;
        }
    }
    // 19 at P4.6d + the two P4.6an dangerousContentSettings cases.
    assert!(n >= 21, "expected >= 21 cases, got {n}");
    // P4.D50: the Taboo family must actually be present — a stale oracle that
    // predates it would otherwise pass by simply not carrying those rows.
    assert!(
        taboo_cases >= 18,
        "expected >= 18 taboo cases, got {taboo_cases} — regenerate the oracle"
    );
    // P4.D57: the Brahma Console turn-budget family must actually be present — a
    // stale oracle that predates it would otherwise pass by not carrying it.
    assert!(
        brahma_console_cases >= 12,
        "expected >= 12 brahma_console cases, got {brahma_console_cases} — regenerate the oracle"
    );
    // P4.D73: the three 4.8.2 composer/typography keys. A stale oracle that
    // predates them would otherwise pass by not carrying the rows at all.
    assert!(
        composer_settings_cases >= 10,
        "expected >= 10 composer_settings cases, got {composer_settings_cases} — regenerate the oracle"
    );
    eprintln!("settings-routes differential: {n} cases matched");
}
