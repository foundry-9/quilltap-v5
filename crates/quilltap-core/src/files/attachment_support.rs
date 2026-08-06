//! v4 `lib/llm/attachment-support.ts`'s **client-safe** provider→MIME capability
//! map (`PROVIDER_ATTACHMENT_CAPABILITIES`), the source of truth
//! `profileSupportsMimeType` consults for **non-image** MIME types.
//!
//! ## Two independent data sources — do not conflate
//!
//! v4 has TWO attachment datasets:
//!   1. The **registry** `getAttachmentSupport(provider)` (in the plugin bundles;
//!      carries `maxBase64Size`) — ported as the v5 manifest
//!      [`crate::provider_manifest::Attachment`]. Consulted ONLY by
//!      [`crate::files::image_processing::get_provider_max_base64_size`].
//!   2. This **client-safe hardcoded** map — UPPERCASE provider keys, no
//!      `maxBase64Size`. Consulted by `supportsMimeType` (→
//!      `profileSupportsMimeType`) for **non-image** types.
//!
//! They differ in coverage (e.g. z-ai has a registry entry but is ABSENT here, so
//! `supportsMimeType("Z_AI", "application/pdf")` is false). Images never consult
//! either map — they are gated solely by the profile's `supportsImageUpload` flag.
//!
//! v4 `supportsMimeType(provider, mimeType)` is `getSupportedMimeTypes(provider)
//! .includes(mimeType)` — an exact, case-sensitive `Array.includes`. An unknown
//! provider (not in this map, incl. `DEEPSEEK`/`Z_AI`) → `[]` → always false.

/// v4 `getSupportedMimeTypes(provider)` — the client-safe non-image capability
/// list for a provider, or `&[]` for an unknown provider (v4's `isKnownProvider`
/// false path). Keys are the UPPERCASE provider enum values (v4 keys the map by
/// them). `baseUrl` is accepted-but-unused in v4, so it is not a parameter here.
pub fn supported_mime_types(provider: &str) -> &'static [&'static str] {
    match provider {
        "OPENAI" => &["image/jpeg", "image/png", "image/gif", "image/webp"],
        "ANTHROPIC" => &[
            "image/jpeg",
            "image/png",
            "image/gif",
            "image/webp",
            "application/pdf",
            "text/plain",
        ],
        "GOOGLE" => &["image/jpeg", "image/png", "image/gif", "image/webp"],
        "GROK" => &["image/jpeg", "image/png", "image/gif", "image/webp"],
        "OLLAMA" => &[],
        // v4 bug 32 (`43a1b5b1`): `PROVIDER_ATTACHMENT_CAPABILITIES.OPENROUTER`
        // now mirrors the plugin's `SUPPORTED_IMAGE_MIME_TYPES` (the four image
        // types) instead of the old "unsupported" `[]`, opening the client
        // vision gate onto the now-working non-streaming vision path (bug 31).
        "OPENROUTER" => &["image/jpeg", "image/png", "image/gif", "image/webp"],
        "OPENAI_COMPATIBLE" => &[],
        // DEEPSEEK / Z_AI and any other provider are absent from
        // PROVIDER_ATTACHMENT_CAPABILITIES → isKnownProvider false → [].
        _ => &[],
    }
}

/// v4 `supportsMimeType(provider, mimeType)` — `getSupportedMimeTypes(provider)
/// .includes(mimeType)` (exact, case-sensitive membership).
pub fn supports_mime_type(provider: &str, mime_type: &str) -> bool {
    supported_mime_types(provider).contains(&mime_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_supports_pdf_and_text_others_do_not() {
        assert!(supports_mime_type("ANTHROPIC", "application/pdf"));
        assert!(supports_mime_type("ANTHROPIC", "text/plain"));
        assert!(!supports_mime_type("OPENAI", "application/pdf"));
        assert!(!supports_mime_type("GOOGLE", "text/plain"));
    }

    #[test]
    fn unknown_and_empty_providers_support_nothing() {
        assert!(!supports_mime_type("DEEPSEEK", "image/jpeg"));
        assert!(!supports_mime_type("Z_AI", "application/pdf"));
        assert!(!supports_mime_type("OLLAMA", "text/plain"));
        assert!(!supports_mime_type("BOGUS", "text/plain"));
    }

    #[test]
    fn image_types_are_listed_but_gated_elsewhere_by_the_profile_flag() {
        // The map DOES list image types for image-capable providers; but
        // `profileSupportsMimeType` short-circuits image/* on the profile flag
        // BEFORE consulting this map, so these entries are only reached for the
        // non-image branch (never with an image/* argument in practice).
        assert!(supports_mime_type("OPENAI", "image/png"));
        assert!(supports_mime_type("GROK", "image/webp"));
    }

    #[test]
    fn openrouter_now_lists_the_four_image_types_but_not_documents() {
        // v4 bug 32 (`43a1b5b1`): OPENROUTER flipped from `[]` to the plugin's
        // four image MIME types. (v4's `llm-attachment-support.test.ts` asserts
        // the same flip.) The DB-visible consequence lives at the settings route
        // — a connection-profile create that omits `supportsImageUpload` now
        // defaults it from `supports_mime_type("OPENROUTER", "image/jpeg")`, so
        // this true is what flips that default false → true.
        for mime in ["image/jpeg", "image/png", "image/gif", "image/webp"] {
            assert!(
                supports_mime_type("OPENROUTER", mime),
                "OPENROUTER should list {mime}"
            );
        }
        // Documents are NOT in OpenRouter's image-only map (the plugin forwards
        // images inline; PDF/text are unsupported on this provider).
        assert!(!supports_mime_type("OPENROUTER", "application/pdf"));
        assert!(!supports_mime_type("OPENROUTER", "text/plain"));
    }
}
