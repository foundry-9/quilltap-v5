//! API-key resolution + the user-scoped wrapper semantics (W4.7d).
//!
//! Ports v4's `lib/services/api-key.service.ts` (the decrypted-key resolvers the
//! cheap-LLM + dangerous-content paths share) plus the user-scoped wrapper
//! semantics from `UserScopedConnectionsRepository`
//! (`lib/repositories/user-scoped.ts:225–261`). These are the **service-facing**
//! functions; the table marshaling is [`crate::db::api_keys`].
//!
//! Reads take a `&rusqlite::Connection` (the read pool), matching every other
//! scoped read (e.g. [`crate::db::connection_profiles::find_by_id`]). "Decryption"
//! is a no-op — the stored `key_value` is plaintext (the DB cipher is the only
//! protection); see [`crate::db::api_keys`].
//!
//! ## Two distinct lookup styles — never unified (per the W4.7d survey)
//!
//!   - the **connection-profile** style ([`get_api_key_for_connection_profile`]):
//!     read the profile, follow its `apiKeyId` to a key **scoped to the user**;
//!   - the **provider-scan** style (web search, moderation auto-detect): scan
//!     [`crate::db::api_keys::get_api_keys_by_user_id`] for `provider === X &&
//!     isActive` — that scan lives at each call site (it takes a provider filter),
//!     not here.
//!
//! ## The gate+lookup composite (v4 bug 81) — a THIRD thing
//!
//! [`resolve_connection_profile_api_key`] is neither of the two above: it is the
//! *decision* a caller about to make a provider request has to take, folding the
//! two capability questions ("must this provider hold a key?", "may it?") over
//! the profile's own `apiKeyId`. v4 added it (`resolveConnectionProfileApiKey`)
//! when `requiresApiKey` alone was found to be answering both, which left an
//! OpenAI-Compatible profile's key sitting in the database while the request went
//! out bare and the endpoint answered 401. The two capability predicates
//! ([`provider_requires_api_key`] / [`provider_accepts_api_key`], v4's
//! `lib/plugins/provider-validation.ts` pair) live here with it, so the whole
//! question has one home rather than a copy per service.

use rusqlite::Connection;

use crate::cheap_llm::CheapLlmSelection;
use crate::db::{api_keys, connection_profiles, DbError};
use crate::provider_manifest::Registry;

/// v4 `getApiKeyForConnectionProfile` — resolve the (plaintext) key for a
/// connection profile by id. `None` when the profile, its `apiKeyId`, or the key
/// record (scoped to the user) is missing.
pub fn get_api_key_for_connection_profile(
    conn: &Connection,
    profile_id: &str,
    user_id: &str,
) -> Result<Option<String>, DbError> {
    let Some(profile) = connection_profiles::find_by_id(conn, profile_id)? else {
        return Ok(None);
    };
    // v4 `if (!profile?.apiKeyId) return null` — a falsy (missing/empty) id fails.
    let api_key_id = match profile.get("apiKeyId").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id,
        _ => return Ok(None),
    };
    let key = api_keys::find_by_id_and_user_id(conn, api_key_id, user_id)?;
    Ok(key.map(|k| k.key_value))
}

/// The **provider-scan** resolver style (v4 web search's `getAllApiKeys()` scan +
/// the moderation auto-detect) — the FIRST active key for a provider owned by the
/// user (`provider === X && isActive`), in insertion order. Distinct from
/// [`get_api_key_for_connection_profile`] (which follows a profile's `apiKeyId`);
/// the W4.7d survey says NOT to unify them. Returns the whole [`api_keys::ApiKey`]
/// so the caller can read `key_value` (and `label`/`id` for diagnostics).
pub fn find_active_api_key_for_provider(
    conn: &Connection,
    user_id: &str,
    provider: &str,
) -> Result<Option<api_keys::ApiKey>, DbError> {
    let keys = api_keys::get_api_keys_by_user_id(conn, user_id)?;
    Ok(keys
        .into_iter()
        .find(|k| k.provider == provider && k.is_active))
}

/// v4 `getApiKeyForCheapLLMSelection` — resolve the key for a cheap-LLM
/// selection: `Some("")` for a local model (no key needed), `None` when the
/// selection has no profile or the lookup fails, else the profile's key.
pub fn get_api_key_for_cheap_llm_selection(
    conn: &Connection,
    selection: &CheapLlmSelection,
    user_id: &str,
) -> Result<Option<String>, DbError> {
    if selection.is_local {
        return Ok(Some(String::new()));
    }
    let Some(profile_id) = &selection.connection_profile_id else {
        return Ok(None);
    };
    get_api_key_for_connection_profile(conn, profile_id, user_id)
}

// ============================================================================
// The two capability predicates + the gate+lookup composite (v4 bug 81)
// ============================================================================

/// v4 `requiresApiKey(provider)` (`lib/plugins/provider-validation.ts`) —
/// `getConfigRequirements(provider)?.requiresApiKey ?? true`. Exact-case lookup;
/// an unknown provider is **required** (fail-safe, and asymmetric with
/// `requiresBaseUrl`'s `?? false`).
pub fn provider_requires_api_key(provider: &str) -> bool {
    Registry::built_in()
        .get_provider(provider)
        .map(|m| m.config_requirements.requires_api_key)
        .unwrap_or(true)
}

/// v4 `acceptsApiKey(provider)` (`lib/plugins/provider-validation.ts`, bug 81) —
/// whether a key *may* be attached and forwarded at all.
///
/// The companion question to [`provider_requires_api_key`]: OpenAI-Compatible
/// requires no key (a local llama.cpp has nowhere to put one) but accepts one (a
/// hosted endpoint demands a bearer token). Ask this before deciding whether a
/// stored key may reach the wire; ask the other before refusing to send without
/// one. An unknown provider inherits the fail-safe `true` from the fallback.
pub fn provider_accepts_api_key(provider: &str) -> bool {
    Registry::built_in()
        .get_provider(provider)
        .map(|m| m.config_requirements.accepts_api_key())
        .unwrap_or(true)
}

/// Why a profile could not produce the API key its provider needs (v4
/// `ProfileApiKeyFailure`). The two variants carry different sentences at every
/// call site, so they stay distinct rather than collapsing to one error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileApiKeyFailure {
    /// v4 `'no-api-key-configured'` — the provider demands a key and the profile
    /// names none.
    NoApiKeyConfigured,
    /// v4 `'api-key-not-found'` — the profile names one and the row is gone. A
    /// dangling `apiKeyId` fails loudly **even on a provider that merely accepts
    /// a key**, because the user attached it on purpose and going out
    /// unauthenticated instead is the silent-wrong-answer kind of failure.
    ApiKeyNotFound,
}

/// v4 `ProfileApiKeyResolution`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileApiKeyResolution {
    /// v4 `{ ok: true, apiKey }` — `""` for a provider that takes no key.
    Ok(String),
    /// v4 `{ ok: false, reason }`.
    Failed(ProfileApiKeyFailure),
}

/// v4 `resolveConnectionProfileApiKey` (`lib/services/api-key.service.ts`, bug
/// 81) — the decrypted key a connection profile should send.
///
/// Asks both questions rather than one: a provider that *requires* a key must
/// have one before the call goes out, and a provider that merely *accepts* one —
/// OpenAI-Compatible, whose hosted endpoints want a bearer token and whose local
/// ones do not — must still forward the key the user attached. Reading only
/// `requiresApiKey`, as every caller here once did, left an OpenAI-Compatible
/// profile's key sitting in the database while the request went out bare and the
/// endpoint answered 401.
///
/// The order of the three gates is load-bearing:
///   1. a provider that accepts no key returns `Ok("")` and is **never looked
///      up**, so a stale row on such a profile cannot fail the turn;
///   2. no `apiKeyId` refuses only where a key is *required*;
///   3. an `apiKeyId` that is present is ALWAYS followed, and a missing row
///      always refuses.
///
/// The lookup is v4's UNSCOPED `findApiKeyById`, matching the two Brahma sites
/// this replaces. A read error collapses to [`ProfileApiKeyFailure::ApiKeyNotFound`]
/// — the pre-existing behavior of both of those sites (their `_ =>` arms), kept
/// so the ported semantics change and the error mapping do not change together.
pub fn resolve_connection_profile_api_key(
    conn: &Connection,
    provider: &str,
    api_key_id: Option<&str>,
) -> ProfileApiKeyResolution {
    if !provider_accepts_api_key(provider) {
        return ProfileApiKeyResolution::Ok(String::new());
    }

    // v4 `if (!profile.apiKeyId)` — a falsy (missing/empty) id counts as none.
    let Some(id) = api_key_id.filter(|s| !s.is_empty()) else {
        return if provider_requires_api_key(provider) {
            ProfileApiKeyResolution::Failed(ProfileApiKeyFailure::NoApiKeyConfigured)
        } else {
            ProfileApiKeyResolution::Ok(String::new())
        };
    };

    match api_keys::find_by_id(conn, id) {
        Ok(Some(key)) => ProfileApiKeyResolution::Ok(key.key_value),
        _ => ProfileApiKeyResolution::Failed(ProfileApiKeyFailure::ApiKeyNotFound),
    }
}

// ============================================================================
// User-scoped wrapper semantics (v4 `UserScopedConnectionsRepository`)
// ============================================================================
//
// These wrap the [`crate::db::api_keys`] repo with the user-scope rules v4's
// `UserScopedConnectionsRepository` applies: reads route through the scoped
// lookup, write ops pre-check ownership, and `userId` is stripped from update
// payloads. The mutating variants take an [`api_keys::ApiKeysRepository`] (a
// writer-thread borrow); the read variants take a `&Connection`.

/// v4 `UserScopedConnectionsRepository.getAllApiKeys` — all keys for the scoped
/// user (drops invalid rows).
pub fn get_all_api_keys(
    conn: &Connection,
    user_id: &str,
) -> Result<Vec<api_keys::ApiKey>, DbError> {
    api_keys::get_api_keys_by_user_id(conn, user_id)
}

/// v4 `UserScopedConnectionsRepository.findApiKeyById` — re-routed to the
/// **scoped** variant (ownership-checked), NOT the unscoped repo method.
pub fn find_api_key_by_id_scoped(
    conn: &Connection,
    id: &str,
    user_id: &str,
) -> Result<Option<api_keys::ApiKey>, DbError> {
    api_keys::find_by_id_and_user_id(conn, id, user_id)
}

/// v4 `UserScopedConnectionsRepository.updateApiKey` — pre-check ownership
/// (`None` when the scoped lookup misses), then update. The scoped wrapper strips
/// `userId` from the patch; [`api_keys::AkUpdate`] carries no `userId` field, so
/// the strip is structural.
pub fn update_api_key_scoped(
    conn: &Connection,
    repo: &api_keys::ApiKeysRepository<'_>,
    id: &str,
    user_id: &str,
    patch: &api_keys::AkUpdate,
) -> Result<Option<api_keys::ApiKey>, DbError> {
    if find_api_key_by_id_scoped(conn, id, user_id)?.is_none() {
        return Ok(None);
    }
    repo.update(id, patch)
}

/// v4 `UserScopedConnectionsRepository.deleteApiKey` — pre-check ownership
/// (`false` when the scoped lookup misses), then delete.
pub fn delete_api_key_scoped(
    conn: &Connection,
    repo: &api_keys::ApiKeysRepository<'_>,
    id: &str,
    user_id: &str,
) -> Result<bool, DbError> {
    if find_api_key_by_id_scoped(conn, id, user_id)?.is_none() {
        return Ok(false);
    }
    repo.delete(id)
}

/// v4 `UserScopedConnectionsRepository.recordApiKeyUsage` — pre-check ownership
/// (`None` when the scoped lookup misses), then record usage.
pub fn record_api_key_usage_scoped(
    conn: &Connection,
    repo: &api_keys::ApiKeysRepository<'_>,
    id: &str,
    user_id: &str,
) -> Result<Option<api_keys::ApiKey>, DbError> {
    if find_api_key_by_id_scoped(conn, id, user_id)?.is_none() {
        return Ok(None);
    }
    repo.record_usage(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::api_keys::{AkCreate, AkUpdate, ApiKeysRepository};
    use rusqlite::Connection;

    /// A bare in-memory `api_keys` table (DDL transcribed) for the scoped tests.
    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE api_keys (\
               id TEXT PRIMARY KEY, userId TEXT NOT NULL, label TEXT NOT NULL, \
               provider TEXT NOT NULL, key_value TEXT NOT NULL, isActive INTEGER DEFAULT 1, \
               lastUsed TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);",
        )
        .unwrap();
        conn
    }

    fn seed_key(conn: &Connection, user_id: &str) -> String {
        let repo = ApiKeysRepository::new(conn);
        repo.create(&AkCreate {
            user_id: user_id.to_string(),
            label: "k".to_string(),
            provider: "ANTHROPIC".to_string(),
            key_value: "synthetic-x".to_string(),
            is_active: None,
            last_used: None,
        })
        .unwrap()
        .id
    }

    #[test]
    fn cheap_selection_local_returns_empty_key_without_db() {
        let conn = mem_db();
        let sel = CheapLlmSelection {
            provider: "OLLAMA".to_string(),
            model_name: "m".to_string(),
            base_url: None,
            connection_profile_id: Some("whatever".to_string()),
            is_local: true,
            profile_parameters: None,
        };
        // Local short-circuits before any DB read.
        assert_eq!(
            get_api_key_for_cheap_llm_selection(&conn, &sel, "u").unwrap(),
            Some(String::new())
        );
    }

    #[test]
    fn cheap_selection_no_profile_is_none() {
        let conn = mem_db();
        let sel = CheapLlmSelection {
            provider: "ANTHROPIC".to_string(),
            model_name: "m".to_string(),
            base_url: None,
            connection_profile_id: None,
            is_local: false,
            profile_parameters: None,
        };
        assert_eq!(
            get_api_key_for_cheap_llm_selection(&conn, &sel, "u").unwrap(),
            None
        );
    }

    #[test]
    fn provider_scan_finds_first_active_key() {
        let conn = mem_db();
        let user = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let repo = ApiKeysRepository::new(&conn);
        // An inactive ANTHROPIC key, then an active one; an OPENAI key.
        repo.create(&AkCreate {
            user_id: user.to_string(),
            label: "inactive".to_string(),
            provider: "ANTHROPIC".to_string(),
            key_value: "synthetic-inactive".to_string(),
            is_active: Some(false),
            last_used: None,
        })
        .unwrap();
        repo.create(&AkCreate {
            user_id: user.to_string(),
            label: "active".to_string(),
            provider: "ANTHROPIC".to_string(),
            key_value: "synthetic-active".to_string(),
            is_active: Some(true),
            last_used: None,
        })
        .unwrap();

        let found = find_active_api_key_for_provider(&conn, user, "ANTHROPIC")
            .unwrap()
            .expect("an active ANTHROPIC key");
        assert_eq!(found.key_value, "synthetic-active");
        // No key for a provider the user has none of.
        assert!(find_active_api_key_for_provider(&conn, user, "GOOGLE")
            .unwrap()
            .is_none());
    }

    #[test]
    fn scoped_ops_enforce_ownership() {
        let conn = mem_db();
        let owner = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let intruder = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        let id = seed_key(&conn, owner);
        let repo = ApiKeysRepository::new(&conn);

        // Scoped read: owner sees it, intruder does not.
        assert!(find_api_key_by_id_scoped(&conn, &id, owner)
            .unwrap()
            .is_some());
        assert!(find_api_key_by_id_scoped(&conn, &id, intruder)
            .unwrap()
            .is_none());

        // Intruder update/delete/record are no-ops (ownership pre-check).
        assert!(update_api_key_scoped(
            &conn,
            &repo,
            &id,
            intruder,
            &AkUpdate {
                label: Some("hacked".to_string()),
                ..Default::default()
            },
        )
        .unwrap()
        .is_none());
        assert!(!delete_api_key_scoped(&conn, &repo, &id, intruder).unwrap());
        assert!(record_api_key_usage_scoped(&conn, &repo, &id, intruder)
            .unwrap()
            .is_none());

        // The row is untouched, and the owner CAN act.
        let still = find_api_key_by_id_scoped(&conn, &id, owner)
            .unwrap()
            .unwrap();
        assert_eq!(still.label, "k");
        assert!(delete_api_key_scoped(&conn, &repo, &id, owner).unwrap());
        assert!(find_api_key_by_id_scoped(&conn, &id, owner)
            .unwrap()
            .is_none());
    }

    // -----------------------------------------------------------------------
    // v4 bug 81 — `resolveConnectionProfileApiKey`'s truth table, the six rows of
    // v4's own `__tests__/unit/lib/services/api-key-service.test.ts`. v4 mocks the
    // two predicates; here they come from the REAL manifests, so each row names
    // the provider that actually answers that way.
    // -----------------------------------------------------------------------

    fn seed_key_for(conn: &Connection, provider: &str, value: &str) -> String {
        let repo = ApiKeysRepository::new(conn);
        repo.create(&AkCreate {
            user_id: "u".to_string(),
            label: "k".to_string(),
            provider: provider.to_string(),
            key_value: value.to_string(),
            is_active: None,
            last_used: None,
        })
        .unwrap()
        .id
    }

    /// Row 1 — "sends nothing for a provider that takes no key (Ollama)", and the
    /// stale row is **not even looked up**: the id below names a key that exists,
    /// and the empty answer proves the accepts-gate short-circuited before the
    /// lookup could find it.
    #[test]
    fn keyless_provider_never_looks_up_its_stale_row() {
        let conn = mem_db();
        let id = seed_key_for(&conn, "OLLAMA", "sk-stale");
        assert_eq!(
            resolve_connection_profile_api_key(&conn, "OLLAMA", Some(&id)),
            ProfileApiKeyResolution::Ok(String::new())
        );
    }

    /// Row 2 — the attached key is forwarded for a provider that accepts but does
    /// not require one. This is the row bug 81 was: before it, an
    /// OpenAI-Compatible profile's key stayed in the database and the request went
    /// out bare.
    #[test]
    fn accepting_provider_forwards_the_attached_key() {
        let conn = mem_db();
        let id = seed_key_for(&conn, "OPENAI_COMPATIBLE", "sk-together");
        assert_eq!(
            resolve_connection_profile_api_key(&conn, "OPENAI_COMPATIBLE", Some(&id)),
            ProfileApiKeyResolution::Ok("sk-together".to_string())
        );
    }

    /// Row 3 — an accepting provider with none attached proceeds keyless (only
    /// `requiresApiKey` may refuse). The empty-string id is v4's falsy `apiKeyId`.
    #[test]
    fn accepting_provider_with_no_key_proceeds() {
        let conn = mem_db();
        assert_eq!(
            resolve_connection_profile_api_key(&conn, "OPENAI_COMPATIBLE", None),
            ProfileApiKeyResolution::Ok(String::new())
        );
        assert_eq!(
            resolve_connection_profile_api_key(&conn, "OPENAI_COMPATIBLE", Some("")),
            ProfileApiKeyResolution::Ok(String::new())
        );
    }

    /// Row 4 — a requiring provider with none attached refuses.
    #[test]
    fn requiring_provider_with_no_key_refuses() {
        let conn = mem_db();
        assert_eq!(
            resolve_connection_profile_api_key(&conn, "ANTHROPIC", None),
            ProfileApiKeyResolution::Failed(ProfileApiKeyFailure::NoApiKeyConfigured)
        );
    }

    /// Row 5 — a dangling id refuses **even where the key is optional**. The user
    /// attached it on purpose; going out unauthenticated instead is the
    /// silent-wrong-answer failure this whole bug is made of.
    #[test]
    fn dangling_id_refuses_even_where_the_key_is_optional() {
        let conn = mem_db();
        assert_eq!(
            resolve_connection_profile_api_key(&conn, "OPENAI_COMPATIBLE", Some("key-deleted")),
            ProfileApiKeyResolution::Failed(ProfileApiKeyFailure::ApiKeyNotFound)
        );
    }

    /// Row 6 — the ordinary hosted happy path.
    #[test]
    fn hosted_provider_forwards_its_key() {
        let conn = mem_db();
        let id = seed_key_for(&conn, "ANTHROPIC", "sk-ant");
        assert_eq!(
            resolve_connection_profile_api_key(&conn, "ANTHROPIC", Some(&id)),
            ProfileApiKeyResolution::Ok("sk-ant".to_string())
        );
    }

    /// An unknown provider inherits the fail-safe `true` from BOTH predicates, so
    /// it behaves exactly like a hosted one.
    #[test]
    fn unknown_provider_is_treated_as_requiring() {
        let conn = mem_db();
        assert_eq!(
            resolve_connection_profile_api_key(&conn, "NOT_A_PROVIDER", None),
            ProfileApiKeyResolution::Failed(ProfileApiKeyFailure::NoApiKeyConfigured)
        );
    }

    /// **The spine half of v4 bug 81, which v5 never had** (P4.D93's measurement).
    ///
    /// v4's chat-message spine gated the key it already held on `requiresApiKey`,
    /// so an OpenAI-Compatible profile's key was dropped on the way to the wire.
    /// v5 resolves the key host-side instead, through [`find_active_api_key_for_provider`]
    /// (`quilltap-host`'s `DbProviderKeys::key_for`) — a provider SCAN with no
    /// capability gate anywhere on it, so an OAC key has always reached
    /// `apply_auth`. What keeps a genuinely keyless endpoint bare is the manifest
    /// `auth` scheme, not a lookup gate: OLLAMA declares `auth: none` and injects
    /// nothing whatever the scan returns.
    ///
    /// This pins that reading, because the *reason* v5 needs no spine port is the
    /// absence of the gate — if one were ever added here, bug 81 would arrive in
    /// v5 for the first time.
    #[test]
    fn provider_scan_is_capability_blind() {
        let conn = mem_db();
        seed_key_for(&conn, "OPENAI_COMPATIBLE", "sk-hosted-oac");
        seed_key_for(&conn, "OLLAMA", "sk-pointless-but-stored");
        assert_eq!(
            find_active_api_key_for_provider(&conn, "u", "OPENAI_COMPATIBLE")
                .unwrap()
                .map(|k| k.key_value),
            Some("sk-hosted-oac".to_string()),
            "the host key source must forward an OAC key — v5's non-bug"
        );
        assert!(
            find_active_api_key_for_provider(&conn, "u", "OLLAMA")
                .unwrap()
                .is_some(),
            "the scan does not gate on capability; `auth: none` is what keeps \
             an Ollama request bare"
        );
    }
}
