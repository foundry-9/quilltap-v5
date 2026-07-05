//! W4.2 differential #3: the dangerous-content **provider-routing matrix** (v4
//! `resolveProviderForDangerousContent` / `resolveImageProviderForDangerousContent`
//! / `resolveUncensoredImageProfileForReroute` / `isImageModerationError`).
//!
//! Drives the ported functions over a baked `connection_profiles` /
//! `image_profiles` fixture and compares the routing decision field-by-field
//! (`rerouted` / the chosen profile identity / the resolved `apiKey` / the exact
//! `reason` string) against v4's REAL functions. API-key resolution is a canned
//! seam on both sides — the injected [`ApiKeyResolver`] returns keys from the
//! spec's `apiKeys` map (the oracle monkey-patches
//! `findApiKeyByIdAndUserId` to the same map).
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-danger-routing.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-danger-routing-fixture.ts
//!   TMPO=/tmp/qt-danger-oracle; mkdir -p $TMPO/cases $TMPO/fixtures
//!   cp $V5W/harness/oracle/cases/danger-routing.test.ts $TMPO/cases/
//!   cp $V5W/harness/oracle/fixtures/danger-routing.json $TMPO/fixtures/
//!   QT_FIXTURE_ROUTING=/tmp/qt-danger-routing.db QT_ORACLE_OUT=/tmp/oracle-danger-routing.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- danger-routing
//! Run:
//!   QT_ORACLE_DANGER_ROUTING=/tmp/oracle-danger-routing.ndjson \
//!   QT_FIXTURE_ROUTING=/tmp/qt-danger-routing.db \
//!     cargo test -p quilltap-harness --test danger_routing_equivalence

use std::collections::HashMap;

use quilltap_core::db::runtime::Db;
use quilltap_core::db::{connection_profiles, image_profiles};
use quilltap_core::services::dangerous_content::provider_routing::{
    is_image_moderation_error, resolve_image_provider_for_dangerous_content,
    resolve_provider_for_dangerous_content, resolve_uncensored_image_profile_for_reroute,
    ApiKeyResolver, RouteProfile,
};
use serde::Deserialize;
use serde_json::{json, Value};

/// The canned key seam: `apiKeyId` → key, from the spec (the oracle patches
/// `findApiKeyByIdAndUserId` to the same map).
struct CannedApiKeys(HashMap<String, String>);
impl ApiKeyResolver for CannedApiKeys {
    fn resolve(&self, api_key_id: &str, _user_id: &str) -> Option<String> {
        self.0.get(api_key_id).cloned()
    }
}

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userA")]
    user_a: String,
    #[serde(rename = "userB")]
    user_b: String,
    #[serde(rename = "apiKeys")]
    api_keys: HashMap<String, String>,
    #[serde(rename = "textCases")]
    text_cases: Vec<CaseSpec>,
    #[serde(rename = "imageCases")]
    image_cases: Vec<CaseSpec>,
    #[serde(rename = "rerouteCases")]
    reroute_cases: Vec<CaseSpec>,
    #[serde(rename = "imgErrors")]
    img_errors: Vec<String>,
}

#[derive(Deserialize)]
struct CaseSpec {
    id: String,
    user: String,
    #[serde(rename = "originalProfileId", default)]
    original_profile_id: Option<String>,
    #[serde(rename = "currentProfileId", default)]
    current_profile_id: Option<String>,
    #[serde(rename = "originalApiKey", default)]
    original_api_key: Option<String>,
    mode: String,
    #[serde(rename = "uncensoredTextProfileId", default)]
    uncensored_text_profile_id: Option<String>,
    #[serde(rename = "uncensoredImageProfileId", default)]
    uncensored_image_profile_id: Option<String>,
}

/// Oracle rows (tagged by `kind`).
fn oracle_rows(text: &str) -> HashMap<(String, String), Value> {
    let mut map = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse routing oracle row");
        let kind = v.get("kind").and_then(Value::as_str).unwrap().to_string();
        let id = v
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                v.get("message")
                    .and_then(Value::as_str)
                    .unwrap()
                    .to_string()
            });
        map.insert((kind, id), v);
    }
    map
}

fn route_profile_from_value(v: &Value) -> RouteProfile {
    RouteProfile {
        id: v
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        name: v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        provider: v
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        model_name: v
            .get("modelName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        base_url: v.get("baseUrl").and_then(Value::as_str).map(str::to_string),
    }
}

fn profile_subset(p: &RouteProfile) -> Value {
    json!({
        "id": p.id,
        "name": p.name,
        "provider": p.provider,
        "modelName": p.model_name,
        "baseUrl": p.base_url,
    })
}

#[tokio::test]
async fn danger_routing_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_DANGER_ROUTING") else {
        eprintln!("SKIP: set QT_ORACLE_DANGER_ROUTING to the routing NDJSON (see header).");
        return;
    };
    let Ok(fixture) = std::env::var("QT_FIXTURE_ROUTING") else {
        eprintln!("SKIP: set QT_FIXTURE_ROUTING to the seed fixture .db (see header).");
        return;
    };

    let spec_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/danger-routing.json");
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(&spec_path).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse routing spec");
    let oracle = oracle_rows(
        &std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}")),
    );

    let uid = |u: &str| {
        if u == "A" {
            spec.user_a.clone()
        } else {
            spec.user_b.clone()
        }
    };
    let api_keys = CannedApiKeys(spec.api_keys.clone());

    let work =
        std::env::temp_dir().join(format!("qt-danger-routing-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));
    let db = Db::open_main(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    // --- text cases ---
    for c in &spec.text_cases {
        let original_id = c.original_profile_id.clone().unwrap();
        let original = db
            .read_main(|conn| connection_profiles::find_by_id(conn, &original_id))
            .expect("read original")
            .unwrap_or_else(|| panic!("text {}: original missing", c.id));
        let original = route_profile_from_value(&original);
        let user = uid(&c.user);
        let result = db
            .read_main(|conn| {
                Ok(resolve_provider_for_dangerous_content(
                    conn,
                    &api_keys,
                    &original,
                    c.original_api_key.as_deref().unwrap(),
                    &c.mode,
                    c.uncensored_text_profile_id.as_deref(),
                    &user,
                ))
            })
            .expect("resolve text");
        let got = json!({
            "rerouted": result.rerouted,
            "profile": profile_subset(&result.connection_profile),
            "apiKey": result.api_key,
            "reason": result.reason,
        });
        let want = &oracle[&("text".into(), c.id.clone())];
        assert_eq!(got["rerouted"], want["rerouted"], "text {} rerouted", c.id);
        assert_eq!(got["profile"], want["profile"], "text {} profile", c.id);
        assert_eq!(got["apiKey"], want["apiKey"], "text {} apiKey", c.id);
        assert_eq!(got["reason"], want["reason"], "text {} reason", c.id);
    }

    // --- image cases ---
    for c in &spec.image_cases {
        let original_id = c.original_profile_id.clone().unwrap();
        let original = db
            .read_main(|conn| image_profiles::find_by_id(conn, &original_id))
            .expect("read img original")
            .unwrap_or_else(|| panic!("image {}: original missing", c.id));
        let original = route_profile_from_value(&original);
        let user = uid(&c.user);
        let result = db
            .read_main(|conn| {
                Ok(resolve_image_provider_for_dangerous_content(
                    conn,
                    &api_keys,
                    &original,
                    c.original_api_key.as_deref().unwrap(),
                    &c.mode,
                    c.uncensored_image_profile_id.as_deref(),
                    &user,
                ))
            })
            .expect("resolve image");
        let got = json!({
            "rerouted": result.rerouted,
            "profile": profile_subset(&result.image_profile),
            "apiKey": result.api_key,
            "reason": result.reason,
        });
        let want = &oracle[&("image".into(), c.id.clone())];
        assert_eq!(got["rerouted"], want["rerouted"], "image {} rerouted", c.id);
        assert_eq!(got["profile"], want["profile"], "image {} profile", c.id);
        assert_eq!(got["apiKey"], want["apiKey"], "image {} apiKey", c.id);
        assert_eq!(got["reason"], want["reason"], "image {} reason", c.id);
    }

    // --- reroute cases ---
    for c in &spec.reroute_cases {
        let current_id = c.current_profile_id.clone().unwrap();
        let user = uid(&c.user);
        let result = db
            .read_main(|conn| {
                resolve_uncensored_image_profile_for_reroute(
                    conn,
                    &api_keys,
                    &current_id,
                    &c.mode,
                    c.uncensored_image_profile_id.as_deref(),
                    &user,
                )
            })
            .expect("resolve reroute");
        let got = match result {
            Some(r) => json!({ "profile": profile_subset(&r.profile), "apiKey": r.api_key }),
            None => Value::Null,
        };
        let want = &oracle[&("reroute".into(), c.id.clone())];
        assert_eq!(got, want["result"], "reroute {}", c.id);
    }

    drop(db);
    let _ = std::fs::remove_file(&work);

    // --- pure isImageModerationError ---
    for msg in &spec.img_errors {
        let want = &oracle[&("imgerr".into(), msg.clone())];
        assert_eq!(
            is_image_moderation_error(msg),
            want["out"].as_bool().unwrap(),
            "imgerr {msg:?}"
        );
    }

    eprintln!("OK: danger routing matched oracle.");
}
