//! Differential (W4.7f): the Serper web-search wire (`quilltap_core::tools::web_search`
//! — `build_serper_request` / `map_serper_results` / `serper_plugin_error` /
//! `format_web_search_results`) vs v4's REAL Serper plugin `executeSearch` +
//! `formatResults`. Covers the request bytes, the organic→result map, the
//! knowledgeGraph unshift boundary (under- vs at-capacity), the PLUGIN error set,
//! the network-error catch, and the `formatResults` rows (ISO date, "2 days ago"
//! → `Invalid Date`, no date, empty).
//!
//! **P4.59 added the two things the registered arm needs now that it is LIVE:**
//! the request HEADERS (the plugin sends a `User-Agent` the legacy fallback does
//! not — a wire byte that was never compared while the plugin path was dark),
//! and the plugin's SECOND fetch site, `validateApiKey`, which the API-keys
//! screen's Test button reaches through `searchProviderRegistry.validateApiKey`.
//! The validate rows drive v5's REAL [`WireConnectionValidator`] over a canned
//! transport, so the boolean under test is the one the `?action=test` handler
//! would receive.
//!
//! The env-var FALLBACK error set is verified in the tier-3 `web_search_tool`
//! regen (it lives in the main-app handler, not the plugin). The fixture is
//! committed; regenerate with `record-web-search-wire.mjs` from the serper plugin
//! dir under `TZ=UTC`:
//!   set -euo pipefail
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server/plugins/dist/qtap-plugin-search-serper
//!   TZ=UTC node $V5W/harness/oracle/providers/record-web-search-wire.mjs \
//!     --out $V5W/harness/oracle/fixtures/web-search-wire/web-search-wire.recorded.ndjson
//! Run:
//!   cargo test -p quilltap-harness --test web_search_wire_equivalence

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use quilltap_core::api::provider_actions::WireConnectionValidator;
use quilltap_core::api::settings::ConnectionValidator;
use quilltap_core::model::wire::{wire_key, CannedSyncWireTransport, WireResponse};
use quilltap_core::tools::web_search::{
    build_serper_request, build_serper_validate_request, format_web_search_results,
    map_serper_results, serper_plugin_error, WebSearchResult,
};
use serde_json::Value;

/// The host's `Quilltap/<version>`. v4's recorded value is its OWN build version,
/// so the pin is on the header NAME + the `Quilltap/` scheme, not the number —
/// the `provider_header_common::normalize_header` precedent.
const UA: &str = "Quilltap/0.0.0-test";

/// Fold the recorded/actual headers to a comparable map: names lowercased (HTTP
/// header names are case-insensitive and the two sides spell `content-type`
/// differently), the version-bearing User-Agent and the key folded to
/// placeholders.
fn fold_headers<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> BTreeMap<String, String> {
    pairs
        .map(|(k, v)| {
            let name = k.to_lowercase();
            let value = match name.as_str() {
                "user-agent" => {
                    assert!(v.starts_with("Quilltap/"), "unexpected User-Agent {v:?}");
                    "Quilltap/<v>".to_string()
                }
                "x-api-key" => {
                    assert_eq!(v, "test-key", "the corpus uses the synthetic key");
                    "<key>".to_string()
                }
                _ => v.to_string(),
            };
            (name, value)
        })
        .collect()
}

/// The recorded request's headers, folded.
fn recorded_headers(request: &Value) -> BTreeMap<String, String> {
    let obj = request["headers"]
        .as_object()
        .expect("the recorder captures request headers (P4.59)");
    fold_headers(obj.iter().map(|(k, v)| (k.as_str(), v.as_str().unwrap())))
}

/// Assert v5's built request matches the recorded one, headers included.
fn assert_request_eq(
    got: &quilltap_core::model::request_builder::BuiltRequest,
    want: &Value,
    case: &str,
) {
    assert_eq!(
        got.method,
        want["method"].as_str().unwrap(),
        "{case} method"
    );
    assert_eq!(got.url, want["url"].as_str().unwrap(), "{case} url");
    assert_eq!(
        got.body_string(),
        want["body"].as_str().unwrap(),
        "{case} body"
    );
    assert_eq!(
        fold_headers(got.headers.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
        recorded_headers(want),
        "{case} headers"
    );
}

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/web-search-wire/web-search-wire.recorded.ndjson")
}

fn result_from_json(v: &Value) -> WebSearchResult {
    WebSearchResult {
        title: v["title"].as_str().unwrap_or_default().to_string(),
        url: v["url"].as_str().unwrap_or_default().to_string(),
        snippet: v["snippet"].as_str().unwrap_or_default().to_string(),
        published_date: v
            .get("publishedDate")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn assert_results_eq(got: &[WebSearchResult], want: &[Value], case: &str) {
    assert_eq!(got.len(), want.len(), "{case} result count");
    for (g, w) in got.iter().zip(want) {
        assert_eq!(g.title, w["title"].as_str().unwrap(), "{case} title");
        assert_eq!(g.url, w["url"].as_str().unwrap(), "{case} url");
        assert_eq!(g.snippet, w["snippet"].as_str().unwrap(), "{case} snippet");
        assert_eq!(
            g.published_date.as_deref(),
            w.get("publishedDate").and_then(Value::as_str),
            "{case} publishedDate"
        );
    }
}

#[test]
fn web_search_wire_matches_v4() {
    let text = std::fs::read_to_string(corpus_path()).expect("committed web-search-wire NDJSON");
    let mut searches = 0usize;
    let mut formats = 0usize;
    let mut validates = 0usize;

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line).unwrap();
        let case = row["case"].as_str().unwrap();

        match row["kind"].as_str().unwrap() {
            "search" => {
                searches += 1;
                let query = row["query"].as_str().unwrap();
                let max_results = row["maxResults"].as_i64().unwrap();

                // Request bytes — method / url / body AND headers. The PLUGIN
                // arm is the one that sends the User-Agent, so it is built with
                // `Some(UA)` here; the legacy fallback's header set (no UA) is
                // what `build_serper_request(.., None)` emits.
                let req = build_serper_request(query, max_results, "test-key", Some(UA));
                assert_request_eq(&req, &row["request"], case);

                let output = &row["output"];
                let success = output["success"].as_bool().unwrap();

                if let Some(net) = row["networkError"].as_str() {
                    // The plugin catch: `error.message || 'Unknown error…'`.
                    assert!(!success, "{case} network error should fail");
                    let expected = if net.is_empty() {
                        "Unknown error during Serper web search"
                    } else {
                        net
                    };
                    assert_eq!(output["error"].as_str().unwrap(), expected, "{case} catch");
                    continue;
                }

                let wire = &row["wire"];
                let status = wire["status"].as_u64().unwrap() as u16;
                let status_text = wire["statusText"].as_str().unwrap();
                let body = wire["body"].as_str().unwrap();

                if success {
                    let data: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                    let got = map_serper_results(&data, max_results);
                    assert_results_eq(&got, output["results"].as_array().unwrap(), case);
                } else {
                    let got = serper_plugin_error(status, status_text, body);
                    assert_eq!(
                        got,
                        output["error"].as_str().unwrap(),
                        "{case} plugin error"
                    );
                }
            }
            "format" => {
                formats += 1;
                let results: Vec<WebSearchResult> = row["results"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(result_from_json)
                    .collect();
                let got = format_web_search_results(&results);
                assert_eq!(got, row["formatted"].as_str().unwrap(), "{case} formatted");
            }
            "validate" => {
                validates += 1;
                // The plugin's second fetch site. v5's REAL validator answers it
                // — the same code path `POST /api/v1/api-keys/{id}?action=test`
                // reaches — over a transport canned on THIS row's wire.
                let req = build_serper_validate_request("test-key", Some(UA));
                assert_request_eq(&req, &row["request"], case);

                let key = wire_key(&req.method, &req.url, &req.body_string());
                let transport = match row["networkError"].as_str() {
                    // No canned entry for the key → the transport errors, which
                    // is the plugin's `catch { return false }` arm.
                    Some(_) => CannedSyncWireTransport::new(),
                    None => {
                        let wire = &row["wire"];
                        CannedSyncWireTransport::new().with_raw_response(
                            key,
                            WireResponse {
                                status: wire["status"].as_u64().unwrap() as u16,
                                status_text: wire["statusText"].as_str().unwrap().to_string(),
                                body: wire["body"].as_str().unwrap().to_string(),
                            },
                        )
                    }
                };
                let validator = WireConnectionValidator {
                    transport: &transport,
                    user_agent: UA,
                    base_url_env: None,
                    // P4.71: no container gateway — this family measures the
                    // search registry's inherited validateApiKey, whose
                    // `baseUrl` v4's Serper plugin ignores outright.
                    localhost_gateway: None,
                };
                // v4's plugin ignores `baseUrl` entirely (`_baseUrl`), so a
                // supplied one must not change the answer — pass one.
                let got = validator
                    .validate("SERPER", "test-key", Some("https://ignored.example"))
                    .expect("the SERPER arm never throws");
                assert_eq!(got, row["valid"].as_bool().unwrap(), "{case} valid");
            }
            other => panic!("{case}: unknown kind {other}"),
        }
    }

    assert!(searches >= 8, "expected the search corpus, got {searches}");
    assert!(formats >= 4, "expected the format corpus, got {formats}");
    // Shape: the validate corpus must carry BOTH verdicts, or a validator stuck
    // on one answer would pass green.
    assert!(
        validates >= 5,
        "expected the validate corpus (P4.59), got {validates}"
    );
}
