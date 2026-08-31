//! Port of v4's `lib/llm/image-transport.ts` (a14a1811, bug 91) — can this
//! provider's plugin actually put image bytes on the wire?
//!
//! This is a *different* question from "is this model vision-capable", and
//! conflating the two is what bug 91 was. A connection profile's
//! `supportsImageUpload` flag answers the model question: the operator ticks it
//! because `deepseek-v4-flash-vision-exp` really does read pictures. It says
//! nothing about whether the plugin routing to that model serialises an
//! `image_url` part — and three of v4's don't (NanoGPT pre-1.1.0, DeepSeek and
//! OpenAI-Compatible all inherit a base that marks every attachment failed).
//!
//! When the two disagree, the old code took the profile's word for it: the
//! describe-fallback was suppressed *and* the plugin dropped the bytes, so the
//! model received nothing at all and nothing said so. Both halves have to
//! agree before raw bytes are worth sending.
//!
//! ## The registry-vs-static collapse in v5 (measured, P4.D106)
//!
//! v4 resolves in three tiers: the live plugin registry when
//! `isProviderRegistryInitialized()` (production), else the client-safe static
//! map (startup, tests, the job child before it boots plugins), and `true` for
//! a provider neither source knows. v5's provider manifests are **baked** —
//! there is no uninitialized-registry state — so the resolution collapses to:
//! the manifest tier for any provider the manifest set knows, the static map
//! only for a name it lacks (a third-party provider), and `true` when both
//! sources are ignorant. Both tiers are real code with real differentials
//! (`image_transport_equivalence` drives v4 in BOTH configurations).
//!
//! ## The OpenRouter registry/static disagreement — CONVERGED (P4.D111)
//!
//! P4.D106 measured a v4-side contradiction and reproduced it faithfully:
//! OpenRouter's plugin registry entry declared `supportsAttachments: false`
//! while the static map listed its four image types, so v4 production
//! (registry up) answered **false** for OPENROUTER and routed its vision
//! profiles to the describe-fallback — in the same breath as a guard sentence
//! recommending OpenRouter. Filed upstream as v4 bug 97 (`7a6716b5`) and
//! FIXED at v4 `0ba942b1`: plugin 1.0.59 declares `supportsAttachments: true`
//! and imports its MIME list from `provider.ts`'s exported
//! `SUPPORTED_IMAGE_MIME_TYPES`, so declaration and wire cannot drift again.
//! v5's regenerated manifest carries the flip, both tiers now agree, and the
//! former both-directions pin is a plain equality.

use crate::files::attachment_support::static_provider_can_transport_images;
use crate::provider_manifest::Registry;

/// True when the provider's plugin can serialise image attachments into its
/// request payload.
///
/// Consults the manifest registry first — a provider declaring
/// `attachmentSupport.supportsAttachments` with at least one `image/*` MIME
/// type can transport images. Falls back to the client-safe static mirror for
/// a provider the manifest set lacks, and to `true` for a provider neither
/// source knows, so a third-party vision plugin isn't crippled by our
/// ignorance of it.
///
/// v4 `providerCanTransportImages(provider)` — the lookup uppercases the name
/// exactly as v4's `getAttachmentSupport(provider.toUpperCase())` does.
pub fn provider_can_transport_images(provider: &str) -> bool {
    if let Some(support) = Registry::built_in().attachment_support(&provider.to_uppercase()) {
        return support.supports_attachments
            && support
                .supported_mime_types
                .iter()
                .any(|t| t.starts_with("image/"));
    }
    static_provider_can_transport_images(provider)
}

/// The connection-profile fields the attachment predicates read — v4's
/// structural parameter `{ provider, supportsImageUpload?, baseUrl? }`.
///
/// A view rather than a `&Value` because the two call sites hold different
/// shapes: the file subsystem and the router carry the raw profile row, while
/// the fallback chain carries a parsed
/// [`crate::llm_fallback::FallbackProfile`]. `baseUrl` is in v4's parameter
/// type but never read (`supportsMimeType` accepts it and ignores it — see
/// [`crate::files::attachment_support`]), so it is absent here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttachmentProfileView<'a> {
    pub provider: &'a str,
    /// v4 `profile.supportsImageUpload` — the operator's per-profile tick.
    /// `None` is the absent/null column, which is NOT `Some(true)`.
    pub supports_image_upload: Option<bool>,
}

impl<'a> AttachmentProfileView<'a> {
    /// The view over a raw connection-profile row.
    pub fn from_json(profile: &'a serde_json::Value) -> Self {
        Self {
            provider: profile
                .get("provider")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
            supports_image_upload: profile
                .get("supportsImageUpload")
                .and_then(serde_json::Value::as_bool),
        }
    }
}

/// Can this profile actually *receive* an attachment of this MIME type?
///
/// v4 `profileCanReceiveAttachment` (`a1d88aa3a`, bug 106) — the single
/// predicate behind every "should we send the bytes, or describe them?"
/// decision. Two questions, and both have to answer yes:
///
///  1. **Does the model read this?** —
///     [`crate::files::attachment_support::profile_supports_mime_type`], which
///     for images is the operator's per-profile `supportsImageUpload` tick and
///     for everything else is the provider's capability map.
///  2. **Can the plugin put it on the wire?** —
///     [`provider_can_transport_images`], for images only. See the module note
///     above for why these are different questions (bug 91).
///
/// Before this commit the question had three independent spellings — the
/// router asked it not at all, the describe-fallback and the fallback chain
/// asked it differently — and that drift is what produced v4's bugs 91, 97 and
/// 104. Callers who ask the *negative* ("does this need the describe-fallback?")
/// want [`crate::services::file_fallback::needs_fallback_processing`], which
/// delegates here and logs the disagreement case.
pub fn profile_can_receive_attachment(profile: AttachmentProfileView<'_>, mime_type: &str) -> bool {
    if !crate::files::attachment_support::profile_supports_mime_type(profile, mime_type) {
        return false;
    }
    if mime_type.starts_with("image/") && !provider_can_transport_images(profile.provider) {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_tier_answers_for_the_built_ins() {
        // The baked-manifest truths. NANOGPT joined the transporting set when
        // P4.D107's manifest regen (plugin 1.1.0) landed at the a14a1811-round
        // unification — the `image_transport_equivalence` gate constant flipped
        // with it.
        // OPENROUTER joined the transporting set when P4.D111's manifest regen
        // (plugin 1.0.59, v4 bug 97) landed — the declaration finally matches
        // the `image_url` parts the provider has serialised since bug 45.
        for p in [
            "OPENAI",
            "ANTHROPIC",
            "GOOGLE",
            "GROK",
            "Z_AI",
            "NANOGPT",
            "OPENROUTER",
        ] {
            assert!(provider_can_transport_images(p), "{p} transports");
        }
        for p in ["OLLAMA", "DEEPSEEK", "OPENAI_COMPATIBLE"] {
            assert!(!provider_can_transport_images(p), "{p} cannot transport");
        }
    }

    #[test]
    fn lookup_uppercases_like_v4() {
        assert!(provider_can_transport_images("z_ai"));
        assert!(!provider_can_transport_images("deepseek"));
    }

    #[test]
    fn unknown_provider_falls_to_static_then_true() {
        assert!(provider_can_transport_images("SOME_THIRD_PARTY_VISION"));
        assert!(provider_can_transport_images(""));
    }
}
