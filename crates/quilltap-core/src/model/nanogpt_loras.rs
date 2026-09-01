//! NanoGPT LoRA dialects (v4 `plugins/dist/qtap-plugin-nanogpt/image-loras.ts`,
//! `84f33ce94` + `648d5c8aa`). v5 reimplements NanoGPT natively (P4.D100/D101),
//! so the plugin's dialect table lands here as compiled data beside the wire
//! builder that consumes it.
//!
//! NanoGPT's image route takes **flat, model-specific body keys** alongside the
//! common OpenAI-compatible fields — the same passthrough class as the
//! documented `guidance_scale` / `num_inference_steps` / `strength` / `seed`
//! controls. LoRAs ride that channel, and three different families spell them
//! three different ways:
//!
//!   - `indexed` — `lora_url_1`/`lora_scale_1` … up to 3 or 4 pairs (the
//!     wavespeed-hosted Flux 2 / Klein / Z-Image / Krea set)
//!   - `weights` — a single `lora_weights` + `lora_scale`, plus an optional
//!     `hf_api_token` for private or gated HuggingFace weights (the pruna
//!     p-image set)
//!   - `url` — a single `lora_url` + `lora_strength`, plus an optional
//!     `lora_preset` (the fal-hosted flux-lora set)
//!
//! None of this is discoverable: the detailed model listing carries a `lora`
//! *tag* but leaves `allowed_passthrough_parameters` empty, so the tag can only
//! tell us a model takes adapters — never which spelling it wants. Hence a
//! static family table here, matched longest-prefix-first. A LoRA-tagged model
//! this table does not know gets the capability **without** a wire mapping and
//! a "family unknown" warning: guessing a dialect would post a body the model
//! silently ignores, which is the one failure mode nobody can see.

use serde_json::{Map, Value};

use crate::db::js_number_to_json;
use crate::image_gen::lora_support::{ImageLoraSpec, ImageLoraSupport, LoraScale};

/// How a model family spells its LoRA fields on the wire (v4
/// `NanoGPTLoraDialect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NanoGptLoraDialect {
    Indexed,
    Weights,
    Url,
}

impl NanoGptLoraDialect {
    /// The literal v4 emits in `AppliedLoras.dialect` (and in the log lines).
    pub fn as_str(self) -> &'static str {
        match self {
            NanoGptLoraDialect::Indexed => "indexed",
            NanoGptLoraDialect::Weights => "weights",
            NanoGptLoraDialect::Url => "url",
        }
    }
}

/// v4 `NanoGPTLoraFamily`.
#[derive(Debug, Clone)]
pub struct NanoGptLoraFamily {
    /// Model-id prefix; the longest matching prefix wins.
    pub prefix: &'static str,
    pub dialect: NanoGptLoraDialect,
    pub support: ImageLoraSupport,
}

/// Wavespeed-hosted indexed families: `lora_scale_N` runs 0.0–4.0, default 1,
/// step 0.1 (v4 `INDEXED_SCALE`).
const INDEXED_SCALE: LoraScale = LoraScale {
    min: 0.0,
    max: 4.0,
    default: 1.0,
    step: Some(0.1),
};

fn support(
    max_loras: f64,
    scale: LoraScale,
    supports_private_weights_token: Option<bool>,
) -> ImageLoraSupport {
    ImageLoraSupport {
        max_loras,
        scale: Some(scale),
        // Every table entry declares the same pair, in this order.
        source_kinds: vec!["url".to_string(), "hf-repo".to_string()],
        supports_private_weights_token,
    }
}

/// The family table, in v4's declaration order — [`match_lora_family`] sorts by
/// prefix length so the more specific entry always wins (`flux-2-dev-lora`
/// covers `flux-2-dev-lora-image-to-image` only because the latter is not
/// listed separately; when both are listed, the longer one is chosen).
pub fn nanogpt_lora_families() -> Vec<NanoGptLoraFamily> {
    use NanoGptLoraDialect::*;
    vec![
        // ---- indexed: lora_url_N / lora_scale_N -------------------------
        NanoGptLoraFamily {
            prefix: "flux-2-dev-lora",
            dialect: Indexed,
            support: support(4.0, INDEXED_SCALE, None),
        },
        NanoGptLoraFamily {
            prefix: "flux-2-klein-4b",
            dialect: Indexed,
            support: support(3.0, INDEXED_SCALE, None),
        },
        NanoGptLoraFamily {
            prefix: "flux-2-klein-9b",
            dialect: Indexed,
            support: support(3.0, INDEXED_SCALE, None),
        },
        NanoGptLoraFamily {
            prefix: "wavespeed-ai/flux-2-klein-base-4b",
            dialect: Indexed,
            support: support(3.0, INDEXED_SCALE, None),
        },
        NanoGptLoraFamily {
            prefix: "wavespeed-ai/flux-2-klein-base-9b",
            dialect: Indexed,
            support: support(3.0, INDEXED_SCALE, None),
        },
        NanoGptLoraFamily {
            prefix: "z-image-turbo-lora",
            dialect: Indexed,
            support: support(3.0, INDEXED_SCALE, None),
        },
        NanoGptLoraFamily {
            prefix: "wavespeed-ai/krea-v2/turbo-lora",
            dialect: Indexed,
            support: support(3.0, INDEXED_SCALE, None),
        },
        // ---- weights: lora_weights / lora_scale / hf_api_token ----------
        NanoGptLoraFamily {
            prefix: "pruna-ai/p-image/text-to-image-lora",
            dialect: Weights,
            support: support(
                1.0,
                LoraScale {
                    min: 0.0,
                    max: 4.0,
                    default: 0.5,
                    step: Some(0.05),
                },
                Some(true),
            ),
        },
        NanoGptLoraFamily {
            prefix: "pruna-ai/p-image/edit-lora",
            dialect: Weights,
            support: support(
                1.0,
                LoraScale {
                    min: 0.0,
                    max: 4.0,
                    default: 1.0,
                    step: Some(0.05),
                },
                Some(true),
            ),
        },
        // ---- url: lora_url / lora_strength / lora_preset ----------------
        NanoGptLoraFamily {
            prefix: "flux-lora",
            dialect: Url,
            support: support(
                1.0,
                LoraScale {
                    // fal's lora_strength floor is 0.1, not 0 — a zero would be
                    // rejected rather than read as "no adapter".
                    min: 0.1,
                    max: 4.0,
                    default: 1.0,
                    step: Some(0.1),
                },
                None,
            ),
        },
    ]
}

/// The family whose dialect applies to `model` — exact prefix match, longest
/// first, so `flux-lora/inpainting` lands on `flux-lora` and
/// `wavespeed-ai/flux-2-klein-base-4b/edit-lora` lands on its own base entry
/// rather than on some shorter neighbour (v4 `matchLoraFamily`).
pub fn match_lora_family(model: Option<&str>) -> Option<NanoGptLoraFamily> {
    // v4 `if (!model) return undefined` — a JS falsy test, so an empty id is
    // "no model", not "a model matching the shortest prefix".
    let model = model.filter(|m| !m.is_empty())?;
    let mut matches: Vec<NanoGptLoraFamily> = nanogpt_lora_families()
        .into_iter()
        .filter(|f| model == f.prefix || model.starts_with(f.prefix))
        .collect();
    // v4 `.sort((a, b) => b.prefix.length - a.prefix.length)[0]` — Array.sort is
    // stable in every engine v4 runs on, so equal-length prefixes keep table
    // order; `sort_by_key` on `Reverse(len)` is stable in Rust too.
    matches.sort_by_key(|f| std::cmp::Reverse(f.prefix.len()));
    matches.into_iter().next()
}

/// v4 `models.ts` `FLAGSHIP_IMAGE_MODELS` — the documented flagships, which
/// declare no LoRA support of their own.
pub const NANOGPT_FLAGSHIP_IMAGE_MODEL_IDS: [&str; 6] = [
    "hidream",
    "flux-2-flash",
    "flux-2-dev",
    "flux-2-pro",
    "recraft-v3",
    "gpt-image-1.5",
];

/// The `profileParameters` keys the NanoGPT dialect forwards, and nothing else
/// (v4 `NANOGPT_PASSTHROUGH_KEYS`).
///
/// The host hands over the profile's whole residual bag on purpose — deciding
/// what reaches the wire is the provider's job, and an allow-list is the only
/// version of that decision that cannot leak a stray key into someone's bill.
pub const NANOGPT_PASSTHROUGH_KEYS: [&str; 4] =
    ["num_inference_steps", "guidance_scale", "steps", "strength"];

/// The two LoRA-adjacent keys deliberately *absent* from the list above
/// (v4 `NANOGPT_LORA_SCOPED_KEYS`).
///
/// `hf_api_token` is a credential: it goes on the wire only when a `weights`
/// family model is actually loading gated weights, never broadcast to whatever
/// model the profile happens to point at. `lora_preset` means something only to
/// the fal-hosted `url` family. Both are attached inside [`apply_loras`], where
/// the dialect is known.
pub const NANOGPT_LORA_SCOPED_KEYS: [&str; 2] = ["hf_api_token", "lora_preset"];

/// Copy the allow-listed profile parameters onto a request body, skipping
/// blanks (an empty string is how the options panel spells "unset"). Returns
/// the keys it actually attached, for the debug log
/// (v4 `applyPassthroughParameters`).
pub fn apply_passthrough_parameters(
    body: &mut Map<String, Value>,
    profile_parameters: Option<&Value>,
) -> Vec<String> {
    let Some(bag) = profile_parameters.and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut attached = Vec::new();
    for key in NANOGPT_PASSTHROUGH_KEYS {
        // v4 `value === undefined || value === null || value === ''`.
        let value = match bag.get(key) {
            None | Some(Value::Null) => continue,
            Some(Value::String(s)) if s.is_empty() => continue,
            Some(v) => v,
        };
        body.insert(key.to_string(), value.clone());
        attached.push(key.to_string());
    }
    attached
}

/// v4 `AppliedLoras`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppliedLoras {
    /// Wire keys written onto the body.
    pub keys: Vec<String>,
    /// Sources that did not fit the model's cap, named for the log.
    pub dropped: Vec<String>,
    /// The family that decided the spelling, or `None` when none is known.
    /// A KNOWN family reports its dialect even when it wrote no keys
    /// (`648d5c8aa`), since "nothing was configured" and "nothing could be
    /// spelled" are different diagnoses — the reported dialect used to collapse
    /// the two.
    pub dialect: Option<NanoGptLoraDialect>,
}

/// Translate the host's canonical `loras` list — and the two LoRA-scoped
/// profile parameters that travel beside it — into NanoGPT's wire dialect for
/// `model`, mutating `body` (v4 `applyLoras`).
///
/// The host has already capped the list against whatever `loraSupport` this
/// provider declared for the model, so an over-cap list should not reach here —
/// but a model whose family this table does not know resolves capability from
/// the live catalog's `lora` tag alone, and then there is no cap and no
/// spelling. That case drops the whole list loudly rather than posting a body
/// the model will ignore.
///
/// **Capping therefore happens TWICE by design** — once host-side in
/// [`crate::image_gen::lora_support::cap_loras`] and once here, with different
/// log sentences (`the model's` vs `this model's`). Do not collapse them: this
/// one is the unknown-family safety net.
///
/// **An empty adapter list is not an early exit** (v4 `648d5c8aa`, bug 110).
/// `lora_preset` names a style the host already hosts and is valid on its own,
/// so it is applied whenever the family is `url` — the alternative, and the
/// original shape of bug 110, was to discard a configured preset in silence
/// because no adapter happened to sit next to it. The failure mode was a
/// SUCCESS: no error, no dropped entry, a completed and billed job returning a
/// stock image.
///
/// `hf_api_token` keeps the OPPOSITE rule for the opposite reason: it
/// authorises the fetch of caller-supplied weights, so with no weights there is
/// nothing for it to authorise, and a credential with no errand should not go
/// on the wire. **The asymmetry is deliberate — do not "consistency"-fix the
/// two back together.** The conflation of those two keys is the mistake
/// underneath the bug: they look alike in the options panel, and the code
/// applied one rule to both.
pub fn apply_loras(
    body: &mut Map<String, Value>,
    model: &str,
    loras: &[ImageLoraSpec],
    profile_parameters: Option<&Value>,
) -> AppliedLoras {
    // `648d5c8aa` (bug 110): resolve the family FIRST. The early return on an
    // empty adapter list used to sit above this, and `lora_preset`'s attachment
    // lives below it inside the url-dialect branch — two correct decisions with
    // nothing covering the seam.
    let Some(family) = match_lora_family(Some(model)) else {
        // An unknown family has no spelling for anything — adapters or preset —
        // so it writes nothing at all. Guessing posts a body the model silently
        // ignores, which is the one failure mode nobody can see. This refusal
        // was already right and is untouched by the fix.
        if !loras.is_empty() {
            tracing::warn!(
                context = "NanoGPTImageProvider.applyLoras",
                model = %model,
                dropped = ?loras.iter().map(|l| l.source.clone()).collect::<Vec<_>>(),
                "LoRA family unknown for this model; dropping the adapters rather than guessing a dialect"
            );
            return AppliedLoras {
                keys: Vec::new(),
                dropped: loras.iter().map(|l| l.source.clone()).collect(),
                dialect: None,
            };
        }
        return AppliedLoras::default();
    };

    let max = family.support.max_loras.max(0.0) as usize;
    let kept: Vec<&ImageLoraSpec> = loras.iter().take(max).collect();
    let dropped: Vec<String> = loras.iter().skip(max).map(|l| l.source.clone()).collect();
    if !dropped.is_empty() {
        tracing::warn!(
            context = "NanoGPTImageProvider.applyLoras",
            model = %model,
            dialect = %family.dialect.as_str(),
            max_loras = max,
            kept = ?kept.iter().map(|l| l.source.clone()).collect::<Vec<_>>(),
            dropped = ?dropped,
            "Capping the LoRA list to this model's limit"
        );
    }

    let mut keys: Vec<String> = Vec::new();
    let scoped = |key: &str| -> Option<String> {
        profile_parameters
            .and_then(Value::as_object)
            .and_then(|b| b.get(key))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    match family.dialect {
        NanoGptLoraDialect::Indexed => {
            for (index, lora) in kept.iter().enumerate() {
                let url_key = format!("lora_url_{}", index + 1);
                body.insert(url_key.clone(), Value::String(lora.source.clone()));
                keys.push(url_key);
                if let Some(scale) = lora.scale {
                    let scale_key = format!("lora_scale_{}", index + 1);
                    body.insert(scale_key.clone(), js_number_to_json(scale));
                    keys.push(scale_key);
                }
            }
        }
        // `648d5c8aa` guards both single-adapter arms on `kept` being
        // non-empty. That guard is also what retires v5's unguarded `kept[0]`
        // (the unification carried it forward as an owed item): with the empty
        // early return gone, an empty list now REACHES these arms.
        NanoGptLoraDialect::Weights => {
            if let Some(first) = kept.first() {
                body.insert("lora_weights".into(), Value::String(first.source.clone()));
                keys.push("lora_weights".into());
                if let Some(scale) = first.scale {
                    body.insert("lora_scale".into(), js_number_to_json(scale));
                    keys.push("lora_scale".into());
                }
                // Private / gated HuggingFace weights need a token, which rides
                // the options panel as an ordinary parameter rather than living
                // on the LoRA row — one token serves whatever weights the
                // profile points at. It stays gated on there BEING weights: it
                // is a credential for *fetching them*, and means nothing
                // without a source to fetch. Unlike the preset below, an unsent
                // token is not a silent loss — there is nothing it could have
                // authorised.
                if let Some(token) = scoped("hf_api_token") {
                    body.insert("hf_api_token".into(), Value::String(token));
                    keys.push("hf_api_token".into());
                }
            }
        }
        NanoGptLoraDialect::Url => {
            if let Some(first) = kept.first() {
                body.insert("lora_url".into(), Value::String(first.source.clone()));
                keys.push("lora_url".into());
                if let Some(scale) = first.scale {
                    body.insert("lora_strength".into(), js_number_to_json(scale));
                    keys.push("lora_strength".into());
                }
            }
            // A preset is a named style the *host* offers, not an adapter the
            // caller supplies, so it stands on its own — with or without a LoRA
            // row beside it. Gating it on the adapter list is how bug 110
            // discarded a configured preset in silence: the request succeeded,
            // and the only evidence that anything was dropped was a plain image.
            if let Some(preset) = scoped("lora_preset") {
                body.insert("lora_preset".into(), Value::String(preset));
                keys.push("lora_preset".into());
            }
        }
    }

    AppliedLoras {
        keys,
        dropped,
        dialect: Some(family.dialect),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_prefix_wins() {
        assert_eq!(
            match_lora_family(Some("flux-2-dev-lora-image-to-image"))
                .unwrap()
                .prefix,
            "flux-2-dev-lora"
        );
        assert_eq!(
            match_lora_family(Some("flux-lora/inpainting"))
                .unwrap()
                .dialect,
            NanoGptLoraDialect::Url
        );
        assert_eq!(
            match_lora_family(Some("wavespeed-ai/flux-2-klein-base-4b/edit"))
                .unwrap()
                .prefix,
            "wavespeed-ai/flux-2-klein-base-4b"
        );
        assert!(match_lora_family(Some("hidream")).is_none());
        assert!(match_lora_family(None).is_none());
        assert!(match_lora_family(Some("")).is_none());
    }

    #[test]
    fn pruna_families_declare_the_token() {
        let f = match_lora_family(Some("pruna-ai/p-image/edit-lora")).unwrap();
        assert_eq!(f.dialect, NanoGptLoraDialect::Weights);
        assert_eq!(f.support.supports_private_weights_token, Some(true));
        assert_eq!(f.support.scale.unwrap().default, 1.0);
    }
}
