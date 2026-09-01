//! Differential test (W4.1d5, tier-1, DB-free): the image-generation pure leaves
//! `parse_placeholders` + `resolve_orientation` (`quilltap_core::image_gen`) vs
//! v4's REAL `parsePlaceholders` / `resolveOrientation` (with the plugin registry
//! mocked to canned declarations). Diffs `serde_json::to_string` against v4's
//! `JSON.stringify`.
//!
//! P4.D138 (`84f33ce94`) grows it by two families, both driven from the row's own
//! recorded INPUT (no transcription of the canned data into Rust):
//!   - `quilltap_core::model_matchers` vs v4's REAL `modelMatchesPattern` /
//!     `fieldAppliesToModel` — including the JS-`.`-semantics trio (`\n`, `\r`,
//!     U+2028 across a `*`), which is what pins the negated character class the
//!     Rust glob is built from;
//!   - `quilltap_core::image_gen::lora_support` vs v4's REAL `resolveLoraSupport`
//!     / `resolveLoraScaleBounds` / `readLorasFromParameters` / `capLoras` /
//!     `loraTriggerPhrases` / `joinLoraTriggerPhrases`, plus the
//!     `DEFAULT_LORA_SCALE` constant;
//!   - `quilltap_core::image_gen::params_builder` vs v4's REAL
//!     `buildImageGenParams` + `HOST_OWNED_PARAMETER_KEYS` — the merge
//!     semantics key for key, the orientation overwrite, the LoRA cap and
//!     trigger-phrase append, and the residual bag.
//!
//! Generate the oracle (Node 24, from the v4 checkout; STAGE outside `.claude/`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; WT=<worktree> ; STAGE=/tmp/qt-oracle-stage
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases
//!   cp $WT/harness/oracle/cases/image-gen-leaves.test.ts $STAGE/harness/oracle/cases/
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-image-gen-leaves.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- image-gen-leaves
//! Run:
//!   QT_ORACLE_IMAGE_GEN_LEAVES=/tmp/oracle-image-gen-leaves.ndjson \
//!     cargo test -p quilltap-harness --test image_gen_leaves_equivalence

use std::collections::HashMap;

use quilltap_core::image_gen::lora_support::{
    cap_loras, join_lora_trigger_phrases, lora_trigger_phrases, read_loras_from_parameters,
    resolve_lora_scale_bounds, resolve_lora_support, ImageLoraSpec, ImageLoraSupport,
    LoraLogContext, LoraScale, DEFAULT_LORA_SCALE,
};
use quilltap_core::image_gen::params_builder::{
    build_image_gen_params, ImageDeclarations, ImageGenOverrides, ImageParamsLogContext,
    ImageProfileLike, HOST_OWNED_PARAMETER_KEYS,
};
use quilltap_core::image_gen::{
    parse_placeholders, resolve_orientation, ModelInfo, Orientation, OrientationMapping,
    OrientationStrategy, OrientationSupport,
};
use quilltap_core::model_matchers::{field_applies_to_model, model_matches_pattern};
use serde_json::Value;

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

fn m(
    size: Option<&str>,
    ar: Option<&str>,
    hint: Option<&str>,
    nw: Option<f64>,
    nh: Option<f64>,
) -> OrientationMapping {
    OrientationMapping {
        size: size.map(str::to_string),
        aspect_ratio: ar.map(str::to_string),
        prompt_hint: hint.map(str::to_string),
        nominal_width: nw,
        nominal_height: nh,
    }
}

fn size_support() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Size,
        portrait: m(Some("1024x1792"), None, None, Some(1024.0), Some(1792.0)),
        landscape: m(Some("1792x1024"), None, None, Some(1792.0), Some(1024.0)),
        square: Some(m(Some("1024x1024"), None, None, None, None)),
    }
}
fn aspect_support() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::AspectRatio,
        portrait: m(None, Some("3:4"), None, None, None),
        landscape: m(None, Some("4:3"), None, None, None),
        square: None,
    }
}
fn prompt_support() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Prompt,
        portrait: m(None, None, Some("a tall vertical framing"), None, None),
        landscape: m(None, None, Some("a wide framing"), None, None),
        square: None,
    }
}
fn degrade_support() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Size,
        portrait: m(None, None, Some("custom portrait hint"), None, None),
        landscape: m(Some("1792x1024"), None, None, None, None),
        square: None,
    }
}
fn no_square_support() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Size,
        portrait: m(Some("1024x1792"), None, None, None, None),
        landscape: m(Some("1792x1024"), None, None, None, None),
        square: None,
    }
}
fn model(id: &str, support: OrientationSupport) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        orientation_support: Some(support),
        lora_support: None,
    }
}

/// Rebuild an `ImageLoraSupport` from the row's recorded declaration — the
/// oracle emits the exact object it handed v4, so nothing is transcribed.
fn support_from_json(v: &Value) -> ImageLoraSupport {
    ImageLoraSupport {
        max_loras: v["maxLoras"].as_f64().expect("maxLoras is a number"),
        scale: v.get("scale").filter(|s| !s.is_null()).map(|s| LoraScale {
            min: s["min"].as_f64().unwrap(),
            max: s["max"].as_f64().unwrap(),
            default: s["default"].as_f64().unwrap(),
            step: s.get("step").and_then(Value::as_f64),
        }),
        source_kinds: v["sourceKinds"]
            .as_array()
            .expect("sourceKinds is an array")
            .iter()
            .map(|k| k.as_str().unwrap().to_string())
            .collect(),
        supports_private_weights_token: v
            .get("supportsPrivateWeightsToken")
            .and_then(Value::as_bool),
    }
}

/// Rebuild an `OrientationSupport` from a row's recorded declaration.
fn mapping_from_json(v: &Value) -> OrientationMapping {
    OrientationMapping {
        size: v.get("size").and_then(Value::as_str).map(str::to_string),
        aspect_ratio: v
            .get("aspectRatio")
            .and_then(Value::as_str)
            .map(str::to_string),
        prompt_hint: v
            .get("promptHint")
            .and_then(Value::as_str)
            .map(str::to_string),
        nominal_width: v.get("nominalWidth").and_then(Value::as_f64),
        nominal_height: v.get("nominalHeight").and_then(Value::as_f64),
    }
}

fn orientation_from_json(v: &Value) -> OrientationSupport {
    OrientationSupport {
        strategy: match v["strategy"].as_str() {
            Some("size") => OrientationStrategy::Size,
            Some("aspectRatio") => OrientationStrategy::AspectRatio,
            _ => OrientationStrategy::Prompt,
        },
        portrait: mapping_from_json(&v["portrait"]),
        landscape: mapping_from_json(&v["landscape"]),
        square: v
            .get("square")
            .filter(|s| !s.is_null())
            .map(mapping_from_json),
    }
}

/// Rebuild the `ImageLoraSpec[]` a `lora_cap` / `lora_phrases` row was given.
/// Deliberately NOT `read_loras_from_parameters`: these rows hand v4 a
/// well-formed list directly, so reading it through the sanitizer would make
/// the capper's arms depend on the reader's.
fn specs_from_json(v: &Value) -> Vec<ImageLoraSpec> {
    v.as_array()
        .expect("loras is an array")
        .iter()
        .map(|e| ImageLoraSpec {
            source: e["source"].as_str().unwrap().to_string(),
            scale: e.get("scale").and_then(Value::as_f64),
            trigger_phrase: e
                .get("triggerPhrase")
                .and_then(Value::as_str)
                .map(str::to_string),
            label: e.get("label").and_then(Value::as_str).map(str::to_string),
        })
        .collect()
}

#[test]
fn image_gen_leaves_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_IMAGE_GEN_LEAVES") else {
        eprintln!("SKIP: set QT_ORACLE_IMAGE_GEN_LEAVES (see test header).");
        return;
    };
    let oracle = load_oracle(&oracle_path);

    // ---- parse_placeholders ----
    let placeholder_cases: Vec<(&str, &str)> = vec![
        ("p_two", "A scene with {{me}} and {{Aurora}}."),
        ("p_none", "no placeholders here"),
        ("p_ws", "{{ me }}"),
        ("p_adjacent", "{{a}}{{b}}"),
        ("p_empty", "{{}}"),
        ("p_multi", "before {{x}} middle {{y}} after {{z}}"),
        ("p_unclosed", "a {{ dangling name and {{good}}"),
    ];
    for (label, prompt) in &placeholder_cases {
        let got = serde_json::to_string(&parse_placeholders(prompt)).unwrap();
        let want = oracle
            .get(*label)
            .unwrap_or_else(|| panic!("oracle missing {label}"));
        assert_eq!(
            got.as_str(),
            want["json"].as_str().unwrap(),
            "placeholders {label}"
        );
    }

    // ---- resolve_orientation ----
    struct OCase {
        label: &'static str,
        model: Option<&'static str>,
        orientation: Orientation,
        models: Option<Vec<ModelInfo>>,
        constraints: Option<OrientationSupport>,
    }
    let ocases = vec![
        OCase {
            label: "o_fallback_portrait",
            model: None,
            orientation: Orientation::Portrait,
            models: None,
            constraints: None,
        },
        OCase {
            label: "o_fallback_square",
            model: None,
            orientation: Orientation::Square,
            models: None,
            constraints: None,
        },
        OCase {
            label: "o_size_portrait",
            model: Some("dall-e-3"),
            orientation: Orientation::Portrait,
            models: Some(vec![model("dall-e-3", size_support())]),
            constraints: None,
        },
        OCase {
            label: "o_size_landscape",
            model: Some("dall-e-3"),
            orientation: Orientation::Landscape,
            models: Some(vec![model("dall-e-3", size_support())]),
            constraints: None,
        },
        OCase {
            label: "o_prefix_match",
            model: Some("dall-e-3-mini"),
            orientation: Orientation::Portrait,
            models: Some(vec![
                model("dall-e-3", size_support()),
                model("dall-e-2", aspect_support()),
            ]),
            constraints: None,
        },
        OCase {
            label: "o_aspect",
            model: Some("flux-pro"),
            orientation: Orientation::Landscape,
            models: Some(vec![model("flux-pro", aspect_support())]),
            constraints: None,
        },
        OCase {
            label: "o_prompt_strategy",
            model: Some("m"),
            orientation: Orientation::Portrait,
            models: Some(vec![model("m", prompt_support())]),
            constraints: None,
        },
        OCase {
            label: "o_degrade_to_hint",
            model: Some("m"),
            orientation: Orientation::Portrait,
            models: Some(vec![model("m", degrade_support())]),
            constraints: None,
        },
        OCase {
            label: "o_declared_absent_square",
            model: Some("m"),
            orientation: Orientation::Square,
            models: Some(vec![model("m", no_square_support())]),
            constraints: None,
        },
        OCase {
            label: "o_provider_level",
            model: None,
            orientation: Orientation::Portrait,
            models: None,
            constraints: Some(size_support()),
        },
        OCase {
            label: "o_no_match_falls_provider",
            model: Some("unknown-model"),
            orientation: Orientation::Landscape,
            models: Some(vec![model("other", aspect_support())]),
            constraints: Some(prompt_support()),
        },
    ];
    for c in &ocases {
        let got = serde_json::to_string(&resolve_orientation(
            c.models.as_deref(),
            c.model,
            c.constraints.as_ref(),
            c.orientation,
        ))
        .unwrap();
        let want = oracle
            .get(c.label)
            .unwrap_or_else(|| panic!("oracle missing {}", c.label));
        assert_eq!(
            got.as_str(),
            want["json"].as_str().unwrap(),
            "orientation {}",
            c.label
        );
    }

    // ---- model matchers (P4.D138) ----
    let mut matcher_rows = 0usize;
    for (label, row) in &oracle {
        match row["kind"].as_str() {
            Some("matcher_pattern") => {
                let got = serde_json::to_string(&model_matches_pattern(
                    row["model"].as_str().unwrap(),
                    row["pattern"].as_str().unwrap(),
                ))
                .unwrap();
                assert_eq!(
                    got.as_str(),
                    row["json"].as_str().unwrap(),
                    "matcher_pattern {label}"
                );
                matcher_rows += 1;
            }
            Some("matcher_field") => {
                let list: Option<Vec<String>> = row["list"].as_array().map(|a| {
                    a.iter()
                        .map(|v| v.as_str().unwrap().to_string())
                        .collect::<Vec<_>>()
                });
                let got = serde_json::to_string(&field_applies_to_model(
                    list.as_deref(),
                    row["model"].as_str(),
                ))
                .unwrap();
                assert_eq!(
                    got.as_str(),
                    row["json"].as_str().unwrap(),
                    "matcher_field {label}"
                );
                matcher_rows += 1;
            }
            _ => {}
        }
    }
    assert!(
        matcher_rows >= 30,
        "the matcher corpus shrank ({matcher_rows} rows) — a stale oracle"
    );

    // ---- LoRA support (P4.D138) ----
    let ctx = LoraLogContext {
        provider: "NANOGPT".to_string(),
        model: Some("flux-2-dev-lora".to_string()),
    };
    let mut lora_rows = 0usize;
    for (label, row) in &oracle {
        let kind = row["kind"].as_str().unwrap_or_default();
        let want = row["json"].as_str().unwrap();
        match kind {
            "lora_const" => {
                assert_eq!(
                    serde_json::to_string(&DEFAULT_LORA_SCALE).unwrap().as_str(),
                    want,
                    "lora_const {label}"
                );
                lora_rows += 1;
            }
            "lora_support" => {
                let models: Option<Vec<ModelInfo>> = row["models"].as_array().map(|a| {
                    a.iter()
                        .map(|m| ModelInfo {
                            id: m["id"].as_str().unwrap().to_string(),
                            orientation_support: None,
                            lora_support: m
                                .get("loraSupport")
                                .filter(|s| !s.is_null())
                                .map(support_from_json),
                        })
                        .collect()
                });
                let constraints = row["constraints"]
                    .as_object()
                    .map(|_| support_from_json(&row["constraints"]));
                let support = resolve_lora_support(
                    models.as_deref(),
                    row["model"].as_str(),
                    constraints.as_ref(),
                );
                let bounds = support.as_ref().map(resolve_lora_scale_bounds);
                let got = serde_json::json!({
                    "support": support,
                    "bounds": bounds,
                });
                assert_eq!(
                    serde_json::to_string(&got).unwrap().as_str(),
                    want,
                    "lora_support {label}"
                );
                lora_rows += 1;
            }
            "lora_read" => {
                let bag = row["bag"].clone();
                let got =
                    read_loras_from_parameters(if bag.is_null() { None } else { Some(&bag) }, &ctx);
                assert_eq!(
                    serde_json::to_string(&got).unwrap().as_str(),
                    want,
                    "lora_read {label}"
                );
                lora_rows += 1;
            }
            "lora_cap" => {
                let loras = specs_from_json(&row["loras"]);
                let support = row["support"]
                    .as_object()
                    .map(|_| support_from_json(&row["support"]));
                let got = cap_loras(loras, support.as_ref(), &ctx);
                assert_eq!(
                    serde_json::to_string(&got).unwrap().as_str(),
                    want,
                    "lora_cap {label}"
                );
                lora_rows += 1;
            }
            "lora_phrases" => {
                let loras = specs_from_json(&row["loras"]);
                let got = serde_json::json!({
                    "phrases": lora_trigger_phrases(&loras),
                    "joined": join_lora_trigger_phrases(&loras),
                });
                assert_eq!(
                    serde_json::to_string(&got).unwrap().as_str(),
                    want,
                    "lora_phrases {label}"
                );
                lora_rows += 1;
            }
            _ => {}
        }
    }
    assert!(
        lora_rows >= 45,
        "the LoRA corpus shrank ({lora_rows} rows) — a stale oracle"
    );

    // ---- params-builder (P4.D138) ----
    let mut pb_rows = 0usize;
    for (label, row) in &oracle {
        match row["kind"].as_str() {
            Some("params_const") => {
                assert_eq!(
                    serde_json::to_string(&HOST_OWNED_PARAMETER_KEYS)
                        .unwrap()
                        .as_str(),
                    row["json"].as_str().unwrap(),
                    "params_const {label}"
                );
                pb_rows += 1;
            }
            Some("params_builder") => {
                let models: Vec<ModelInfo> = row["models"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|m| ModelInfo {
                                id: m["id"].as_str().unwrap().to_string(),
                                orientation_support: m
                                    .get("orientationSupport")
                                    .filter(|v| !v.is_null())
                                    .map(orientation_from_json),
                                lora_support: m
                                    .get("loraSupport")
                                    .filter(|v| !v.is_null())
                                    .map(support_from_json),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let cons = &row["constraints"];
                let declarations = ImageDeclarations {
                    models,
                    orientation_provider: cons
                        .get("orientationSupport")
                        .filter(|v| !v.is_null())
                        .map(orientation_from_json),
                    lora_provider: cons
                        .get("loraSupport")
                        .filter(|v| !v.is_null())
                        .map(support_from_json),
                };
                let profile_params = row["profile"]["parameters"].clone();
                let profile = ImageProfileLike {
                    provider: row["profile"]["provider"].as_str().unwrap(),
                    model_name: row["profile"]["modelName"].as_str(),
                    parameters: if profile_params.is_null() {
                        None
                    } else {
                        Some(&profile_params)
                    },
                };
                let o = &row["overrides"];
                let overrides = ImageGenOverrides {
                    negative_prompt: o
                        .get("negativePrompt")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    model: o.get("model").and_then(Value::as_str).map(str::to_string),
                    n: o.get("n").and_then(Value::as_f64),
                    size: o.get("size").and_then(Value::as_str).map(str::to_string),
                    aspect_ratio: o
                        .get("aspectRatio")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    quality: o.get("quality").and_then(Value::as_str).map(str::to_string),
                    style: o.get("style").and_then(Value::as_str).map(str::to_string),
                    response_format: o
                        .get("responseFormat")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    seed: o.get("seed").and_then(Value::as_f64),
                    guidance_scale: o.get("guidanceScale").and_then(Value::as_f64),
                    steps: o.get("steps").and_then(Value::as_f64),
                };
                let orientation = match row["orientation"].as_str() {
                    Some("portrait") => Some(Orientation::Portrait),
                    Some("landscape") => Some(Orientation::Landscape),
                    Some("square") => Some(Orientation::Square),
                    _ => None,
                };
                let fallback = row["fallbackModel"].as_str().unwrap_or("dall-e-3");
                let built = build_image_gen_params(
                    profile,
                    row["prompt"].as_str().unwrap(),
                    &overrides,
                    orientation,
                    fallback,
                    &declarations,
                    &ImageParamsLogContext::default(),
                );
                let got = serde_json::json!({
                    "params": built.params.to_key_value(),
                    "loraSupport": built.lora_support,
                    "loras": built.loras,
                    "loraTriggerPhrase": built.lora_trigger_phrase,
                    "appendedTriggerPhrases": built.appended_trigger_phrases,
                    "orientation": built.orientation,
                });
                assert_eq!(
                    serde_json::to_string(&got).unwrap().as_str(),
                    row["json"].as_str().unwrap(),
                    "params_builder {label}"
                );
                pb_rows += 1;
            }
            _ => {}
        }
    }
    assert!(
        pb_rows >= 24,
        "the params-builder corpus shrank ({pb_rows} rows) — a stale oracle"
    );

    eprintln!(
        "OK: image-gen-leaves differential matched the oracle ({matcher_rows} matcher + {lora_rows} LoRA + {pb_rows} params rows)."
    );
}
