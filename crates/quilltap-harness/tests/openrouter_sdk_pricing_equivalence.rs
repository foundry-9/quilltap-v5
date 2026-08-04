//! P4.D33 `openrouter_sdk_pricing_equivalence`: v4's REAL authenticated
//! OpenRouter pricing path — `@openrouter/sdk` 1.2.2 in the loop, only the
//! network mocked underneath it — against v5's reproduction of the two things
//! that SDK does between the wire and the parse.
//!
//! The W4.7e `pricing_fetcher_equivalence` oracle stubs `@openrouter/sdk` and
//! feeds both sides the same hand-written **camelCase** body, so it can never
//! see this seam. This family feeds v4 the RAW snake_case pages the endpoint
//! actually returns and diffs the `ModelPricing[]` that come out, which covers:
//!
//!   - [`remap_openrouter_sdk_models`] — the SDK's `Model$inboundSchema` key
//!     remap. Without it `parse_openrouter_sdk` read camelCase off a snake_case
//!     body: every model lost its `contextLength` and its `supportsTools`.
//!   - [`openrouter_next_page_offset`] — the SDK's page loop. v4 accumulates
//!     every page; a single GET truncates the catalogue at 500 models. Asserted
//!     against the request urls the oracle recorded, so the walk is compared to
//!     v4's actual HTTP, not to our reading of the SDK source.
//!
//! The v5 replay models the host's `openrouter_models_pages`: walk the oracle's
//! pages under the same stop rule, concatenate, remap, parse, sort.
//!
//! Generate the oracle (jest, Node 24, from the v4 checkout). BOTH overrides
//! are load-bearing — see the case file's header; without the
//! `--transformIgnorePatterns` one the suite passes and emits empty scenarios.
//! The header used to name `/private/tmp/qt-v4-pin-<order>-<sha>`; a detached
//! pin never survives the round that made it (P4.34's F6), and a pin also has
//! to have `plugins/node_modules` + the per-plugin `plugins/dist/*/node_modules`
//! symlinked before this family's SDK is resolvable at all. Regen from the
//! checkout; pin only if v4 has drifted past the verified baseline:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   STAGE=/private/tmp/qt-openrouter-sdk-pricing-oracle  # jest ignores /.claude/
//!   rm -rf "$STAGE"; mkdir -p "$STAGE"
//!   cp "$V5W/harness/oracle/cases/openrouter-sdk-pricing.test.ts" "$STAGE/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-openrouter-sdk-pricing.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE" \
//!       --transformIgnorePatterns "node_modules/(?!(@openrouter/sdk|jose)/)" \
//!       -- openrouter-sdk-pricing
//! Run:
//!   QT_ORACLE_OPENROUTER_SDK_PRICING=/tmp/oracle-openrouter-sdk-pricing.ndjson \
//!     cargo test -p quilltap-harness --test openrouter_sdk_pricing_equivalence

use quilltap_core::services::pricing_fetcher::{
    openrouter_next_page_offset, parse_openrouter_sdk, remap_openrouter_sdk_models, sort_by_cost,
    OPENROUTER_PAGE_LIMIT,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Scenario {
    id: String,
    #[serde(rename = "nowMs")]
    now_ms: i64,
    /// The raw wire bodies, in the order the mocked fetch served them.
    pages: Vec<Value>,
    /// Every url v4's SDK actually requested. Its length is the page count.
    requests: Vec<String>,
    /// v4's `ModelPricing[]` from `getProviderPricing('OPENROUTER', …)`.
    outputs: Vec<Value>,
}

/// The host's `openrouter_models_pages`, replayed over the oracle's pages:
/// returns the concatenated+remapped body and how many pages were consumed.
fn walk_pages(pages: &[Value]) -> (Value, usize) {
    let mut collected: Vec<Value> = Vec::new();
    let mut offset: usize = 0;
    let mut consumed = 0usize;
    for page in pages {
        consumed += 1;
        if let Some(models) = page.get("data").and_then(Value::as_array) {
            collected.extend(models.iter().cloned());
        }
        match openrouter_next_page_offset(page, offset) {
            Some(next) => offset = next,
            None => break,
        }
    }
    (
        remap_openrouter_sdk_models(&serde_json::json!({ "data": collected })),
        consumed,
    )
}

#[test]
fn openrouter_sdk_pricing_matches_v4() {
    let path = match std::env::var("QT_ORACLE_OPENROUTER_SDK_PRICING") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_OPENROUTER_SDK_PRICING to the oracle NDJSON.");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let scenarios: Vec<Scenario> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle line"))
        .collect();
    assert!(!scenarios.is_empty(), "oracle NDJSON is empty");

    // A truncated corpus must not pass silently: the pagination arm is the whole
    // point of two of these scenarios.
    let multi_page = scenarios.iter().filter(|s| s.requests.len() > 1).count();
    assert!(
        multi_page >= 1,
        "corpus has no multi-page scenario — the page-loop arm is uncovered"
    );

    let mut checked = 0usize;
    for s in &scenarios {
        let (body, consumed) = walk_pages(&s.pages);

        // 1. The walk matches v4's actual HTTP, url for url.
        assert_eq!(
            consumed,
            s.requests.len(),
            "[{}] page count: v5 walked {consumed}, v4 requested {:?}",
            s.id,
            s.requests
        );
        // Every follow-up request carries the SDK's materialized limit + the
        // offset our stop rule computed.
        let mut offset = 0usize;
        for (i, url) in s.requests.iter().enumerate().skip(1) {
            let prev = &s.pages[i - 1];
            offset = openrouter_next_page_offset(prev, offset)
                .unwrap_or_else(|| panic!("[{}] v4 requested {url} but v5 would stop", s.id));
            let expected = format!(
                "https://openrouter.ai/api/v1/models?limit={OPENROUTER_PAGE_LIMIT}&offset={offset}"
            );
            assert_eq!(*url, expected, "[{}] follow-up request url", s.id);
        }

        // 2. The models v4 built from those same bytes.
        let got: Vec<Value> = sort_by_cost(parse_openrouter_sdk(&body, s.now_ms))
            .into_iter()
            .map(|m| serde_json::to_value(m).expect("serialize"))
            .collect();
        assert_eq!(
            got.len(),
            s.outputs.len(),
            "[{}] model count: v5 {} vs v4 {}",
            s.id,
            got.len(),
            s.outputs.len()
        );
        for (i, (a, b)) in got.iter().zip(s.outputs.iter()).enumerate() {
            assert_eq!(a, b, "[{}] model[{i}] diverged", s.id);
        }
        checked += 1;
    }
    println!("OK: {checked} openrouter SDK pricing scenario(s) matched v4.");
}
