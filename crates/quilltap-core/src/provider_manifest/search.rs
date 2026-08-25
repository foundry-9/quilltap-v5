//! The native SEARCH-provider manifest + registry (P4.59).
//!
//! v4 ships exactly one search provider — the `qtap-plugin-search-serper` dist
//! plugin, `enabledByDefault: true` — registered into the singleton
//! `search-provider-registry` at boot by `lib/startup/plugin-initialization.ts`.
//! Everything the host reads off that registry is *data*: the plugin's
//! `metadata` (`providerName` / `displayName` / `description` / `abbreviation` /
//! `colors`) and its `config` (`requiresApiKey` / `apiKeyLabel` /
//! `requiresBaseUrl`). So the search half takes exactly the shape the LLM half
//! took in W4.7a — a generated declarative manifest plus a compiled
//! implementation ([`crate::tools::web_search`]) — and no plugin runtime is
//! introduced (the standing tier-3 deferral).
//!
//! **Registration is gated exactly as v4 gates it.** v4's manifest loader drops
//! a plugin whose name fails `isSitePluginEnabled` (`lib/plugins/site-plugins.ts`
//! — the `SITE_PLUGINS_ENABLED` / `SITE_PLUGINS_DISABLED` deployment env vars)
//! BEFORE the enabled-by-default flag is even read, so an operator who writes
//! `SITE_PLUGINS_DISABLED=qtap-plugin-search-serper` gets an instance with no
//! search provider at all. That predicate is ported here as
//! [`is_site_plugin_enabled`] — pure, with both env values injected, because the
//! core reads no environment; the host resolves them once and threads the
//! resulting registration into BOTH the providers listing and the search runner,
//! so advertised and executed can never disagree (the P4.42 invariant).
//!
//! ⚠ **Recorded divergence.** v5's TEN LLM providers are native and are NOT
//! gated by `SITE_PLUGINS_*` — they have no plugin name to gate on, and the
//! loader that consults the gate is part of the un-ported plugin runtime. So
//! `SITE_PLUGINS_ENABLED=qtap-plugin-anthropic` narrows v4 to one LLM provider
//! and narrows v5 not at all; only the Serper arm of the gate is faithful. The
//! arm that IS faithful is the one an operator can actually exercise today
//! (disabling the bundled search plugin), and it is the arm this port needs so
//! that registration has a single answer.

use serde::Deserialize;
use std::sync::LazyLock;

use super::{Colors, ManifestError, SUPPORTED_SCHEMA_VERSION};

// ============================================================================
// The manifest
// ============================================================================

/// A search provider's declarative manifest — v4's `SearchProviderPlugin`
/// `metadata` + `config`, plus the two manifest.json facts registration needs
/// (`pluginName` for the site-plugins gate, `enabledByDefault`).
///
/// Deliberately NOT the LLM [`Manifest`](super::Manifest): v4's search plugin
/// declares no capability bag, no endpoints, no pricing and no models, and the
/// `/api/v1/providers` search row is a hand-built three-key
/// `configRequirements` — modelling it as an LLM manifest would invent fields v4
/// does not have.
#[derive(Clone, Debug, Deserialize)]
pub struct SearchManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    /// `metadata.providerName` — the `api_keys.provider` value too (`SERPER`).
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub description: String,
    pub abbreviation: String,
    pub colors: Colors,
    #[serde(rename = "configRequirements")]
    pub config_requirements: SearchConfigRequirements,
    /// The v4 `manifest.json` `name` — the key `isSitePluginEnabled` is asked
    /// about (`qtap-plugin-search-serper`).
    #[serde(rename = "pluginName")]
    pub plugin_name: String,
    /// The v4 `manifest.json` `enabledByDefault`.
    #[serde(rename = "enabledByDefault")]
    pub enabled_by_default: bool,
}

/// The search plugin's `config` — v4's `SearchProviderConfigRequirements`. THREE
/// keys, and the providers route hand-builds them in exactly this order.
#[derive(Clone, Debug, Deserialize)]
pub struct SearchConfigRequirements {
    #[serde(rename = "requiresApiKey")]
    pub requires_api_key: bool,
    #[serde(rename = "requiresBaseUrl")]
    pub requires_base_url: bool,
    #[serde(default, rename = "apiKeyLabel")]
    pub api_key_label: Option<String>,
}

impl SearchManifest {
    /// Load + validate from JSON (the [`super::Manifest::from_json`] contract:
    /// deserialization IS the schema check, `schemaVersion` gated explicitly).
    pub fn from_json(json: &str) -> Result<SearchManifest, ManifestError> {
        let manifest: SearchManifest =
            serde_json::from_str(json).map_err(|e| ManifestError::Invalid(e.to_string()))?;
        if manifest.schema_version != SUPPORTED_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedSchemaVersion(
                manifest.schema_version,
            ));
        }
        Ok(manifest)
    }
}

// ============================================================================
// The built-in search manifests
// ============================================================================

/// The one built-in search manifest, generated by
/// `harness/oracle/providers/gen-provider-manifests.mjs` from v4's real dist
/// plugin (transcription, not re-derivation).
const BUILT_IN_SEARCH_MANIFEST_JSON: &[&str] = &[include_str!("manifests/search_serper.json")];

static BUILT_IN_SEARCH_REGISTRY: LazyLock<SearchRegistry> = LazyLock::new(|| {
    let providers: Vec<SearchManifest> = BUILT_IN_SEARCH_MANIFEST_JSON
        .iter()
        .map(|json| SearchManifest::from_json(json).expect("built-in search manifest is valid"))
        .collect();
    SearchRegistry { providers }
});

/// The search-provider registry — v4's `searchProviderRegistry`, minus the
/// hot-load/HMR machinery (static data + accessors, the [`super::Registry`]
/// precedent).
#[derive(Clone, Debug)]
pub struct SearchRegistry {
    providers: Vec<SearchManifest>,
}

impl SearchRegistry {
    /// Every search provider v4 bundles, in registration order.
    pub fn built_in() -> &'static SearchRegistry {
        &BUILT_IN_SEARCH_REGISTRY
    }

    /// `getAllProviders()`.
    pub fn all_providers(&self) -> &[SearchManifest] {
        &self.providers
    }

    /// `getDefaultProvider()` — the FIRST registered provider, or `None`.
    pub fn default_provider(&self) -> Option<&SearchManifest> {
        self.providers.first()
    }

    /// `hasProvider(name)` — exact-case lookup (the `api_keys.provider` string).
    pub fn get_provider(&self, name: &str) -> Option<&SearchManifest> {
        self.providers.iter().find(|p| p.id == name)
    }

    /// The providers that a boot with these `SITE_PLUGINS_*` values would
    /// register: `enabledByDefault` AND site-enabled, exactly as v4's manifest
    /// loader + `getEnabledByCapability('SEARCH_PROVIDER')` decide it.
    pub fn registered(
        &self,
        enabled_env: Option<&str>,
        disabled_env: Option<&str>,
    ) -> Vec<&SearchManifest> {
        self.providers
            .iter()
            .filter(|p| {
                p.enabled_by_default
                    && is_site_plugin_enabled(&p.plugin_name, enabled_env, disabled_env)
            })
            .collect()
    }
}

// ============================================================================
// The site-plugins gate (v4 `lib/plugins/site-plugins.ts`, ported whole)
// ============================================================================

/// v4 `parsePluginList` — `undefined`/blank → `[]`; the literal (case-insensitive)
/// `all` → [`PluginList::All`]; otherwise the comma-split, trimmed, empties
/// dropped.
#[derive(Debug, PartialEq, Eq)]
enum PluginList {
    All,
    Names(Vec<String>),
}

fn parse_plugin_list(env_value: Option<&str>) -> PluginList {
    let Some(value) = env_value else {
        return PluginList::Names(Vec::new());
    };
    // v4 `!envValue` is falsy for the EMPTY string too, and `.trim() === ''`
    // catches whitespace-only.
    if value.is_empty() || value.trim().is_empty() {
        return PluginList::Names(Vec::new());
    }
    let trimmed = value.trim();
    // `trimmed.toLowerCase() === 'all'`.
    if trimmed.to_lowercase() == "all" {
        return PluginList::All;
    }
    PluginList::Names(
        trimmed
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// v4 `getSitePluginsEnabled` — the parsed list, except that an EMPTY result
/// becomes the `'all'` default. Note v4 tests `result.length === 0`, which is
/// `false` for the string `'all'` (length 3), so `'all'` passes through.
fn site_plugins_enabled(enabled_env: Option<&str>) -> PluginList {
    match parse_plugin_list(enabled_env) {
        PluginList::Names(names) if names.is_empty() => PluginList::All,
        other => other,
    }
}

/// v4 `getSitePluginsDisabled` — the parsed list, with `'all'` (nonsensical for
/// a disabled list) folded to empty.
fn site_plugins_disabled(disabled_env: Option<&str>) -> Vec<String> {
    match parse_plugin_list(disabled_env) {
        PluginList::All => Vec::new(),
        PluginList::Names(names) => names,
    }
}

/// v4 `isSitePluginEnabled(pluginName)`: not in the disabled list, AND in the
/// enabled list (or the enabled list is `'all'`). The env values are injected —
/// the core reads no environment.
pub fn is_site_plugin_enabled(
    plugin_name: &str,
    enabled_env: Option<&str>,
    disabled_env: Option<&str>,
) -> bool {
    let enabled = site_plugins_enabled(enabled_env);
    let disabled = site_plugins_disabled(disabled_env);
    if disabled.iter().any(|d| d == plugin_name) {
        return false;
    }
    match enabled {
        PluginList::All => true,
        PluginList::Names(names) => names.iter().any(|n| n == plugin_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_search_manifest_is_serper() {
        let reg = SearchRegistry::built_in();
        let p = reg.default_provider().expect("serper registered");
        assert_eq!(p.id, "SERPER");
        assert_eq!(p.display_name, "Serper Web Search");
        assert_eq!(
            p.description,
            "Google search results via the Serper.dev API"
        );
        assert_eq!(p.abbreviation, "SRP");
        assert_eq!(p.colors.bg, "bg-orange-100");
        assert!(p.config_requirements.requires_api_key);
        assert!(!p.config_requirements.requires_base_url);
        assert_eq!(
            p.config_requirements.api_key_label.as_deref(),
            Some("Serper API Key")
        );
        assert_eq!(p.plugin_name, "qtap-plugin-search-serper");
        assert!(p.enabled_by_default);
        assert!(reg.get_provider("SERPER").is_some());
        assert!(reg.get_provider("serper").is_none());
    }

    #[test]
    fn registration_honours_the_site_plugins_gate() {
        let reg = SearchRegistry::built_in();
        assert_eq!(reg.registered(None, None).len(), 1);
        assert_eq!(
            reg.registered(None, Some("qtap-plugin-search-serper"))
                .len(),
            0
        );
        assert_eq!(reg.registered(Some("qtap-plugin-anthropic"), None).len(), 0);
        assert_eq!(
            reg.registered(
                Some("qtap-plugin-anthropic,qtap-plugin-search-serper"),
                None
            )
            .len(),
            1
        );
    }
}
