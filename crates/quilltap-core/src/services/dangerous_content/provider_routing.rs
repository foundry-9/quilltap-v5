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
    /// The chosen profile's raw row. v4's result carries the whole
    /// `ConnectionProfile`; v5 projects the identity into [`RouteProfile`] for
    /// the routing comparands and keeps the row here for the callers that need
    /// more of it — since v4 `a1d88aa3a` (bug 106) the empty-response reroute
    /// re-runs the attachment decision against it. `None` on the arms that
    /// return the ORIGINAL profile (no reroute happened, so there is no new row
    /// to decide against).
    pub profile_row: Option<Value>,
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
                crate::db::api_keys::find_by_id_and_user_id(conn, &api_key_id, &user_id)
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

/// Whether a profile can take every attachment this turn is carrying.
///
/// v4 `profileCanCarryTurn` (`a1d88aa3a`, bug 106). A reroute swaps the model
/// but inherits the message array the *original* profile's call was built
/// against, bytes and all. A substitute that cannot receive those bytes is not
/// a slightly worse choice, it is a guaranteed 400 from the gateway. An empty
/// list means the turn carries nothing and every profile qualifies (JS
/// `[].every(…)` is `true`, and so is Rust's `all`).
fn profile_can_carry_turn(profile: &Value, mime_types: &[String]) -> bool {
    let view = crate::files::image_transport::AttachmentProfileView::from_json(profile);
    mime_types
        .iter()
        .all(|m| crate::files::image_transport::profile_can_receive_attachment(view, m))
}

/// v4 `resolveProviderForDangerousContent`. Reads connection profiles from
/// `conn` and resolves an uncensored text provider (or returns the original with
/// `rerouted: false`). Never throws (v4 catches → the "Routing failed" result).
///
/// `mode` / `uncensored_text_profile_id` are the two settings fields the text
/// resolution consumes.
///
/// `turn_attachment_mime_types` are the MIME types riding in this turn's
/// message array, if any (v4 `a1d88aa3a`, bug 106). The scan *prefers* a
/// substitute that can receive them; without it the scan answers a question the
/// payload has already settled. Note this is a **preference, not a filter**: an
/// explicitly configured uncensored profile is still honoured whatever it can
/// read, and a text-only stand-in is still better than no reroute at all —
/// the caller re-runs the attachment decision against whichever profile comes
/// back (`adapt_messages_for_profile`), so an image becomes a description
/// rather than a 400.
// v4's parameter list, one for one — it grew to eight at `a1d88aa3a`.
#[allow(clippy::too_many_arguments)]
pub fn resolve_provider_for_dangerous_content<A: ApiKeyResolver>(
    conn: &rusqlite::Connection,
    api_keys: &A,
    original_profile: &RouteProfile,
    original_api_key: &str,
    mode: &str,
    uncensored_text_profile_id: Option<&str>,
    user_id: &str,
    turn_attachment_mime_types: &[String],
) -> DangerousProviderRouteResult {
    // If mode is not AUTO_ROUTE, don't reroute.
    if mode != "AUTO_ROUTE" {
        return DangerousProviderRouteResult {
            rerouted: false,
            connection_profile: original_profile.clone(),
            api_key: original_api_key.to_string(),
            reason: format!("Mode is {mode}, no rerouting"),
            profile_row: None,
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
                            profile_row: Some(profile.clone()),
                        });
                    }
                    // Configured profile has no valid API key — fall through.
                }
                // else: not owned by user — fall through.
            }
        }

        // Scan for any isDangerousCompatible profile.
        //
        // Ordered, not filtered (v4 `a1d88aa3a`, bug 106): profiles that can
        // carry this turn's attachments come first, and the rest follow behind
        // them. Filtering outright would trade a degraded-but-delivered turn
        // for no reroute at all when the only uncensored route on the instance
        // happens to be text-only.
        let eligible: Vec<Value> = connection_profiles::find_all(conn)?
            .into_iter()
            .filter(|p| user_id_of(p) == Some(user_id) && is_dangerous_compatible(p))
            .collect();
        let (can_carry, cannot_carry): (Vec<Value>, Vec<Value>) = eligible
            .into_iter()
            .partition(|p| profile_can_carry_turn(p, turn_attachment_mime_types));

        if !turn_attachment_mime_types.is_empty() && !cannot_carry.is_empty() {
            let names = |v: &[Value]| -> Vec<String> {
                v.iter()
                    .map(|p| {
                        p.get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string()
                    })
                    .collect()
            };
            tracing::info!(
                turn_attachment_mime_types = ?turn_attachment_mime_types,
                can_carry = ?names(&can_carry),
                cannot_carry = ?names(&cannot_carry),
                "[DangerousContent] Deprioritising uncensored candidates that cannot carry this turn"
            );
        }

        for profile in can_carry.into_iter().chain(cannot_carry) {
            if let Some(api_key) = decrypt_profile_api_key(api_keys, &profile, user_id) {
                let rp = route_profile_from_value(&profile);
                return Ok(DangerousProviderRouteResult {
                    rerouted: true,
                    reason: format!("Rerouted to uncensored-compatible profile: {}", rp.name),
                    connection_profile: rp,
                    api_key,
                    profile_row: Some(profile.clone()),
                });
            }
        }

        // No uncensored provider available — send to original anyway.
        Ok(DangerousProviderRouteResult {
            rerouted: false,
            connection_profile: original_profile.clone(),
            api_key: original_api_key.to_string(),
            reason: "No uncensored provider available - sending to regular provider".to_string(),
            profile_row: None,
        })
    };

    attempt().unwrap_or_else(|e| DangerousProviderRouteResult {
        rerouted: false,
        connection_profile: original_profile.clone(),
        api_key: original_api_key.to_string(),
        reason: format!("Routing failed: {e}"),
        profile_row: None,
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
        turn_attachment_mime_types: &[String],
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
                    turn_attachment_mime_types,
                ))
            })
            .unwrap_or_else(|_| DangerousProviderRouteResult {
                rerouted: false,
                connection_profile: original.clone(),
                api_key: original_api_key.to_string(),
                reason: "Routing failed".to_string(),
                profile_row: None,
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
            profile_row: result.profile_row,
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
