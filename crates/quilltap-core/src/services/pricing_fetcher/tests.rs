//! Self-tests for the pricing fetcher: parseFloat semantics, the two casings,
//! cache TTL + negative-cache windows, slug matching, capability + cost cascade.
//! The byte-fidelity against v4 is proven by `pricing_fetcher_equivalence`.

use super::*;
use std::sync::Mutex as StdMutex;

const HOUR: i64 = 60 * 60 * 1000;

#[test]
fn parse_float_js_semantics() {
    assert_eq!(js_parse_float("0.000003"), 0.000003);
    assert_eq!(js_parse_float("3"), 3.0);
    assert_eq!(js_parse_float("1.5x"), 1.5);
    assert_eq!(js_parse_float("  2.0"), 2.0);
    assert_eq!(js_parse_float("1e-6"), 1e-6);
    assert_eq!(js_parse_float("1e"), 1.0); // bare exponent marker dropped
    assert!(js_parse_float("abc").is_nan()); // garbage -> NaN
    assert!(js_parse_float("").is_nan());
    assert_eq!(js_parse_float("Infinity"), f64::INFINITY);
    assert_eq!(js_parse_float("-2.5"), -2.5);
}

#[test]
fn price_string_falsy_coercion() {
    assert_eq!(price_string(None), "0");
    assert_eq!(price_string(Some(&serde_json::json!(null))), "0");
    assert_eq!(price_string(Some(&serde_json::json!(""))), "0");
    assert_eq!(price_string(Some(&serde_json::json!(0))), "0");
    assert_eq!(
        price_string(Some(&serde_json::json!("0.000003"))),
        "0.000003"
    );
}

#[test]
fn public_parse_string_prices_and_flags() {
    let data = serde_json::json!({
        "data": [
            {
                "id": "openai/gpt-4o",
                "name": "GPT-4o",
                "pricing": { "prompt": "0.0000025", "completion": "0.00001" },
                "context_length": 128000,
                "architecture": { "modality": "text+image->text" },
                "supported_parameters": ["tools", "temperature"]
            },
            {
                "id": "garbage/model",
                "pricing": { "prompt": "abc", "completion": "1.0" },
                "supported_parameters": []
            }
        ]
    });
    let models = parse_openrouter_public(&data, 0);
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].model_id, "openai/gpt-4o");
    assert_eq!(models[0].prompt_cost_per_1m, 2.5);
    assert_eq!(models[0].completion_cost_per_1m, 10.0);
    assert_eq!(models[0].context_length, Some(128000));
    assert_eq!(models[0].supports_vision, Some(true));
    assert_eq!(models[0].supports_tools, Some(true));
    // Garbage price -> NaN propagates.
    assert!(models[1].prompt_cost_per_1m.is_nan());
    assert_eq!(models[1].supports_tools, Some(false));
    // Name falls back to id when absent (public path).
    assert_eq!(models[1].name, "garbage/model");
}

#[test]
fn sdk_parse_camelcase() {
    let data = serde_json::json!({
        "data": [{
            "id": "anthropic/claude",
            "name": "Claude",
            "pricing": { "prompt": "0.000003", "completion": "0.000015" },
            "contextLength": 200000,
            "supportedParameters": ["tool_choice"]
        }]
    });
    let models = parse_openrouter_sdk(&data, 0);
    assert_eq!(models[0].context_length, Some(200000));
    assert_eq!(models[0].supports_tools, Some(true));
    assert_eq!(models[0].name, "Claude");
}

/// A scripted fetch seam: yields a queued body per public-fetch call (None = a
/// simulated failure), counting calls to assert the TTL/negative-cache windows.
struct SeqFetch {
    public: StdMutex<std::collections::VecDeque<Option<Value>>>,
    public_calls: StdMutex<usize>,
    sdk: Option<Value>,
    ollama: Option<Value>,
}
impl PricingFetch for SeqFetch {
    fn openrouter_public_models(&self) -> Option<Value> {
        *self.public_calls.lock().unwrap() += 1;
        self.public.lock().unwrap().pop_front().unwrap_or(None)
    }
    fn openrouter_sdk_models(&self, _api_key: &str) -> Option<Value> {
        self.sdk.clone()
    }
    fn ollama_tags(&self, _base_url: &str) -> Option<Value> {
        self.ollama.clone()
    }
}

fn public_body() -> Value {
    serde_json::json!({
        "data": [
            { "id": "openai/gpt-4o", "name": "GPT-4o",
              "pricing": { "prompt": "0.0000025", "completion": "0.00001" },
              "context_length": 128000, "architecture": { "modality": "text+image" },
              "supported_parameters": ["tools"] },
            { "id": "openai/gpt-4o-mini", "name": "mini",
              "pricing": { "prompt": "0.00000015", "completion": "0.0000006" },
              "context_length": 128000, "supported_parameters": ["tools"] }
        ]
    })
}

#[test]
fn openrouter_ttl_and_negative_cache() {
    let fetch = SeqFetch {
        public: StdMutex::new(
            vec![Some(public_body()), None, Some(public_body())]
                .into_iter()
                .collect(),
        ),
        public_calls: StdMutex::new(0),
        sdk: None,
        ollama: None,
    };
    let f = PricingFetcher::new(fetch);

    // t=0: fetch #1 succeeds, cache populated.
    let m = f.get_openrouter_pricing_for_model("OPENAI", "gpt-4o", 0);
    assert!(m.is_some());
    assert_eq!(*f.fetch.public_calls.lock().unwrap(), 1);

    // t=1h: cache fresh, no fetch.
    let m = f.get_openrouter_pricing_for_model("OPENAI", "gpt-4o", HOUR);
    assert!(m.is_some());
    assert_eq!(*f.fetch.public_calls.lock().unwrap(), 1);

    // t=25h: stale -> fetch #2 fails -> negative cache -> [] -> None.
    let m = f.get_openrouter_pricing_for_model("OPENAI", "gpt-4o", 25 * HOUR);
    assert!(m.is_none());
    assert_eq!(*f.fetch.public_calls.lock().unwrap(), 2);

    // t=25h + 1min: within negative window -> no fetch, still None.
    let m = f.get_openrouter_pricing_for_model("OPENAI", "gpt-4o", 25 * HOUR + 60_000);
    assert!(m.is_none());
    assert_eq!(*f.fetch.public_calls.lock().unwrap(), 2);

    // t=25h + 6min: negative window expired -> fetch #3 succeeds.
    let m = f.get_openrouter_pricing_for_model("OPENAI", "gpt-4o", 25 * HOUR + 6 * 60_000);
    assert!(m.is_some());
    assert_eq!(*f.fetch.public_calls.lock().unwrap(), 3);
}

#[test]
fn slug_fuzzy_match() {
    let body = serde_json::json!({
        "data": [{ "id": "openai/gpt-4o", "name": "GPT-4o",
                   "pricing": { "prompt": "0.0000025", "completion": "0.00001" },
                   "supported_parameters": ["tools"] }]
    });
    let fetch = SeqFetch {
        public: StdMutex::new(vec![Some(body)].into_iter().collect()),
        public_calls: StdMutex::new(0),
        sdk: None,
        ollama: None,
    };
    let f = PricingFetcher::new(fetch);
    // Fuzzy: 'gpt-4o-2024-11-20' includes 'gpt-4o'.
    let m = f.get_openrouter_pricing_for_model("OPENAI", "gpt-4o-2024-11-20", 0);
    assert_eq!(m.unwrap().model_id, "openai/gpt-4o");
    // Non-slug provider -> None.
    assert!(f
        .get_openrouter_pricing_for_model("OLLAMA", "gpt-4o", 0)
        .is_none());
}

fn ctx_anthropic() -> PricingContext {
    PricingContext {
        profiles: vec![PricingProfile {
            provider: "ANTHROPIC".to_string(),
            api_key_id: None,
            base_url: None,
        }],
        api_keys: HashMap::new(),
    }
}

#[test]
fn check_tools_fallback_and_unknown() {
    let fetch = SeqFetch {
        public: StdMutex::new(Default::default()),
        public_calls: StdMutex::new(0),
        sdk: None,
        ollama: None,
    };
    let f = PricingFetcher::new(fetch);
    let ctx = ctx_anthropic();
    // A known ANTHROPIC fallback model supports tools.
    assert!(f.check_model_supports_tools("ANTHROPIC", "claude-haiku-4-5-20251001", 0, &ctx));
    // An unknown model on a known provider -> default true.
    assert!(f.check_model_supports_tools("ANTHROPIC", "nonexistent", 0, &ctx));
    // Unknown provider -> true.
    assert!(f.check_model_supports_tools("MYSTERY", "x", 0, &ctx));
}

#[test]
fn estimate_cost_registry_and_unavailable() {
    let fetch = SeqFetch {
        public: StdMutex::new(Default::default()),
        public_calls: StdMutex::new(0),
        sdk: None,
        ollama: None,
    };
    let f = PricingFetcher::new(fetch);
    let ctx = ctx_anthropic();
    let reg = Registry::built_in();

    // ANTHROPIC known model: registry (manifest empty) -> FALLBACK substring ->
    // 'registry' source, cost computed.
    let r = f.estimate_message_cost(
        reg,
        "ANTHROPIC",
        "claude-haiku-4-5-20251001",
        1000,
        500,
        0,
        &ctx,
    );
    assert_eq!(r.source, "registry");
    assert!(r.cost.unwrap() > 0.0);

    // Unknown provider/model, no public slug -> unavailable.
    let r = f.estimate_message_cost(reg, "OPENAI_COMPATIBLE", "mystery", 100, 100, 0, &ctx);
    assert_eq!(r.source, "unavailable");
    assert_eq!(r.cost, None);
}
