//! LoRA support resolver (v4 `lib/image-gen/lora-support.ts`, `84f33ce94`).
//!
//! The single host-side helper that answers "may this provider/model take LoRA
//! adapters, and how many?" — and, given a user's stored list, produces the
//! capped, validated list that will actually ride the request.
//!
//! A plugin opts in by declaring `loraSupport`, either per-model on
//! `getImageGenerationModels()` or provider-wide on
//! `getImageProviderConstraints()`. A plugin that declares nothing resolves to
//! `None` here, the editor hides itself, and `ImageGenParams.loras` is never
//! set — which is the whole genericity guarantee: adding LoRAs to one provider
//! costs every other provider zero lines.
//!
//! Lookup order mirrors `resolveOrientation` exactly:
//!   1. Per-model `loraSupport` (exact id, then longest-prefix family match).
//!   2. Provider-level `loraSupport`.
//!   3. None.
//!
//! Like the orientation resolver this module is **pure** — it reads the
//! in-process declarations with no DB or network access (v5 injects them as
//! data through [`crate::image_gen_data::lora_data_for`], the
//! `orientation_data_for` precedent).

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::db::js_number_to_json;
use crate::image_gen::{match_model, ModelInfo};
use crate::pascal::js_value::{to_js_string, to_number};

/// Scale bounds used when a plugin declares `loraSupport` but no `scale` block
/// (v4 `DEFAULT_LORA_SCALE`). **v4 `d4138b96b` MOVED this constant and
/// `resolveLoraScaleBounds` out of `lora-support.ts` into the client-safe
/// `lib/image-gen/lora-scale.ts`** (the bounds were dead in the old module while
/// the profile editor carried a byte-identical copy) and dropped the re-export.
/// The move was value-neutral — measured identical at `0b0617fee` and
/// `d883a5ee1` — and `default_lora_scale_matches_v4` below pins the four
/// literals against their NEW home so a future v4 edit there cannot pass unseen.
///
/// Permissive on purpose — the provider's own default applies when the user
/// leaves the slider alone, and every provider surveyed tops out at or below 4.
pub const DEFAULT_LORA_SCALE: LoraScale = LoraScale {
    min: 0.0,
    max: 2.0,
    default: 1.0,
    step: Some(0.05),
};

/// v4 `ImageLoraSupport['scale']` — `{ min, max, default, step? }`, in that
/// declaration order (v4's `JSON.stringify` emits insertion order).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoraScale {
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub step: Option<f64>,
}

impl Serialize for LoraScale {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut len = 3;
        if self.step.is_some() {
            len += 1;
        }
        let mut st = s.serialize_struct("LoraScale", len)?;
        st.serialize_field("min", &js_number_to_json(self.min))?;
        st.serialize_field("max", &js_number_to_json(self.max))?;
        st.serialize_field("default", &js_number_to_json(self.default))?;
        if let Some(step) = self.step {
            st.serialize_field("step", &js_number_to_json(step))?;
        }
        st.end()
    }
}

/// v4 `ImageLoraSupport` — `{ maxLoras, scale?, sourceKinds, supportsPrivateWeightsToken? }`.
/// Key order is the declaration order every provider entry uses; the two
/// optional keys are OMITTED when absent (v4 never spells them `undefined`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageLoraSupport {
    pub max_loras: f64,
    pub scale: Option<LoraScale>,
    /// `'url' | 'hf-repo' | 'provider-id'`.
    pub source_kinds: Vec<String>,
    pub supports_private_weights_token: Option<bool>,
}

impl Serialize for ImageLoraSupport {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut len = 2;
        if self.scale.is_some() {
            len += 1;
        }
        if self.supports_private_weights_token.is_some() {
            len += 1;
        }
        let mut st = s.serialize_struct("ImageLoraSupport", len)?;
        st.serialize_field("maxLoras", &js_number_to_json(self.max_loras))?;
        if let Some(scale) = &self.scale {
            st.serialize_field("scale", scale)?;
        }
        st.serialize_field("sourceKinds", &self.source_kinds)?;
        if let Some(b) = self.supports_private_weights_token {
            st.serialize_field("supportsPrivateWeightsToken", &b)?;
        }
        st.end()
    }
}

/// v4 `ImageLoraSpec` — one adapter on the wire. Built key-by-key in
/// [`read_loras_from_parameters`], so the serialized order is
/// `source, scale?, triggerPhrase?, label?`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImageLoraSpec {
    pub source: String,
    pub scale: Option<f64>,
    pub trigger_phrase: Option<String>,
    pub label: Option<String>,
}

impl Serialize for ImageLoraSpec {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut len = 1;
        if self.scale.is_some() {
            len += 1;
        }
        if self.trigger_phrase.is_some() {
            len += 1;
        }
        if self.label.is_some() {
            len += 1;
        }
        let mut st = s.serialize_struct("ImageLoraSpec", len)?;
        st.serialize_field("source", &self.source)?;
        if let Some(v) = self.scale {
            st.serialize_field("scale", &js_number_to_json(v))?;
        }
        if let Some(v) = &self.trigger_phrase {
            st.serialize_field("triggerPhrase", v)?;
        }
        if let Some(v) = &self.label {
            st.serialize_field("label", v)?;
        }
        st.end()
    }
}

/// The log context v4 spreads into every `[Image LoRA]` line. v5 renders these
/// through `tracing`; a log-only field is differential-invisible, so the
/// sentences are pinned by the capturing-layer tests instead.
///
/// v4 builds it in `resolveProfileLoras` as `{ provider, model, ...logContext }`
/// (`lib/image-gen/params-builder.ts:150`) — the profile's provider/model plus
/// **the caller's own fields**, which is how an operator reading
/// `combined.log` learns WHICH generation dropped a malformed adapter. Until
/// P4.70 v5 carried only the first two, so every `[Image LoRA]` line named a
/// provider and nothing else; the caller half is now here.
#[derive(Debug, Clone, Default)]
pub struct LoraLogContext {
    pub provider: String,
    pub model: Option<String>,
    /// The call-site label — v4's `context` key (`'tools.generate_image'`,
    /// `'tools.generate_image.style-options'`, `'background-jobs.story-background'`, …).
    pub context: &'static str,
    pub chat_id: Option<String>,
    pub job_id: Option<String>,
    pub profile_id: Option<String>,
}

/// Resolve LoRA support for a provider/model pair (v4 `resolveLoraSupport`).
///
/// `models` / `provider_support` are the injected declarations —
/// `getImageGenerationModels(provider)` and
/// `getImageProviderConstraints(provider)?.loraSupport`. Returns `None` when
/// neither the model nor the provider declares any: the signal every caller
/// reads as "this profile has no LoRA story; do not offer one, do not send
/// one."
///
/// One matcher, two capabilities — this walks the SAME [`match_model`] the
/// orientation resolver walks, so a family prefix that resolves an orientation
/// resolves a LoRA capability the same way.
pub fn resolve_lora_support(
    models: Option<&[ModelInfo]>,
    model: Option<&str>,
    provider_support: Option<&ImageLoraSupport>,
) -> Option<ImageLoraSupport> {
    // v4 `matchModel(...)?.loraSupport` then `if (perModel)` — a JS truthy
    // test on the declaration object.
    if let Some(per_model) = match_model(models, model).and_then(|m| m.lora_support.as_ref()) {
        return Some(per_model.clone());
    }
    provider_support.cloned()
}

/// The scale bounds the editor and the capper should use for this support
/// (v4 `resolveLoraScaleBounds`).
pub fn resolve_lora_scale_bounds(support: &ImageLoraSupport) -> LoraScale {
    match support.scale {
        None => DEFAULT_LORA_SCALE,
        Some(declared) => LoraScale {
            min: declared.min,
            max: declared.max,
            default: declared.default,
            // `declared.step ?? DEFAULT_LORA_SCALE.step`.
            step: declared.step.or(DEFAULT_LORA_SCALE.step),
        },
    }
}

/// Read the `loras` list off an image profile's `parameters` bag
/// (v4 `readLorasFromParameters`).
///
/// Storage is an opaque JSON blob that also round-trips through `.qtap`
/// imports and hand-edited backups, so this re-checks the shape rather than
/// trusting it: entries that are not objects with a non-empty `source` are
/// dropped, and a non-finite or out-of-range `scale` is dropped down to
/// "unset" rather than poisoning the request. Every drop is named in the log.
pub fn read_loras_from_parameters(
    parameters: Option<&Value>,
    log_context: &LoraLogContext,
) -> Vec<ImageLoraSpec> {
    // v4 `parameters?.loras` — an absent bag, a non-object bag, an absent key
    // and an explicit null all land on the same empty answer.
    let raw = match parameters.and_then(|p| p.get("loras")) {
        None | Some(Value::Null) => return Vec::new(),
        Some(v) => v,
    };
    let Some(list) = raw.as_array() else {
        tracing::warn!(
            provider = %log_context.provider,
            model = ?log_context.model,
            context = %log_context.context,
            chat_id = ?log_context.chat_id,
            job_id = ?log_context.job_id,
            profile_id = ?log_context.profile_id,
            stored_type = %js_typeof(raw),
            "[Image LoRA] Ignoring a `loras` parameter that is not a list"
        );
        return Vec::new();
    };

    let mut kept: Vec<ImageLoraSpec> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();

    for entry in list {
        if !entry.is_object() {
            dropped.push(to_js_string(entry));
            continue;
        }
        let source = entry
            .get("source")
            .and_then(Value::as_str)
            .map(|s| crate::jsstr::js_trim(s).to_string())
            .unwrap_or_default();
        if source.is_empty() {
            dropped.push("(entry with no source)".to_string());
            continue;
        }

        let mut spec = ImageLoraSpec {
            source: source.clone(),
            ..Default::default()
        };

        if let Some(raw_scale) = entry.get("scale") {
            let scale = to_number(raw_scale);
            if scale.is_finite() && (0.0..=10.0).contains(&scale) {
                spec.scale = Some(scale);
            } else {
                tracing::warn!(
                    provider = %log_context.provider,
                    model = ?log_context.model,
                    context = %log_context.context,
                    chat_id = ?log_context.chat_id,
                    job_id = ?log_context.job_id,
                    profile_id = ?log_context.profile_id,
                    source = %source,
                    stored_scale = %raw_scale,
                    "[Image LoRA] Dropping an out-of-range scale; the provider default applies"
                );
            }
        }
        if let Some(p) = entry.get("triggerPhrase").and_then(Value::as_str) {
            let t = crate::jsstr::js_trim(p);
            if !t.is_empty() {
                spec.trigger_phrase = Some(t.to_string());
            }
        }
        if let Some(l) = entry.get("label").and_then(Value::as_str) {
            let t = crate::jsstr::js_trim(l);
            if !t.is_empty() {
                spec.label = Some(t.to_string());
            }
        }

        kept.push(spec);
    }

    if !dropped.is_empty() {
        tracing::warn!(
            provider = %log_context.provider,
            model = ?log_context.model,
            context = %log_context.context,
            chat_id = ?log_context.chat_id,
            job_id = ?log_context.job_id,
            profile_id = ?log_context.profile_id,
            dropped = ?dropped,
            kept_count = kept.len(),
            "[Image LoRA] Dropped malformed entries from a profile's stored LoRA list"
        );
    }

    kept
}

/// JS `typeof` for the non-list warn's `storedType` field.
fn js_typeof(v: &Value) -> &'static str {
    match v {
        Value::Null => "object",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) | Value::Object(_) => "object",
    }
}

/// Cap a stored LoRA list against the resolved support, naming anything that
/// falls off (v4 `capLoras`).
///
/// Never silently drops: an over-cap profile (saved against a four-adapter
/// model, then pointed at a one-adapter model) logs the sources it is leaving
/// behind, and the profile itself keeps them so switching the model back loses
/// nothing.
pub fn cap_loras(
    loras: Vec<ImageLoraSpec>,
    support: Option<&ImageLoraSupport>,
    log_context: &LoraLogContext,
) -> Vec<ImageLoraSpec> {
    if loras.is_empty() {
        return Vec::new();
    }
    let Some(support) = support else {
        tracing::warn!(
            provider = %log_context.provider,
            model = ?log_context.model,
            context = %log_context.context,
            chat_id = ?log_context.chat_id,
            job_id = ?log_context.job_id,
            profile_id = ?log_context.profile_id,
            stripped = ?loras.iter().map(|l| l.source.clone()).collect::<Vec<_>>(),
            "[Image LoRA] Stripping LoRAs — this provider/model declares no LoRA support"
        );
        return Vec::new();
    };

    // v4 `Math.max(0, Math.floor(support.maxLoras))`. The NaN cap is worth a
    // word: JS's `Math.max(0, NaN)` is NaN, `length <= NaN` is false, and both
    // `slice(0, NaN)` and `slice(NaN)` coerce NaN to 0 — so a NaN cap keeps
    // NOTHING and names every entry as dropped. Rust's `f64::max` returns the
    // non-NaN operand, giving `max = 0`, which lands on exactly that.
    let max = support.max_loras.floor().max(0.0) as usize;
    if loras.len() <= max {
        return loras;
    }

    let kept: Vec<ImageLoraSpec> = loras.iter().take(max).cloned().collect();
    tracing::warn!(
        provider = %log_context.provider,
        model = ?log_context.model,
        context = %log_context.context,
        chat_id = ?log_context.chat_id,
        job_id = ?log_context.job_id,
        profile_id = ?log_context.profile_id,
        max_loras = max,
        kept = ?kept.iter().map(|l| l.source.clone()).collect::<Vec<_>>(),
        dropped = ?loras.iter().skip(max).map(|l| l.source.clone()).collect::<Vec<_>>(),
        "[Image LoRA] Capping the LoRA list to the model's limit"
    );
    kept
}

/// The trigger phrases carried by a resolved LoRA list, in order, deduplicated
/// (two adapters from the same family often share a magic word) and with the
/// blanks removed (v4 `loraTriggerPhrases`).
pub fn lora_trigger_phrases(loras: &[ImageLoraSpec]) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut phrases = Vec::new();
    for lora in loras {
        let phrase = match lora.trigger_phrase.as_deref() {
            Some(p) => crate::jsstr::js_trim(p),
            None => continue,
        };
        if phrase.is_empty() {
            continue;
        }
        // v4 `phrase.toLowerCase()` — `str::to_lowercase` is byte-identical to
        // JS's for the whole corpus (the Phase-1 ICU cluster).
        let key = phrase.to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        phrases.push(phrase.to_string());
    }
    phrases
}

/// Those same phrases as the single string the prompt crafter's
/// `styleTriggerPhrase` seam takes; empty when no adapter asks for one
/// (v4 `joinLoraTriggerPhrases`).
pub fn join_lora_trigger_phrases(loras: &[ImageLoraSpec]) -> String {
    lora_trigger_phrases(loras).join(", ")
}

#[cfg(test)]
mod scale_bounds_tests {
    use super::*;

    /// P4.D157: v4's LoRA scale bounds now live in `lib/image-gen/lora-scale.ts`
    /// (moved out of `lora-support.ts` by `d4138b96b`). The four literals are
    /// transcribed twice in v5 — here and in the SPA's `lora-list-editor.ts`
    /// `DEFAULT_SCALE` — so pin the server copy against the values in the new
    /// home. A drift here is a real behaviour change: these bounds are what the
    /// editor's slider offers and what the capper falls back to.
    #[test]
    fn default_lora_scale_matches_v4() {
        // v4 `lib/image-gen/lora-scale.ts`:
        //   export const DEFAULT_LORA_SCALE =
        //     { min: 0, max: 2, default: 1, step: 0.05 } as const;
        assert_eq!(DEFAULT_LORA_SCALE.min, 0.0);
        assert_eq!(DEFAULT_LORA_SCALE.max, 2.0);
        assert_eq!(DEFAULT_LORA_SCALE.default, 1.0);
        assert_eq!(DEFAULT_LORA_SCALE.step, Some(0.05));
    }

    /// v4 `resolveLoraScaleBounds`: a declared block with no `step` falls back
    /// to the default's step, and the other three come from the declaration.
    #[test]
    fn resolve_bounds_step_falls_back_to_the_default() {
        let declared = ImageLoraSupport {
            max_loras: 2.0,
            scale: Some(LoraScale {
                min: 0.25,
                max: 4.0,
                default: 0.8,
                step: None,
            }),
            source_kinds: vec!["hf-repo".to_string()],
            supports_private_weights_token: None,
        };
        let got = resolve_lora_scale_bounds(&declared);
        assert_eq!(got.min, 0.25);
        assert_eq!(got.max, 4.0);
        assert_eq!(got.default, 0.8);
        assert_eq!(got.step, DEFAULT_LORA_SCALE.step);
    }
}

#[cfg(test)]
mod log_context_tests {
    use super::*;
    use crate::test_support::captured;

    // === P4.70: the `[Image LoRA]` caller spread ===
    //
    // v4 builds each line's context as `{ provider, model, ...logContext }`
    // (`lib/image-gen/params-builder.ts:150`), so every one of the five warns
    // names the CALL SITE and the chat/job/profile it belongs to. v5 carried
    // only `provider` and `model`, which made a `combined.log` line true but
    // useless: an operator could see that a malformed adapter was dropped and
    // not which generation dropped it.
    //
    // A differential cannot see a log-only fix, so the capture layer is the
    // proof (`crate::test_support`, P4.77). One test per line, each mutation
    // (deleting one field from one `tracing::warn!`) reddening exactly one of
    // them.

    /// The spread a caller supplies — v4's `tools.generate_image` shape.
    fn ctx() -> LoraLogContext {
        LoraLogContext {
            provider: "NANOGPT".to_string(),
            model: Some("flux-2-dev-lora".to_string()),
            context: "tools.generate_image",
            chat_id: Some("chat-77".to_string()),
            job_id: None,
            profile_id: Some("profile-9".to_string()),
        }
    }

    /// Assert one captured line carries the sentence AND the whole caller
    /// spread. The sentence match is a substring because the level/target
    /// prefix and the field list surround it.
    fn assert_line(lines: &[String], sentence: &str) {
        let line = lines
            .iter()
            .find(|l| l.contains(sentence))
            .unwrap_or_else(|| panic!("no line carried {sentence:?}; got {lines:#?}"));
        for field in [
            "provider=NANOGPT",
            "model=Some(\"flux-2-dev-lora\")",
            "context=tools.generate_image",
            "chat_id=Some(\"chat-77\")",
            "job_id=None",
            "profile_id=Some(\"profile-9\")",
        ] {
            assert!(
                line.contains(field),
                "line is missing {field}: {line}\n(all: {lines:#?})"
            );
        }
    }

    #[test]
    fn the_not_a_list_warn_carries_the_caller_context() {
        let params = serde_json::json!({ "loras": "owner/portrait-lora" });
        let lines = captured(|| {
            read_loras_from_parameters(Some(&params), &ctx());
        });
        assert_line(
            &lines,
            "[Image LoRA] Ignoring a `loras` parameter that is not a list",
        );
    }

    #[test]
    fn the_out_of_range_scale_warn_carries_the_caller_context() {
        let params = serde_json::json!({
            "loras": [{ "source": "owner/portrait-lora", "scale": 42 }]
        });
        let lines = captured(|| {
            read_loras_from_parameters(Some(&params), &ctx());
        });
        assert_line(
            &lines,
            "[Image LoRA] Dropping an out-of-range scale; the provider default applies",
        );
    }

    #[test]
    fn the_dropped_entries_warn_carries_the_caller_context() {
        let params = serde_json::json!({
            "loras": [{ "source": "owner/portrait-lora" }, "not-an-object", { "scale": 1 }]
        });
        let lines = captured(|| {
            read_loras_from_parameters(Some(&params), &ctx());
        });
        assert_line(
            &lines,
            "[Image LoRA] Dropped malformed entries from a profile's stored LoRA list",
        );
    }

    #[test]
    fn the_stripping_warn_carries_the_caller_context() {
        let loras = vec![ImageLoraSpec {
            source: "owner/portrait-lora".to_string(),
            ..Default::default()
        }];
        let lines = captured(|| {
            cap_loras(loras, None, &ctx());
        });
        assert_line(
            &lines,
            "[Image LoRA] Stripping LoRAs — this provider/model declares no LoRA support",
        );
    }

    #[test]
    fn the_capping_warn_carries_the_caller_context() {
        let loras = (0..3)
            .map(|i| ImageLoraSpec {
                source: format!("owner/lora-{i}"),
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let support = ImageLoraSupport {
            max_loras: 1.0,
            scale: None,
            source_kinds: vec!["hf-repo".to_string()],
            supports_private_weights_token: None,
        };
        let lines = captured(|| {
            cap_loras(loras, Some(&support), &ctx());
        });
        assert_line(
            &lines,
            "[Image LoRA] Capping the LoRA list to the model's limit",
        );
    }
}
