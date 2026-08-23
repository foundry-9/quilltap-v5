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
//! ⚠ Known v4-side inconsistency, reproduced faithfully: v4's OpenRouter
//! plugin registry entry declares `supportsAttachments: false` while the
//! static map lists its four image types — so v4 production (registry up)
//! answers **false** for OPENROUTER and routes its vision profiles to the
//! describe-fallback, even though the OpenRouter non-streaming vision path
//! works and the describer guard sentence names OpenRouter as transporting.
//! v5's baked manifest carries the same `false`, so both sides agree; the
//! registry/static disagreement is a v4 bug to file upstream (P4.D106 lane
//! record).

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_tier_answers_for_the_built_ins() {
        // The baked-manifest truths at this lane's pin. NANOGPT is asserted in
        // `image_transport_equivalence` behind the P4.D107 gate (the manifest
        // regen from plugin 1.1.0 is that lane's deliverable), not here.
        for p in ["OPENAI", "ANTHROPIC", "GOOGLE", "GROK", "Z_AI"] {
            assert!(provider_can_transport_images(p), "{p} transports");
        }
        for p in ["OLLAMA", "DEEPSEEK", "OPENAI_COMPATIBLE"] {
            assert!(!provider_can_transport_images(p), "{p} cannot transport");
        }
        // OPENROUTER: the manifest declares supportsAttachments false (the v4
        // registry/static inconsistency, reproduced faithfully — see the
        // module header).
        assert!(!provider_can_transport_images("OPENROUTER"));
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
