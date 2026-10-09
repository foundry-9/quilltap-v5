//! P4.D263 — the host seams behind the wardrobe item pictures (v4
//! `lib/wardrobe/item-image-generation.ts`, `7c8572869`): the editor's
//! Generate (`POST /api/v1/wardrobe/[itemId]/images?action=generate`) and the
//! tool-queued `WARDROBE_ITEM_IMAGE_GENERATION` job take the SAME pair — the
//! image provider and the pixel codec (R-F). Built ONCE per caller here,
//! beside [`crate::images_generate::images_generate_seams`] (whose provider
//! and codec it reuses; the prompt classifier is not needed — this path does
//! no pre-screen).
//!
//! ⚠ 💸 Real money the moment a real provider is configured: one
//! image-generation call per picture (plus one understudy call on a reroute).

use std::sync::Arc;

use quilltap_core::model::image::ErasedImageGenerate;
use quilltap_core::services::wardrobe_item_image_generation::WardrobeItemImageSeams;

use crate::providers::ProviderIo;

/// The live wardrobe item-picture seams for a host running `version`.
pub fn wardrobe_item_image_seams(version: &str) -> WardrobeItemImageSeams {
    let io = Arc::new(ProviderIo::new(version));
    WardrobeItemImageSeams {
        provider: ErasedImageGenerate::new(io.image_provider()),
        // v4 `convertToWebP` (quality 90) — the production sharp-equivalent,
        // and (through `PixelCodecWebp`) the blob normalizer's encoder.
        codec: Arc::new(crate::image_codec::HostImageCodec),
    }
}
