//! Provider auth injection — the manifest-`auth`-scheme → (headers, url) mapping
//! shared by the non-streaming [`completion_provider`](super::completion_provider)
//! and the streaming [`streaming_provider`](super::streaming_provider) compositions
//! (hoisted out of `completion_provider.rs` in P4.1a so the two paths cannot
//! drift).
//!
//! v4 injects the key per plugin construction (`Authorization: Bearer` on the
//! OpenAI-family SDKs, `x-api-key` on Anthropic's, `x-goog-api-key` on Google's
//! genai SDK, nothing on Ollama); the manifest `auth` block declares the same
//! scheme, so one injector serves every provider.
//!
//! **Google was `query` here until P4.47 (B), and it was wrong.** The comment
//! this replaces said "the `?key=` query param on Google's raw fetch" — but v4
//! builds `new GoogleGenAI({apiKey, httpOptions})` and the SDK sets
//! `X-Goog-Api-Key` on every request, leaving the url untouched (the only
//! `?key=` in the whole plugin is the unused Live/WebSocket path). v5's own
//! image dialect already used the header, so the two v5 paths disagreed with
//! each other as well as with v4. Nothing measured it because the google-wire
//! corpus recorded the outgoing headers and asserted none of them — the D76
//! bank. Now pinned by `request_builder_google_wire_equivalence`, in both
//! directions: v5 must send the header AND must not append the query param.

use crate::provider_manifest::Registry;

/// Inject the api key per the manifest `auth` scheme onto (headers, url).
///
/// - `bearer` → `Authorization: Bearer <key>` header
/// - `header` → the manifest-named header (anthropic `x-api-key`, google
///   `x-goog-api-key`)
/// - `query` → the manifest-named query param appended to the url (no shipped
///   manifest uses it; kept because the schema declares it)
/// - `none` (ollama) and unknown schemes inject nothing
pub(crate) fn apply_auth(
    registry: &Registry,
    provider: &str,
    api_key: &str,
    headers: &mut Vec<(String, String)>,
    url: &mut String,
) {
    let Some(manifest) = registry.get_provider(provider) else {
        return;
    };
    match manifest.auth.kind.as_str() {
        "bearer" => headers.push(("Authorization".to_string(), format!("Bearer {api_key}"))),
        "header" => {
            if let Some(h) = &manifest.auth.header {
                headers.push((h.clone(), api_key.to_string()));
            }
        }
        "query" => {
            if let Some(param) = &manifest.auth.param {
                let sep = if url.contains('?') { '&' } else { '?' };
                url.push(sep);
                url.push_str(param);
                url.push('=');
                url.push_str(api_key);
            }
        }
        // "none" (ollama) and unknown schemes inject nothing.
        _ => {}
    }
}
