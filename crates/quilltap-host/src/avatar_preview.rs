//! The host avatar-preview renderer (P4.6bf; P4.D283 R-G) — the production
//! implementation of the core's [`AvatarPreviewRenderer`] seam
//! (`quilltap_core::api::wardrobe`). Since v4 `9ecebf986` the core handler
//! (`wardrobe_preview_avatar`) threads the whole pipeline — parse + guards +
//! `resolveAvatarImageProfile` + prompt build + the shared logged attempt
//! inside the Concierge's image failover + `decodeProviderImage` + persist —
//! and this renderer is ONLY the two host capabilities under it: ONE raw
//! `provider.generateImage(params, apiKey)` per attempt, and the pixel codec
//! (`HostImageCodec`, v4's `sharp`) the decode's `convertToWebP` runs over.
//!
//! **Wiring this makes the wardrobe dialog's out-of-chat Preview button cost
//! real money** — an image-provider generation per click (two on a Concierge
//! reroute).
//!
//! The provider call rides the same W4.7f [`RealImageProvider`] machinery the
//! `imageProfileGenerate` runner uses (`HostImageGenerationRunner`): the
//! renderer rebuilds `RealImageProvider::with_bytes_fetch(…)` per call,
//! exactly as that runner rebuilds its deps per run.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use quilltap_core::api::wardrobe::AvatarPreviewRenderer;
use quilltap_core::model::image::{ImageGenError, ImageGenParams, ImageGenResponse, ImageProvider};
use quilltap_core::model::image_dialects::RealImageProvider;
use quilltap_core::services::file_storage::PixelCodec;

use crate::image_codec::HostImageCodec;
use crate::wire::{ReqwestImageBytes, ReqwestWireTransport};

/// The production avatar-preview renderer. Stateless; share via `Arc`.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostAvatarPreviewRenderer;

impl AvatarPreviewRenderer for HostAvatarPreviewRenderer {
    fn generate<'a>(
        &'a self,
        provider: &'a str,
        api_key: &'a str,
        params: &'a ImageGenParams,
    ) -> Pin<Box<dyn Future<Output = Result<ImageGenResponse, ImageGenError>> + Send + 'a>> {
        Box::pin(async move {
            // Rebuild the provider per call (the HostImageGenerationRunner
            // idiom — no shared client state to carry).
            let real = RealImageProvider::with_bytes_fetch(
                ReqwestWireTransport::new(),
                ReqwestImageBytes::new(),
            );
            real.generate_image(provider, api_key, params).await
        })
    }

    fn codec(&self) -> Arc<dyn PixelCodec> {
        Arc::new(HostImageCodec)
    }
}
