//! P4.6p IMAGE-PROFILES route-surface differential (+ P4.D100's `list-models`): `api::image_profiles::*` vs
//! v4's REAL image-profile route handlers, over a FRESH copy of the committed
//! groups-projects fixture per case. Create mints id + timestamps (blanked);
//! update mints updatedAt (blanked); the isDefault side effect is verified via a
//! `{name, isDefault}` dump. Error cases assert the Error kind's HTTP status +
//! message.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .test.ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-image-profiles-routes.ndjson npx jest -- image-profiles-routes
//! Run:
//!   QT_ORACLE_IMAGE_ROUTES=/tmp/oracle-image-profiles-routes.ndjson \
//!     cargo test -p quilltap-harness --test image_profiles_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::image_profiles as ip;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::image::ErasedImageDiscovery;
use quilltap_core::model::image_dialects::RealImageProvider;
use quilltap_core::model::wire::{CannedWireTransport, WireResponse};
use serde::Deserialize;
use serde_json::{json, Value};

const APIKEY: &str = "a0000001-0000-4000-8000-000000000001";
const DIANA: &str = "a1000000-0000-4000-8000-000000000004";
const IP_1: &str = "a6000000-0000-4000-8000-000000000001";
const IP_2: &str = "a6000000-0000-4000-8000-000000000002";
const IP_3: &str = "a6000000-0000-4000-8000-000000000003";
const BOGUS: &str = "a6000000-0000-4000-8000-0000000000ff";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec() -> Spec {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/groups-projects.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).expect("spec")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}
fn blank_keys(v: &mut Value, keys: &[&str]) {
    match v {
        Value::Object(o) => {
            for k in keys {
                if o.contains_key(*k) {
                    o.insert((*k).to_string(), Value::String(format!("<{k}>")));
                }
            }
            o.iter_mut().for_each(|(_, x)| blank_keys(x, keys));
        }
        Value::Array(a) => a.iter_mut().for_each(|x| blank_keys(x, keys)),
        _ => {}
    }
}
/// ⚠ PENDING P4.D138 UNIT 6 — retire this when the `list-models` read side lands.
///
/// v4 `84f33ce94` made `GET ?action=list-models` answer a `loraSupport` map
/// (`Record<modelId, ImageLoraSupport>`, between `source` and the conditional
/// `fetchError`; models resolving no support are ABSENT). The oracle for this
/// family is regenerated from a v4 pin that carries it; v5's read side is
/// P4.D138's unit 6, which is OPEN (the lane landed units 1–4 only — see the
/// order's status header). Rather than leave the four success arms red for the
/// unified gate, the key is stripped from v4's body — but ONLY after
/// [`strip_pending_lora_support`] measures the divergence in exactly the shape
/// the open unit predicts: v4 carries an OBJECT under `loraSupport`, v5 carries
/// no key at all, and nothing else differs. The moment v5 answers the key this
/// measurement reddens: flip the constant to `false` and delete the helper.
const LORA_SUPPORT_PENDING_P4D138_UNIT6: bool = true;

/// The [`LORA_SUPPORT_PENDING_P4D138_UNIT6`] measurement + strip. Returns v4's
/// body without `loraSupport` so the plain comparison can run on the rest.
fn strip_pending_lora_support(name: &str, got: &Value, want: &Value) -> Value {
    if !LORA_SUPPORT_PENDING_P4D138_UNIT6 {
        return want.clone();
    }
    assert!(
        want.get("loraSupport").is_some_and(Value::is_object),
        "[{name}] v4's list-models body must carry a `loraSupport` object at the \
         pin (84f33ce94) — a missing key means the oracle predates the LoRA train \
         and the mask is hiding something else"
    );
    assert!(
        got.get("loraSupport").is_none(),
        "[{name}] v5 now answers `loraSupport`: P4.D138 unit 6 has landed — flip \
         LORA_SUPPORT_PENDING_P4D138_UNIT6 to false and drop the strip"
    );
    let mut w = want.clone();
    w.as_object_mut().unwrap().remove("loraSupport");
    w
}

fn norm(v: &Value, blank: &[&str]) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    blank_keys(&mut v, blank);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            return format!("  GOT : {gi}\n  WANT: {wi}");
        }
    }
    "(identical)".to_string()
}
fn response_data(r: &Response) -> Value {
    serde_json::to_value(r)
        .unwrap()
        .get("data")
        .cloned()
        .unwrap_or(Value::Null)
}
fn http_for(kind: ErrorKind) -> i64 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 503,
        // The store-unavailable refusal (P4.23) — also 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-ip-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("groups-projects-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("groups-projects-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

/// The `provider_models` cache dump the list-models arms compare, mirroring the
/// oracle's projection and ordering.
fn dump_provider_models(db: &Db) -> Value {
    db.read_main(|conn| {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='provider_models'",
            [],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Ok(json!({ "providerModels": [] }));
        }
        let mut stmt = conn.prepare(
            "SELECT provider, modelId, modelType, displayName, baseUrl FROM provider_models \
             ORDER BY provider, modelType, modelId",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({
                    "provider": r.get::<_, String>(0)?,
                    "modelId": r.get::<_, String>(1)?,
                    "modelType": r.get::<_, String>(2)?,
                    "displayName": r.get::<_, String>(3)?,
                    "baseUrl": r.get::<_, Option<String>>(4)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "providerModels": rows }))
    })
    .unwrap()
}

/// Apply v4's OWN `CREATE TABLE provider_models` text (recorded by the oracle)
/// to a fresh fixture copy.
///
/// The committed `groups-projects` fixture predates the collection; v4 creates
/// it lazily on first write, v5's boot ensure creates it at startup and the
/// harness opens the file directly. Starting both sides from the same table
/// shape is what keeps "nothing cached" an OBSERVATION rather than a failed
/// write on a missing table — otherwise the cache-only-live rule would be
/// vacuously green on every built-in arm.
fn ensure_provider_models(db: &Db, ddl: &str) {
    let sql = ddl.to_string();
    db.write_blocking(move |ws| {
        ws.main()
            .connection()
            .execute_batch(&sql)
            .map_err(quilltap_core::db::DbError::from)
    })
    .expect("create provider_models");
}

/// `parameters` as stored — the P4.D138 LoRA arms' storage comparand: an
/// over-cap adapter list is KEPT, never trimmed at the write, and a refused
/// update leaves the fixture's bag untouched.
fn dump_profile_parameters(db: &Db) -> Value {
    db.read_main(|conn| {
        let mut stmt = conn.prepare("SELECT name, parameters FROM image_profiles ORDER BY name")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({
                    "name": r.get::<_, String>(0)?,
                    "parameters": r.get::<_, String>(1)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "profileParameters": rows }))
    })
    .unwrap()
}

fn dump_profiles(db: &Db) -> Value {
    db.read_main(|conn| {
        let mut stmt = conn.prepare("SELECT name, isDefault FROM image_profiles ORDER BY name")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({ "name": r.get::<_, String>(0)?, "isDefault": r.get::<_, i64>(1)? }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "profiles": rows }))
    })
    .unwrap()
}

#[test]
fn image_profiles_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_IMAGE_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_IMAGE_ROUTES (see test header).");
        return;
    };
    let spec = spec();
    let uid = spec.user_id.clone();
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    // A STALE-oracle guard: a regen from before P4.D100 carries none of the
    // list-models arms, and every one of them would otherwise surface as an
    // opaque index panic. Name them.
    for required in [
        "list_models_missing_provider",
        "list_models_unknown_provider",
        "list_models_no_key",
        "list_models_legacy_alias",
        "list_models_live_ok",
        "list_models_live_failure",
        "list_models_dangling_key",
        // P4.D138 (`84f33ce94`) — the LoRA guard arms.
        "create_loras_ok",
        "create_loras_over_cap_kept",
        "create_loras_not_a_list",
        "create_loras_two_bad_entries",
        "create_loras_guard_precedes_apikey",
        "update_loras_malformed",
    ] {
        assert!(
            oracle.contains_key(required),
            "the oracle NDJSON is stale: no `{required}` case (regenerate it — see the test header)"
        );
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();

    let ok = |name: &str, resp: &Response, blank: &[&str], failed: &mut Vec<String>| {
        if let Response::Error(e) = resp {
            eprintln!("[{name}] expected success, got {:?}: {}", e.kind, e.message);
            failed.push(name.to_string());
            return;
        }
        let want = &oracle[name]["body"];
        let got = response_data(resp);
        if norm(&got, blank) != norm(want, blank) {
            eprintln!(
                "[{name}] MISMATCH:\n{}",
                first_diff(&norm(&got, blank), &norm(want, blank))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK.");
        }
    };
    // The four success arms of `list-models` compare against v4's body with the
    // pending `loraSupport` key measured-then-stripped (P4.D138 unit 6 OPEN).
    let ok_pending_lora = |name: &str, resp: &Response, blank: &[&str], failed: &mut Vec<String>| {
        if let Response::Error(e) = resp {
            eprintln!("[{name}] expected success, got {:?}: {}", e.kind, e.message);
            failed.push(name.to_string());
            return;
        }
        let got = response_data(resp);
        let want = strip_pending_lora_support(name, &got, &oracle[name]["body"]);
        if norm(&got, blank) != norm(&want, blank) {
            eprintln!(
                "[{name}] MISMATCH:\n{}",
                first_diff(&norm(&got, blank), &norm(&want, blank))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK (loraSupport pending P4.D138 unit 6).");
        }
    };
    let check_tables_at = |name: &str, key: &str, got: &Value, failed: &mut Vec<String>| {
        let want = json!({ key: oracle[name]["tables"][key].clone() });
        if norm(got, &[]) != norm(&want, &[]) {
            eprintln!(
                "[{name} {key}] MISMATCH:\n{}",
                first_diff(&norm(got, &[]), &norm(&want, &[]))
            );
            failed.push(format!("{name}_{key}"));
        } else {
            eprintln!("[{name} {key}] OK.");
        }
    };
    let check_tables = |name: &str, got: &Value, failed: &mut Vec<String>| {
        let want = &oracle[name]["tables"];
        if norm(got, &[]) != norm(want, &[]) {
            eprintln!(
                "[{name} tables] MISMATCH:\n{}",
                first_diff(&norm(got, &[]), &norm(want, &[]))
            );
            failed.push(format!("{name}_tables"));
        } else {
            eprintln!("[{name} tables] OK.");
        }
    };
    let err = |name: &str, resp: &Response, failed: &mut Vec<String>| {
        let want = &oracle[name];
        let want_status = want["status"].as_i64().unwrap();
        let want_msg = want["body"]["error"].as_str().unwrap_or("");
        match resp {
            Response::Error(e) if http_for(e.kind) == want_status && e.message == want_msg => {
                eprintln!("[{name}] OK (err {want_status}).");
            }
            Response::Error(e) => {
                eprintln!(
                    "[{name}] ERR MISMATCH: got {}:'{}' want {}/'{}'",
                    http_for(e.kind),
                    e.message,
                    want_status,
                    want_msg
                );
                failed.push(name.to_string());
            }
            _ => {
                eprintln!("[{name}] expected error {want_status}, got success");
                failed.push(name.to_string());
            }
        }
    };

    // P4.D138 (`84f33ce94`): the LoRA guard's refusal is v4's Zod ENVELOPE, so
    // the arm compares the WHOLE body — `{error, details}` — not just the
    // sentence. A plain `err` here would go green on any `Validation error`
    // 400 whatever the issues said.
    let err_details = |name: &str, resp: &Response, failed: &mut Vec<String>| {
        let want = &oracle[name];
        let want_status = want["status"].as_i64().unwrap();
        match resp {
            Response::Error(e) => {
                let got_status = http_for(e.kind);
                let got_body = e
                    .validation_wire_body()
                    .unwrap_or_else(|| json!({ "error": e.message }));
                if got_status == want_status && norm(&got_body, &[]) == norm(&want["body"], &[]) {
                    eprintln!("[{name}] OK (err {want_status} + details).");
                } else {
                    eprintln!(
                        "[{name}] ERR MISMATCH (status {got_status} want {want_status}):\n{}",
                        first_diff(&norm(&got_body, &[]), &norm(&want["body"], &[]))
                    );
                    failed.push(name.to_string());
                }
            }
            _ => {
                eprintln!("[{name}] expected error {want_status}, got success");
                failed.push(name.to_string());
            }
        }
    };

    const CREATED: &[&str] = &["id", "createdAt", "updatedAt"];
    const UPDATED: &[&str] = &["updatedAt"];

    // --- Reads ---
    ok(
        "list_plain",
        &ip::image_profile_list(&fresh_db(&spec, "lp"), &uid, None),
        &[],
        &mut failed,
    );
    ok(
        "list_by_char",
        &ip::image_profile_list(&fresh_db(&spec, "lc"), &uid, Some(DIANA.to_string())),
        &[],
        &mut failed,
    );
    ok(
        "list_providers",
        &ip::image_provider_list(),
        &[],
        &mut failed,
    );
    ok(
        "get",
        &ip::image_profile_get(&fresh_db(&spec, "g"), IP_1),
        &[],
        &mut failed,
    );
    err(
        "get_404",
        &ip::image_profile_get(&fresh_db(&spec, "g4"), BOGUS),
        &mut failed,
    );

    // --- list-models (P4.D100 / v4 `ca22ec45`) ---
    {
        // v4's own `CREATE TABLE provider_models` text, recorded by the oracle.
        let ddl = oracle["list_models_live_ok"]["tables"]["providerModelsDDL"]
            .as_str()
            .expect("the oracle must record v4's provider_models DDL")
            .to_string();

        // A canned model list, byte-identical to the page the oracle fed v4's
        // plugin. The transport keys on the exact request signature, so a
        // request-building divergence surfaces here as a canned miss.
        let openai_models_url = "https://api.openai.com/v1/models";
        let live_page = r#"{"object":"list","data":[{"id":"gpt-4o"},{"id":"dall-e-3"},{"id":"gpt-image-1"},{"id":"text-embedding-3-small"}]}"#;
        let live_401 = r#"{"error":{"message":"Incorrect API key provided: sk-****.","type":"invalid_request_error"}}"#;
        let discovery = |status: u16, body: &str| {
            ErasedImageDiscovery::new(RealImageProvider::new(
                CannedWireTransport::new().with_response(
                    "GET",
                    openai_models_url,
                    "",
                    WireResponse::new(status, body),
                ),
            ))
        };
        // A discovery seam that must never be reached (the no-key arms).
        let unreachable =
            || ErasedImageDiscovery::new(RealImageProvider::new(CannedWireTransport::new()));

        err(
            "list_models_missing_provider",
            &rt.block_on(ip::image_profile_list_models(
                &fresh_db(&spec, "lm_np"),
                &unreachable(),
                None,
                None,
            )),
            &mut failed,
        );
        err(
            "list_models_unknown_provider",
            &rt.block_on(ip::image_profile_list_models(
                &fresh_db(&spec, "lm_up"),
                &unreachable(),
                Some("NOPE"),
                None,
            )),
            &mut failed,
        );
        {
            let db = fresh_db(&spec, "lm_nk");
            ensure_provider_models(&db, &ddl);
            ok_pending_lora(
                "list_models_no_key",
                &rt.block_on(ip::image_profile_list_models(
                    &db,
                    &unreachable(),
                    Some("OPENAI"),
                    None,
                )),
                &[],
                &mut failed,
            );
            check_tables_at(
                "list_models_no_key",
                "providerModels",
                &dump_provider_models(&db),
                &mut failed,
            );
        }
        {
            // The legacy alias resolves to GOOGLE's plugin list, but the
            // response echoes the RAW provider string.
            let db = fresh_db(&spec, "lm_alias");
            ensure_provider_models(&db, &ddl);
            ok_pending_lora(
                "list_models_legacy_alias",
                &rt.block_on(ip::image_profile_list_models(
                    &db,
                    &unreachable(),
                    Some("GOOGLE_IMAGEN"),
                    None,
                )),
                &[],
                &mut failed,
            );
            check_tables_at(
                "list_models_legacy_alias",
                "providerModels",
                &dump_provider_models(&db),
                &mut failed,
            );
        }
        {
            let db = fresh_db(&spec, "lm_ok");
            ensure_provider_models(&db, &ddl);
            ok_pending_lora(
                "list_models_live_ok",
                &rt.block_on(ip::image_profile_list_models(
                    &db,
                    &discovery(200, live_page),
                    Some("OPENAI"),
                    Some(APIKEY),
                )),
                &[],
                &mut failed,
            );
            check_tables_at(
                "list_models_live_ok",
                "providerModels",
                &dump_provider_models(&db),
                &mut failed,
            );
        }
        {
            let db = fresh_db(&spec, "lm_fail");
            ensure_provider_models(&db, &ddl);
            ok_pending_lora(
                "list_models_live_failure",
                &rt.block_on(ip::image_profile_list_models(
                    &db,
                    &discovery(401, live_401),
                    Some("OPENAI"),
                    Some(APIKEY),
                )),
                &[],
                &mut failed,
            );
            check_tables_at(
                "list_models_live_failure",
                "providerModels",
                &dump_provider_models(&db),
                &mut failed,
            );
        }
        err(
            "list_models_dangling_key",
            &rt.block_on(ip::image_profile_list_models(
                &fresh_db(&spec, "lm_dk"),
                &unreachable(),
                Some("OPENAI"),
                Some("00000000-0000-4000-8000-0000000000ff"),
            )),
            &mut failed,
        );
    }

    // --- Create ---
    {
        let db = fresh_db(&spec, "c_happy");
        let body = json!({
            "name": "New Imagery",
            "provider": "OPENAI",
            "apiKeyId": APIKEY,
            "baseUrl": "http://127.0.0.1:2/v1",
            "modelName": "  gpt-image-1  ",
            "parameters": { "steps": 25 },
            "isDangerousCompatible": true
        });
        ok(
            "create_happy",
            &rt.block_on(ip::image_profile_create(&db, &uid, body)),
            CREATED,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "c_def");
        let body = json!({ "name": "Defaulted", "provider": "OPENAI", "modelName": "gpt-image-1", "isDefault": true });
        ok(
            "create_default_unsets",
            &rt.block_on(ip::image_profile_create(&db, &uid, body)),
            CREATED,
            &mut failed,
        );
        check_tables("create_default_unsets", &dump_profiles(&db), &mut failed);
    }
    err(
        "create_name_required",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "cnr"),
            &uid,
            json!({}),
        )),
        &mut failed,
    );
    err(
        "create_provider_required",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "cpr"),
            &uid,
            json!({ "name": "X" }),
        )),
        &mut failed,
    );
    err(
        "create_model_required",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "cmr"),
            &uid,
            json!({ "name": "X", "provider": "OPENAI" }),
        )),
        &mut failed,
    );
    err(
        "create_params_not_object",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "cpo"),
            &uid,
            json!({ "name": "X", "provider": "OPENAI", "modelName": "m", "parameters": [1, 2] }),
        )),
        &mut failed,
    );
    err(
        "create_apikey_404",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "ca4"),
            &uid,
            json!({ "name": "X", "provider": "OPENAI", "modelName": "m", "apiKeyId": "00000000-0000-4000-8000-0000000000ff" }),
        )),
        &mut failed,
    );
    err(
        "create_dup_409",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "cd4"),
            &uid,
            json!({ "name": "Primary Imagery", "provider": "OPENAI", "modelName": "m" }),
        )),
        &mut failed,
    );

    // --- Update ---
    ok(
        "update_apikey_null",
        &rt.block_on(ip::image_profile_update(
            &fresh_db(&spec, "uak"),
            &uid,
            IP_1,
            json!({ "apiKeyId": null }),
        )),
        UPDATED,
        &mut failed,
    );
    ok(
        "update_baseurl_empty",
        &rt.block_on(ip::image_profile_update(
            &fresh_db(&spec, "ube"),
            &uid,
            IP_1,
            json!({ "baseUrl": "" }),
        )),
        UPDATED,
        &mut failed,
    );
    // P4.55 (the missing-`else` sub-family): a present non-string `apiKeyId`
    // used to be dropped silently for a 200; v4 falls into
    // `findApiKeyById(<non-string>)` and answers 404. Measured on v4 for both
    // `5` and `{}`.
    err(
        "update_apikey_non_string",
        &rt.block_on(ip::image_profile_update(
            &fresh_db(&spec, "uans"),
            &uid,
            IP_1,
            json!({ "apiKeyId": 5 }),
        )),
        &mut failed,
    );
    err(
        "update_apikey_object",
        &rt.block_on(ip::image_profile_update(
            &fresh_db(&spec, "uao"),
            &uid,
            IP_1,
            json!({ "apiKeyId": {} }),
        )),
        &mut failed,
    );
    {
        // The `baseUrl || null` sibling: a TRUTHY non-string is assigned
        // verbatim by v4 and the row validation then rejects it → the route's
        // fixed 500, nothing written. v5 used to collapse it to null and
        // silently CLEAR the column.
        let db = fresh_db(&spec, "ubns");
        err(
            "update_baseurl_non_string",
            &rt.block_on(ip::image_profile_update(
                &db,
                &uid,
                IP_1,
                json!({ "baseUrl": 5 }),
            )),
            &mut failed,
        );
        check_tables(
            "update_baseurl_non_string",
            &dump_profiles(&db),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "uid");
        ok(
            "update_isdefault",
            &rt.block_on(ip::image_profile_update(
                &db,
                &uid,
                IP_2,
                json!({ "isDefault": true }),
            )),
            UPDATED,
            &mut failed,
        );
        check_tables("update_isdefault", &dump_profiles(&db), &mut failed);
    }

    // --- Delete ---
    ok(
        "delete",
        &rt.block_on(ip::image_profile_delete(&fresh_db(&spec, "d"), IP_3)),
        &[],
        &mut failed,
    );
    err(
        "delete_404",
        &rt.block_on(ip::image_profile_delete(&fresh_db(&spec, "d4"), BOGUS)),
        &mut failed,
    );

    // --- 84f33ce94: the write-side `parameters.loras` guard ---
    {
        let db = fresh_db(&spec, "lok");
        ok(
            "create_loras_ok",
            &rt.block_on(ip::image_profile_create(
                &db,
                &uid,
                json!({
                    "name": "LoRA Profile",
                    "provider": "NANOGPT",
                    "modelName": "flux-2-dev-lora",
                    "parameters": { "loras": [
                        { "source": "owner/adapter", "scale": 0.8, "triggerPhrase": "shou_xin", "label": "Shou Xin" },
                        { "source": "https://example.test/w.safetensors" }
                    ]}
                }),
            )),
            CREATED,
            &mut failed,
        );
    }
    {
        // No cap check on the write path: three adapters against a one-adapter
        // model save intact, and the STORED bag proves it (not just the echo).
        let db = fresh_db(&spec, "locap");
        ok(
            "create_loras_over_cap_kept",
            &rt.block_on(ip::image_profile_create(
                &db,
                &uid,
                json!({
                    "name": "Over Cap",
                    "provider": "NANOGPT",
                    "modelName": "flux-lora",
                    "parameters": { "loras": [{ "source": "a/1" }, { "source": "a/2" }, { "source": "a/3" }] }
                }),
            )),
            CREATED,
            &mut failed,
        );
        check_tables(
            "create_loras_over_cap_kept",
            &dump_profile_parameters(&db),
            &mut failed,
        );
    }
    for (name, tag, params) in [
        (
            "create_loras_not_a_list",
            "lnl",
            json!({ "loras": "owner/adapter" }),
        ),
        ("create_loras_null", "lnu", json!({ "loras": Value::Null })),
        (
            "create_loras_entry_not_object",
            "leo",
            json!({ "loras": ["a/b"] }),
        ),
        (
            "create_loras_missing_source",
            "lms",
            json!({ "loras": [{ "scale": 1 }] }),
        ),
        (
            "create_loras_blank_source",
            "lbs",
            json!({ "loras": [{ "source": "   " }] }),
        ),
        (
            "create_loras_scale_negative",
            "lsn",
            json!({ "loras": [{ "source": "a/b", "scale": -1 }] }),
        ),
        (
            "create_loras_scale_too_big",
            "lstb",
            json!({ "loras": [{ "source": "a/b", "scale": 10.5 }] }),
        ),
        (
            "create_loras_scale_not_a_number",
            "lsna",
            json!({ "loras": [{ "source": "a/b", "scale": "1" }] }),
        ),
        (
            "create_loras_two_bad_entries",
            "ltbe",
            json!({ "loras": [{ "source": "" }, { "source": "a/b", "scale": 99 }] }),
        ),
    ] {
        err_details(
            name,
            &rt.block_on(ip::image_profile_create(
                &fresh_db(&spec, tag),
                &uid,
                json!({ "name": "X", "provider": "OPENAI", "modelName": "m", "parameters": params }),
            )),
            &mut failed,
        );
    }
    // ORDER PIN: the guard sits between the parameters-object check and the
    // apiKeyId lookup, so a body wrong in BOTH ways answers the LoRA 400.
    err_details(
        "create_loras_guard_precedes_apikey",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "lgpa"),
            &uid,
            json!({
                "name": "X", "provider": "OPENAI", "modelName": "m",
                "parameters": { "loras": [{ "source": "" }] },
                "apiKeyId": "00000000-0000-4000-8000-0000000000ff"
            }),
        )),
        &mut failed,
    );
    // ORDER PIN (the other side): the caller's own check owns a non-object bag,
    // so the LoRA validator never runs and the sentence is the plain one.
    err_details(
        "create_params_array_still_wins",
        &rt.block_on(ip::image_profile_create(
            &fresh_db(&spec, "lpas"),
            &uid,
            json!({ "name": "X", "provider": "OPENAI", "modelName": "m", "parameters": [{ "loras": [] }] }),
        )),
        &mut failed,
    );
    {
        let db = fresh_db(&spec, "ulok");
        ok(
            "update_loras_ok",
            &rt.block_on(ip::image_profile_update(
                &db,
                &uid,
                IP_1,
                json!({ "parameters": { "quality": "hd", "loras": [{ "source": "owner/adapter", "triggerPhrase": "magic" }] } }),
            )),
            UPDATED,
            &mut failed,
        );
        check_tables(
            "update_loras_ok",
            &dump_profile_parameters(&db),
            &mut failed,
        );
    }
    {
        // The refusal writes NOTHING: the stored bag is the fixture's.
        let db = fresh_db(&spec, "ulmal");
        err_details(
            "update_loras_malformed",
            &rt.block_on(ip::image_profile_update(
                &db,
                &uid,
                IP_1,
                json!({ "parameters": { "loras": [{ "source": "a/b", "scale": 42 }] } }),
            )),
            &mut failed,
        );
        check_tables(
            "update_loras_malformed",
            &dump_profile_parameters(&db),
            &mut failed,
        );
    }
    err_details(
        "update_loras_not_a_list",
        &rt.block_on(ip::image_profile_update(
            &fresh_db(&spec, "ulnl"),
            &uid,
            IP_1,
            json!({ "parameters": { "loras": 7 } }),
        )),
        &mut failed,
    );

    assert!(
        failed.is_empty(),
        "image-profiles-routes FAILED: {failed:?}"
    );
}

/// `validate-key` is the one remaining loud typed refusal. `imageProfileGenerate`
/// was un-refused in P4.6ai (its not-assembled refusal now lives at the engine's
/// `image_generation` seam gate); `list-models` was un-refused in P4.D100, and
/// its own not-assembled refusal likewise moved to the engine's discovery gate.
#[test]
fn image_refusal_arms_are_loud() {
    for resp in [ip::image_profile_validate_key()] {
        match resp {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::Internal);
                assert!(
                    e.message.contains("recognized but not yet available"),
                    "unexpected refusal message: {}",
                    e.message
                );
            }
            _ => panic!("refusal arm returned success"),
        }
    }
}
