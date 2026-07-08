//! Provider auth injection — the manifest-`auth`-scheme → (headers, url) mapping
//! shared by the non-streaming [`completion_provider`](super::completion_provider)
//! and the streaming [`streaming_provider`](super::streaming_provider) compositions
//! (hoisted out of `completion_provider.rs` in P4.1a so the two paths cannot
//! drift).
//!
//! v4 injects the key per plugin construction (`Authorization: Bearer` on the
//! OpenAI-family SDKs, `x-api-key` on Anthropic's, the `?key=` query param on
//! Google's raw fetch, nothing on Ollama); the manifest `auth` block declares the
//! same scheme, so one injector serves every provider.

use crate::provider_manifest::Registry;

/// Inject the api key per the manifest `auth` scheme onto (headers, url).
///
/// - `bearer` → `Authorization: Bearer <key>` header
/// - `header` → the manifest-named header (e.g. anthropic `x-api-key`)
/// - `query` → the manifest-named query param appended to the url (google)
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
