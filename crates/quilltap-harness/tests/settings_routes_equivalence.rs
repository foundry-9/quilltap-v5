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
//! per-family `>=` count guards below), and P4.47 (`settings_zod` — the three
//! D73-banked sibling arms `answerConfirmationSettings` / `cheapLLMSettings` /
//! `dangerousContentSettings`, whose present-but-invalid legs had NO corpus
//! case and so collapsed to invented sentences; the family reaches five Zod
//! issue codes and pins the cheap-LLM ordering, whose parse happens in the
//! repo's whole-object validate rather than at the route), and P4.D85
//! (`connection_profile_tags` — v4 Bug 74's `get-tags` / `add-tag` /
//! `remove-tag`, over a fixture whose OPENAI profile finally carries tags: an
//! unsorted bag with a dangling id, so order-preservation, drop-missing and
//! present-vs-omitted `visualStyle` are all measurable. Three of its rows are
//! RECORDED-ONLY — v4 action-gate arms with no v5 counterpart by design; see
//! the main loop).
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
use quilltap_core::api::types::{Request, Response};
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
    // P4.D85: `tags` joins the baked-id groups so the tag ids in an enriched
    // profile / a `get-tags` answer are compared LITERALLY rather than collapsing
    // to `<newid>` on both sides (which would blind the order-preservation and
    // drop-missing arms the fixture's unsorted, partly-dangling bag exists to
    // prove). The dangling id rides the same group for the same reason.
    for group in ["apiKeys", "profiles", "providerModels", "tags"] {
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

/// Decode a data-retention PUT body into the `Request` variant exactly as the
/// wire does: only the schema's own key is kept, the tagged `type` is inserted,
/// and serde's `double_option` decides absent vs explicit-`null` vs value. A
/// NON-object body (v4's `{...current, ...body}` spread of a string / array /
/// number contributes no `staleChatDays`) decodes as the empty object, i.e. the
/// key-absent arm.
fn data_retention_update_request(body: &Value) -> Request {
    let mut map = match body {
        Value::Object(o) => o.clone(),
        _ => serde_json::Map::new(),
    };
    map.retain(|k, _| k == "staleChatDays");
    map.insert(
        "type".into(),
        Value::String("dataRetentionSettingsUpdate".into()),
    );
    serde_json::from_value::<Request>(Value::Object(map))
        .expect("data-retention update body decodes into the Request variant")
}

/// Run a single case's handler and return `(body, is_ack, status)`.
fn run_handler(
    rt: &tokio::runtime::Runtime,
    db: &Db,
    user: &str,
    req: &Value,
) -> (Value, bool, u16) {
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
        // P4.D85. v5 carries no `?action=` surface for connection profiles — the
        // verbs ARE the action selection — so the URL's action selects the verb
        // here exactly as `connProfiles` POST does above. Only the actions v5
        // implements reach this point; v4's two action-GATE arms are `recorded`
        // rows, asserted for shape without a v5 drive (see the main loop).
        ("connProfileItem", "GET") => {
            assert!(
                url.contains("action=get-tags"),
                "only get-tags is driven on the item GET; other actions are recorded rows"
            );
            settings::connection_profile_get_tags(db, param_id.unwrap())
        }
        ("connProfileItem", "POST") => {
            let tag_id = body["tagId"].as_str().unwrap_or("");
            if url.contains("action=add-tag") {
                rt.block_on(settings::connection_profile_add_tag(
                    db,
                    param_id.unwrap(),
                    tag_id,
                ))
            } else if url.contains("action=remove-tag") {
                rt.block_on(settings::connection_profile_remove_tag(
                    db,
                    param_id.unwrap(),
                    tag_id,
                ))
            } else {
                panic!("unhandled connProfileItem POST action: {url}");
            }
        }
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
        // P4.56. This leg used to hand the oracle's RAW body straight to the
        // handler, bypassing the `Request` enum's serde entirely — so a
        // present-`null` arm would have passed green while the real wire
        // (dispatch and the REST edge, both of which decode into `Request`)
        // silently collapsed it to key-absent. The body now rides through the
        // SAME serde decode the wire uses, and the resulting tri-state is
        // mapped to the handler's bag exactly as `engine.rs`'s dispatch arm and
        // `quilltap-web`'s edge do (the taboo / brahma-console shape).
        ("dataRetention", "PUT") => {
            let Request::DataRetentionSettingsUpdate { stale_chat_days } =
                data_retention_update_request(body)
            else {
                unreachable!("the tagged decode can only answer this variant");
            };
            let bag = match stale_chat_days {
                Some(v) => serde_json::json!({ "staleChatDays": v }),
                None => serde_json::json!({}),
            };
            rt.block_on(settings::data_retention_settings_update(db, bag))
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
    response_to_body_status(resp)
}

fn response_to_body(resp: Response) -> (Value, bool) {
    let (body, is_ack, _) = response_to_body_status(resp);
    (body, is_ack)
}

/// Like [`response_to_body`] but also projects the HTTP status the web edge
/// would answer, so a row can pin v4's recorded `status` — added at the
/// help-drift unification after the P4.47 §3 review found v4's
/// `includes('Invalid') ? 400 : 500` split (a threshold-only Zod message
/// carries no "Invalid" and answers 500) was invisible to a body-only diff.
fn response_to_body_status(resp: Response) -> (Value, bool, u16) {
    use quilltap_core::api::types::ErrorKind;
    match resp {
        Response::ChatSettings(v)
        | Response::ConnectionProfiles(v)
        | Response::ConnectionProfile(v)
        | Response::ApiKeys(v)
        | Response::ApiKey(v)
        | Response::DataRetention(v)
        | Response::Taboo(v)
        | Response::BrahmaConsole(v)
        | Response::Models(v) => (v, false, 200),
        Response::Ack(_) => (serde_json::json!({}), true, 200),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                _ => 500,
            };
            (serde_json::json!({ "error": e.message }), false, status)
        }
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
    let mut data_retention_cases = 0;
    let mut taboo_cases = 0;
    let mut brahma_console_cases = 0;
    let mut composer_settings_cases = 0;
    let mut settings_zod_cases = 0;
    let mut connection_profile_cases = 0;
    let mut profile_tag_cases = 0;
    let mut recorded_cases = 0;
    for line in oracle.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let name = row["name"].as_str().unwrap().to_string();
        let req = &row["req"];
        let user = if req["user"].as_str() == Some("A") {
            &user_a
        } else {
            &user_b
        };

        // P4.D85 RECORDED-ONLY arms. v4 reaches the connection-profile tag
        // actions through `?action=` on the item route; v5 has no such surface
        // (no REST edge exists — the SPA and every other consumer ride
        // `/api/dispatch`, and the §1 verbs ARE the action selection). So v4's
        // two action-GATE 400s and the no-action GET body have no v5
        // counterpart BY DESIGN. Their bytes are recorded and asserted here so
        // upstream drift in either sentence is still caught — the
        // `search_replace_equivalence` middleware-arm precedent.
        if req["recorded"].as_bool() == Some(true) {
            recorded_cases += 1;
            profile_tag_cases += 1;
            let want = match name.as_str() {
                "cp_get_unknown_action" => Some((
                    400_u64,
                    "Unknown action: bogus. Available actions: get-tags".to_string(),
                )),
                "cp_post_unknown_action" => Some((
                    400,
                    "Unknown action: bogus. Available actions: add-tag, remove-tag, auto-configure"
                        .to_string(),
                )),
                "cp_get_no_action" => None,
                other => panic!("unknown recorded case: {other}"),
            };
            match want {
                Some((status, error)) => {
                    assert_eq!(
                        row["status"].as_u64(),
                        Some(status),
                        "[{name}] recorded v4 status changed"
                    );
                    assert_eq!(
                        row["body"]["error"].as_str(),
                        Some(error.as_str()),
                        "[{name}] recorded v4 action-gate sentence changed"
                    );
                }
                None => {
                    // The no-action GET must still answer the enriched profile —
                    // the new action gate must not have swallowed it.
                    assert_eq!(
                        row["status"].as_u64(),
                        Some(200),
                        "[{name}] recorded v4 status changed"
                    );
                    assert_eq!(
                        row["body"]["profile"]["id"].as_str(),
                        spec["profiles"]["gpt"].as_str(),
                        "[{name}] the no-action GET no longer answers the enriched profile"
                    );
                    assert!(
                        row["body"]["profile"]["tags"][0]["tag"]["name"].is_string(),
                        "[{name}] the no-action GET lost the {{tagId, tag}} envelope"
                    );
                }
            }
            n += 1;
            continue;
        }

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

        // P4.56: seed `instance_settings['dataRetention']` through the real
        // setter (the oracle does the same) — the merge-keeps-current and
        // writes-nothing arms need a NON-default stored value.
        if let Some(seed) = req["seedDataRetention"].as_i64() {
            rt.block_on(db.write(move |w| {
                quilltap_core::db::instance_settings::set_data_retention_settings(
                    w.main().connection(),
                    seed,
                )
            }))
            .expect("seed data-retention");
        }

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

        let (mut got_body, is_ack, got_status) = run_handler(&rt, &db, user, req);

        // The `after` refetch (post-mutation family list).
        let after = req["after"].as_str();
        let got_after = after.map(|kind| {
            let r = match kind {
                "connProfiles" => settings::connection_profile_list(&db, user, false),
                "apiKeys" => settings::api_key_list(&db, user),
                "dataRetention" => settings::data_retention_settings_get(&db),
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
        // The wire status on ERROR rows (P4.47 §3 at the help-drift
        // unification: v4's `includes('Invalid') ? 400 : 500` split was
        // invisible to a body-only diff — a threshold-only Zod message answers
        // 500). Success rows are deliberately NOT status-compared: v4's REST
        // routes carry per-route 200-vs-201 (cp_create answers 201), while
        // v5's settings surfaces ride `/api/dispatch`, whose envelope has no
        // per-verb status — the modeled contract here is ErrorKind → status.
        if let Some(want_status) = row["status"].as_u64().filter(|s| *s >= 400) {
            assert_eq!(
                want_status as u16, got_status,
                "[{name}] error-status mismatch: oracle {want_status}, got {got_status}"
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
        if row["family"].as_str() == Some("data_retention") {
            data_retention_cases += 1;
        }
        if row["family"].as_str() == Some("taboo") {
            taboo_cases += 1;
        }
        if row["family"].as_str() == Some("brahma_console") {
            brahma_console_cases += 1;
        }
        if row["family"].as_str() == Some("composer_settings") {
            composer_settings_cases += 1;
        }
        if row["family"].as_str() == Some("settings_zod") {
            settings_zod_cases += 1;
        }
        if row["family"].as_str() == Some("connection_profiles") {
            connection_profile_cases += 1;
        }
        if row["family"].as_str() == Some("connection_profile_tags") {
            profile_tag_cases += 1;
        }
    }
    // 19 at P4.6d + the two P4.6an dangerousContentSettings cases.
    assert!(n >= 21, "expected >= 21 cases, got {n}");
    // P4.56: the data-retention family, whose arms are meaningful only through
    // the `Request` serde path this lane rewired it onto. Row-driven floor — it
    // moves with every arm batch (P4.55's rule).
    assert!(
        data_retention_cases >= 13,
        "expected >= 13 data_retention cases, got {data_retention_cases} — regenerate the oracle"
    );
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
    // P4.47 (A): the three sibling Zod arms
    // (`answerConfirmationSettings` / `cheapLLMSettings` /
    // `dangerousContentSettings`). Same stale-oracle guard as the families
    // above — before this lane NOTHING exercised a present-but-invalid value on
    // any of them, which is exactly how the collapse survived.
    assert!(
        settings_zod_cases >= 27,
        "expected >= 27 settings_zod cases, got {settings_zod_cases} — regenerate the oracle"
    );
    // P4.D79: the eight multi-character-prefill arms on top of the family's
    // original seven. Same stale-oracle guard as the families above — the three
    // 400 arms in particular would simply be absent from a pre-P4.D79 oracle.
    assert!(
        connection_profile_cases >= 22,
        "expected >= 22 connection_profiles cases, got {connection_profile_cases} — regenerate the oracle (P4.D97 added the four thinking-default create arms; P4.55 added the three missing-`else` apiKeyId/baseUrl arms)"
    );
    // P4.D85: the connection-profile tag family (v4 Bug 74). Same stale-oracle
    // guard — before this lane v5 had NO profile tag verbs at all, so a
    // pre-P4.D85 oracle carries none of these rows and would pass by absence.
    assert!(
        profile_tag_cases >= 21,
        "expected >= 21 connection_profile_tags cases, got {profile_tag_cases} — regenerate the oracle"
    );
    // …and the three RECORDED-ONLY v4 arms specifically (the two action-gate
    // 400s + the no-action GET body). These are the rows a regeneration is most
    // likely to lose silently, because nothing on the v5 side drives them.
    assert!(
        recorded_cases == 3,
        "expected exactly 3 recorded-only v4 arms, got {recorded_cases} — regenerate the oracle"
    );
    eprintln!("settings-routes differential: {n} cases matched");
}
