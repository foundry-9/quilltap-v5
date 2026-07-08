//! Dangerous-content provider routing (v4
//! `lib/services/dangerous-content/provider-routing.service.ts`) — reroutes
//! content flagged dangerous to an uncensored-compatible provider. If no
//! uncensored provider is available, returns the original (never blocks).
//!
//! This module is the REAL implementor of the
//! [`crate::services::provider_failover::DangerousContentRouter`] seam consumed
//! by the (already verified) empty-response failover. The connection-profile
//! resolution logic is ported here; the API-key material stays host-side (an
//! injected [`ApiKeyResolver`] seam, mirroring the
//! [`crate::services::cheap_llm_exec`] precedent — the differential feeds canned
//! keys keyed by `apiKeyId`, so the resolution *choice* is what is verified).

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::DbError;
use crate::db::{connection_profiles, image_profiles};
use crate::services::primary_stream::EffectiveProfile;
use crate::services::provider_failover::{DangerSettings, DangerousContentRouter, RouteResult};

/// The profile-identity subset the routing decision, reason strings, and the
/// downstream failover all consume (a byte-diffable projection of v4's returned
/// `ConnectionProfile` / `ImageProfile`). Names are used only in reason strings.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteProfile {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model_name: String,
    pub base_url: Option<String>,
}

/// v4 `DangerousProviderRouteResult` (text).
#[derive(Clone, Debug, PartialEq)]
pub struct DangerousProviderRouteResult {
    pub rerouted: bool,
    pub connection_profile: RouteProfile,
    pub api_key: String,
    pub reason: String,
}

/// v4 `DangerousImageProviderRouteResult`.
#[derive(Clone, Debug, PartialEq)]
pub struct DangerousImageProviderRouteResult {
    pub rerouted: bool,
    pub image_profile: RouteProfile,
    pub api_key: String,
    pub reason: String,
}

/// v4 `PostHocImageReroute` — the resolution payload for a post-hoc image reroute.
#[derive(Clone, Debug, PartialEq)]
pub struct PostHocImageReroute {
    pub profile: RouteProfile,
    pub api_key: String,
}

/// The API-key resolution seam (v4 `repos.connections.findApiKeyByIdAndUserId` /
/// the image equivalent + `decryptProfileApiKey`). Given an `apiKeyId` + user,
/// return the decrypted key, or `None` when no key exists / can't be loaded.
/// Key management + decryption is host-side (Phase-4 transport).
pub trait ApiKeyResolver {
    fn resolve(&self, api_key_id: &str, user_id: &str) -> Option<String>;
}

/// An [`ApiKeyResolver`] that never resolves a key — the faithful wiring when
/// key material is unavailable (every reroute then fails open to the original).
pub struct NoApiKeys;
impl ApiKeyResolver for NoApiKeys {
    fn resolve(&self, _api_key_id: &str, _user_id: &str) -> Option<String> {
        None
    }
}

/// The REAL [`ApiKeyResolver`] (W4.7d) — reads the plaintext key from the
/// `api_keys` table (v4 `repos.connections.findApiKeyByIdAndUserId(id, userId)`),
/// closing the seam wherever a read connection is in hand. `resolve` mirrors v4's
/// `apiKey?.key_value ?? null`: a missing / non-owned key → `None`. The spine
/// composition points swap `NoApiKeys` for this (that wiring is W4.4b, per the
/// W4.7d order); the resolver itself lives here so the routing logic and the read
/// stay one unit.
pub struct ConnApiKeys<'c> {
    conn: &'c rusqlite::Connection,
}

impl<'c> ConnApiKeys<'c> {
    pub fn new(conn: &'c rusqlite::Connection) -> Self {
        Self { conn }
    }
}

impl ApiKeyResolver for ConnApiKeys<'_> {
    fn resolve(&self, api_key_id: &str, user_id: &str) -> Option<String> {
        crate::db::api_keys::find_by_id_and_user_id(self.conn, api_key_id, user_id)
            .ok()
            .flatten()
            .map(|k| k.key_value)
    }
}

/// The owned-[`Db`] form of [`ConnApiKeys`] — the same real resolution
/// (`find_by_id_and_user_id`) but reading off the read pool via a held [`Db`]
/// handle rather than a borrowed connection. The [`DangerContentRouter`] STORES
/// the resolver, so it cannot hold the borrowed connection the router opens for
/// each resolve; this owned form (opening its own pooled read) closes the
/// spine-composition seam (W4.10a). Additive: `ConnApiKeys` stays for callers
/// that already hold a connection.
pub struct DbApiKeys(pub Db);
impl ApiKeyResolver for DbApiKeys {
    fn resolve(&self, api_key_id: &str, user_id: &str) -> Option<String> {
        let api_key_id = api_key_id.to_string();
        let user_id = user_id.to_string();
        self.0
            .read_main(move |conn| {
                Ok(crate::db::api_keys::find_by_id_and_user_id(
                    conn,
                    &api_key_id,
                    &user_id,
                )?)
            })
            .ok()
            .flatten()
            .map(|k| k.key_value)
    }
}

fn route_profile_from_value(v: &Value) -> RouteProfile {
    RouteProfile {
        id: str_field(v, "id"),
        name: str_field(v, "name"),
        provider: str_field(v, "provider"),
        model_name: str_field(v, "modelName"),
        base_url: v.get("baseUrl").and_then(Value::as_str).map(str::to_string),
    }
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn user_id_of(v: &Value) -> Option<&str> {
    v.get("userId").and_then(Value::as_str)
}

fn is_dangerous_compatible(v: &Value) -> bool {
    v.get("isDangerousCompatible").and_then(Value::as_bool) == Some(true)
}

/// v4 `decryptProfileApiKey` / `decryptImageProfileApiKey`: `None` when the
/// profile carries no `apiKeyId`, else the seam's decrypted key.
fn decrypt_profile_api_key<A: ApiKeyResolver>(
    api_keys: &A,
    profile: &Value,
    user_id: &str,
) -> Option<String> {
    let api_key_id = profile.get("apiKeyId").and_then(Value::as_str)?;
    if api_key_id.is_empty() {
        return None;
    }
    api_keys.resolve(api_key_id, user_id)
}

/// v4 `resolveProviderForDangerousContent`. Reads connection profiles from
/// `conn` and resolves an uncensored text provider (or returns the original with
/// `rerouted: false`). Never throws (v4 catches → the "Routing failed" result).
///
/// `mode` / `uncensored_text_profile_id` are the two settings fields the text
/// resolution consumes.
pub fn resolve_provider_for_dangerous_content<A: ApiKeyResolver>(
    conn: &rusqlite::Connection,
    api_keys: &A,
    original_profile: &RouteProfile,
    original_api_key: &str,
    mode: &str,
    uncensored_text_profile_id: Option<&str>,
    user_id: &str,
) -> DangerousProviderRouteResult {
    // If mode is not AUTO_ROUTE, don't reroute.
    if mode != "AUTO_ROUTE" {
        return DangerousProviderRouteResult {
            rerouted: false,
            connection_profile: original_profile.clone(),
            api_key: original_api_key.to_string(),
            reason: format!("Mode is {mode}, no rerouting"),
        };
    }

    let attempt = || -> Result<DangerousProviderRouteResult, DbError> {
        // Try explicit uncensored profile first.
        if let Some(uncensored_id) = uncensored_text_profile_id.filter(|s| !s.is_empty()) {
            if let Some(profile) = connection_profiles::find_by_id(conn, uncensored_id)? {
                if user_id_of(&profile) == Some(user_id) {
                    if let Some(api_key) = decrypt_profile_api_key(api_keys, &profile, user_id) {
                        let rp = route_profile_from_value(&profile);
                        return Ok(DangerousProviderRouteResult {
                            rerouted: true,
                            reason: format!(
                                "Rerouted to configured uncensored profile: {}",
                                rp.name
                            ),
                            connection_profile: rp,
                            api_key,
                        });
                    }
                    // Configured profile has no valid API key — fall through.
                }
                // else: not owned by user — fall through.
            }
        }

        // Scan for any isDangerousCompatible profile.
        for profile in connection_profiles::find_all(conn)? {
            if user_id_of(&profile) == Some(user_id) && is_dangerous_compatible(&profile) {
                if let Some(api_key) = decrypt_profile_api_key(api_keys, &profile, user_id) {
                    let rp = route_profile_from_value(&profile);
                    return Ok(DangerousProviderRouteResult {
                        rerouted: true,
                        reason: format!("Rerouted to uncensored-compatible profile: {}", rp.name),
                        connection_profile: rp,
                        api_key,
                    });
                }
            }
        }

        // No uncensored provider available — send to original anyway.
        Ok(DangerousProviderRouteResult {
            rerouted: false,
            connection_profile: original_profile.clone(),
            api_key: original_api_key.to_string(),
            reason: "No uncensored provider available - sending to regular provider".to_string(),
        })
    };

    attempt().unwrap_or_else(|e| DangerousProviderRouteResult {
        rerouted: false,
        connection_profile: original_profile.clone(),
        api_key: original_api_key.to_string(),
        reason: format!("Routing failed: {e}"),
    })
}

/// v4 `resolveImageProviderForDangerousContent`. Image analogue of
/// [`resolve_provider_for_dangerous_content`].
pub fn resolve_image_provider_for_dangerous_content<A: ApiKeyResolver>(
    conn: &rusqlite::Connection,
    api_keys: &A,
    original_profile: &RouteProfile,
    original_api_key: &str,
    mode: &str,
    uncensored_image_profile_id: Option<&str>,
    user_id: &str,
) -> DangerousImageProviderRouteResult {
    if mode != "AUTO_ROUTE" {
        return DangerousImageProviderRouteResult {
            rerouted: false,
            image_profile: original_profile.clone(),
            api_key: original_api_key.to_string(),
            reason: format!("Mode is {mode}, no rerouting"),
        };
    }

    let attempt = || -> Result<DangerousImageProviderRouteResult, DbError> {
        if let Some(uncensored_id) = uncensored_image_profile_id.filter(|s| !s.is_empty()) {
            if let Some(profile) = image_profiles::find_by_id(conn, uncensored_id)? {
                if user_id_of(&profile) == Some(user_id) {
                    if let Some(api_key) = decrypt_profile_api_key(api_keys, &profile, user_id) {
                        let rp = route_profile_from_value(&profile);
                        return Ok(DangerousImageProviderRouteResult {
                            rerouted: true,
                            reason: format!(
                                "Rerouted to configured uncensored image profile: {}",
                                rp.name
                            ),
                            image_profile: rp,
                            api_key,
                        });
                    }
                }
            }
        }

        for profile in image_profiles::find_all(conn)? {
            if user_id_of(&profile) == Some(user_id) && is_dangerous_compatible(&profile) {
                if let Some(api_key) = decrypt_profile_api_key(api_keys, &profile, user_id) {
                    let rp = route_profile_from_value(&profile);
                    return Ok(DangerousImageProviderRouteResult {
                        rerouted: true,
                        reason: format!(
                            "Rerouted to uncensored-compatible image profile: {}",
                            rp.name
                        ),
                        image_profile: rp,
                        api_key,
                    });
                }
            }
        }

        Ok(DangerousImageProviderRouteResult {
            rerouted: false,
            image_profile: original_profile.clone(),
            api_key: original_api_key.to_string(),
            reason: "No uncensored image provider available - sending to regular provider"
                .to_string(),
        })
    };

    attempt().unwrap_or_else(|e| DangerousImageProviderRouteResult {
        rerouted: false,
        image_profile: original_profile.clone(),
        api_key: original_api_key.to_string(),
        reason: format!("Routing failed: {e}"),
    })
}

/// v4 `resolveUncensoredImageProfileForReroute`: the post-hoc image reroute after
/// a provider rejects an already-issued request for moderation reasons. Unlike
/// [`resolve_image_provider_for_dangerous_content`], this does NOT scan for any
/// `isDangerousCompatible` profile — it is keyed on the user's explicit
/// uncensored choice only. Returns `None` when no reroute is possible.
pub fn resolve_uncensored_image_profile_for_reroute<A: ApiKeyResolver>(
    conn: &rusqlite::Connection,
    api_keys: &A,
    current_profile_id: &str,
    mode: &str,
    uncensored_image_profile_id: Option<&str>,
    user_id: &str,
) -> Result<Option<PostHocImageReroute>, DbError> {
    if mode != "AUTO_ROUTE" {
        return Ok(None);
    }
    let Some(uncensored_id) = uncensored_image_profile_id.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if uncensored_id == current_profile_id {
        return Ok(None);
    }

    let Some(profile) = image_profiles::find_by_id(conn, uncensored_id)? else {
        return Ok(None);
    };
    if user_id_of(&profile) != Some(user_id) {
        return Ok(None);
    }
    let Some(api_key) = decrypt_profile_api_key(api_keys, &profile, user_id) else {
        return Ok(None);
    };

    Ok(Some(PostHocImageReroute {
        profile: route_profile_from_value(&profile),
        api_key,
    }))
}

/// v4 `isImageModerationError`: detect a post-hoc content-moderation rejection
/// from an image provider by keyword-matching the (lowercased) error message.
pub fn is_image_moderation_error(message: &str) -> bool {
    let message = message.to_lowercase();
    message.contains("content moderation")
        || message.contains("content_policy")
        || message.contains("content policy")
        || message.contains("safety system")
        || message.contains("rejected by content")
        || message.contains("moderation_blocked")
}

/// The real [`DangerousContentRouter`] implementor. Holds the [`Db`] handle
/// (to read connection profiles off the read pool) and the [`ApiKeyResolver`]
/// seam. Constructed at the spine composition point (a unification handoff).
pub struct DangerContentRouter<A: ApiKeyResolver> {
    db: Db,
    api_keys: A,
}

impl<A: ApiKeyResolver> DangerContentRouter<A> {
    pub fn new(db: Db, api_keys: A) -> Self {
        Self { db, api_keys }
    }
}

impl<A: ApiKeyResolver + Send + Sync> DangerousContentRouter for DangerContentRouter<A> {
    async fn resolve(
        &self,
        original_profile: &EffectiveProfile,
        original_api_key: &str,
        settings: &DangerSettings,
        user_id: &str,
    ) -> RouteResult {
        let original = RouteProfile {
            id: original_profile.id.clone(),
            // The failover doesn't consume the profile name; only reason strings
            // do, and those aren't surfaced through the trait.
            name: String::new(),
            provider: original_profile.provider.clone(),
            model_name: original_profile.model_name.clone(),
            base_url: original_profile.base_url.clone(),
        };
        let result = self
            .db
            .read_main(|conn| {
                Ok(resolve_provider_for_dangerous_content(
                    conn,
                    &self.api_keys,
                    &original,
                    original_api_key,
                    &settings.mode,
                    settings.uncensored_text_profile_id.as_deref(),
                    user_id,
                ))
            })
            .unwrap_or_else(|_| DangerousProviderRouteResult {
                rerouted: false,
                connection_profile: original.clone(),
                api_key: original_api_key.to_string(),
                reason: "Routing failed".to_string(),
            });

        RouteResult {
            rerouted: result.rerouted,
            connection_profile: EffectiveProfile {
                id: result.connection_profile.id,
                provider: result.connection_profile.provider,
                model_name: result.connection_profile.model_name,
                base_url: result.connection_profile.base_url,
            },
            api_key: result.api_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_moderation_error_matches_common_shapes() {
        assert!(is_image_moderation_error(
            "Your request was rejected as a result of our safety system."
        ));
        assert!(is_image_moderation_error(
            "Generated image rejected by content moderation."
        ));
        assert!(is_image_moderation_error("content_policy violation"));
        assert!(!is_image_moderation_error("network timeout"));
    }
}
