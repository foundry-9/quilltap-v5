//! The NanoGPT detailed image catalog cache + the image-profile options schema
//! it feeds (v4 `plugins/.../qtap-plugin-nanogpt/image-provider.ts`, `84f33ce94`).
//!
//! v4 keeps both in one plugin module and for the same reason: **the
//! options-schema hook is synchronous and gets no API key, so it cannot fetch
//! anything itself** — but the profile editor always lists models before (and
//! whenever) it asks for a schema, and that listing does have the key. So the
//! listing fills this cache and the schema hook reads it. A cold cache is not a
//! failure: the schema falls back to the provider-wide size list, which is what
//! the hand-written panel offered before this existed.
//!
//! The cache is a process-global with a 60-minute TTL, mirroring v4's
//! module-level `let detailedCatalog` — same lifetime, same staleness rule. It
//! is the one piece of the LoRA feature that is runtime state rather than
//! compiled data, which is why [`crate::image_gen_data::lora_data_for`] reaches
//! in here for its augmentation arm rather than declaring it statically.

use std::sync::{LazyLock, Mutex};

use serde_json::{json, Map, Value};

use crate::clock::now_unix_ms;
use crate::image_gen::lora_support::ImageLoraSupport;
use crate::model::nanogpt_loras::{match_lora_family, nanogpt_lora_families};

/// v4 `NanoGPTImageModelEntry` — the fields the plugin reads off
/// `/image-models?detailed=true`. Everything is optional but the id: the
/// listing is NanoGPT's, not ours, and a missing key must degrade rather than
/// drop the row.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEntry {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub max_images: Option<f64>,
    pub resolutions: Option<Vec<String>>,
    /// `capabilities.image_generation` — read as a STRICT `=== true` everywhere
    /// v4 reads it, so a missing or non-boolean flag is not a generator.
    pub image_generation: bool,
}

/// v4 `const CATALOG_TTL_MS = 60 * 60 * 1000`.
const CATALOG_TTL_MS: i64 = 60 * 60 * 1000;

struct CatalogState {
    /// Insertion-ordered, as v4's `new Map(entries.map(...))` is: the
    /// augmentation walks `detailedCatalog.values()` in listing order.
    entries: Vec<CatalogEntry>,
    fetched_at: i64,
}

static CATALOG: LazyLock<Mutex<Option<CatalogState>>> = LazyLock::new(|| Mutex::new(None));

/// v4 `catalogIsFresh()` — a cache exists AND is inside the TTL.
fn is_fresh(state: &Option<CatalogState>) -> bool {
    match state {
        None => false,
        Some(s) => now_unix_ms() - s.fetched_at < CATALOG_TTL_MS,
    }
}

/// Parse and cache one `/image-models?detailed=true` body — v4's
/// `fetchDetailedCatalog` tail (`detailedCatalog = new Map(...)`;
/// `detailedCatalogFetchedAt = Date.now()`).
///
/// v4 caches `payload.data` when it is an array and the EMPTY array otherwise,
/// so a malformed payload still stamps a fresh (empty) cache rather than
/// leaving a stale one in place. Reproduced exactly: the schema then falls back
/// to `FALLBACK_SIZES`, which is the honest answer.
pub fn remember_detailed_catalog(body: &str) {
    let payload: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let entries: Vec<CatalogEntry> = payload
        .get("data")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(parse_entry).collect())
        .unwrap_or_default();
    let mut guard = CATALOG.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(CatalogState {
        entries,
        fetched_at: now_unix_ms(),
    });
}

/// One listing row. A row without a string `id` cannot be keyed and is dropped
/// — v4's `new Map(entries.map(e => [e.id, e]))` would key it on `undefined`
/// and no lookup can ever reach that, so dropping is the same behaviour said
/// out loud.
fn parse_entry(v: &Value) -> Option<CatalogEntry> {
    let id = v.get("id").and_then(Value::as_str)?.to_string();
    Some(CatalogEntry {
        id,
        name: v.get("name").and_then(Value::as_str).map(str::to_string),
        description: v
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        tags: v
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        max_images: v.get("max_images").and_then(Value::as_f64),
        resolutions: v
            .get("supported_parameters")
            .and_then(|p| p.get("resolutions"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            }),
        image_generation: v
            .get("capabilities")
            .and_then(|c| c.get("image_generation"))
            == Some(&Value::Bool(true)),
    })
}

/// v4 `catalogEntry(model)` — the cached entry for a model id, or `None` when
/// the cache cannot help (no model asked for, no cache, or a stale one).
fn catalog_entry(model: Option<&str>) -> Option<CatalogEntry> {
    // v4 `if (!model || !catalogIsFresh()) return undefined` — a JS falsy test,
    // so an EMPTY model id takes the cold path exactly as an absent one does.
    let model = model.filter(|m| !m.is_empty())?;
    let guard = CATALOG.lock().unwrap_or_else(|e| e.into_inner());
    if !is_fresh(&guard) {
        return None;
    }
    guard
        .as_ref()?
        .entries
        .iter()
        .find(|e| e.id == model)
        .cloned()
}

/// The fresh catalog's rows in listing order, or `None` when the cache is cold
/// or stale — v4's `if (!catalogIsFresh()) return models;` guard, hoisted so
/// the augmentation reads as one expression.
pub fn fresh_entries() -> Option<Vec<CatalogEntry>> {
    let guard = CATALOG.lock().unwrap_or_else(|e| e.into_inner());
    if !is_fresh(&guard) {
        return None;
    }
    guard.as_ref().map(|s| s.entries.clone())
}

/// The LoRA support a LIVE `lora`-tagged model outside the static dialect table
/// earns (v4 `getNanoGPTImageModels`): capability without a dialect — one
/// adapter, permissive scale, and `apply_loras` refuses to guess a spelling for
/// it and says so. Deliberately better than inventing an indexed body the model
/// would ignore.
pub fn live_tagged_lora_support() -> ImageLoraSupport {
    ImageLoraSupport {
        max_loras: 1.0,
        scale: None,
        source_kinds: vec!["url".to_string(), "hf-repo".to_string()],
        supports_private_weights_token: None,
    }
}

/// The ids a fresh catalog contributes on top of the static table, in listing
/// order — v4's `getNanoGPTImageModels()` augmentation loop, arm for arm: skip
/// what the static list already names, skip non-generators, skip anything not
/// tagged `lora`, and skip a model the dialect table already covers BY PREFIX
/// (the host's longest-prefix match finds the declared one).
pub fn augmenting_lora_model_ids(known: &[String]) -> Vec<String> {
    let Some(entries) = fresh_entries() else {
        return Vec::new();
    };
    entries
        .into_iter()
        .filter(|e| !known.contains(&e.id))
        .filter(|e| e.image_generation)
        .filter(|e| e.tags.iter().any(|t| t == "lora"))
        .filter(|e| match_lora_family(Some(&e.id)).is_none())
        .map(|e| e.id)
        .collect()
}

/// v4 `FALLBACK_SIZES` — offered when the catalog has nothing to say: the ones
/// hidream advertises plus the 1536-wide pair the Flux and GPT-Image families
/// share. Kept in step with `NANOGPT_IMAGE_CONSTRAINTS.supportedSizes`.
const FALLBACK_SIZES: &[&str] = &[
    "1024x1024",
    "768x1360",
    "1360x768",
    "880x1168",
    "1168x880",
    "1248x832",
    "832x1248",
    "1536x1024",
    "1024x1536",
];

/// v4 `diffusionModels` — NanoGPT documents these as model-specific generation
/// controls; they mean nothing to the routed API models (GPT Image, Recraft),
/// so they are offered only to the open-weight families that read them.
const DIFFUSION_MODELS: &[&str] = &[
    "flux-lora",
    "flux-2-dev",
    "flux-2-klein-4b",
    "flux-2-klein-9b",
    "z-image-turbo-lora",
    "hidream",
    "wavespeed-ai/*",
    "pruna-ai/*",
];

/// v4 `labelForSize` — `"1248x832"` → `"Landscape (1248x832)"`, for the size
/// picker's labels.
fn label_for_size(size: &str) -> String {
    // v4 `/^(\d+)\s*[x×]\s*(\d+)$/.exec(size.trim())`.
    let trimmed = crate::jsstr::js_trim(size);
    let Some((w, h)) = split_size(trimmed) else {
        return size.to_string();
    };
    if w == h {
        return format!("Square ({size})");
    }
    let ratio = w / h;
    if ratio > 1.0 {
        return if ratio >= 1.6 {
            format!("Wide ({size})")
        } else {
            format!("Landscape ({size})")
        };
    }
    if ratio <= 0.625 {
        format!("Tall ({size})")
    } else {
        format!("Portrait ({size})")
    }
}

/// The anchored `^(\d+)\s*[x×]\s*(\d+)$` match, hand-rolled: ASCII digits only
/// (JS `\d` is ASCII), the separator either `x` or `×`, and JS `\s` around it.
fn split_size(s: &str) -> Option<(f64, f64)> {
    let (lhs, rhs) = s
        .char_indices()
        .find(|(_, c)| *c == 'x' || *c == '\u{d7}')
        .map(|(i, c)| (&s[..i], &s[i + c.len_utf8()..]))?;
    let lhs = lhs.trim_end_matches(crate::jsstr::is_js_ws);
    let rhs = rhs.trim_start_matches(crate::jsstr::is_js_ws);
    if lhs.is_empty()
        || rhs.is_empty()
        || !lhs.bytes().all(|b| b.is_ascii_digit())
        || !rhs.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some((lhs.parse().ok()?, rhs.parse().ok()?))
}

/// v4 `getNanoGPTImageOptionsSchema(model?)` — the image-profile options schema
/// for a model.
///
/// Sizes and the image count come from the cached detailed catalog when it has
/// the model, so the picker offers what this model actually accepts rather than
/// one hardcoded list for two hundred models. The per-family extras (steps,
/// guidance, the fal preset, the pruna token) are gated with `appliesToModels`,
/// so a profile pointed at `hidream` is not offered a `lora_preset` box it has
/// no use for.
///
/// LoRA URLs and scales are deliberately absent: they are a structured
/// repeating pair with their own editor, declared through `loraSupport`.
pub fn nanogpt_image_options_schema(model: Option<&str>) -> Value {
    let entry = catalog_entry(model);
    // v4 `entry?.supported_parameters?.resolutions?.length ? … : FALLBACK_SIZES`
    // — a JS truthy test on `.length`, so an EMPTY advertised list falls back.
    let sizes: Vec<String> = entry
        .as_ref()
        .and_then(|e| e.resolutions.as_ref())
        .filter(|r| !r.is_empty())
        .cloned()
        .unwrap_or_else(|| FALLBACK_SIZES.iter().map(|s| (*s).to_string()).collect());
    // `entry?.max_images && entry.max_images > 0 ? entry.max_images : 1`.
    let max_images = entry
        .as_ref()
        .and_then(|e| e.max_images)
        .filter(|n| *n != 0.0 && !n.is_nan() && *n > 0.0)
        .unwrap_or(1.0);

    let mut fields: Vec<Value> = Vec::new();
    fields.push(json!({
        "key": "size",
        "label": "Default Size",
        "type": "enum",
        // `sizes.includes('1024x1024') ? '1024x1024' : sizes[0]` — an empty
        // list is unreachable (the fallback is non-empty and an empty
        // advertised list already fell back), so `sizes[0]` always exists.
        "default": if sizes.iter().any(|s| s == "1024x1024") { "1024x1024".to_string() } else { sizes[0].clone() },
        "helpText": if entry.is_some() {
            "The resolutions this model advertises. Requests that name no size take the first."
        } else {
            "Common sizes across NanoGPT's image models; each model maps to its nearest native resolution."
        },
        "enumValues": sizes.iter().map(|s| json!({"value": s, "label": label_for_size(s)})).collect::<Vec<_>>(),
    }));

    if max_images > 1.0 {
        fields.push(json!({
            "key": "n",
            "label": "Images per Request",
            "type": "number",
            "default": 1,
            "helpText": format!(
                "This model returns up to {} per request. Leave blank for one.",
                crate::pascal::js_value::number_to_string(max_images)
            ),
        }));
    }

    fields.push(json!({
        "key": "num_inference_steps",
        "label": "Inference Steps",
        "type": "number",
        "helpText": "More steps, more refinement, more time and money. Blank leaves it to the model.",
        "appliesToModels": DIFFUSION_MODELS,
    }));
    fields.push(json!({
        "key": "guidance_scale",
        "label": "Guidance Scale",
        "type": "number",
        "helpText": "How closely the model is held to the prompt. Low wanders and invents; high obeys and stiffens. Blank leaves it to the model.",
        "appliesToModels": DIFFUSION_MODELS,
    }));

    // The fal-hosted flux-lora family is the only one that takes a named preset.
    fields.push(json!({
        "key": "lora_preset",
        "label": "LoRA Preset",
        "type": "string",
        "helpText": "A named preset offered by this model's host, applied alongside whatever adapter you list below. Leave blank unless you have been given one.",
        "appliesToModels": ["flux-lora"],
    }));

    // The pruna families can load private or gated HuggingFace weights.
    let token_models: Vec<String> = nanogpt_lora_families()
        .into_iter()
        .filter(|f| f.support.supports_private_weights_token == Some(true))
        .map(|f| format!("{}*", f.prefix))
        .collect();
    fields.push(json!({
        "key": "hf_api_token",
        "label": "HuggingFace Token (private weights)",
        "type": "string",
        "helpText": "Only needed when your LoRA lives behind a gated or private HuggingFace repository. It is sent to NanoGPT with the request, and only for the models that can use it.",
        "appliesToModels": token_models,
    }));

    let mut group = Map::new();
    group.insert("title".into(), json!("NanoGPT Image Options"));
    group.insert("helpText".into(), json!("NanoGPT routes each request to the named model's own atelier, so these controls mean whatever that establishment takes them to mean — and the ones it has no use for are simply not offered. Sizes and the image count come from the model's own advertised capabilities where NanoGPT publishes them."));
    group.insert("fields".into(), Value::Array(fields));

    let mut out = Map::new();
    out.insert("groups".into(), Value::Array(vec![Value::Object(group)]));
    Value::Object(out)
}

/// v4 `plugin.getImageProviderOptionsSchema?.({ modelName: model }) ?? null` —
/// the per-provider hook, dispatched by provider id.
///
/// NanoGPT is the ONLY built-in that declares it (`84f33ce94`; grep
/// `getImageProviderOptionsSchema` across v4's plugins). Every other provider
/// answers `None`, which the route serves as `null` and the editor reads as
/// "fall back to the legacy hand-written panel".
pub fn image_provider_options_schema(provider: &str, model: Option<&str>) -> Option<Value> {
    match provider {
        "NANOGPT" => Some(nanogpt_image_options_schema(model)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_labels_match_v4s_ratio_bands() {
        assert_eq!(label_for_size("1024x1024"), "Square (1024x1024)");
        assert_eq!(label_for_size("1248x832"), "Landscape (1248x832)");
        // 1360/768 = 1.77… ≥ 1.6.
        assert_eq!(label_for_size("1360x768"), "Wide (1360x768)");
        assert_eq!(label_for_size("832x1248"), "Portrait (832x1248)");
        // 768/1360 = 0.564… ≤ 0.625.
        assert_eq!(label_for_size("768x1360"), "Tall (768x1360)");
        // Unparseable shapes come back verbatim, as v4's failed `exec` does.
        assert_eq!(label_for_size("wide"), "wide");
        assert_eq!(label_for_size("1024"), "1024");
        // The `×` alternative and JS whitespace around the separator.
        assert_eq!(label_for_size("1024 × 1024"), "Square (1024 × 1024)");
    }

    #[test]
    fn a_cold_cache_serves_the_fallback_sizes() {
        // The cache is process-global; this asserts the COLD-path help text,
        // which no other test in this file arms.
        let schema = nanogpt_image_options_schema(Some("a-model-no-catalog-can-know"));
        let fields = schema["groups"][0]["fields"].as_array().unwrap();
        assert_eq!(fields[0]["key"], "size");
        assert_eq!(
            fields[0]["helpText"],
            "Common sizes across NanoGPT's image models; each model maps to its nearest native resolution."
        );
        assert_eq!(fields[0]["enumValues"].as_array().unwrap().len(), 9);
        // No `n` field: a cold cache means max_images 1, so v4 omits it.
        assert!(fields.iter().all(|f| f["key"] != "n"));
    }

    /// The cache's whole runtime story in ONE test, deliberately: it is a
    /// process global, so a second `#[test]` writing it could race this one's
    /// reads. The differential cannot reach any of this — its NanoGPT arms use
    /// no API key, so the listing never runs and the cache stays cold — which
    /// is exactly why these assertions exist.
    #[test]
    fn the_live_catalog_drives_augmentation_and_the_warm_schema() {
        let body = r#"{"data":[
            {"id":"someone/new-lora-model","tags":["lora","fast"],
             "capabilities":{"image_generation":true},
             "max_images":4,
             "supported_parameters":{"resolutions":["512x512","1920x1080"]}},
            {"id":"someone/not-a-generator","tags":["lora"],
             "capabilities":{"image_generation":false}},
            {"id":"someone/untagged","capabilities":{"image_generation":true}},
            {"id":"flux-2-dev-lora-turbo","tags":["lora"],
             "capabilities":{"image_generation":true}},
            {"id":"flux-lora","tags":["lora"],
             "capabilities":{"image_generation":true}}
        ]}"#;
        remember_detailed_catalog(body);

        // Only the first row augments: the second is not a generator, the third
        // is untagged, the fourth is already covered BY PREFIX
        // (`flux-2-dev-lora`), and the fifth is already a static id.
        let known: Vec<String> = crate::image_gen_data::lora_data_for("NANOGPT")
            .0
            .iter()
            .map(|m| m.id.clone())
            .collect();
        assert!(
            known.contains(&"someone/new-lora-model".to_string()),
            "the live-tagged model must join the declarations: {known:?}"
        );
        assert!(!known.contains(&"someone/not-a-generator".to_string()));
        assert!(!known.contains(&"someone/untagged".to_string()));
        assert!(!known.contains(&"flux-2-dev-lora-turbo".to_string()));
        assert_eq!(known.iter().filter(|k| *k == "flux-lora").count(), 1);

        // Capability WITHOUT a dialect — one adapter, no scale band declared.
        let decls = crate::image_gen_data::image_declarations_for("NANOGPT");
        let support = crate::image_gen::lora_support::resolve_lora_support(
            Some(&decls.models),
            Some("someone/new-lora-model"),
            decls.lora_provider.as_ref(),
        )
        .expect("the live-tagged model resolves support");
        assert_eq!(support.max_loras, 1.0);
        assert_eq!(support.scale, None);
        assert_eq!(support.source_kinds, vec!["url", "hf-repo"]);
        assert_eq!(support.supports_private_weights_token, None);

        // A WARM cache changes the schema: the advertised resolutions replace
        // the fallback list, the help text switches, and `max_images > 1` adds
        // the `n` field the cold path omits.
        let schema = nanogpt_image_options_schema(Some("someone/new-lora-model"));
        let fields = schema["groups"][0]["fields"].as_array().unwrap();
        assert_eq!(fields[0]["key"], "size");
        assert_eq!(
            fields[0]["helpText"],
            "The resolutions this model advertises. Requests that name no size take the first."
        );
        // No 1024x1024 among them, so the default is the FIRST advertised size.
        assert_eq!(fields[0]["default"], "512x512");
        assert_eq!(
            fields[0]["enumValues"],
            serde_json::json!([
                {"value": "512x512", "label": "Square (512x512)"},
                {"value": "1920x1080", "label": "Wide (1920x1080)"},
            ])
        );
        assert_eq!(fields[1]["key"], "n");
        assert_eq!(fields[1]["default"], 1);
        assert_eq!(
            fields[1]["helpText"],
            "This model returns up to 4 per request. Leave blank for one."
        );

        // A model the catalog does not name still takes the cold path.
        let cold = nanogpt_image_options_schema(Some("hidream"));
        assert!(cold["groups"][0]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["key"] != "n"));

        // A malformed payload stamps a fresh EMPTY cache rather than leaving
        // this one in place — v4's `Array.isArray(payload.data) ? … : []`.
        remember_detailed_catalog("not json at all");
        assert_eq!(augmenting_lora_model_ids(&[]), Vec::<String>::new());
    }

    #[test]
    fn the_token_models_come_from_the_dialect_tables_own_flag() {
        let schema = nanogpt_image_options_schema(None);
        let fields = schema["groups"][0]["fields"].as_array().unwrap();
        let token = fields.iter().find(|f| f["key"] == "hf_api_token").unwrap();
        let applies: Vec<&str> = token["appliesToModels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            applies,
            vec![
                "pruna-ai/p-image/text-to-image-lora*",
                "pruna-ai/p-image/edit-lora*"
            ]
        );
    }
}
