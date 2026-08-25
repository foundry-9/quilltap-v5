//! Differential (W4.1d5 → W4.7f → P4.59): the `search_web` tool handler
//! (`quilltap_core::tools::web_search`) vs v4's REAL handler.
//!
//! Both sides now run the registered arm for real. v4's oracle initializes the
//! REAL `searchProviderRegistry` with the REAL built Serper plugin and lets the
//! plugin's own `executeSearch` / `formatResults` run over a mocked `fetch`;
//! this side drives [`RealWebSearchProvider`] with `serper_registered = true`
//! over a [`CannedSyncWireTransport`] carrying the same wire.
//!
//! **The api-key lookup is the other half.** v4 resolves the per-call key with
//! `getAllApiKeys().find(k => k.provider === 'SERPER' && k.isActive)`; the
//! oracle mocks only that repository read (with a realistic multi-row list) so
//! v4's own predicate decides. This side goes through the production
//! [`DbSearchApiKeys`] over a REAL provisioned instance whose `api_keys` rows
//! are written by the REAL repository — one instance, one user per row-set, so
//! the read is user-scoped exactly as production's is. Arms: an active row, an
//! inactive-only row, another provider's row, an inactive row that must be
//! SKIPPED in favour of a later active one, and two active rows where the FIRST
//! wins.
//!
//! Two precedence arms pin what registration changes: with BOTH a registered
//! provider and `SERPER_API_KEY` set, v4 takes the provider path (the tell is
//! the PLUGIN's 401 sentence, not the fallback's), and a registered-but-keyless
//! provider refuses with `MissingApiKey` rather than falling back to the env key.
//!
//! Regenerate the oracle + run (Node 24; STAGE outside `.claude/`):
//!   set -euo pipefail
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   STAGE=/tmp/qt-oracle-stage-web-search
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases
//!   cp $V5W/harness/oracle/cases/web-search-tool.test.ts $STAGE/harness/oracle/cases/
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-web-search-tool.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- web-search-tool
//!   cd $V5W
//!   QT_ORACLE_WEB_SEARCH=/tmp/oracle-web-search-tool.ndjson \
//!     cargo test -p quilltap-harness --test web_search_tool_equivalence

use std::collections::HashMap;

use quilltap_core::db::api_keys::{AkCreate, ApiKeysRepository};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::model::wire::{wire_key, CannedSyncWireTransport, WireResponse};
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::tools::web_search::{
    build_serper_request, execute_web_search, format_web_search_results, NoSearchApiKeys,
    RealWebSearchProvider, WebSearchProvider,
};
use quilltap_host::spine::DbSearchApiKeys;
use serde_json::{json, Value};

/// The host's `Quilltap/<version>` User-Agent (v4 `getQuilltapUserAgent()`). It
/// never reaches the canned transport's key (method + url + body), so it does
/// not affect these cases — its wire-byte pin is `web_search_wire_equivalence`.
const UA: &str = "Quilltap/0.0.0-test";

/// The synthetic pepper every harness fixture uses. No real key ever appears.
const TEST_PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";

// ---------------------------------------------------------------------------
// The api_keys instance: one provisioned DB, one user per row-set
// ---------------------------------------------------------------------------

/// One `api_keys` row, as the oracle's mocked `getAllApiKeys()` returns it.
struct KeyRow {
    provider: &'static str,
    key_value: &'static str,
    is_active: bool,
}

const ACTIVE: KeyRow = KeyRow {
    provider: "SERPER",
    key_value: "db-key",
    is_active: true,
};
const INACTIVE: KeyRow = KeyRow {
    provider: "SERPER",
    key_value: "stale-key",
    is_active: false,
};
const OTHER: KeyRow = KeyRow {
    provider: "OPENAI",
    key_value: "sk-other",
    is_active: true,
};
const SECOND_ACTIVE: KeyRow = KeyRow {
    provider: "SERPER",
    key_value: "second-key",
    is_active: true,
};

/// The users, one per distinct row-set the oracle's cases use. The rows are
/// created in list order through the REAL repository, so insertion order — which
/// is what `find` walks on both sides — is the order written here.
fn row_sets() -> Vec<(&'static str, Vec<&'static KeyRow>)> {
    vec![
        ("user-active", vec![&ACTIVE]),
        ("user-none", vec![]),
        ("user-inactive-only", vec![&INACTIVE]),
        ("user-other-provider", vec![&OTHER]),
        ("user-skip-inactive", vec![&INACTIVE, &OTHER, &ACTIVE]),
        ("user-two-active", vec![&ACTIVE, &SECOND_ACTIVE]),
    ]
}

/// A freshly provisioned instance carrying every row-set, kept alive by its
/// TempDir. The rows go in through `ApiKeysRepository::create` (the production
/// writer), not raw SQL, so what the lookup reads is what production wrote.
struct Instance {
    db: Db,
    _tmp: tempfile::TempDir,
}

fn provisioned_instance() -> Instance {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, TEST_PEPPER).expect("provision a fresh instance");

    let main_path = data.join("quilltap.db");
    {
        let writer = Writer::open_writable(&main_path, TEST_PEPPER).unwrap();
        let repo = ApiKeysRepository::new(writer.connection());
        for (user_id, rows) in row_sets() {
            for (i, row) in rows.iter().enumerate() {
                repo.create(&AkCreate {
                    user_id: user_id.to_string(),
                    label: format!("{} key {}", row.provider, i + 1),
                    provider: row.provider.to_string(),
                    key_value: row.key_value.to_string(),
                    is_active: Some(row.is_active),
                    last_used: None,
                })
                .expect("write an api_keys row");
            }
        }
    }

    let db = Db::open(
        DbPaths {
            main: main_path,
            mount_index: None,
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .expect("open the provisioned instance");
    Instance { db, _tmp: tmp }
}

/// A transport that echoes the `X-API-KEY` header back in the result title.
///
/// WHICH key the lookup chose is invisible in the tool's output — it travels as
/// a request HEADER and appears in no field either side emits, and `wire_key` is
/// `METHOD\nURL\nBODY`, so a canned transport keyed the ordinary way answers a
/// stale key and a fresh one identically. The first pass of these cases was
/// exactly that vacuous, and a mutation taking the LAST active row survived it.
/// Echoing the header makes "the FIRST active row, not the stale one" a real
/// comparand; the oracle's `fetch` mock echoes the same header the same way.
struct KeyEchoTransport;
impl quilltap_core::model::wire::SyncWireTransport for KeyEchoTransport {
    fn send(
        &self,
        _method: &str,
        _url: &str,
        headers: &[(String, String)],
        _body: &str,
    ) -> Result<WireResponse, String> {
        let sent = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-api-key"))
            .map(|(_, v)| v.as_str())
            .unwrap_or("<none>");
        Ok(WireResponse::new(
            200,
            json!({
                "organic": [{
                    "title": format!("key:{sent}"),
                    "link": "https://example.com/key",
                    "snippet": "The key that was sent."
                }]
            })
            .to_string(),
        ))
    }
}

/// A provider that returns a fixed canned outcome (for the handler-default edge
/// that the real Serper wire never produces — `success:false, error:null`).
struct Canned(quilltap_core::tools::web_search::WebSearchOutcome);
impl WebSearchProvider for Canned {
    fn search(
        &self,
        _q: &str,
        _m: i64,
        _u: &str,
    ) -> quilltap_core::tools::web_search::WebSearchOutcome {
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
    // Headers do not participate in `wire_key`, so the UA is irrelevant here.
    let req = build_serper_request(query, max_results, key, None);
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

/// Run one case and diff against the oracle row. `user_id` selects the row-set
/// the DB-backed key lookup sees; the oracle's own user id never reaches the
/// output, so the two sides differ in that string alone.
fn check(
    oracle: &HashMap<String, Value>,
    label: &str,
    user_id: &str,
    args: Value,
    provider: &dyn WebSearchProvider,
) {
    let out = execute_web_search(provider, user_id, &args);
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
    let instance = provisioned_instance();
    let keys = || DbSearchApiKeys(instance.db.clone());

    let ok = |body: Value| WireResponse::new(200, body.to_string());
    let err = |status: u16, st: &str, body: &str| WireResponse {
        status,
        status_text: st.to_string(),
        body: body.to_string(),
    };

    // --- provider path: registered, key resolved from `api_keys` ---
    {
        let body = json!({ "organic": [organic("Quantum leap", Some("2026-06-15T00:00:00.000Z")), organic("More news", None)] });
        let p = RealWebSearchProvider::new(
            transport_for("latest AI news", 5, "db-key", ok(body)),
            keys(),
            true,
            None,
            UA.to_string(),
        );
        check(
            &oracle,
            "provider_success",
            "user-active",
            json!({ "query": "latest AI news" }),
            &p,
        );
    }
    {
        let body =
            json!({ "organic": [organic("Sunny in Tokyo", Some("2020-12-31T23:00:00.000Z"))] });
        let p = RealWebSearchProvider::new(
            transport_for("tokyo weather", 3, "db-key", ok(body)),
            keys(),
            true,
            None,
            UA.to_string(),
        );
        check(
            &oracle,
            "provider_success_maxresults",
            "user-active",
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
            transport_for("tokyo weather", 3, "db-key", ok(body)),
            keys(),
            true,
            None,
            UA.to_string(),
        );
        check(
            &oracle,
            "lenient_quoted_maxresults",
            "user-active",
            json!({ "query": "tokyo weather", "maxResults": "3" }),
            &p,
        );
    }
    // lenient_true_refused — `true` is REFUSED, not coerced to 1. The tool never
    // reaches the wire, so the transport is deliberately empty: any request at all
    // would fail it.
    {
        let p = RealWebSearchProvider::new(
            CannedSyncWireTransport::new(),
            keys(),
            true,
            None,
            UA.to_string(),
        );
        check(
            &oracle,
            "lenient_true_refused",
            "user-active",
            json!({ "query": "tokyo weather", "maxResults": true }),
            &p,
        );
    }
    {
        let body = json!({ "organic": [] });
        let p = RealWebSearchProvider::new(
            transport_for("obscure", 5, "db-key", ok(body)),
            keys(),
            true,
            None,
            UA.to_string(),
        );
        check(
            &oracle,
            "provider_no_results",
            "user-active",
            json!({ "query": "obscure" }),
            &p,
        );
    }
    // The knowledgeGraph unshift, through the plugin's own mapping on v4's side.
    {
        let body = json!({
            "organic": [organic("A history of lighthouses", None)],
            "knowledgeGraph": {
                "title": "Pharos of Alexandria",
                "description": "One of the Seven Wonders of the Ancient World.",
                "source": { "name": "Wikipedia", "link": "https://example.com/pharos" }
            }
        });
        let p = RealWebSearchProvider::new(
            transport_for("pharos", 5, "db-key", ok(body)),
            keys(),
            true,
            None,
            UA.to_string(),
        );
        check(
            &oracle,
            "provider_knowledge_graph",
            "user-active",
            json!({ "query": "pharos" }),
            &p,
        );
    }
    for (label, status, st) in [
        ("provider_error_401", 401u16, "Unauthorized"),
        ("provider_error_429", 429, "Too Many Requests"),
        ("provider_error_500", 500, "Internal Server Error"),
    ] {
        let body = if status == 500 { "boom" } else { "nope" };
        let p = RealWebSearchProvider::new(
            transport_for("x", 5, "db-key", err(status, st, body)),
            keys(),
            true,
            None,
            UA.to_string(),
        );
        check(&oracle, label, "user-active", json!({ "query": "x" }), &p);
    }
    // The plugin's catch arm: `error.message` reaches the output verbatim.
    {
        let req = build_serper_request("x", 5, "db-key", None);
        let transport = CannedSyncWireTransport::new().with_raw_throw(
            wire_key(&req.method, &req.url, &req.body_string()),
            "socket hang up",
        );
        let p = RealWebSearchProvider::new(transport, keys(), true, None, UA.to_string());
        check(
            &oracle,
            "provider_network_error",
            "user-active",
            json!({ "query": "x" }),
            &p,
        );
    }

    // --- the api-key predicate: `provider === 'SERPER' && isActive` ---
    for (label, user) in [
        ("missing_api_key_no_rows", "user-none"),
        ("missing_api_key_inactive_only", "user-inactive-only"),
        ("missing_api_key_other_provider", "user-other-provider"),
    ] {
        // No wire is reachable: a lookup that wrongly found a key would send a
        // request the empty transport cannot answer, and the case would red.
        let p = RealWebSearchProvider::new(
            CannedSyncWireTransport::new(),
            keys(),
            true,
            None,
            UA.to_string(),
        );
        check(&oracle, label, user, json!({ "query": "needs a key" }), &p);
    }
    // WHICH key was chosen, made visible by echoing the header (see
    // `KeyEchoTransport`). The inactive row is SKIPPED for the later active one;
    // with two active rows the FIRST wins.
    for (label, user, query) in [
        (
            "key_skips_inactive_takes_active",
            "user-skip-inactive",
            "skip the stale one",
        ),
        ("key_takes_first_active", "user-two-active", "first wins"),
    ] {
        let p = RealWebSearchProvider::new(KeyEchoTransport, keys(), true, None, UA.to_string());
        check(&oracle, label, user, json!({ "query": query }), &p);
    }
    // The registered path sends the DB row's key even with an env key present;
    // the fallback path sends the env key.
    {
        let p = RealWebSearchProvider::new(
            KeyEchoTransport,
            keys(),
            true,
            Some("env-key".into()),
            UA.to_string(),
        );
        check(
            &oracle,
            "key_registered_sends_db_key_not_env",
            "user-active",
            json!({ "query": "whose key" }),
            &p,
        );
        let f = RealWebSearchProvider::new(
            KeyEchoTransport,
            NoSearchApiKeys,
            false,
            Some("env-key".into()),
            UA.to_string(),
        );
        check(
            &oracle,
            "key_fallback_sends_env_key",
            "user-active",
            json!({ "query": "whose key" }),
            &f,
        );
    }

    // --- registration short-circuits the env fallback ---
    // BOTH configured. The tell is the PLUGIN's 401 sentence; the fallback's is
    // different, and the transport is keyed on the DB key, not the env one.
    {
        let p = RealWebSearchProvider::new(
            transport_for("x", 5, "db-key", err(401, "Unauthorized", "nope")),
            keys(),
            true,
            Some("env-key".into()),
            UA.to_string(),
        );
        check(
            &oracle,
            "registered_shortcircuits_env",
            "user-active",
            json!({ "query": "x" }),
            &p,
        );
    }
    // Registered but keyless WITH an env key: still `MissingApiKey`, never the
    // fallback. The empty transport proves no request went out at all.
    {
        let p = RealWebSearchProvider::new(
            CannedSyncWireTransport::new(),
            keys(),
            true,
            Some("env-key".into()),
            UA.to_string(),
        );
        check(
            &oracle,
            "registered_keyless_does_not_fall_back",
            "user-none",
            json!({ "query": "x" }),
            &p,
        );
    }

    // --- fallback path (no plugin registered, SERPER_API_KEY set) ---
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
            UA.to_string(),
        );
        check(
            &oracle,
            "fallback_success",
            "user-active",
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
            UA.to_string(),
        );
        check(&oracle, label, "user-active", json!({ "query": "x" }), &p);
    }
    // not_configured: no plugin, no env key.
    {
        let p = RealWebSearchProvider::new(
            CannedSyncWireTransport::new(),
            NoSearchApiKeys,
            false,
            None,
            UA.to_string(),
        );
        check(
            &oracle,
            "not_configured",
            "user-active",
            json!({ "query": "nothing" }),
            &p,
        );
    }
    // validation (provider never consulted).
    {
        let p = RealWebSearchProvider::new(
            CannedSyncWireTransport::new(),
            NoSearchApiKeys,
            false,
            None,
            UA.to_string(),
        );
        check(
            &oracle,
            "validation_empty",
            "user-active",
            json!({ "query": "   " }),
            &p,
        );
        check(
            &oracle,
            "validation_nonobject",
            "user-active",
            json!("nope"),
            &p,
        );
    }

    // The handler-default edge the real Serper wire never produces
    // (`success:false, error:null` → v4's `?? 'Search provider returned an
    // error'`). v4's oracle cannot express it now that the REAL plugin runs, so
    // it is kept here as a Rust-side unit arm on the same mapping.
    {
        let p = Canned(
            quilltap_core::tools::web_search::WebSearchOutcome::ProviderResult {
                success: false,
                results: vec![],
                error: None,
            },
        );
        let out = execute_web_search(&p, "user-active", &json!({ "query": "x" }));
        assert_eq!(
            out.error.as_deref(),
            Some("Search provider returned an error"),
            "the provider-failure default"
        );
    }

    eprintln!("OK: web-search-tool differential matched the oracle (26 cases).");
}
