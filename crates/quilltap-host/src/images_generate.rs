//! P4.76 — the host seams behind `POST /api/v1/images?action=generate`.
//!
//! v4's `handleGenerateImage` reaches for two things a portable core cannot
//! construct: `createImageProvider(profile.provider)` and the Concierge stack
//! (`classifyContent`'s moderation + completion providers). Both are built here
//! and handed across as ONE
//! [`ImagesGenerateSeams`](quilltap_core::api::images::ImagesGenerateSeams), so
//! a half-wired host cannot generate while silently skipping the Concierge.
//!
//! Why not the spine bundle: this route needs neither the streaming provider,
//! the tool runner nor the cost/pricing bundle — only the plain completion +
//! moderation wires and the image dialect, which are exactly what
//! [`ProviderIo`] hands out. That is the same reasoning that keeps
//! `image_discovery` and `lora_metadata` out of the bundle (`host.rs`'s P4.6ai
//! block), and it means the arm stays LIVE on any assembly that reaches this
//! constructor rather than only on a spine-bearing one.
//!
//! ⚠ 💸 Both seams cost real money the moment a real provider is configured:
//! one image-generation call per request, plus one cheap-LLM classification
//! whenever the Concierge is armed (`mode != 'OFF'` and `scanImagePrompts`).

use std::sync::Arc;

use quilltap_core::api::images::{
    ErasedImagePromptClassifier, ImagePromptClassifier, ImagesGenerateSeams,
};
use quilltap_core::cheap_llm::CheapLlmSelection;
use quilltap_core::db::chat_settings::DangerousContentSettings;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::image::ErasedImageGenerate;
use quilltap_core::services::dangerous_content::gatekeeper::{
    classify_content, DangerClassificationResult,
};
use quilltap_core::services::dangerous_content::provider_routing::DbApiKeys;

use crate::providers::ProviderIo;
use crate::spine::{DbProviderKeys, WireCompletionProvider};

/// The route's Concierge classification, over freshly-built wires.
///
/// Rebuilt per call for the same reason `HostImageGenerationRunner` rebuilds
/// its `ImageGenDeps`: the moderation provider closes over the request's `Db`,
/// and a `reqwest` client is cheap (it pools per client).
struct HostImagePromptClassifier {
    io: Arc<ProviderIo>,
}

impl ImagePromptClassifier for HostImagePromptClassifier {
    fn classify<'a>(
        &'a self,
        db: &'a Db,
        content: &'a str,
        selection: &'a CheapLlmSelection,
        user_id: &'a str,
        settings: &'a DangerousContentSettings,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DangerClassificationResult> + Send + 'a>>
    {
        Box::pin(async move {
            let completion = WireCompletionProvider::new(
                DbProviderKeys(db.clone()),
                self.io.policy(),
                self.io.user_agent().to_string(),
                self.io.base_url_env().map(String::from),
            )
            // P4.71: the same container-gateway rewrite every other completion
            // construction site applies.
            .with_localhost_gateway(self.io.localhost_gateway());
            let moderation = self
                .io
                .moderation_provider(db.clone(), DbApiKeys(db.clone()));
            classify_content(
                db,
                &moderation,
                &completion,
                content,
                selection,
                user_id,
                settings,
                // v4's route has no chat — `classifyContent(prompt, selection,
                // user.id, dangerSettings)`, four arguments, no fifth.
                None,
            )
            .await
        })
    }
}

/// The live `?action=generate` seams for a host running `version`.
pub fn images_generate_seams(version: &str) -> ImagesGenerateSeams {
    let io = Arc::new(ProviderIo::new(version));
    ImagesGenerateSeams {
        provider: ErasedImageGenerate::new(io.image_provider()),
        classifier: ErasedImagePromptClassifier::new(HostImagePromptClassifier { io }),
        // v4 `convertToWebP` (quality 90) — the production sharp-equivalent.
        codec: Arc::new(crate::image_codec::HostImageCodec),
    }
}
