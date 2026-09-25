//! The uncensored understudies — the ONE order the Concierge uses to find an
//! uncensored stand-in for a text or an image call (port of v4
//! `lib/services/dangerous-content/understudy.ts`, NEW at `8bd080267`, #73;
//! unchanged by `49059fb14`).
//!
//! The order, for both kinds: the configured uncensored profile first (when it
//! is not excluded, is the user's own, and — for text — is eligible), then any
//! `isDangerousCompatible` profile in `findAll()` order. The caller's own ids
//! are EXCLUDED, so a refused profile is never asked twice; text candidates on
//! the Courier transport are skipped (a clipboard hand-off cannot stand in for
//! a refused call); a caller's `filter` applies to the explicit pick as well as
//! to the scan.
//!
//! **These resolvers never read the mode.** The policy lives with the callers:
//! the two pre-flight wrappers in [`super::provider_routing`] keep v4's
//! `AUTO_ROUTE` gate, the image failover chokepoint and the text failover gate
//! their own. A lookup that fails is swallowed (logged, `None`) — v4's catch.
//!
//! Deltas from v5's pre-#73 inline order (`provider_routing.rs`, recorded for
//! the review): the original profile is now EXCLUDED (the old "rerouted to the
//! same id" outcome is gone); couriers are skipped; an excluded / courier /
//! keyless / filtered explicit pick logs and then scans; the deprioritising
//! line moved here WITHOUT its `[DangerousContent]` prefix; a keyless or
//! not-owned explicit IMAGE pick now WARNs (the old image code fell through
//! silently).

use serde_json::Value;

use super::provider_routing::{route_profile_from_value, ApiKeyResolver, RouteProfile};
use crate::db::{connection_profiles, image_profiles};

/// A resolved understudy: the chosen profile's identity, its raw row (callers
/// re-run the attachment decision against it — v4's result carries the whole
/// profile), and its decrypted key.
#[derive(Clone, Debug, PartialEq)]
pub struct Understudy {
    pub profile: RouteProfile,
    pub row: Value,
    pub api_key: String,
}

/// v4 `TextUnderstudyLookup`. `uncensored_text_profile_id` is the one settings
/// field the text resolver reads (v4 hands it the whole settings object).
#[derive(Clone, Copy)]
pub struct TextUnderstudyLookup<'a> {
    pub user_id: &'a str,
    pub uncensored_text_profile_id: Option<&'a str>,
    /// Ids that must not be chosen on this call — the refused profile, and
    /// whatever the caller has already tried.
    pub exclude: &'a [String],
    /// MIME types riding in this turn's message array (bug 106): a preference,
    /// not a filter — carriers are tried first.
    pub turn_attachment_mime_types: &'a [String],
    /// A caller's own eligibility test (the legacy image dialog restricts the
    /// connection-profile understudy to providers that can generate images).
    pub filter: Option<&'a dyn Fn(&Value) -> bool>,
}

/// v4 `UnderstudyLookup` for images.
#[derive(Clone, Copy)]
pub struct ImageUnderstudyLookup<'a> {
    pub user_id: &'a str,
    pub uncensored_image_profile_id: Option<&'a str>,
    pub exclude: &'a [String],
}

fn str_of<'v>(v: &'v Value, key: &str) -> &'v str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn user_id_of(v: &Value) -> Option<&str> {
    v.get("userId").and_then(Value::as_str)
}

fn is_dangerous_compatible(v: &Value) -> bool {
    v.get("isDangerousCompatible").and_then(Value::as_bool) == Some(true)
}

fn is_courier(v: &Value) -> bool {
    v.get("transport").and_then(Value::as_str) == Some("courier")
}

/// v4 `decryptKey`: no `apiKeyId` → `None`; the key's `key_value` when truthy;
/// a lookup that throws WARNs and answers `None`.
fn decrypt_key<A: ApiKeyResolver>(api_keys: &A, profile: &Value, user_id: &str) -> Option<String> {
    let api_key_id = profile
        .get("apiKeyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?;
    match api_keys.try_resolve(api_key_id, user_id) {
        Ok(key) => key.filter(|k| !k.is_empty()),
        Err(error) => {
            tracing::warn!(
                target: "quilltap::concierge_understudy",
                profile_id = %str_of(profile, "id"),
                error = %error,
                "Could not decrypt an understudy candidate's API key"
            );
            None
        }
    }
}

/// v4 `profileCanCarryTurn` (bug 106): every MIME type this turn carries can
/// be received by the profile (`[].every` is `true`).
fn profile_can_carry_turn(profile: &Value, mime_types: &[String]) -> bool {
    let view = crate::files::image_transport::AttachmentProfileView::from_json(profile);
    mime_types
        .iter()
        .all(|m| crate::files::image_transport::profile_can_receive_attachment(view, m))
}

fn found(profile: &Value, api_key: String) -> Understudy {
    Understudy {
        profile: route_profile_from_value(profile),
        row: profile.clone(),
        api_key,
    }
}

/// v4 `resolveUncensoredTextUnderstudy` — the uncensored connection profile that
/// could take a text call, or `None`.
pub fn resolve_uncensored_text_understudy<A: ApiKeyResolver>(
    conn: &rusqlite::Connection,
    api_keys: &A,
    lookup: TextUnderstudyLookup<'_>,
) -> Option<Understudy> {
    let excluded = |id: &str| lookup.exclude.iter().any(|e| e == id);
    let eligible = |p: &Value| {
        user_id_of(p) == Some(lookup.user_id)
            && !excluded(str_of(p, "id"))
            && !is_courier(p)
            && lookup.filter.is_none_or(|f| f(p))
    };

    let attempt = || -> Result<Option<Understudy>, crate::db::DbError> {
        // `explicitId && …` — an empty id is no pick at all.
        let explicit_id = lookup.uncensored_text_profile_id.filter(|s| !s.is_empty());
        if let Some(explicit_id) = explicit_id.filter(|id| !excluded(id)) {
            let explicit = connection_profiles::find_by_id(conn, explicit_id)?;
            match &explicit {
                Some(p) if eligible(p) => {
                    if let Some(api_key) = decrypt_key(api_keys, p, lookup.user_id) {
                        tracing::debug!(
                            target: "quilltap::concierge_understudy",
                            profile_id = %str_of(p, "id"),
                            profile_name = %str_of(p, "name"),
                            provider = %str_of(p, "provider"),
                            model = %str_of(p, "modelName"),
                            "Text understudy: the configured uncensored profile"
                        );
                        return Ok(Some(found(p, api_key)));
                    }
                    tracing::warn!(
                        target: "quilltap::concierge_understudy",
                        profile_id = %explicit_id,
                        "Configured uncensored text profile has no usable API key; scanning instead"
                    );
                }
                _ => {
                    tracing::warn!(
                        target: "quilltap::concierge_understudy",
                        profile_id = %explicit_id,
                        found = explicit.is_some(),
                        courier = explicit.as_ref().is_some_and(is_courier),
                        "Configured uncensored text profile is missing, not owned, or not eligible; scanning instead"
                    );
                }
            }
        } else if let Some(explicit_id) = explicit_id {
            tracing::debug!(
                target: "quilltap::concierge_understudy",
                profile_id = %explicit_id,
                "Configured uncensored text profile is excluded on this call"
            );
        }

        // Ordered, not filtered: profiles that can carry this turn's
        // attachments first, the rest behind them.
        let compatible: Vec<Value> = connection_profiles::find_all(conn)?
            .into_iter()
            .filter(|p| is_dangerous_compatible(p) && eligible(p))
            .collect();
        let (can_carry, cannot_carry): (Vec<&Value>, Vec<&Value>) = compatible
            .iter()
            .partition(|p| profile_can_carry_turn(p, lookup.turn_attachment_mime_types));
        if !lookup.turn_attachment_mime_types.is_empty() && !cannot_carry.is_empty() {
            let names = |v: &[&Value]| -> Vec<String> {
                v.iter().map(|p| str_of(p, "name").to_string()).collect()
            };
            // v4 moved this line here from the wrapper WITHOUT its
            // `[DangerousContent]` prefix (`8bd080267`).
            tracing::info!(
                target: "quilltap::concierge_understudy",
                turn_attachment_mime_types = ?lookup.turn_attachment_mime_types,
                can_carry = ?names(&can_carry),
                cannot_carry = ?names(&cannot_carry),
                "Deprioritising uncensored candidates that cannot carry this turn"
            );
        }

        for p in can_carry.iter().chain(cannot_carry.iter()) {
            if let Some(api_key) = decrypt_key(api_keys, p, lookup.user_id) {
                tracing::debug!(
                    target: "quilltap::concierge_understudy",
                    profile_id = %str_of(p, "id"),
                    profile_name = %str_of(p, "name"),
                    provider = %str_of(p, "provider"),
                    model = %str_of(p, "modelName"),
                    "Text understudy: a discovered uncensored-compatible profile"
                );
                return Ok(Some(found(p, api_key)));
            }
        }

        tracing::debug!(
            target: "quilltap::concierge_understudy",
            user_id = %lookup.user_id,
            excluded = ?lookup.exclude,
            candidates = compatible.len(),
            "No uncensored text understudy is available"
        );
        Ok(None)
    };

    attempt().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::concierge_understudy",
            error = %error,
            "Text understudy lookup failed"
        );
        None
    })
}

/// v4 `resolveUncensoredImageUnderstudy` — the uncensored image profile that
/// could take an image call, or `None`. The explicit gate is ownership alone
/// (no courier, no filter: image profiles have neither).
pub fn resolve_uncensored_image_understudy<A: ApiKeyResolver>(
    conn: &rusqlite::Connection,
    api_keys: &A,
    lookup: ImageUnderstudyLookup<'_>,
) -> Option<Understudy> {
    let excluded = |id: &str| lookup.exclude.iter().any(|e| e == id);

    let attempt = || -> Result<Option<Understudy>, crate::db::DbError> {
        let explicit_id = lookup.uncensored_image_profile_id.filter(|s| !s.is_empty());
        if let Some(explicit_id) = explicit_id.filter(|id| !excluded(id)) {
            let explicit = image_profiles::find_by_id(conn, explicit_id)?;
            match &explicit {
                Some(p) if user_id_of(p) == Some(lookup.user_id) => {
                    if let Some(api_key) = decrypt_key(api_keys, p, lookup.user_id) {
                        tracing::debug!(
                            target: "quilltap::concierge_understudy",
                            profile_id = %str_of(p, "id"),
                            profile_name = %str_of(p, "name"),
                            provider = %str_of(p, "provider"),
                            model = %str_of(p, "modelName"),
                            "Image understudy: the configured uncensored profile"
                        );
                        return Ok(Some(found(p, api_key)));
                    }
                    tracing::warn!(
                        target: "quilltap::concierge_understudy",
                        profile_id = %explicit_id,
                        "Configured uncensored image profile has no usable API key; scanning instead"
                    );
                }
                _ => {
                    tracing::warn!(
                        target: "quilltap::concierge_understudy",
                        profile_id = %explicit_id,
                        found = explicit.is_some(),
                        "Configured uncensored image profile is missing or not owned; scanning instead"
                    );
                }
            }
        } else if let Some(explicit_id) = explicit_id {
            tracing::debug!(
                target: "quilltap::concierge_understudy",
                profile_id = %explicit_id,
                "Configured uncensored image profile is excluded on this call"
            );
        }

        let compatible: Vec<Value> = image_profiles::find_all(conn)?
            .into_iter()
            .filter(|p| {
                user_id_of(p) == Some(lookup.user_id)
                    && is_dangerous_compatible(p)
                    && !excluded(str_of(p, "id"))
            })
            .collect();

        for p in &compatible {
            if let Some(api_key) = decrypt_key(api_keys, p, lookup.user_id) {
                tracing::debug!(
                    target: "quilltap::concierge_understudy",
                    profile_id = %str_of(p, "id"),
                    profile_name = %str_of(p, "name"),
                    provider = %str_of(p, "provider"),
                    model = %str_of(p, "modelName"),
                    "Image understudy: a discovered uncensored-compatible profile"
                );
                return Ok(Some(found(p, api_key)));
            }
        }

        tracing::debug!(
            target: "quilltap::concierge_understudy",
            user_id = %lookup.user_id,
            excluded = ?lookup.exclude,
            candidates = compatible.len(),
            "No uncensored image understudy is available"
        );
        Ok(None)
    };

    attempt().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::concierge_understudy",
            error = %error,
            "Image understudy lookup failed"
        );
        None
    })
}
