//! W4.7e `pricing_fetcher_equivalence`: drives v4's REAL pricing-fetcher /
//! cost-estimation / checkModelSupportsTools (jest oracle, fetch/SDK/repo mocked)
//! and replays each scenario through the Rust `services::pricing_fetcher` on a
//! fresh `PricingFetcher` + scripted `SeqFetch`, diffing every op's output.
//!
//! Generate the oracle (jest, Node 24):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-pricing-fetcher.ndjson \
//!     $N/npx jest --silent --watchman=false \
//!       --roots "$PWD" --roots "$V5/harness/oracle/cases" -- pricing-fetcher
//! Run:
//!   QT_ORACLE_PRICING_FETCHER=/tmp/oracle-pricing-fetcher.ndjson \
//!     cargo test -p quilltap-harness --test pricing_fetcher_equivalence

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Mutex;

use quilltap_core::provider_manifest::Registry;
use quilltap_core::services::pricing_fetcher::{
    FindCheapestOptions, PricingContext, PricingFetch, PricingFetcher, PricingProfile,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Profile {
    provider: String,
    #[serde(rename = "apiKeyId")]
    api_key_id: Option<String>,
    #[serde(rename = "baseUrl")]
    base_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "getORForModel")]
    GetOrForModel {
        provider: String,
        #[serde(rename = "modelId")]
        model_id: String,
        #[serde(rename = "nowMs")]
        now_ms: i64,
    },
    #[serde(rename = "checkTools")]
    CheckTools {
        provider: String,
        #[serde(rename = "modelName")]
        model_name: String,
        #[serde(rename = "nowMs")]
        now_ms: i64,
    },
    #[serde(rename = "estimate")]
    Estimate {
        provider: String,
        #[serde(rename = "modelName")]
        model_name: String,
        #[serde(rename = "promptTokens")]
        prompt_tokens: i64,
        #[serde(rename = "completionTokens")]
        completion_tokens: i64,
        #[serde(rename = "nowMs")]
        now_ms: i64,
    },
    #[serde(rename = "findCheapest")]
    FindCheapest {
        #[serde(rename = "nowMs")]
        now_ms: i64,
        #[serde(default, rename = "requireVision")]
        require_vision: Option<bool>,
        #[serde(default, rename = "requireTools")]
        require_tools: Option<bool>,
        #[serde(default, rename = "excludeProviders")]
        exclude_providers: Option<Vec<String>>,
    },
}

#[derive(Deserialize)]
struct Scenario {
    id: String,
    #[serde(rename = "publicBody")]
    public_body: Value,
    #[serde(rename = "publicScript")]
    public_script: Vec<String>,
    #[serde(rename = "sdkBody")]
    sdk_body: Option<Value>,
    #[serde(rename = "ollamaBody")]
    ollama_body: Option<Value>,
    profiles: Vec<Profile>,
    #[serde(rename = "apiKeys")]
    api_keys: HashMap<String, String>,
    ops: Vec<Op>,
    outputs: Vec<Value>,
}

/// The scripted fetch seam mirroring the oracle: public fetches consume the
/// script queue (`ok` -> the body, `fail` -> None); SDK/Ollama return their fixed
/// body (or None to simulate an SDK/host failure).
struct SeqFetch {
    public_body: Value,
    public_script: Mutex<VecDeque<String>>,
    sdk_body: Option<Value>,
    ollama_body: Option<Value>,
}
impl PricingFetch for SeqFetch {
    fn openrouter_public_models(&self) -> Option<Value> {
        match self.public_script.lock().unwrap().pop_front().as_deref() {
            Some("ok") => Some(self.public_body.clone()),
            _ => None,
        }
    }
    fn openrouter_sdk_models(&self, _api_key: &str) -> Option<Value> {
        self.sdk_body.clone()
    }
    fn ollama_tags(&self, _base_url: &str) -> Option<Value> {
        self.ollama_body.clone()
    }
}

/// Serialize a `FetchedModelPricing` (or None) the way the oracle emits a
/// ModelPricing (JSON.stringify -> null for a missing model).
fn or_model_to_value(
    m: Option<quilltap_core::services::pricing_fetcher::FetchedModelPricing>,
) -> Value {
    match m {
        Some(m) => serde_json::to_value(m).unwrap(),
        None => Value::Null,
    }
}

#[test]
fn pricing_fetcher_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PRICING_FETCHER") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PRICING_FETCHER to the oracle NDJSON.");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let registry = Registry::built_in();

    let mut scenarios = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let sc: Scenario = serde_json::from_str(line).unwrap();
        let fetch = SeqFetch {
            public_body: sc.public_body.clone(),
            public_script: Mutex::new(sc.public_script.iter().cloned().collect()),
            sdk_body: sc.sdk_body.clone(),
            ollama_body: sc.ollama_body.clone(),
        };
        let fetcher = PricingFetcher::new(fetch);
        let ctx = PricingContext {
            profiles: sc
                .profiles
                .iter()
                .map(|p| PricingProfile {
                    provider: p.provider.clone(),
                    api_key_id: p.api_key_id.clone(),
                    base_url: p.base_url.clone(),
                })
                .collect(),
            api_keys: sc.api_keys.clone(),
        };

        for (i, op) in sc.ops.iter().enumerate() {
            let got: Value = match op {
                Op::GetOrForModel {
                    provider,
                    model_id,
                    now_ms,
                } => or_model_to_value(
                    fetcher.get_openrouter_pricing_for_model(provider, model_id, *now_ms),
                ),
                Op::CheckTools {
                    provider,
                    model_name,
                    now_ms,
                } => Value::Bool(
                    fetcher.check_model_supports_tools(provider, model_name, *now_ms, &ctx),
                ),
                Op::Estimate {
                    provider,
                    model_name,
                    prompt_tokens,
                    completion_tokens,
                    now_ms,
                } => serde_json::to_value(fetcher.estimate_message_cost(
                    registry,
                    provider,
                    model_name,
                    *prompt_tokens,
                    *completion_tokens,
                    *now_ms,
                    &ctx,
                ))
                .unwrap(),
                Op::FindCheapest {
                    now_ms,
                    require_vision,
                    require_tools,
                    exclude_providers,
                } => or_model_to_value(fetcher.find_cheapest_available_model(
                    &FindCheapestOptions {
                        require_vision: require_vision.unwrap_or(false),
                        require_tools: require_tools.unwrap_or(false),
                        exclude_providers: exclude_providers.clone().unwrap_or_default(),
                    },
                    *now_ms,
                    &ctx,
                )),
            };
            assert_eq!(got, sc.outputs[i], "scenario '{}' op #{i}", sc.id);
        }
        scenarios += 1;
    }

    assert!(scenarios > 0, "oracle file looks empty");
    eprintln!("OK: pricing-fetcher matched oracle ({scenarios} scenarios).");
}
