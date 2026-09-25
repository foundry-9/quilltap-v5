//! The dangerous-content **provider-routing matrix** — since v4 `8bd080267`
//! (#73, P4.D225) the two thin pre-flight wrappers
//! (`resolveProviderForDangerousContent` / `resolveImageProviderForDangerousContent`)
//! AND the understudies they delegate to (`resolveUncensoredTextUnderstudy` /
//! `resolveUncensoredImageUnderstudy`, driven directly for the arms the
//! wrappers cannot reach: an arbitrary `exclude`, a caller's `filter`, a
//! throwing key lookup, a failing profile lookup).
//!
//! Compares the routing decision field-by-field (`rerouted` / the chosen
//! profile identity / the resolved `apiKey` / the exact `reason` string) AND
//! every line the `ConciergeUnderstudy` and `DangerousContentProviderRouting`
//! loggers wrote, in order (level, message, every bag field; camelCase keys →
//! the repo's snake_case fields; a string array → its `Debug` rendering; the
//! one lookup-failure `error` value normalised on both sides, since v4's is a
//! thrown JS message and v5's a rusqlite error). API-key resolution is a canned
//! seam on both sides — the spec's `apiKeys` map, plus `throwingApiKeys` whose
//! lookup THROWS in v4 / answers `Err` through `try_resolve` in v5.
//!
//! RETIRED rows (P4.D225): v4 deleted `resolveUncensoredImageProfileForReroute`
//! and `isImageModerationError` at `8bd080267` — the unchanged oracle case dies
//! at the target pin with `TypeError: routing.resolveUncensoredImageProfileFor
//! Reroute is not a function` (the recorded red-first). Their spec rows
//! (`rerouteCases`, `imgErrors`) stay in the committed JSON and are not driven
//! on either side.
//!
//! Generate (Node 24, from the v4 checkout; stage the case OUTSIDE any
//! `.claude/` path — v4's jest ignores `/\.claude/`). While v4 HEAD is past the
//! oracle baseline this needs a PINNED worktree; that is the sweep driver's
//! `--v4`, never a path in this header.
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -f /tmp/qt-danger-routing.db
//!   QT_FIXTURE_OUT=/tmp/qt-danger-routing.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-danger-routing-fixture.ts
//!   TMPO=/tmp/qt-danger-oracle; rm -rf $TMPO; mkdir -p $TMPO/cases $TMPO/fixtures
//!   cp $V5W/harness/oracle/cases/danger-routing.test.ts $TMPO/cases/
//!   cp $V5W/harness/oracle/fixtures/danger-routing.json $TMPO/fixtures/
//!   rm -f /tmp/oracle-danger-routing.ndjson
//!   QT_FIXTURE_ROUTING=/tmp/qt-danger-routing.db QT_ORACLE_OUT=/tmp/oracle-danger-routing.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- "cases/danger-routing\.test\.ts$"
//! Run:
//!   QT_ORACLE_DANGER_ROUTING=/tmp/oracle-danger-routing.ndjson \
//!   QT_FIXTURE_ROUTING=/tmp/qt-danger-routing.db \
//!     cargo test -p quilltap-harness --test danger_routing_equivalence

use std::collections::HashMap;

use quilltap_core::db::runtime::Db;
use quilltap_core::db::{connection_profiles, image_profiles};
use quilltap_core::services::dangerous_content::provider_routing::{
    resolve_image_provider_for_dangerous_content, resolve_provider_for_dangerous_content,
    ApiKeyResolver, RouteProfile,
};
use quilltap_core::services::dangerous_content::understudy::{
    resolve_uncensored_image_understudy, resolve_uncensored_text_understudy, ImageUnderstudyLookup,
    TextUnderstudyLookup,
};
use quilltap_core::test_support::captured_with;
use serde::Deserialize;
use serde_json::{json, Value};

/// The canned key seam: `apiKeyId` → key, from the spec (the oracle patches
/// `findApiKeyByIdAndUserId` to the same map).
struct CannedApiKeys {
    keys: HashMap<String, String>,
    throwing: Vec<String>,
}
impl ApiKeyResolver for CannedApiKeys {
    fn resolve(&self, api_key_id: &str, user_id: &str) -> Option<String> {
        self.try_resolve(api_key_id, user_id).ok().flatten()
    }
    fn try_resolve(&self, api_key_id: &str, _user_id: &str) -> Result<Option<String>, String> {
        if self.throwing.iter().any(|t| t == api_key_id) {
            return Err("canned key lookup failure".to_string());
        }
        Ok(self.keys.get(api_key_id).cloned())
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
    #[serde(rename = "throwingApiKeys", default)]
    throwing_api_keys: Vec<String>,
    #[serde(rename = "textCases")]
    text_cases: Vec<CaseSpec>,
    #[serde(rename = "imageCases")]
    image_cases: Vec<CaseSpec>,
    // `rerouteCases` / `imgErrors`: RETIRED with v4's deleted functions — the
    // committed JSON keeps them; nothing reads them.
    #[serde(rename = "textUnderstudyCases")]
    text_understudy_cases: Vec<UnderstudyCase>,
    #[serde(rename = "imageUnderstudyCases")]
    image_understudy_cases: Vec<UnderstudyCase>,
}

#[derive(Deserialize)]
struct UnderstudyCase {
    id: String,
    user: String,
    #[serde(rename = "uncensoredTextProfileId", default)]
    uncensored_text_profile_id: Option<String>,
    #[serde(rename = "uncensoredImageProfileId", default)]
    uncensored_image_profile_id: Option<String>,
    exclude: Vec<String>,
    #[serde(rename = "turnAttachmentMimeTypes", default)]
    turn_attachment_mime_types: Vec<String>,
    #[serde(rename = "filterProviders", default)]
    filter_providers: Option<Vec<String>>,
    #[serde(rename = "failLookup", default)]
    fail_lookup: bool,
}

#[derive(Deserialize)]
struct CaseSpec {
    id: String,
    user: String,
    #[serde(rename = "originalProfileId", default)]
    original_profile_id: Option<String>,
    #[serde(rename = "originalApiKey", default)]
    original_api_key: Option<String>,
    mode: String,
    #[serde(rename = "uncensoredTextProfileId", default)]
    uncensored_text_profile_id: Option<String>,
    #[serde(rename = "uncensoredImageProfileId", default)]
    uncensored_image_profile_id: Option<String>,
    /// v4's fifth parameter since `a1d88aa3a` (bug 106) — the MIME types this
    /// turn's message array carries. Absent takes v4's `[]` default, which is
    /// why every pre-existing case's row is unchanged.
    #[serde(rename = "turnAttachmentMimeTypes", default)]
    turn_attachment_mime_types: Vec<String>,
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

fn snake(k: &str) -> String {
    let mut out = String::new();
    for c in k.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The one lookup-failure `error` value is normalised on both sides.
fn normalise(line: &str) -> String {
    match line.find(" error=") {
        Some(i) if line.contains("understudy lookup failed") => {
            format!("{} error=<error>", &line[..i])
        }
        _ => line.to_string(),
    }
}

/// v4's recorded lines, rendered as the capture rig renders v5's.
fn v4_lines(row: &Value) -> Vec<String> {
    row["logs"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .map(|l| {
            let target = match l["service"].as_str().unwrap() {
                "ConciergeUnderstudy" => "quilltap::concierge_understudy",
                "DangerousContentProviderRouting" => "quilltap::dangerous_content_routing",
                other => panic!("unexpected service {other}"),
            };
            let mut line = format!(
                "{} {target} {}",
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap()
            );
            for (k, v) in l["bag"].as_object().unwrap() {
                let rendered = match v {
                    Value::String(s) => s.clone(),
                    Value::Array(a) => format!(
                        "{:?}",
                        a.iter()
                            .map(|x| x.as_str().unwrap_or("").to_string())
                            .collect::<Vec<_>>()
                    ),
                    other => other.to_string(),
                };
                line.push_str(&format!(" {}={rendered}", snake(k)));
            }
            normalise(&line)
        })
        .collect()
}

fn v5_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| {
            l.contains(" quilltap::concierge_understudy ")
                || l.contains(" quilltap::dangerous_content_routing ")
        })
        .map(|l| normalise(l))
        .collect()
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

#[test]
fn danger_routing_matches_oracle() {
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
    let api_keys = CannedApiKeys {
        keys: spec.api_keys.clone(),
        throwing: spec.throwing_api_keys.clone(),
    };
    let mut log_rows = 0usize;

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
        let (result, lines) = captured_with(|| {
            db.read_main(|conn| {
                Ok(resolve_provider_for_dangerous_content(
                    conn,
                    &api_keys,
                    &original,
                    c.original_api_key.as_deref().unwrap(),
                    &c.mode,
                    c.uncensored_text_profile_id.as_deref(),
                    &user,
                    &c.turn_attachment_mime_types,
                ))
            })
            .expect("resolve text")
        });
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
        assert_eq!(v5_lines(&lines), v4_lines(want), "text {} lines", c.id);
        log_rows += usize::from(!v4_lines(want).is_empty());
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
        let (result, lines) = captured_with(|| {
            db.read_main(|conn| {
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
            .expect("resolve image")
        });
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
        assert_eq!(v5_lines(&lines), v4_lines(want), "image {} lines", c.id);
        log_rows += usize::from(!v4_lines(want).is_empty());
    }

    // A connection with no tables: every profile read fails, as v4's patched
    // `findAll` throws — the understudy swallows it (ERROR + `None`).
    let broken = rusqlite::Connection::open_in_memory().unwrap();

    // --- the text understudy, driven directly ---
    for c in &spec.text_understudy_cases {
        let user = uid(&c.user);
        let providers = c.filter_providers.clone();
        let filter = move |p: &Value| {
            let provider = p.get("provider").and_then(Value::as_str).unwrap_or("");
            providers
                .as_ref()
                .is_some_and(|ps| ps.iter().any(|x| x == provider))
        };
        let lookup = TextUnderstudyLookup {
            user_id: &user,
            uncensored_text_profile_id: c.uncensored_text_profile_id.as_deref(),
            exclude: &c.exclude,
            turn_attachment_mime_types: &c.turn_attachment_mime_types,
            filter: if c.filter_providers.is_some() {
                Some(&filter)
            } else {
                None
            },
        };
        let (result, lines) = captured_with(|| {
            if c.fail_lookup {
                resolve_uncensored_text_understudy(&broken, &api_keys, lookup)
            } else {
                db.read_main(|conn| Ok(resolve_uncensored_text_understudy(conn, &api_keys, lookup)))
                    .expect("text understudy")
            }
        });
        let got = match result {
            Some(u) => json!({ "profile": profile_subset(&u.profile), "apiKey": u.api_key }),
            None => Value::Null,
        };
        let want = &oracle[&("textUnderstudy".into(), c.id.clone())];
        assert_eq!(got, want["result"], "textUnderstudy {}", c.id);
        assert_eq!(
            v5_lines(&lines),
            v4_lines(want),
            "textUnderstudy {} lines",
            c.id
        );
        log_rows += 1;
    }

    // --- the image understudy, driven directly ---
    for c in &spec.image_understudy_cases {
        let user = uid(&c.user);
        let lookup = ImageUnderstudyLookup {
            user_id: &user,
            uncensored_image_profile_id: c.uncensored_image_profile_id.as_deref(),
            exclude: &c.exclude,
        };
        let (result, lines) = captured_with(|| {
            if c.fail_lookup {
                resolve_uncensored_image_understudy(&broken, &api_keys, lookup)
            } else {
                db.read_main(|conn| {
                    Ok(resolve_uncensored_image_understudy(conn, &api_keys, lookup))
                })
                .expect("image understudy")
            }
        });
        let got = match result {
            Some(u) => json!({ "profile": profile_subset(&u.profile), "apiKey": u.api_key }),
            None => Value::Null,
        };
        let want = &oracle[&("imageUnderstudy".into(), c.id.clone())];
        assert_eq!(got, want["result"], "imageUnderstudy {}", c.id);
        assert_eq!(
            v5_lines(&lines),
            v4_lines(want),
            "imageUnderstudy {} lines",
            c.id
        );
        log_rows += 1;
    }

    drop(db);
    let _ = std::fs::remove_file(&work);

    // Corpus floors: every kind driven, and the lines genuinely compared.
    assert!(spec.text_understudy_cases.len() >= 12 && spec.image_understudy_cases.len() >= 7);
    assert!(
        log_rows >= 40,
        "expected the log-bearing rows, got {log_rows}"
    );
    eprintln!("OK: danger routing matched oracle ({log_rows} rows with lines).");
}
