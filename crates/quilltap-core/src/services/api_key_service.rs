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

use rusqlite::Connection;

use crate::cheap_llm::CheapLlmSelection;
use crate::db::{api_keys, connection_profiles, DbError};

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
}
