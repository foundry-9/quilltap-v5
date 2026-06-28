//! Tier-1 differential test #9 (Wave 1 / B1): cost-aware model selection +
//! model classes.
//!
//! Covers getAverageCostPer1M / sortByCost / findCheapestModel /
//! getModelsUnderCost / calculateCostTier / calculateSavings (pricing.ts) and
//! getModelClass / isValidModelClassName (model-classes.ts). Floats within
//! 1e-12; orderings and ids exact.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/model-selection.ts \
//!     > /tmp/oracle-model-selection.ndjson
//! Run:
//!   QT_ORACLE_MODEL_SELECTION=/tmp/oracle-model-selection.ndjson cargo test -p quilltap-harness

use quilltap_core::model_classes::get_model_class;
use quilltap_core::model_classes::is_valid_model_class_name;
use quilltap_core::pricing::{
    calculate_cost_tier, calculate_savings, find_cheapest_model, get_average_cost_per_1m,
    get_models_under_cost, sort_by_cost, FindCheapestOptions, ModelPricing,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct WireModel {
    #[serde(rename = "modelId")]
    model_id: String,
    #[serde(rename = "promptCostPer1M")]
    prompt_cost_per_1m: f64,
    #[serde(rename = "completionCostPer1M")]
    completion_cost_per_1m: f64,
    #[serde(rename = "contextLength")]
    context_length: Option<i64>,
    #[serde(rename = "supportsVision")]
    supports_vision: bool,
    #[serde(rename = "supportsTools")]
    supports_tools: bool,
}

impl WireModel {
    fn to_core(&self) -> ModelPricing {
        ModelPricing {
            model_id: self.model_id.clone(),
            prompt_cost_per_1m: self.prompt_cost_per_1m,
            completion_cost_per_1m: self.completion_cost_per_1m,
            context_length: self.context_length,
            supports_vision: self.supports_vision,
            supports_tools: self.supports_tools,
        }
    }
}

fn to_core_vec(v: &[WireModel]) -> Vec<ModelPricing> {
    v.iter().map(WireModel::to_core).collect()
}

#[derive(Deserialize)]
struct WireOpts {
    #[serde(rename = "requireVision")]
    require_vision: Option<bool>,
    #[serde(rename = "requireTools")]
    require_tools: Option<bool>,
    #[serde(rename = "minContextLength")]
    min_context_length: Option<i64>,
}

#[derive(Deserialize)]
struct WireClass {
    name: String,
    tier: String,
    #[serde(rename = "maxContext")]
    max_context: i64,
    #[serde(rename = "maxOutput")]
    max_output: i64,
    tags: Vec<String>,
    quality: i64,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "avg")]
    Avg {
        id: String,
        model: WireModel,
        out: f64,
    },
    #[serde(rename = "tier")]
    Tier {
        id: String,
        model: WireModel,
        out: i64,
    },
    #[serde(rename = "savings")]
    Savings {
        id: String,
        expensive: WireModel,
        cheaper: WireModel,
        out: f64,
    },
    #[serde(rename = "sort")]
    Sort {
        id: String,
        models: Vec<WireModel>,
        out: Vec<String>,
    },
    #[serde(rename = "underCost")]
    UnderCost {
        id: String,
        models: Vec<WireModel>,
        max: f64,
        out: Vec<String>,
    },
    #[serde(rename = "cheapest")]
    Cheapest {
        id: String,
        models: Vec<WireModel>,
        opts: WireOpts,
        out: Option<String>,
    },
    #[serde(rename = "modelClass")]
    ModelClass {
        id: String,
        name: String,
        out: Option<WireClass>,
    },
    #[serde(rename = "validName")]
    ValidName { id: String, name: String, out: bool },
}

#[test]
fn model_selection_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MODEL_SELECTION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_MODEL_SELECTION to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = [0usize; 8];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::Avg { id, model, out } => {
                let got = get_average_cost_per_1m(&model.to_core());
                assert!(
                    (got - out).abs() < 1e-12,
                    "avg '{id}': rust={got} oracle={out}"
                );
                counts[0] += 1;
            }
            OracleRow::Tier { id, model, out } => {
                assert_eq!(calculate_cost_tier(&model.to_core()), out, "tier '{id}'");
                counts[1] += 1;
            }
            OracleRow::Savings {
                id,
                expensive,
                cheaper,
                out,
            } => {
                let got = calculate_savings(&expensive.to_core(), &cheaper.to_core());
                assert!(
                    (got - out).abs() < 1e-12,
                    "savings '{id}': rust={got} oracle={out}"
                );
                counts[2] += 1;
            }
            OracleRow::Sort { id, models, out } => {
                let got: Vec<String> = sort_by_cost(&to_core_vec(&models))
                    .into_iter()
                    .map(|m| m.model_id)
                    .collect();
                assert_eq!(got, out, "sort '{id}'");
                counts[3] += 1;
            }
            OracleRow::UnderCost {
                id,
                models,
                max,
                out,
            } => {
                let got: Vec<String> = get_models_under_cost(&to_core_vec(&models), max)
                    .into_iter()
                    .map(|m| m.model_id)
                    .collect();
                assert_eq!(got, out, "underCost '{id}'");
                counts[4] += 1;
            }
            OracleRow::Cheapest {
                id,
                models,
                opts,
                out,
            } => {
                let options = FindCheapestOptions {
                    require_vision: opts.require_vision.unwrap_or(false),
                    require_tools: opts.require_tools.unwrap_or(false),
                    min_context_length: opts.min_context_length,
                };
                let got = find_cheapest_model(&to_core_vec(&models), options).map(|m| m.model_id);
                assert_eq!(got, out, "cheapest '{id}'");
                counts[5] += 1;
            }
            OracleRow::ModelClass { id, name, out } => {
                let got = get_model_class(&name);
                match (got, out) {
                    (None, None) => {}
                    (Some(c), Some(o)) => {
                        assert_eq!(c.name, o.name, "modelClass '{id}' name");
                        assert_eq!(c.tier, o.tier, "modelClass '{id}' tier");
                        assert_eq!(c.max_context, o.max_context, "modelClass '{id}' maxContext");
                        assert_eq!(c.max_output, o.max_output, "modelClass '{id}' maxOutput");
                        assert_eq!(c.tags, o.tags.as_slice(), "modelClass '{id}' tags");
                        assert_eq!(c.quality, o.quality, "modelClass '{id}' quality");
                    }
                    (g, o) => panic!(
                        "modelClass '{id}': presence mismatch rust={} oracle={}",
                        g.is_some(),
                        o.is_some()
                    ),
                }
                counts[6] += 1;
            }
            OracleRow::ValidName { id, name, out } => {
                assert_eq!(is_valid_model_class_name(&name), out, "validName '{id}'");
                counts[7] += 1;
            }
        }
    }

    assert!(
        counts.iter().all(|&c| c > 0),
        "oracle file looks empty/partial: {counts:?}"
    );
    eprintln!("OK: model-selection matched oracle (counts {counts:?}).");
}
