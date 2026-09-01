//! The image half of the model boundary — the tier-3 seam for the
//! `generate_image` tool. Mirrors the subset of v4's image-provider contract the
//! handler consumes: `provider.generateImage(params, apiKey)` from
//! `@quilltap/plugin-types` (the merged request → `{ images: [...] }` result),
//! as driven by `lib/tools/handlers/image-generation-handler.ts`.
//!
//! The boundary sits at the provider call itself. The real image wire dialects
//! (the OpenAI / Grok / Google-Imagen request builders + response decoders) are
//! **W4.7f** — the provider stays canned here, exactly as the v4 oracle mocks
//! `provider.generateImage`. API-key acquisition stays host-side (the
//! `ApiKeyResolver` seam supplies the decrypted key; the canned responder needs
//! none). The `image/webp` transcode is a separate injected seam
//! ([`ImageTranscoder`]) — no image-codec crate in the core (the `doc_blob`
//! precedent).

use std::collections::HashMap;
use std::future::Future;

use serde_json::Value;

use crate::db::js_number_to_json;
use crate::image_gen::lora_support::ImageLoraSpec;

/// The merged image-generation request (v4 `mergeParameters` output — the fields
/// `applyOrientation` mutates plus the profile-default carry-overs). Bound
/// verbatim into [`image_gen_key`]: the canned key proves the merged params
/// (incl. the orientation-driven `size`/`aspectRatio` and the appended prompt
/// hint) reach the provider.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImageGenParams {
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub model: String,
    /// v4 `n` — image count. A JS number: the builder's
    /// `overrides.n ?? asNumber(defaults.n) ?? 1` can carry a fractional stored
    /// default, so this is `f64` and renders through `js_number_to_json`
    /// (an integral value still prints as `1`, never `1.0`).
    pub n: Option<f64>,
    pub size: Option<String>,
    pub aspect_ratio: Option<String>,
    /// `'standard' | 'hd'`.
    pub quality: Option<String>,
    /// `'vivid' | 'natural'`.
    pub style: Option<String>,
    /// v4 `responseFormat` — read off the profile bag by the shared builder
    /// (`84f33ce94`). No v5 dialect consumes it yet; it is carried because the
    /// builder sets it and the params object is a comparand.
    pub response_format: Option<String>,
    pub seed: Option<f64>,
    pub guidance_scale: Option<f64>,
    pub steps: Option<f64>,
    /// The capped, validated adapter list (`84f33ce94`). Empty means the key is
    /// ABSENT on the wire — v4 only ever assigns `params.loras` when the list is
    /// non-empty.
    pub loras: Vec<ImageLoraSpec>,
    /// The profile's residual `parameters` bag — everything the host does not
    /// own by name, handed to the provider so per-model options travel without
    /// the host enumerating them. `None` when nothing is left over.
    pub profile_parameters: Option<Value>,
    /// `size` was INSERTED by the orientation pass (section 2 of v4's
    /// `buildImageGenParams`) because the merge (section 1) produced none. JS
    /// objects remember insertion order, so that `size` sits AFTER `steps` in
    /// v4's `JSON.stringify` — [`Self::to_key_value`] reproduces the slot. An
    /// orientation that OVERWRITES a merged size keeps section 1's slot.
    /// (The unification review's catch: declaration order ≠ insertion order in
    /// exactly this shape, and no corpus row hit it until
    /// `pb_orientation_inserts_size_last`.)
    pub size_inserted_by_orientation: bool,
    /// The `aspectRatio` twin of [`Self::size_inserted_by_orientation`].
    pub aspect_ratio_inserted_by_orientation: bool,
}

impl ImageGenParams {
    /// The params as the canonical JSON object v4 would `JSON.stringify` — the
    /// deterministic [`image_gen_key`]'s payload AND the params-builder
    /// differential's comparand. Optional fields are omitted when `None`
    /// (matching v4's `undefined`-drop).
    ///
    /// **The key ORDER is v4's `buildImageGenParams` INSERTION order**
    /// (`84f33ce94`): the literal `{prompt, model, n}` first, then each
    /// conditional assignment in source order, then `loras`, then
    /// `profileParameters`. `JSON.stringify` emits insertion order, so getting
    /// this wrong silently shifts every canned image key in the harness. One
    /// shape makes insertion order differ from declaration order: a `size` /
    /// `aspectRatio` the merge did NOT produce but the orientation pass did is
    /// inserted after `steps` (see the two `*_inserted_by_orientation` flags).
    pub fn to_key_value(&self) -> Value {
        let mut map = serde_json::Map::new();
        map.insert("prompt".into(), Value::String(self.prompt.clone()));
        map.insert("model".into(), Value::String(self.model.clone()));
        if let Some(v) = self.n {
            map.insert("n".into(), js_number_to_json(v));
        }
        if let Some(v) = &self.negative_prompt {
            map.insert("negativePrompt".into(), Value::String(v.clone()));
        }
        if let Some(v) = &self.size {
            if !self.size_inserted_by_orientation {
                map.insert("size".into(), Value::String(v.clone()));
            }
        }
        if let Some(v) = &self.aspect_ratio {
            if !self.aspect_ratio_inserted_by_orientation {
                map.insert("aspectRatio".into(), Value::String(v.clone()));
            }
        }
        if let Some(v) = &self.quality {
            map.insert("quality".into(), Value::String(v.clone()));
        }
        if let Some(v) = &self.style {
            map.insert("style".into(), Value::String(v.clone()));
        }
        if let Some(v) = &self.response_format {
            map.insert("responseFormat".into(), Value::String(v.clone()));
        }
        if let Some(v) = self.seed {
            map.insert("seed".into(), js_number_to_json(v));
        }
        if let Some(v) = self.guidance_scale {
            map.insert("guidanceScale".into(), js_number_to_json(v));
        }
        if let Some(v) = self.steps {
            map.insert("steps".into(), js_number_to_json(v));
        }
        // Section 2's own insertions — only when section 1 set nothing.
        if let Some(v) = &self.size {
            if self.size_inserted_by_orientation {
                map.insert("size".into(), Value::String(v.clone()));
            }
        }
        if let Some(v) = &self.aspect_ratio {
            if self.aspect_ratio_inserted_by_orientation {
                map.insert("aspectRatio".into(), Value::String(v.clone()));
            }
        }
        if !self.loras.is_empty() {
            map.insert(
                "loras".into(),
                serde_json::to_value(&self.loras).unwrap_or(Value::Null),
            );
        }
        if let Some(v) = &self.profile_parameters {
            map.insert("profileParameters".into(), v.clone());
        }
        Value::Object(map)
    }
}

/// One generated image (v4 `GeneratedImage` in the provider response's `images`
/// array). `data` carries the base64-encoded bytes; `url` carries a hosted image
/// URL (populated by z-ai's 30-day URLs and OpenRouter's external-URL fallback —
/// the only providers that set it). Both are optional because z-ai emits
/// `data: img.b64_json` verbatim (JS `undefined` → `None` when the provider
/// returned only a URL); `revised_prompt` is the provider's revision (or `None`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeneratedImageData {
    /// Base64-encoded image bytes (v4 `img.data`; may be `None` for a URL-only
    /// z-ai / OpenRouter-external result).
    pub data: Option<String>,
    /// A hosted image URL (v4 `img.url`).
    pub url: Option<String>,
    /// `image/png` etc. (v4 `img.mimeType || 'image/png'` — the default is
    /// applied by the caller, not here).
    pub mime_type: Option<String>,
    pub revised_prompt: Option<String>,
}

/// The image-generation provider response (v4 `{ images: GeneratedImage[] }`).
#[derive(Clone, Debug, PartialEq)]
pub struct ImageGenResponse {
    pub images: Vec<GeneratedImageData>,
}

/// Error from an image-generation call. The message text matters: the handler
/// inspects it via [`is_image_moderation_error`](crate::services::dangerous_content::provider_routing::is_image_moderation_error)
/// to decide the post-hoc Concierge reroute.
#[derive(Clone, Debug)]
pub struct ImageGenError {
    pub message: String,
}

impl ImageGenError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ImageGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ImageGenError {}

/// The image-generation boundary — every `provider.generateImage(params, key)`
/// call goes through this trait. `provider` mirrors v4's
/// `createImageProvider(profile.provider)` factory argument; folding it into the
/// call keeps the trait a single seam. Async and generic-consumed like the
/// completion / embedding boundaries.
pub trait ImageProvider {
    fn generate_image(
        &self,
        provider: &str,
        api_key: &str,
        params: &ImageGenParams,
    ) -> impl Future<Output = Result<ImageGenResponse, ImageGenError>> + Send;
}

/// The keyed model-discovery boundary (v4 `ca22ec45`
/// `ImageProvider.getAvailableModels(apiKey?)`). `api_key` is `None` when the
/// caller has no key to offer, in which case the provider answers its curated
/// static list without touching the network. A live failure is an `Err` whose
/// message is the sentence v4 throws — the `list-models` route surfaces it as
/// `fetchError` and falls back to `supportedModels`.
pub trait ImageModelDiscovery {
    fn available_models(
        &self,
        provider: &str,
        api_key: Option<&str>,
    ) -> impl Future<Output = Result<Vec<String>, ImageGenError>> + Send;
}

/// The object-safe form of [`ImageModelDiscovery`], which the dispatch engine
/// holds behind an `Arc<dyn …>` (the [`ErasedImageGeneration`](crate::tools::generate_image::ErasedImageGeneration)
/// precedent) so the `imageProfileListModels` arm needs none of the provider's
/// transport generics.
pub trait ImageModelDiscoveryDyn: Send + Sync {
    fn available_models<'a>(
        &'a self,
        provider: &'a str,
        api_key: Option<&'a str>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<String>, ImageGenError>> + Send + 'a>>;
}

impl<T: ImageModelDiscovery + Send + Sync> ImageModelDiscoveryDyn for T {
    fn available_models<'a>(
        &'a self,
        provider: &'a str,
        api_key: Option<&'a str>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<String>, ImageGenError>> + Send + 'a>>
    {
        Box::pin(ImageModelDiscovery::available_models(
            self, provider, api_key,
        ))
    }
}

/// A type-erased [`ImageModelDiscovery`].
#[derive(Clone)]
pub struct ErasedImageDiscovery(std::sync::Arc<dyn ImageModelDiscoveryDyn>);

impl ErasedImageDiscovery {
    /// Wrap a concrete discovery provider.
    pub fn new<D: ImageModelDiscoveryDyn + 'static>(inner: D) -> Self {
        Self(std::sync::Arc::new(inner))
    }

    /// Discover `provider`'s image models, optionally with a key.
    pub async fn available_models(
        &self,
        provider: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<String>, ImageGenError> {
        self.0.available_models(provider, api_key).await
    }
}

/// The image → WebP transcode boundary (v4 `convertToWebP` /
/// `transcodeToWebP`). No image-codec crate in the core (the `doc_blob`
/// precedent). Given the decoded bytes + the provider mime type + the desired
/// filename, return the post-transcode bytes + mime + filename + measured
/// dimensions. A pass-through implementation (bytes unchanged) is used both
/// sides of the differential.
pub trait ImageTranscoder {
    fn transcode(&self, input: &TranscodeInput) -> TranscodeOutput;
}

/// Input to the transcode seam.
#[derive(Clone, Debug)]
pub struct TranscodeInput {
    /// Decoded (post-base64) image bytes.
    pub bytes: Vec<u8>,
    /// The provider's declared mime type (`image/png` etc.).
    pub mime_type: String,
    /// The provider filename (`generated_<ts>.<ext>`).
    pub filename: String,
}

/// Output of the transcode seam (v4 `convertToWebP`'s
/// `{ buffer, mimeType, filename, width?, height? }`).
#[derive(Clone, Debug)]
pub struct TranscodeOutput {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub filename: String,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

/// The canonical lookup key for a canned image generation: `provider|model|<params
/// JSON>`. The v4 oracle's mock computes the identical string (provider name +
/// the merged params it received), so a call either matches an entry registered
/// on both sides or surfaces as a corpus omission on both sides. Crucially the
/// params JSON includes the orientation-mutated `size`/`aspectRatio` and the
/// appended prompt hint, so a divergence in `mergeParameters` / `applyOrientation`
/// surfaces as a canned miss.
pub fn image_gen_key(provider: &str, params: &ImageGenParams) -> String {
    let params_json =
        serde_json::to_string(&params.to_key_value()).expect("image params serialize infallibly");
    format!("{provider}|{}|{params_json}", params.model)
}

/// A deterministic [`ImageProvider`] for the tier-3 differential. Returns a fixed
/// [`ImageGenResponse`] keyed by the exact call ([`image_gen_key`]), or an
/// explicit failure for keys registered as failing. The Rust test and the v4
/// oracle build the same key→response map, so the image call is pinned
/// identically on both sides.
///
/// A failure entry fails with its registered message, so the moderation-error
/// post-hoc reroute can be driven deterministically. An unregistered input is an
/// error (a corpus omission — surfaced, never silently answered).
#[derive(Clone, Default)]
pub struct CannedImageProvider {
    responses: HashMap<String, ImageGenResponse>,
    failures: HashMap<String, String>,
}

impl CannedImageProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a response for the exact call `(provider, params)`.
    pub fn with_response(
        mut self,
        provider: &str,
        params: &ImageGenParams,
        response: ImageGenResponse,
    ) -> Self {
        self.responses
            .insert(image_gen_key(provider, params), response);
        self
    }

    /// Register a failure (with its exact error message) for the call key.
    pub fn with_failure(
        mut self,
        provider: &str,
        params: &ImageGenParams,
        message: impl Into<String>,
    ) -> Self {
        self.failures
            .insert(image_gen_key(provider, params), message.into());
        self
    }

    /// Register a response under a RAW pre-computed key string (the differential
    /// harness records v4's exact `provider|model|JSON.stringify(params)` key and
    /// registers it verbatim; the lookup then computes the Rust [`image_gen_key`],
    /// so a `mergeParameters`/`applyOrientation` divergence surfaces as a miss).
    pub fn with_raw_key(mut self, key: impl Into<String>, response: ImageGenResponse) -> Self {
        self.responses.insert(key.into(), response);
        self
    }

    /// Register a failure under a RAW pre-computed key string (the post-hoc
    /// moderation reroute — v4's original-profile call throws its exact message).
    pub fn with_raw_failure(mut self, key: impl Into<String>, message: impl Into<String>) -> Self {
        self.failures.insert(key.into(), message.into());
        self
    }
}

impl ImageProvider for CannedImageProvider {
    async fn generate_image(
        &self,
        provider: &str,
        _api_key: &str,
        params: &ImageGenParams,
    ) -> Result<ImageGenResponse, ImageGenError> {
        let key = image_gen_key(provider, params);
        if let Some(message) = self.failures.get(&key) {
            return Err(ImageGenError::new(message.clone()));
        }
        match self.responses.get(&key) {
            Some(resp) => Ok(resp.clone()),
            None => Err(ImageGenError::new(format!(
                "CannedImageProvider: no canned response for key `{key}`"
            ))),
        }
    }
}

/// A pass-through [`ImageTranscoder`]: returns the input bytes / mime / filename
/// unchanged, with no measured dimensions. The faithful wiring for the tier-3
/// differential (the v4 oracle mocks `convertToWebP` to the identical
/// pass-through), and the host default until the real WebP encoder lands.
#[derive(Clone, Copy, Debug, Default)]
pub struct PassthroughTranscoder;

impl ImageTranscoder for PassthroughTranscoder {
    fn transcode(&self, input: &TranscodeInput) -> TranscodeOutput {
        TranscodeOutput {
            bytes: input.bytes.clone(),
            mime_type: input.mime_type.clone(),
            filename: input.filename.clone(),
            width: None,
            height: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(model: &str, size: Option<&str>) -> ImageGenParams {
        ImageGenParams {
            prompt: "a cat".into(),
            model: model.into(),
            n: Some(1.0),
            size: size.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn key_reflects_merged_params() {
        // Same provider/model, different size ⇒ different key (orientation proof).
        let a = image_gen_key("OPENAI", &params("dall-e-3", Some("1024x1024")));
        let b = image_gen_key("OPENAI", &params("dall-e-3", Some("1792x1024")));
        assert_ne!(a, b);
        assert!(a.starts_with("OPENAI|dall-e-3|"));
    }

    #[tokio::test]
    async fn canned_response_and_failure() {
        let p = params("dall-e-3", Some("1024x1024"));
        let provider = CannedImageProvider::new()
            .with_response(
                "OPENAI",
                &p,
                ImageGenResponse {
                    images: vec![GeneratedImageData {
                        data: Some("AAAA".into()),
                        url: None,
                        mime_type: Some("image/png".into()),
                        revised_prompt: Some("a fluffy cat".into()),
                    }],
                },
            )
            .with_failure("GROK", &p, "content moderation rejected");

        let ok = provider.generate_image("OPENAI", "k", &p).await.unwrap();
        assert_eq!(ok.images[0].revised_prompt.as_deref(), Some("a fluffy cat"));

        let err = provider.generate_image("GROK", "k", &p).await.unwrap_err();
        assert!(
            crate::services::dangerous_content::provider_routing::is_image_moderation_error(
                &err.message
            )
        );

        // Unregistered ⇒ surfaced error.
        let miss = params("other", None);
        assert!(provider.generate_image("OPENAI", "k", &miss).await.is_err());
    }

    #[test]
    fn passthrough_transcoder_preserves_bytes() {
        let out = PassthroughTranscoder.transcode(&TranscodeInput {
            bytes: vec![1, 2, 3],
            mime_type: "image/png".into(),
            filename: "generated_1.png".into(),
        });
        assert_eq!(out.bytes, vec![1, 2, 3]);
        assert_eq!(out.mime_type, "image/png");
        assert!(out.width.is_none());
    }
}
