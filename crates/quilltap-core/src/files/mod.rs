//! The chat file/attachment subsystem's pure leaves (W4.4b).
//!
//! Ports v4's `lib/files/*` helpers the chat send path consumes:
//!
//! * [`text_detection`] — v4 `lib/files/text-detection.ts` (content/MIME sniffing;
//!   the upload-half oracle leaf, ported for completeness with its own tier-1
//!   differential).
//! * [`image_processing`] — v4 `lib/files/image-processing.ts` (the base64-size +
//!   resize DECISION logic over an injected [`image_processing::ImageTranscoder`]
//!   seam; no image codec crate in the core).
//! * [`llm_image_budget`] — v4 `lib/files/llm-image-budget.ts` (bug 151,
//!   `bcd7e4852`): what an image costs on the WIRE as opposed to in storage —
//!   the per-image long-edge cap + WebP quality ladder applied at both
//!   attachment loaders, and the per-turn base64 budget the Lantern walk spends.
//! * [`attachment_support`] — v4 `lib/llm/attachment-support.ts`'s client-safe
//!   `PROVIDER_ATTACHMENT_CAPABILITIES` map, feeding
//!   [`crate::services::file_fallback::profile_supports_mime_type`].

pub mod attachment_support;
pub mod image_processing;
pub mod image_transport;
pub mod llm_image_budget;
pub mod text_detection;
