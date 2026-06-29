//! Tier-1 differential test #30 (Wave 7 / B22): getModelContextLimit and its
//! consumers (hasExtendedContext, getSafeInputLimit) — exact equality against
//! the v4 oracle. The oracle emits the per-provider injected context
//! (`providerctx` rows: plugin model-info, FALLBACK_PRICING rows, registry
//! default) so the Rust port is fed exactly what the real function saw.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/model-context.ts \
//!     > /tmp/oracle-model-context.ndjson
//! Run:
//!   QT_ORACLE_MODEL_CONTEXT=/tmp/oracle-model-context.ndjson \
//!     cargo test -p quilltap-harness

use quilltap_core::model_context::{
    get_model_context_limit, get_safe_input_limit, has_extended_context, ModelInfo, PricingRow,
};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct WModelInfo {
    id: String,
    #[serde(rename = "contextWindow")]
    context_window: Option<i64>,
}

#[derive(Deserialize)]
struct WPricingRow {
    #[serde(rename = "modelId")]
    model_id: String,
    #[serde(rename = "contextLength")]
    context_length: Option<i64>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "providerctx")]
    ProviderCtx {
        provider: String,
        #[serde(rename = "modelInfo")]
        model_info: Vec<WModelInfo>,
        #[serde(rename = "fallbackPricing")]
        fallback_pricing: Vec<WPricingRow>,
        #[serde(rename = "registryDefault")]
        registry_default: i64,
    },
    #[serde(rename = "query")]
    Query {
        provider: String,
        model: String,
        #[serde(rename = "maxResponseTokens")]
        max_response_tokens: i64,
        limit: i64,
        extended: bool,
        #[serde(rename = "safeInput")]
        safe_input: i64,
    },
}

struct Ctx {
    model_info: Vec<ModelInfo>,
    fallback_pricing: Vec<PricingRow>,
    registry_default: i64,
}

#[test]
fn model_context_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MODEL_CONTEXT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MODEL_CONTEXT to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut ctx: HashMap<String, Ctx> = HashMap::new();
    let mut queries: Vec<Row> = Vec::new();

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::ProviderCtx {
                provider,
                model_info,
                fallback_pricing,
                registry_default,
            } => {
                ctx.insert(
                    provider,
                    Ctx {
                        model_info: model_info
                            .into_iter()
                            .map(|m| ModelInfo {
                                id: m.id,
                                context_window: m.context_window,
                            })
                            .collect(),
                        fallback_pricing: fallback_pricing
                            .into_iter()
                            .map(|m| PricingRow {
                                model_id: m.model_id,
                                context_length: m.context_length,
                            })
                            .collect(),
                        registry_default,
                    },
                );
            }
            q @ Row::Query { .. } => queries.push(q),
        }
    }

    let mut count = 0usize;
    for q in &queries {
        let Row::Query {
            provider,
            model,
            max_response_tokens,
            limit,
            extended,
            safe_input,
        } = q
        else {
            unreachable!()
        };
        let c = ctx
            .get(provider)
            .unwrap_or_else(|| panic!("no providerctx for {provider}"));

        assert_eq!(
            get_model_context_limit(
                provider,
                model,
                &c.model_info,
                &c.fallback_pricing,
                c.registry_default
            ),
            *limit,
            "getModelContextLimit({provider}, {model})"
        );
        assert_eq!(
            has_extended_context(
                provider,
                model,
                &c.model_info,
                &c.fallback_pricing,
                c.registry_default
            ),
            *extended,
            "hasExtendedContext({provider}, {model})"
        );
        assert_eq!(
            get_safe_input_limit(
                provider,
                model,
                &c.model_info,
                &c.fallback_pricing,
                c.registry_default,
                *max_response_tokens
            ),
            *safe_input,
            "getSafeInputLimit({provider}, {model}, {max_response_tokens})"
        );
        count += 1;
    }

    assert!(count > 0, "oracle file had no query rows: {count}");
    eprintln!("OK: model-context matched oracle ({count} queries).");
}
