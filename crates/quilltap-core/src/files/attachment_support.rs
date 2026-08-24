//! v4 `lib/llm/attachment-support.ts`'s **client-safe** provider→MIME capability
//! map (`PROVIDER_ATTACHMENT_CAPABILITIES`), the source of truth
//! `profileSupportsMimeType` consults for **non-image** MIME types — and, since
//! v4 `a14a1811` (bug 91), the static tier of the image-transport predicate.
//!
//! ## Two independent data sources — do not conflate
//!
//! v4 has TWO attachment datasets:
//!   1. The **registry** `getAttachmentSupport(provider)` (in the plugin bundles;
//!      carries `maxBase64Size`) — ported as the v5 manifest
//!      [`crate::provider_manifest::Attachment`]. Consulted by
//!      [`crate::files::image_processing::get_provider_max_base64_size`] and by
//!      the registry tier of
//!      [`crate::files::image_transport::provider_can_transport_images`].
//!   2. This **client-safe hardcoded** map — UPPERCASE provider keys, no
//!      `maxBase64Size`. Consulted by `supportsMimeType` (→
//!      `profileSupportsMimeType`) for **non-image** types, and by
//!      [`static_provider_can_transport_images`] (the predicate's static tier /
//!      v4's uninitialized-registry fallback).
//!
//! They differ in coverage (this map knows non-image types the registry has no
//! opinion on, and the registry carries `maxBase64Size` this map lacks), and
//! they CAN differ in answers — when they do, v4 production has the registry up
//! and answers the registry, so a stale plugin declaration silently wins over a
//! correct map. That is exactly what v4 bug 97 was (OPENROUTER: the map here was
//! right throughout, the plugin's `supportsAttachments: false` was stale, and
//! the registry tier won) — fixed upstream at `0ba942b1` and converged here by
//! P4.D111; v4's `__tests__/unit/lib/llm/image-transport.test.ts` now holds the
//! two sources together. Whether a profile's images are RAW-SENT is gated by the
//! profile's `supportsImageUpload` flag AND (since bug 91) the transport
//! predicate.
//!
//! v4 `supportsMimeType(provider, mimeType)` is `getSupportedMimeTypes(provider)
//! .includes(mimeType)` — an exact, case-sensitive `Array.includes`. An unknown
//! provider (not in this map) → `[]` → always false.

/// The map rows: `Some(types)` for a provider v4's
/// `PROVIDER_ATTACHMENT_CAPABILITIES` lists (v4 `isKnownProvider` true), `None`
/// for an unknown provider. The distinction is load-bearing for
/// [`static_provider_can_transport_images`]: a KNOWN provider with no image
/// types cannot transport; an UNKNOWN provider answers `true`.
fn known_provider_types(provider: &str) -> Option<&'static [&'static str]> {
    match provider {
        "OPENAI" => Some(&["image/jpeg", "image/png", "image/gif", "image/webp"]),
        "ANTHROPIC" => Some(&[
            "image/jpeg",
            "image/png",
            "image/gif",
            "image/webp",
            "application/pdf",
            "text/plain",
        ]),
        "GOOGLE" => Some(&["image/jpeg", "image/png", "image/gif", "image/webp"]),
        "GROK" => Some(&["image/jpeg", "image/png", "image/gif", "image/webp"]),
        "OLLAMA" => Some(&[]),
        // v4 bug 32 (`43a1b5b1`): `PROVIDER_ATTACHMENT_CAPABILITIES.OPENROUTER`
        // now mirrors the plugin's `SUPPORTED_IMAGE_MIME_TYPES` (the four image
        // types) instead of the old "unsupported" `[]`, opening the client
        // vision gate onto the now-working non-streaming vision path (bug 31).
        "OPENROUTER" => Some(&["image/jpeg", "image/png", "image/gif", "image/webp"]),
        // a14a1811: v4's map comment — the shared base class marks every
        // attachment failed; no `image_url` part is ever emitted.
        "OPENAI_COMPATIBLE" => Some(&[]),
        // a14a1811 (bug 91): NANOGPT serialises image_url as of plugin 1.1.0
        // (before that it inherited the OpenAI-compatible base's "not yet
        // implemented" handling and dropped images silently).
        "NANOGPT" => Some(&["image/jpeg", "image/png", "image/gif", "image/webp"]),
        // a14a1811: DEEPSEEK — same inherited base, same drop; the direct API
        // is text-only in Quilltap.
        "DEEPSEEK" => Some(&[]),
        // a14a1811: Z_AI serialises image_url for vision models (glm-4.6v,
        // glm-5v-turbo); 5MB and 6000x6000 per image.
        "Z_AI" => Some(&["image/jpeg", "image/png", "image/gif", "image/webp"]),
        // Any other provider is absent from PROVIDER_ATTACHMENT_CAPABILITIES →
        // isKnownProvider false.
        _ => None,
    }
}

/// v4 `getSupportedMimeTypes(provider)` — the client-safe non-image capability
/// list for a provider, or `&[]` for an unknown provider (v4's `isKnownProvider`
/// false path). Keys are the UPPERCASE provider enum values (v4 keys the map by
/// them). `baseUrl` is accepted-but-unused in v4, so it is not a parameter here.
pub fn supported_mime_types(provider: &str) -> &'static [&'static str] {
    known_provider_types(provider).unwrap_or(&[])
}

/// v4 `supportsMimeType(provider, mimeType)` — `getSupportedMimeTypes(provider)
/// .includes(mimeType)` (exact, case-sensitive membership).
pub fn supports_mime_type(provider: &str, mime_type: &str) -> bool {
    supported_mime_types(provider).contains(&mime_type)
}

/// v4 `staticProviderCanTransportImages(provider)` (a14a1811, bug 91) — the
/// client-safe answer to "can this provider's plugin put image bytes on the
/// wire?", the static mirror behind
/// [`crate::files::image_transport::provider_can_transport_images`], which
/// prefers the live plugin registry (in v5: the baked manifests).
///
/// A provider this map has never heard of returns `true`: a third-party vision
/// plugin shouldn't be crippled because our table predates it. The providers
/// that genuinely cannot transport images are listed explicitly above, which is
/// the case bug 91 needed and the case this map now covers.
///
/// Note it keys off the `types` list (`capabilities.types.some(t =>
/// t.startsWith('image/'))`), **not** a `supportsAttachments` bool — this map
/// has no such field.
pub fn static_provider_can_transport_images(provider: &str) -> bool {
    let key = provider.to_uppercase();
    match known_provider_types(&key) {
        None => true,
        Some(types) => types.iter().any(|t| t.starts_with("image/")),
    }
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
        // DEEPSEEK is now a KNOWN provider with an empty list (a14a1811);
        // BOGUS stays unknown — both answer false here (`[]` either way).
        assert!(!supports_mime_type("DEEPSEEK", "image/jpeg"));
        assert!(!supports_mime_type("Z_AI", "application/pdf"));
        assert!(!supports_mime_type("OLLAMA", "text/plain"));
        assert!(!supports_mime_type("BOGUS", "text/plain"));
    }

    #[test]
    fn image_types_are_listed_but_raw_send_is_gated_elsewhere() {
        // The map DOES list image types for image-capable providers; the raw
        // image send is gated by the profile's `supportsImageUpload` flag AND
        // (since a14a1811) the transport predicate — `profileSupportsMimeType`
        // short-circuits image/* on the profile flag BEFORE consulting this
        // map, so these entries are only reached for the non-image branch and
        // for `static_provider_can_transport_images`.
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

    #[test]
    fn a14a1811_rows_list_image_types_for_nanogpt_and_z_ai_only() {
        // The §C1 values: NANOGPT and Z_AI gain the four image types (their
        // plugins serialise image_url); DEEPSEEK is known-empty (the inherited
        // base drops attachments). The same map drives the settings route's
        // `supportsImageUpload` default, so these rows flip that default for
        // NANOGPT/Z_AI profile creates exactly as v4's do.
        for mime in ["image/jpeg", "image/png", "image/gif", "image/webp"] {
            assert!(supports_mime_type("NANOGPT", mime));
            assert!(supports_mime_type("Z_AI", mime));
        }
        assert_eq!(supported_mime_types("DEEPSEEK"), &[] as &[&str]);
    }

    #[test]
    fn static_transport_keys_off_the_types_list() {
        // Transporting: an image/* entry in the list.
        for p in [
            "OPENAI",
            "ANTHROPIC",
            "GOOGLE",
            "GROK",
            "OPENROUTER",
            "NANOGPT",
            "Z_AI",
        ] {
            assert!(static_provider_can_transport_images(p), "{p} transports");
        }
        // Known but empty-listed: cannot transport.
        for p in ["OLLAMA", "OPENAI_COMPATIBLE", "DEEPSEEK"] {
            assert!(
                !static_provider_can_transport_images(p),
                "{p} cannot transport"
            );
        }
        // ANTHROPIC's list carries non-image types too — `.some(image/*)`
        // still finds the image entries (the predicate is not `all`).
        assert!(static_provider_can_transport_images("ANTHROPIC"));
    }

    #[test]
    fn static_transport_uppercases_and_defaults_unknown_to_true() {
        assert!(static_provider_can_transport_images("nanogpt"));
        assert!(!static_provider_can_transport_images("deepseek"));
        // Unknown → true: a third-party vision plugin isn't crippled by our
        // ignorance of it.
        assert!(static_provider_can_transport_images("SOME_THIRD_PARTY"));
        assert!(static_provider_can_transport_images(""));
    }
}
