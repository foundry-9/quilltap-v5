//! Differential (W4.1d5 → regenerated for W4.7f): the `search_web` tool handler
//! (`quilltap_core::tools::web_search`) vs v4's REAL handler, now with the REAL
//! `RealWebSearchProvider` (Serper wire) behind a canned [`CannedSyncWireTransport`]
//! (canned wire per case) — the provider path AND the env-var fallback path.
//! Diffs the serialized output + `format_web_search_results` against the oracle.
//!
//! Generate the oracle (Node 24, from the v4 checkout; STAGE outside `.claude/`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; WT=<worktree> ; STAGE=/tmp/qt-oracle-stage
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases
//!   cp $WT/harness/oracle/cases/web-search-tool.test.ts $STAGE/harness/oracle/cases/
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-web-search-tool.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- web-search-tool
//! Run:
//!   QT_ORACLE_WEB_SEARCH=/tmp/oracle-web-search-tool.ndjson \
//!     cargo test -p quilltap-harness --test web_search_tool_equivalence

use std::collections::HashMap;

use quilltap_core::model::wire::{wire_key, CannedSyncWireTransport, WireResponse};
use quilltap_core::tools::web_search::{
    build_serper_request, execute_web_search, format_web_search_results, NoSearchApiKeys,
    RealWebSearchProvider, SearchApiKeyLookup, WebSearchOutcome, WebSearchProvider,
};
use serde_json::{json, Value};

/// A key lookup that always returns a fixed active key (the provider path).
struct OneKey(&'static str);
impl SearchApiKeyLookup for OneKey {
    fn find_active_key(&self, _p: &str, _u: &str) -> Option<String> {
        Some(self.0.to_string())
    }
}

/// A provider that returns a fixed canned outcome (for the handler-default edge
/// that the real Serper wire never produces — `success:false, error:null`).
struct Canned(WebSearchOutcome);
impl WebSearchProvider for Canned {
    fn search(&self, _q: &str, _m: i64, _u: &str) -> WebSearchOutcome {
        self.0.clone()
    }
}

/// Build a Serper organic array entry the way the oracle's `r()` helper does.
fn organic(title: &str, date: Option<&str>) -> Value {
    let slug = title.to_lowercase().replace(' ', "-");
    let mut o = json!({
        "title": title,
        "link": format!("https://example.com/{slug}"),
        "snippet": format!("A snippet about {title}."),
    });
    if let Some(d) = date {
        o["date"] = Value::String(d.to_string());
    }
    o
}

fn transport_for(
    query: &str,
    max_results: i64,
    key: &str,
    resp: WireResponse,
) -> CannedSyncWireTransport {
    let req = build_serper_request(query, max_results, key);
    CannedSyncWireTransport::new()
        .with_raw_response(wire_key(&req.method, &req.url, &req.body_string()), resp)
}

fn load_oracle(path: &str) -> HashMap<String, Value> {
    let mut map = HashMap::new();
    for line in std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read oracle {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: Value = serde_json::from_str(line).expect("oracle line parses");
        map.insert(row["label"].as_str().unwrap().to_string(), row);
    }
    map
}

/// Run one case and diff against the oracle row.
fn check(
    oracle: &HashMap<String, Value>,
    label: &str,
    args: Value,
    provider: &dyn WebSearchProvider,
) {
    let out = execute_web_search(provider, "user-1", &args);
    let got_json = serde_json::to_string(&out).unwrap();
    let got_fmt = if out.success {
        Some(format_web_search_results(
            out.results.as_deref().unwrap_or(&[]),
        ))
    } else {
        None
    };
    let want = oracle
        .get(label)
        .unwrap_or_else(|| panic!("web-search oracle missing {label}"));
    assert_eq!(
        got_json.as_str(),
        want["resultJson"].as_str().unwrap(),
        "json {label}"
    );
    assert_eq!(
        got_fmt,
        want["formatted"].as_str().map(str::to_string),
        "formatted {label}"
    );
}

#[test]
fn web_search_tool_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_WEB_SEARCH") else {
        eprintln!("SKIP: set QT_ORACLE_WEB_SEARCH (see test header).");
        return;
    };
    let oracle = load_oracle(&oracle_path);

    // --- provider path (Serper registered, key present) ---
    let ok = |body: Value| WireResponse::new(200, body.to_string());
    let err = |status: u16, st: &str, body: &str| WireResponse {
        status,
        status_text: st.to_string(),
        body: body.to_string(),
    };

    // provider_success (default maxResults 5).
    {
        let body = json!({ "organic": [organic("Quantum leap", Some("2026-06-15T00:00:00.000Z")), organic("More news", None)] });
        let p = RealWebSearchProvider::new(
            transport_for("latest AI news", 5, "k", ok(body)),
            OneKey("k"),
            true,
            None,
        );
        check(
            &oracle,
            "provider_success",
            json!({ "query": "latest AI news" }),
            &p,
        );
    }
    // provider_success_maxresults (maxResults 3).
    {
        let body =
            json!({ "organic": [organic("Sunny in Tokyo", Some("2020-12-31T23:00:00.000Z"))] });
        let p = RealWebSearchProvider::new(
            transport_for("tokyo weather", 3, "k", ok(body)),
            OneKey("k"),
            true,
            None,
        );
        check(
            &oracle,
            "provider_success_maxresults",
            json!({ "query": "tokyo weather", "maxResults": 3 }),
            &p,
        );
    }
    // lenient_quoted_maxresults — the llmNumber read site, end to end. The canned
    // transport is keyed on the EXACT request body, and this one is built for
    // `num: 3`: if the quoted "3" survived to the wire as a string, the key would
    // miss and the case could not pass. That is the assertion.
    {
        let body =
            json!({ "organic": [organic("Sunny in Tokyo", Some("2020-12-31T23:00:00.000Z"))] });
        let p = RealWebSearchProvider::new(
            transport_for("tokyo weather", 3, "k", ok(body)),
            OneKey("k"),
            true,
            None,
        );
        check(
            &oracle,
            "lenient_quoted_maxresults",
            json!({ "query": "tokyo weather", "maxResults": "3" }),
            &p,
        );
    }
    // lenient_true_refused — `true` is REFUSED, not coerced to 1. The tool never
    // reaches the wire, so the transport is deliberately empty: any request at all
    // would fail it.
    {
        let p = RealWebSearchProvider::new(CannedSyncWireTransport::new(), OneKey("k"), true, None);
        check(
            &oracle,
            "lenient_true_refused",
            json!({ "query": "tokyo weather", "maxResults": true }),
            &p,
        );
    }
    // provider_no_results.
    {
        let body = json!({ "organic": [] });
        let p = RealWebSearchProvider::new(
            transport_for("obscure", 5, "k", ok(body)),
            OneKey("k"),
            true,
            None,
        );
        check(
            &oracle,
            "provider_no_results",
            json!({ "query": "obscure" }),
            &p,
        );
    }
    // provider errors.
    for (label, status, st) in [
        ("provider_error_401", 401u16, "Unauthorized"),
        ("provider_error_429", 429, "Too Many Requests"),
        ("provider_error_500", 500, "Internal Server Error"),
    ] {
        let body = if status == 500 { "boom" } else { "nope" };
        let p = RealWebSearchProvider::new(
            transport_for("x", 5, "k", err(status, st, body)),
            OneKey("k"),
            true,
            None,
        );
        check(&oracle, label, json!({ "query": "x" }), &p);
    }
    // provider_failure_no_error → the handler default (real wire never yields this).
    {
        let p = Canned(WebSearchOutcome::ProviderResult {
            success: false,
            results: vec![],
            error: None,
        });
        check(
            &oracle,
            "provider_failure_no_error",
            json!({ "query": "x" }),
            &p,
        );
    }
    // missing_api_key: registered but no key.
    {
        let p =
            RealWebSearchProvider::new(CannedSyncWireTransport::new(), NoSearchApiKeys, true, None);
        check(
            &oracle,
            "missing_api_key",
            json!({ "query": "needs a key" }),
            &p,
        );
    }

    // --- fallback path (no plugin, SERPER_API_KEY set) ---
    {
        let body = json!({
            "organic": [{ "title": "F1", "link": "https://f/1", "snippet": "s1", "date": "2026-01-02T00:00:00.000Z" }],
            "knowledgeGraph": { "title": "KG", "description": "kg desc", "source": { "link": "https://kg" } }
        });
        let p = RealWebSearchProvider::new(
            transport_for("fallback query", 5, "env-key", ok(body)),
            NoSearchApiKeys,
            false,
            Some("env-key".into()),
        );
        check(
            &oracle,
            "fallback_success",
            json!({ "query": "fallback query" }),
            &p,
        );
    }
    for (label, status, st, body) in [
        ("fallback_401", 401u16, "Unauthorized", "nope"),
        ("fallback_429", 429, "Too Many Requests", "slow"),
        ("fallback_500", 500, "Internal Server Error", "boom"),
    ] {
        let p = RealWebSearchProvider::new(
            transport_for("x", 5, "env-key", err(status, st, body)),
            NoSearchApiKeys,
            false,
            Some("env-key".into()),
        );
        check(&oracle, label, json!({ "query": "x" }), &p);
    }
    // not_configured: no plugin, no env key.
    {
        let p = RealWebSearchProvider::new(
            CannedSyncWireTransport::new(),
            NoSearchApiKeys,
            false,
            None,
        );
        check(&oracle, "not_configured", json!({ "query": "nothing" }), &p);
    }
    // validation (provider never consulted).
    {
        let p = RealWebSearchProvider::new(
            CannedSyncWireTransport::new(),
            NoSearchApiKeys,
            false,
            None,
        );
        check(&oracle, "validation_empty", json!({ "query": "   " }), &p);
        check(&oracle, "validation_nonobject", json!("nope"), &p);
    }

    eprintln!("OK: web-search-tool differential matched the oracle.");
}
