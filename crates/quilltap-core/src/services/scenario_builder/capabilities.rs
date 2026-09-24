//! The Scenario Builder's capability probe — v4
//! `resolveScenarioBuilderCapabilities(repos, userId)`
//! (`lib/services/scenario-builder/scenario-builder.service.ts`, `d1c06cd9d`):
//! what the builder could reach on the web for this user, independent of any
//! profile — a configured search provider, and a curl plugin with at least one
//! allowed URL pattern.
//!
//! ## `curlConfigured` is ALWAYS `false` on v5 — a RECORDED divergence (§R.4(k))
//!
//! v4 answers `curlConfigured: true` only when `toolRegistry.hasPlugin('curl')`
//! AND `pluginConfigs.findByUserAndPlugin(userId, 'qtap-plugin-curl').config.
//! allowedUrlPatterns` is a non-empty array. v5 builds NO plugin tools (the
//! plugin-only `includePluginTools`/`toolConfigs` are a standing non-port,
//! `services/tool_build.rs`) and has no curl plugin, so there is nothing a
//! `true` could admit: real mode's `['curl']` allowlist admits nothing either.
//! The WARN `Curl capability lookup failed; treating curl as unavailable` has
//! no v5 emitter for the same reason. This is not a stub: it is the honest
//! answer for a host without the plugin — which is also exactly what v4
//! answers when the plugin is not installed (the route family RECORDS v4's
//! value rather than asserting equality, and pins v5's `false` both ways).
//!
//! `webSearchConfigured` is the engine's existing host fact
//! (`QuilltapEngine::web_search_configured` — v4 `isWebSearchConfigured()`,
//! `searchProviderRegistry.isSearchConfigured() || !!SERPER_API_KEY`).

use serde_json::{json, Value};

/// v4 `ScenarioBuilderCapabilities` — `{ webSearchConfigured, curlConfigured }`,
/// in v4's key order.
pub fn resolve_scenario_builder_capabilities(web_search_configured: bool) -> Value {
    // The recorded divergence (module header): no curl plugin exists on v5.
    let curl_configured = false;
    tracing::debug!(
        webSearchConfigured = web_search_configured,
        curlConfigured = curl_configured,
        "Resolved Scenario Builder capabilities"
    );
    json!({
        "webSearchConfigured": web_search_configured,
        "curlConfigured": curl_configured,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_carries_v4s_key_order_and_the_recorded_curl_false() {
        assert_eq!(
            resolve_scenario_builder_capabilities(true).to_string(),
            r#"{"webSearchConfigured":true,"curlConfigured":false}"#
        );
        assert_eq!(
            resolve_scenario_builder_capabilities(false).to_string(),
            r#"{"webSearchConfigured":false,"curlConfigured":false}"#
        );
    }
}
