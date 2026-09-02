//! HuggingFace repository lookup for LoRA sources (v4
//! `lib/image-gen/huggingface-lookup.ts`, `2ece98c90`).
//!
//! A LoRA row on an image profile is free text: the user types `owner/name` or
//! a weights URL, and it goes to the provider unexamined. This module is the
//! one place that asks HuggingFace what it knows about such a source, so the
//! editor can show the user the facts and let them draw their own conclusions.
//!
//! **It deliberately renders no verdict on compatibility.** Whether a given
//! adapter works with a given provider model would have to be inferred by
//! matching two independent naming conventions — NanoGPT's model ids against
//! HuggingFace's `base_model` strings — and neither owes us stability. A false
//! "this will not work" on an adapter that works is worse than silence, so this
//! reports what the repository declares and stops there. (v4 pins that with a
//! test; the corpus carries the same guard.)
//!
//! Network-touching and host-side only: the browser never calls HuggingFace
//! directly, so egress stays in one place and a token for gated weights can be
//! attached without ever reaching the page.

use std::future::Future;

use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;
use serde_json::Value;

use crate::image_gen::huggingface_repo_id::{extract_huggingface_repo_id, huggingface_card_url};
use crate::model::wire::WireResponse;

/// v4 `HUGGINGFACE_API_BASE`.
const HUGGINGFACE_API_BASE: &str = "https://huggingface.co/api/models";

/// v4 `LOOKUP_TIMEOUT_MS` — "HuggingFace is not in the request path of anything;
/// it gets ten seconds." v4 spends it as `AbortSignal.timeout`; v5's host spends
/// it as the transport's per-request timeout, which is the same bound in the
/// same place (the one call this seam makes).
pub const LOOKUP_TIMEOUT_MS: u64 = 10_000;

/// A JS `Error` as this module reads one: `lookupHuggingFaceLora` branches on
/// `error.name` (`TimeoutError` / `AbortError` → timeout, anything else →
/// network) and reports `error.message` as the detail. A bare message string
/// could not carry that distinction, so the seam carries both — the host maps
/// its own transport failure onto these two fields.
#[derive(Clone, Debug, PartialEq)]
pub struct ThrownError {
    pub name: String,
    pub message: String,
}

impl ThrownError {
    /// v4 `error.name === 'TimeoutError' || error.name === 'AbortError'`.
    fn timed_out(&self) -> bool {
        self.name == "TimeoutError" || self.name == "AbortError"
    }
}

/// The one outbound call this module makes. Its own seam rather than the shared
/// [`crate::model::wire::WireTransport`] because that one collapses a throw to a
/// message, and this module's timeout/network split reads the thrown error's
/// NAME.
pub trait LoraMetadataTransport: Send + Sync {
    fn get(
        &self,
        url: &str,
        headers: &[(String, String)],
    ) -> impl Future<Output = Result<WireResponse, ThrownError>> + Send;
}

/// The dyn-safe shadow of [`LoraMetadataTransport`] (an `async fn` in a trait is
/// not object-safe, so the erasure needs a boxed-future twin). `pub` only
/// because it appears in [`lookup_huggingface_lora`]'s bound; the blanket impl
/// below means every [`LoraMetadataTransport`] is one, and callers name the
/// real trait.
pub trait LoraMetadataTransportDyn: Send + Sync {
    fn get<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<WireResponse, ThrownError>> + Send + 'a>>;
}

impl<T: LoraMetadataTransport> LoraMetadataTransportDyn for T {
    fn get<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<WireResponse, ThrownError>> + Send + 'a>>
    {
        Box::pin(LoraMetadataTransport::get(self, url, headers))
    }
}

/// A type-erased [`LoraMetadataTransport`] (the [`ErasedImageDiscovery`]
/// precedent).
///
/// [`ErasedImageDiscovery`]: crate::model::image::ErasedImageDiscovery
#[derive(Clone)]
pub struct ErasedLoraMetadata(std::sync::Arc<dyn LoraMetadataTransportDyn>);

impl ErasedLoraMetadata {
    pub fn new<T: LoraMetadataTransport + 'static>(inner: T) -> Self {
        Self(std::sync::Arc::new(inner))
    }

    /// Ask HuggingFace what it knows about a LoRA source.
    pub async fn lookup(&self, source: &str, token: Option<&str>) -> HuggingFaceLookupResult {
        lookup_huggingface_lora(&*self.0, source, token).await
    }
}

/// v4 `HuggingFaceLookupFailure` — why a lookup produced no facts. The UI owns
/// the wording for each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HuggingFaceLookupFailure {
    NotARepoId,
    MissingOrPrivate,
    NotFound,
    RateLimited,
    Timeout,
    Network,
    Http,
}

impl HuggingFaceLookupFailure {
    /// The literal the wire carries.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotARepoId => "not-a-repo-id",
            Self::MissingOrPrivate => "missing-or-private",
            Self::NotFound => "not-found",
            Self::RateLimited => "rate-limited",
            Self::Timeout => "timeout",
            Self::Network => "network",
            Self::Http => "http",
        }
    }
}

impl Serialize for HuggingFaceLookupFailure {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// v4 `gated: string | false` — `false`, or HuggingFace's gate mode (`auto` /
/// `manual`). Anything but `false` means the weights need a token, which only
/// some provider models have anywhere to put.
#[derive(Clone, Debug, PartialEq)]
pub enum Gated {
    No,
    Mode(String),
}

impl Serialize for Gated {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::No => s.serialize_bool(false),
            Self::Mode(m) => s.serialize_str(m),
        }
    }
}

/// v4 `HuggingFaceLoraFacts` — what a repository declares about itself, as facts
/// and nothing more. Key order is v4's object-literal order in `readFacts`.
#[derive(Clone, Debug, PartialEq)]
pub struct HuggingFaceLoraFacts {
    pub repo_id: String,
    pub url: String,
    pub base_models: Vec<String>,
    pub is_adapter: bool,
    pub is_lora: bool,
    pub pipeline_tag: Option<String>,
    pub gated: Gated,
    pub weight_files: Vec<String>,
    pub trigger_phrase: Option<String>,
    pub downloads: Option<f64>,
    pub likes: Option<f64>,
    pub last_modified: Option<String>,
}

impl Serialize for HuggingFaceLoraFacts {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("HuggingFaceLoraFacts", 12)?;
        st.serialize_field("repoId", &self.repo_id)?;
        st.serialize_field("url", &self.url)?;
        st.serialize_field("baseModels", &self.base_models)?;
        st.serialize_field("isAdapter", &self.is_adapter)?;
        st.serialize_field("isLora", &self.is_lora)?;
        st.serialize_field("pipelineTag", &self.pipeline_tag)?;
        st.serialize_field("gated", &self.gated)?;
        st.serialize_field("weightFiles", &self.weight_files)?;
        st.serialize_field("triggerPhrase", &self.trigger_phrase)?;
        st.serialize_field("downloads", &self.downloads.map(js_num))?;
        st.serialize_field("likes", &self.likes.map(js_num))?;
        st.serialize_field("lastModified", &self.last_modified)?;
        st.end()
    }
}

/// An integral `f64` renders as JS renders it (`1232`, not `1232.0`).
fn js_num(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
        Value::from(f as i64)
    } else {
        serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
    }
}

/// v4 `HuggingFaceLookupResult`.
#[derive(Clone, Debug, PartialEq)]
pub enum HuggingFaceLookupResult {
    Facts(Box<HuggingFaceLoraFacts>),
    Failure {
        reason: HuggingFaceLookupFailure,
        /// The id that was attempted, when one could be made out.
        repo_id: Option<String>,
        /// The card URL, so "go look yourself" stays available even on failure.
        url: Option<String>,
        /// Transport or status detail, for the log and for the curious.
        /// OMITTED when absent — v4's `{ok:false, reason, repoId, url}` object
        /// literal simply has no `detail` key on the not-a-repo-id arm.
        detail: Option<String>,
    },
}

impl Serialize for HuggingFaceLookupResult {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Facts(facts) => {
                let mut st = s.serialize_struct("HuggingFaceLookupResult", 2)?;
                st.serialize_field("ok", &true)?;
                st.serialize_field("facts", facts)?;
                st.end()
            }
            Self::Failure {
                reason,
                repo_id,
                url,
                detail,
            } => {
                let len = if detail.is_some() { 5 } else { 4 };
                let mut st = s.serialize_struct("HuggingFaceLookupResult", len)?;
                st.serialize_field("ok", &false)?;
                st.serialize_field("reason", reason)?;
                st.serialize_field("repoId", repo_id)?;
                st.serialize_field("url", url)?;
                if let Some(d) = detail {
                    st.serialize_field("detail", d)?;
                }
                st.end()
            }
        }
    }
}

/// v4 `readCardBaseModels` — `base_model` may be a string, a list, or absent.
fn read_card_base_models(card_data: Option<&Value>) -> Vec<String> {
    let Some(raw) = card_data.and_then(|c| c.get("base_model")) else {
        return Vec::new();
    };
    if let Some(s) = raw.as_str() {
        let t = crate::jsstr::js_trim(s);
        return if t.is_empty() {
            Vec::new()
        } else {
            vec![t.to_string()]
        };
    }
    if let Some(arr) = raw.as_array() {
        return arr
            .iter()
            .filter_map(Value::as_str)
            .filter(|s| !crate::jsstr::js_trim(s).is_empty())
            .map(|s| crate::jsstr::js_trim(s).to_string())
            .collect();
    }
    Vec::new()
}

/// v4 `readInstancePrompt` — nearly always a string; a list is tolerated.
fn read_instance_prompt(card_data: Option<&Value>) -> Option<String> {
    let raw = card_data.and_then(|c| c.get("instance_prompt"))?;
    if let Some(s) = raw.as_str() {
        let t = crate::jsstr::js_trim(s);
        return (!t.is_empty()).then(|| t.to_string());
    }
    if let Some(arr) = raw.as_array() {
        let first = arr
            .iter()
            .filter_map(Value::as_str)
            .find(|s| !crate::jsstr::js_trim(s).is_empty())?;
        return Some(crate::jsstr::js_trim(first).to_string());
    }
    None
}

/// v4 `readFacts` — turn the API payload into the facts we are willing to stand
/// behind.
fn read_facts(repo_id: &str, payload: &Value) -> HuggingFaceLoraFacts {
    let tags: Vec<&str> = payload
        .get("tags")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let lower_tags: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();
    // `typeof cardData === 'object' && !== null && !Array.isArray(...)`.
    let card_data = payload
        .get("cardData")
        .filter(|c| c.is_object())
        .filter(|c| !c.is_array());

    const ADAPTER_PREFIX: &str = "base_model:adapter:";
    let adapter_targets: Vec<String> = tags
        .iter()
        .filter_map(|t| t.strip_prefix(ADAPTER_PREFIX))
        // `.filter(Boolean)` — an empty remainder is dropped.
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();

    // The card's own declaration first, then the tags — the two usually agree,
    // and when they don't the card is the author's more deliberate statement.
    let mut base_models: Vec<String> = Vec::new();
    for candidate in read_card_base_models(card_data)
        .into_iter()
        .chain(adapter_targets.iter().cloned())
    {
        if !base_models.contains(&candidate) {
            base_models.push(candidate);
        }
    }

    let weight_files: Vec<String> = payload
        .get("siblings")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.get("rfilename").and_then(Value::as_str))
                .filter(|n| n.ends_with(".safetensors"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    // `typeof payload.id === 'string' ? payload.id : repoId` — TWICE in v4, for
    // `repoId` and for the card URL it derives.
    let resolved_id = payload.get("id").and_then(Value::as_str).unwrap_or(repo_id);

    HuggingFaceLoraFacts {
        repo_id: resolved_id.to_string(),
        url: huggingface_card_url(resolved_id),
        base_models,
        is_adapter: !adapter_targets.is_empty(),
        is_lora: lower_tags.iter().any(|t| t == "lora"),
        pipeline_tag: payload
            .get("pipeline_tag")
            .and_then(Value::as_str)
            .map(str::to_string),
        gated: payload
            .get("gated")
            .and_then(Value::as_str)
            .map_or(Gated::No, |g| Gated::Mode(g.to_string())),
        weight_files,
        trigger_phrase: read_instance_prompt(card_data),
        downloads: payload.get("downloads").and_then(Value::as_f64),
        likes: payload.get("likes").and_then(Value::as_f64),
        last_modified: payload
            .get("lastModified")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// Ask HuggingFace what it knows about a LoRA source (v4
/// `lookupHuggingFaceLora`).
///
/// `token` is the profile's `hf_api_token`, when one is configured — it widens
/// the lookup to private and gated repositories, and it is the reason this runs
/// host-side. It is never logged and never returned.
///
/// **The 401 case is reported honestly as "missing or private".** HuggingFace
/// answers an unauthenticated request for a nonexistent repository and one for a
/// private repository identically — both 401, both `"Invalid username or
/// password."` — and deliberately so. Calling that "doesn't exist" would be
/// wrong exactly when it matters most, so the two stay fused until a token is
/// supplied and HuggingFace itself distinguishes them with a 404.
pub async fn lookup_huggingface_lora<T: LoraMetadataTransportDyn + ?Sized>(
    transport: &T,
    source: &str,
    token: Option<&str>,
) -> HuggingFaceLookupResult {
    let Some(repo_id) = extract_huggingface_repo_id(source) else {
        tracing::debug!(
            source_length = crate::jsstr::js_trim(source).len(),
            "[Image LoRA] Source carries no HuggingFace repository id; nothing to query"
        );
        return HuggingFaceLookupResult::Failure {
            reason: HuggingFaceLookupFailure::NotARepoId,
            repo_id: None,
            url: None,
            detail: None,
        };
    };

    let url = huggingface_card_url(&repo_id);
    let mut headers = vec![("Accept".to_string(), "application/json".to_string())];
    // v4 `if (token)` — a JS truthy test, so an EMPTY token attaches no header.
    if let Some(t) = token.filter(|t| !t.is_empty()) {
        headers.push(("Authorization".to_string(), format!("Bearer {t}")));
    }
    let has_token = headers.len() > 1;

    tracing::debug!(
        repo_id = %repo_id,
        has_token,
        "[Image LoRA] Querying HuggingFace for adapter metadata"
    );

    let response = match transport
        .get(&format!("{HUGGINGFACE_API_BASE}/{repo_id}"), &headers)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let timed_out = e.timed_out();
            tracing::warn!(
                repo_id = %repo_id,
                timed_out,
                detail = %e.message,
                "[Image LoRA] HuggingFace lookup could not complete"
            );
            return HuggingFaceLookupResult::Failure {
                reason: if timed_out {
                    HuggingFaceLookupFailure::Timeout
                } else {
                    HuggingFaceLookupFailure::Network
                },
                repo_id: Some(repo_id),
                url: Some(url),
                detail: Some(e.message),
            };
        }
    };

    if !response.ok() {
        // 401 fuses "no such repository" with "private and not yours"; 404 only
        // appears once a token has proved who is asking.
        let reason = match response.status {
            401 | 403 => HuggingFaceLookupFailure::MissingOrPrivate,
            404 => HuggingFaceLookupFailure::NotFound,
            429 => HuggingFaceLookupFailure::RateLimited,
            _ => HuggingFaceLookupFailure::Http,
        };
        let status = response.status;
        tracing::info!(
            repo_id = %repo_id,
            status,
            reason = reason.as_str(),
            has_token,
            "[Image LoRA] HuggingFace declined the lookup"
        );
        return HuggingFaceLookupResult::Failure {
            reason,
            repo_id: Some(repo_id),
            url: Some(url),
            detail: Some(format!("HTTP {status}")),
        };
    }

    let payload: Value = match serde_json::from_str(&response.body) {
        Ok(v) => v,
        Err(e) => {
            // v4's detail is the `SyntaxError` message from `response.json()`,
            // which is V8's wording and not reproducible from Rust. The corpus
            // compares the arm, the reason and the ids; the detail STRING on
            // this one arm is a recorded divergence (see the lane record).
            tracing::warn!(
                repo_id = %repo_id,
                detail = %e,
                "[Image LoRA] HuggingFace answered with something that was not JSON"
            );
            return HuggingFaceLookupResult::Failure {
                reason: HuggingFaceLookupFailure::Http,
                repo_id: Some(repo_id),
                url: Some(url),
                detail: Some(e.to_string()),
            };
        }
    };

    // `typeof payload !== 'object' || payload === null || Array.isArray(payload)`.
    if !payload.is_object() {
        tracing::warn!(
            repo_id = %repo_id,
            "[Image LoRA] HuggingFace answered with an unexpected payload shape"
        );
        return HuggingFaceLookupResult::Failure {
            reason: HuggingFaceLookupFailure::Http,
            repo_id: Some(repo_id),
            url: Some(url),
            detail: Some("Unexpected payload shape".to_string()),
        };
    }

    let facts = read_facts(&repo_id, &payload);
    let weight_file_count = facts.weight_files.len();
    let has_trigger_phrase = facts.trigger_phrase.is_some();
    tracing::debug!(
        repo_id = %facts.repo_id,
        base_models = ?facts.base_models,
        is_lora = facts.is_lora,
        gated = ?facts.gated,
        weight_file_count,
        has_trigger_phrase,
        "[Image LoRA] HuggingFace answered"
    );
    HuggingFaceLookupResult::Facts(Box::new(facts))
}
