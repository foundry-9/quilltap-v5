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
use crate::services::primary_stream::EffectiveProfile;
use crate::services::provider_failover::{
    DangerSettings, DangerousContentRouter, RouteResult, TextUnderstudy,
};

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

/// The API-key resolution seam (v4 `repos.connections.findApiKeyByIdAndUserId` /
/// the image equivalent + `decryptProfileApiKey`). Given an `apiKeyId` + user,
/// return the decrypted key, or `None` when no key exists / can't be loaded.
/// Key management + decryption is host-side (Phase-4 transport).
pub trait ApiKeyResolver {
    fn resolve(&self, api_key_id: &str, user_id: &str) -> Option<String>;

    /// The lookup with its failure kept (P4.D225): v4's understudy
    /// `decryptKey` catches a throwing lookup and WARNs `Could not decrypt an
    /// understudy candidate's API key` before answering null — a `resolve`
    /// that folds the error into `None` cannot say so. The default is a
    /// resolver with no failure mode.
    fn try_resolve(&self, api_key_id: &str, user_id: &str) -> Result<Option<String>, String> {
        Ok(self.resolve(api_key_id, user_id))
    }
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
        self.try_resolve(api_key_id, user_id).ok().flatten()
    }

    fn try_resolve(&self, api_key_id: &str, user_id: &str) -> Result<Option<String>, String> {
        crate::db::api_keys::find_by_id_and_user_id(self.conn, api_key_id, user_id)
            .map(|k| k.map(|k| k.key_value))
            .map_err(|e| e.to_string())
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
        self.try_resolve(api_key_id, user_id).ok().flatten()
    }

    fn try_resolve(&self, api_key_id: &str, user_id: &str) -> Result<Option<String>, String> {
        let api_key_id = api_key_id.to_string();
        let user_id = user_id.to_string();
        self.0
            .read_main(move |conn| {
                crate::db::api_keys::find_by_id_and_user_id(conn, &api_key_id, &user_id)
            })
            .map(|k| k.map(|k| k.key_value))
            .map_err(|e| e.to_string())
    }
}

pub(crate) fn route_profile_from_value(v: &Value) -> RouteProfile {
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

/// v4 `resolveProviderForDangerousContent` — the pre-flight TEXT wrapper, thin
/// since `8bd080267` (#73): the `AUTO_ROUTE` gate lives here ("the policy lives
/// here, in the wrapper; the resolver never reads the mode"), and the choice is
/// [`resolve_uncensored_text_understudy`] with the ORIGINAL profile excluded.
/// One merged INFO line (`configured` says which of v4's two old lines it would
/// have been); the unchanged WARN when nothing is available.
///
/// v4's `catch` (`Provider routing failed, using original` + `Routing failed:
/// …`) is unreachable through v4's real code — the understudy swallows its own
/// lookup failures — and the understudy's `Option` makes it unrepresentable
/// here; recorded, not ported.
///
/// `turn_attachment_mime_types` (bug 106) is a PREFERENCE the understudy
/// applies to the scan; the caller re-runs the attachment decision against
/// whichever profile comes back.
// v4's parameter list, one for one.
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
    if mode != "AUTO_ROUTE" {
        return DangerousProviderRouteResult {
            rerouted: false,
            connection_profile: original_profile.clone(),
            api_key: original_api_key.to_string(),
            reason: format!("Mode is {mode}, no rerouting"),
            profile_row: None,
        };
    }

    let exclude = [original_profile.id.clone()];
    let understudy = super::understudy::resolve_uncensored_text_understudy(
        conn,
        api_keys,
        super::understudy::TextUnderstudyLookup {
            user_id,
            uncensored_text_profile_id,
            exclude: &exclude,
            turn_attachment_mime_types,
            filter: None,
        },
    );

    if let Some(u) = understudy {
        let configured = Some(u.profile.id.as_str()) == uncensored_text_profile_id;
        tracing::info!(
            target: "quilltap::dangerous_content_routing",
            profile_id = %u.profile.id,
            profile_name = %u.profile.name,
            provider = %u.profile.provider,
            model = %u.profile.model_name,
            configured,
            "[DangerousContent] Rerouting to uncensored text profile"
        );
        let reason = if configured {
            format!(
                "Rerouted to configured uncensored profile: {}",
                u.profile.name
            )
        } else {
            format!(
                "Rerouted to uncensored-compatible profile: {}",
                u.profile.name
            )
        };
        return DangerousProviderRouteResult {
            rerouted: true,
            connection_profile: u.profile,
            api_key: u.api_key,
            reason,
            profile_row: Some(u.row),
        };
    }

    tracing::warn!(
        target: "quilltap::dangerous_content_routing",
        original_profile = %original_profile.name,
        original_provider = %original_profile.provider,
        "[DangerousContent] No uncensored provider available, sending to original profile"
    );
    DangerousProviderRouteResult {
        rerouted: false,
        connection_profile: original_profile.clone(),
        api_key: original_api_key.to_string(),
        reason: "No uncensored provider available - sending to regular provider".to_string(),
        profile_row: None,
    }
}

/// v4 `resolveImageProviderForDangerousContent` — the pre-flight IMAGE wrapper
/// (`8bd080267`): the `AUTO_ROUTE` gate, then
/// [`resolve_uncensored_image_understudy`] with the original excluded — "same
/// order as the post-hoc failover, because both ask" it.
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

    let exclude = [original_profile.id.clone()];
    let understudy = super::understudy::resolve_uncensored_image_understudy(
        conn,
        api_keys,
        super::understudy::ImageUnderstudyLookup {
            user_id,
            uncensored_image_profile_id,
            exclude: &exclude,
        },
    );

    if let Some(u) = understudy {
        let configured = Some(u.profile.id.as_str()) == uncensored_image_profile_id;
        tracing::info!(
            target: "quilltap::dangerous_content_routing",
            profile_id = %u.profile.id,
            profile_name = %u.profile.name,
            provider = %u.profile.provider,
            configured,
            "[DangerousContent] Rerouting to uncensored image profile"
        );
        let reason = if configured {
            format!(
                "Rerouted to configured uncensored image profile: {}",
                u.profile.name
            )
        } else {
            format!(
                "Rerouted to uncensored-compatible image profile: {}",
                u.profile.name
            )
        };
        return DangerousImageProviderRouteResult {
            rerouted: true,
            image_profile: u.profile,
            api_key: u.api_key,
            reason,
        };
    }

    tracing::warn!(
        target: "quilltap::dangerous_content_routing",
        original_profile = %original_profile.name,
        "[DangerousContent] No uncensored image provider available, sending to original"
    );
    DangerousImageProviderRouteResult {
        rerouted: false,
        image_profile: original_profile.clone(),
        api_key: original_api_key.to_string(),
        reason: "No uncensored image provider available - sending to regular provider".to_string(),
    }
}

// v4 `resolveUncensoredImageProfileForReroute` + `isImageModerationError` —
// RETIRED by v4 at `8bd080267` (#73) and DELETED here with their last callers
// (P4.D225 unit 8c): the image failover chokepoint
// (`super::image_failover`) asks `resolve_uncensored_image_understudy` and
// detects a refusal with the ONE classifier (`super::refusal::classify_refusal`)
// instead of this keyword list.

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
            // The route trail names every seat it records (P4.D173), and the
            // Concierge's uncensored profile is recorded from THIS result — so
            // the name has to make the round trip rather than be blanked here.
            name: original_profile.name.clone(),
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
                name: result.connection_profile.name,
                provider: result.connection_profile.provider,
                model_name: result.connection_profile.model_name,
                base_url: result.connection_profile.base_url,
            },
            api_key: result.api_key,
            profile_row: result.profile_row,
        }
    }

    async fn resolve_understudy(
        &self,
        user_id: &str,
        settings: &DangerSettings,
        exclude: &[String],
        turn_attachment_mime_types: &[String],
    ) -> Option<TextUnderstudy> {
        // The resolver swallows its own lookup failures (v4's catch); a read
        // pool that cannot even hand out a connection is the same "nobody".
        let found = self
            .db
            .read_main(|conn| {
                Ok(super::understudy::resolve_uncensored_text_understudy(
                    conn,
                    &self.api_keys,
                    super::understudy::TextUnderstudyLookup {
                        user_id,
                        uncensored_text_profile_id: settings.uncensored_text_profile_id.as_deref(),
                        exclude,
                        turn_attachment_mime_types,
                        filter: None,
                    },
                ))
            })
            .ok()
            .flatten()?;
        Some(TextUnderstudy {
            connection_profile: EffectiveProfile {
                id: found.profile.id,
                name: found.profile.name,
                provider: found.profile.provider,
                model_name: found.profile.model_name,
                base_url: found.profile.base_url,
            },
            api_key: found.api_key,
            profile_row: Some(found.row),
        })
    }

    async fn record_refusal(&self, rec: super::refusal_ledger::RefusalRecord) {
        super::refusal_ledger::record_moderation_refusal(&self.db, &rec).await;
    }

    async fn announce_refusal(
        &self,
        chat_id: &str,
        kind: crate::services::concierge_notifications::ConciergeRefusalKind,
        details: crate::services::concierge_notifications::ConciergeRefusalDetails,
    ) {
        crate::services::concierge_notifications::post_concierge_refusal_announcement(
            &self.db, chat_id, kind, &details,
        )
        .await;
    }
}
