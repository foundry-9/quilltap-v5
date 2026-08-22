//! Differential (W4.7f): the Serper web-search wire (`quilltap_core::tools::web_search`
//! — `build_serper_request` / `map_serper_results` / `serper_plugin_error` /
//! `format_web_search_results`) vs v4's REAL Serper plugin `executeSearch` +
//! `formatResults`. Covers the request bytes, the organic→result map, the
//! knowledgeGraph unshift boundary (under- vs at-capacity), the PLUGIN error set,
//! the network-error catch, and the `formatResults` rows (ISO date, "2 days ago"
//! → `Invalid Date`, no date, empty).
//!
//! The env-var FALLBACK error set is verified in the tier-3 `web_search_tool`
//! regen (it lives in the main-app handler, not the plugin). The fixture is
//! committed; regenerate with `record-web-search-wire.mjs` from the serper plugin
//! dir under `TZ=UTC`.
//!
//! Run:
//!   cargo test -p quilltap-harness --test web_search_wire_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::tools::web_search::{
    build_serper_request, format_web_search_results, map_serper_results, serper_plugin_error,
    WebSearchResult,
};
use serde_json::Value;

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

                // Request bytes.
                let req = build_serper_request(query, max_results, "test-key");
                let want_req = &row["request"];
                assert_eq!(
                    req.method,
                    want_req["method"].as_str().unwrap(),
                    "{case} method"
                );
                assert_eq!(req.url, want_req["url"].as_str().unwrap(), "{case} url");
                assert_eq!(
                    req.body_string(),
                    want_req["body"].as_str().unwrap(),
                    "{case} body"
                );

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
            other => panic!("{case}: unknown kind {other}"),
        }
    }

    assert!(searches >= 8, "expected the search corpus, got {searches}");
    assert!(formats >= 4, "expected the format corpus, got {formats}");
}
