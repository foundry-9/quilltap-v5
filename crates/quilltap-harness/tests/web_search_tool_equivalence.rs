//! Differential test (W4.1d5): the `search_web` tool handler
//! (`quilltap_core::tools::web_search`) vs v4's REAL handler. DB-FREE: the search
//! boundary is the injected `WebSearchProvider` seam (canned outcome per case,
//! mirroring the oracle's jest-mocked registry). Diffs the serialized output +
//! `format_web_search_results`.
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

use quilltap_core::tools::web_search::{
    execute_web_search, format_web_search_results, WebSearchOutcome, WebSearchProvider,
    WebSearchResult,
};
use serde_json::{json, Value};

struct Canned(WebSearchOutcome);
impl WebSearchProvider for Canned {
    fn search(&self, _q: &str, _m: i64, _u: &str) -> WebSearchOutcome {
        self.0.clone()
    }
}

fn result(title: &str, date: Option<&str>) -> WebSearchResult {
    let slug = title.to_lowercase().replace(' ', "-");
    WebSearchResult {
        title: title.to_string(),
        url: format!("https://example.com/{slug}"),
        snippet: format!("A snippet about {title}."),
        published_date: date.map(str::to_string),
    }
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

#[test]
fn web_search_tool_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_WEB_SEARCH") else {
        eprintln!("SKIP: set QT_ORACLE_WEB_SEARCH (see test header).");
        return;
    };
    let oracle = load_oracle(&oracle_path);

    struct Case {
        label: &'static str,
        args: Value,
        outcome: WebSearchOutcome,
    }
    let cases = vec![
        Case {
            label: "success_with_dates",
            args: json!({ "query": "latest AI news" }),
            outcome: WebSearchOutcome::ProviderResult {
                success: true,
                results: vec![
                    result("Quantum leap", Some("2026-06-15T00:00:00.000Z")),
                    result("More news", None),
                ],
                error: None,
            },
        },
        Case {
            label: "success_maxresults",
            args: json!({ "query": "tokyo weather", "maxResults": 3 }),
            outcome: WebSearchOutcome::ProviderResult {
                success: true,
                results: vec![result("Sunny in Tokyo", Some("2020-12-31T23:00:00.000Z"))],
                error: None,
            },
        },
        Case {
            label: "success_no_results",
            args: json!({ "query": "obscure query" }),
            outcome: WebSearchOutcome::ProviderResult {
                success: true,
                results: vec![],
                error: None,
            },
        },
        Case {
            label: "provider_failure",
            args: json!({ "query": "fail me" }),
            outcome: WebSearchOutcome::ProviderResult {
                success: false,
                results: vec![],
                error: Some("quota exceeded".into()),
            },
        },
        Case {
            label: "provider_failure_no_error",
            args: json!({ "query": "fail silently" }),
            outcome: WebSearchOutcome::ProviderResult {
                success: false,
                results: vec![],
                error: None,
            },
        },
        Case {
            label: "missing_api_key",
            args: json!({ "query": "needs a key" }),
            outcome: WebSearchOutcome::MissingApiKey {
                display_name: "Serper".into(),
            },
        },
        Case {
            label: "not_configured",
            args: json!({ "query": "nothing configured" }),
            outcome: WebSearchOutcome::NotConfigured,
        },
        Case {
            label: "validation_empty",
            args: json!({ "query": "   " }),
            outcome: WebSearchOutcome::NotConfigured,
        },
        Case {
            label: "validation_nonobject",
            args: json!("nope"),
            outcome: WebSearchOutcome::NotConfigured,
        },
    ];

    for c in &cases {
        let provider = Canned(c.outcome.clone());
        let out = execute_web_search(&provider, "user-1", &c.args);
        let got_json = serde_json::to_string(&out).unwrap();
        let got_fmt = if out.success {
            Some(format_web_search_results(
                out.results.as_deref().unwrap_or(&[]),
            ))
        } else {
            None
        };

        let want = oracle
            .get(c.label)
            .unwrap_or_else(|| panic!("web-search oracle missing {}", c.label));
        assert_eq!(
            got_json.as_str(),
            want["resultJson"].as_str().unwrap(),
            "json {}",
            c.label
        );
        let want_fmt = want["formatted"].as_str().map(str::to_string);
        assert_eq!(got_fmt, want_fmt, "formatted {}", c.label);
    }

    eprintln!("OK: web-search-tool differential matched the oracle.");
}
