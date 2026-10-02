//! API-key resolution + the user-scoped wrapper semantics (W4.7d).
//!
//! Ports v4's `lib/services/api-key.service.ts` (the decrypted-key resolvers the
//! cheap-LLM + dangerous-content paths share) plus the user-scoped wrapper
//! semantics from `UserScopedConnectionsRepository`
//! (`lib/repositories/user-scoped.ts:225–261`). These are the **service-facing**
//! functions; the table marshaling is [`crate::db::api_keys`].
//!
//! Reads go through [`MainReads`]: a held `&rusqlite::Connection`, or the pooled
//! [`crate::db::runtime::Db`], whose checkout then sits inside each read's
//! fallback wrap (P4.136). "Decryption"
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

/// Where a resolver's reads run (P4.136): a held connection, or the read pool.
/// With the pool, each wrapped read checks out its OWN connection INSIDE its
/// fallback wrap — as v4's `getCollection()` sits inside each repository
/// `safeQuery` — so a pool failure lands on that read's line, not on no line.
/// It is ONE of v4's lines on that arm, not both: v4's key reads first call
/// `getApiKeysCollection()`, a rethrowing `safeQuery` that logs `Failed to get
/// API keys collection` before the outer line (`connection-profiles.repository.
/// ts:205-215`). v5 has no collection layer, so it emits the outer line alone.
/// That is a recorded divergence on the pool-failure arm only. A corrupt cell
/// fails outside the inner wrap, so it is one line on both sides.
pub trait MainReads {
    fn read_main_with<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, DbError>,
    ) -> Result<T, DbError>;
}

impl MainReads for Connection {
    fn read_main_with<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
        f(self)
    }
}

impl MainReads for crate::db::runtime::Db {
    fn read_main_with<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
        self.read_main(f)
    }
}

/// v4 `findApiKeyById` as every caller sees it (P4.139): the UNSCOPED read of
/// one key over `reads`, through `db::fallback`'s home — a read error (a
/// corrupt cell, a pool failure) logs `Error finding API key by ID
/// {collection: 'connection_profiles', keyId, error}` and answers `None`
/// (`connection-profiles.repository.ts:249-265`, a 4-arg fallback
/// `safeQuery`). v4 has no API-key read whose error propagates, so a caller
/// never wants the raw `api_keys::find_by_id` — `api_key_read_sites_census`
/// holds that rule.
pub fn read_api_key<R: MainReads>(reads: &R, id: &str) -> Option<api_keys::ApiKey> {
    crate::db::fallback::find_api_key_by_id_or_none(id, || {
        reads.read_main_with(|conn| api_keys::find_by_id(conn, id))
    })
}

/// v4 `findApiKeyByIdAndUserId` as every caller sees it (P4.139): the SCOPED
/// read (a key owned by another user is `None`), through `db::fallback`'s
/// home — a read error logs `Error finding API key by ID and user ID
/// {collection, keyId, userId, error}` and answers `None`
/// (`connection-profiles.repository.ts:270-288`). Also the read v4's
/// user-scoped repository makes for `findApiKeyById` (`user-scoped.ts:246-
/// 274` reroutes it here).
pub fn read_api_key_scoped<R: MainReads>(
    reads: &R,
    id: &str,
    user_id: &str,
) -> Option<api_keys::ApiKey> {
    crate::db::fallback::find_api_key_by_id_and_user_id_or_none(id, user_id, || {
        reads.read_main_with(|conn| api_keys::find_by_id_and_user_id(conn, id, user_id))
    })
}

/// v4 `getApiKeysByUserId` as every caller sees it (P4.139): the user's keys
/// over `reads` — the per-row drop and its WARN inside
/// [`api_keys::get_api_keys_by_user_id`] — and, when the STATEMENT fails, the
/// fallback `safeQuery`'s line `Error finding API keys by user ID
/// {collection: 'connection_profiles', userId, error}` and `[]`
/// (`connection-profiles.repository.ts:218-244`, a 4-arg fallback on the
/// repository constructed with `'connection_profiles'`).
// §S fold → db::fallback::find_api_keys_by_user_id_or_empty
pub(crate) fn api_keys_by_user_id_or_empty<R: MainReads>(
    reads: &R,
    user_id: &str,
) -> Vec<api_keys::ApiKey> {
    reads
        .read_main_with(|conn| api_keys::get_api_keys_by_user_id(conn, user_id))
        .unwrap_or_else(|error| {
            tracing::error!(
                target: "quilltap::db",
                collection = "connection_profiles",
                userId = %user_id,
                error = %crate::db::fallback::error_text(&error),
                "Error finding API keys by user ID"
            );
            Vec::new()
        })
}

/// The generators' key idiom (P4.139) — v4's external-prompt generator, the
/// character optimizer, the wizard (primary + vision) and the AI import all
/// read it the same way (`external-prompt-generator.service.ts:102-108`,
/// `character-optimizer.service.ts:815-821`, `character-wizard.service.ts:
/// 727-733,766-771`, `ai-import.service.ts:831-837`):
/// `let apiKey = ''; if (profile.apiKeyId) { const k = await
/// repos.connections.findApiKeyByIdAndUserId(profile.apiKeyId, userId); if (k)
/// apiKey = k.key_value; }`. The read is the SCOPED fallback, so a read error
/// logs its line and the generation PROCEEDS with `''` (the provider then
/// refuses or not) — v5's `?` had failed the whole generator instead.
pub fn profile_api_key_value_scoped<R: MainReads>(
    reads: &R,
    profile: &serde_json::Value,
    user_id: &str,
) -> String {
    match profile
        .get("apiKeyId")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some(key_id) => read_api_key_scoped(reads, key_id, user_id)
            .map(|k| k.key_value)
            .unwrap_or_default(),
        None => String::new(),
    }
}

/// v4 `getApiKeyForConnectionProfile` — resolve the (plaintext) key for a
/// connection profile by id. `None` when the profile, its `apiKeyId`, or the key
/// record (scoped to the user) is missing — or when either read FAILS: v4's
/// two reads are both fallback `safeQuery`s (`api-key.service.ts:23-32`), the
/// UNSCOPED profile read (`_findById` → `Error finding entity by ID`) then the
/// SCOPED key read (`Error finding API key by ID and user ID`), each logging
/// its own line and answering `null` (P4.136; the two `?`s here had surfaced
/// the `Err` for every caller to fold silently).
pub fn get_api_key_for_connection_profile<R: MainReads>(
    reads: &R,
    profile_id: &str,
    user_id: &str,
) -> Option<String> {
    let profile =
        crate::db::fallback::find_by_id_or_none("connection_profiles", profile_id, || {
            reads.read_main_with(|conn| connection_profiles::find_by_id(conn, profile_id))
        })?;
    // v4 `if (!profile?.apiKeyId) return null` — a falsy (missing/empty) id fails.
    let api_key_id = match profile.get("apiKeyId").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id,
        _ => return None,
    };
    read_api_key_scoped(reads, api_key_id, user_id).map(|k| k.key_value)
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

/// [`find_active_api_key_for_provider`] at v4's fallback (P4.139): the web
/// search's key pick (`web-search-handler.ts:88-111` →
/// `getUserRepositories(userId).connections.getAllApiKeys()`) reads through
/// the fallback by-user list, so a failed read is its line and "no key", and
/// a bad ROW is dropped with its WARN while the other keys are still found.
/// The propagating original stays for `DbProviderKeys` (no v4 counterpart —
/// no v4 model call scans).
pub fn find_active_api_key_for_provider_or_none<R: MainReads>(
    reads: &R,
    user_id: &str,
    provider: &str,
) -> Option<api_keys::ApiKey> {
    api_keys_by_user_id_or_empty(reads, user_id)
        .into_iter()
        .find(|k| k.provider == provider && k.is_active)
}

std::thread_local! {
    /// The differential's twin of v4's mocked `getApiKeyForCheapLLMSelection`
    /// (armed only through `test_support::CannedCheapLlmKey`; `None` always in
    /// production).
    static CANNED_CHEAP_LLM_KEY: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

std::thread_local! {
    /// The differential's twin of v4's mocked `requiresApiKey: () => false`
    /// (armed only through `test_support::CannedRequiresApiKey`).
    static CANNED_REQUIRES_API_KEY: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Arm / disarm the thread-scoped `requiresApiKey` answer; answers the
/// previous value. Test-support only — see `test_support::CannedRequiresApiKey`.
#[doc(hidden)]
pub fn set_canned_requires_api_key(answer: Option<bool>) -> Option<bool> {
    CANNED_REQUIRES_API_KEY.with(|c| c.replace(answer))
}

/// Arm / disarm the thread-scoped canned key; answers the previous value.
/// Test-support only — see `test_support::CannedCheapLlmKey`.
#[doc(hidden)]
pub fn set_canned_cheap_llm_key(key: Option<String>) -> Option<String> {
    CANNED_CHEAP_LLM_KEY.with(|k| std::mem::replace(&mut *k.borrow_mut(), key))
}

/// v4 `getApiKeyForCheapLLMSelection` — resolve the key for a cheap-LLM
/// selection: `Some("")` for a local model (no key needed), `None` when the
/// selection has no profile or the lookup fails (a failed read logs v4's
/// repository line first — [`get_api_key_for_connection_profile`]), else the
/// profile's key. The canned seam answers FIRST, before any read.
pub fn get_api_key_for_cheap_llm_selection<R: MainReads>(
    reads: &R,
    selection: &CheapLlmSelection,
    user_id: &str,
) -> Option<String> {
    if let Some(canned) = CANNED_CHEAP_LLM_KEY.with(|k| k.borrow().clone()) {
        return Some(canned);
    }
    if selection.is_local {
        return Some(String::new());
    }
    let profile_id = selection.connection_profile_id.as_deref()?;
    get_api_key_for_connection_profile(reads, profile_id, user_id)
}

// ============================================================================
// The two capability predicates + the gate+lookup composite (v4 bug 81)
// ============================================================================

/// v4 `requiresApiKey(provider)` (`lib/plugins/provider-validation.ts`) —
/// `getConfigRequirements(provider)?.requiresApiKey ?? true`. Exact-case lookup;
/// an unknown provider is **required** (fail-safe, and asymmetric with
/// `requiresBaseUrl`'s `?? false`).
pub fn provider_requires_api_key(provider: &str) -> bool {
    if let Some(answer) = CANNED_REQUIRES_API_KEY.with(|c| c.get()) {
        return answer;
    }
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

impl ProfileApiKeyFailure {
    /// v4's `reason` string verbatim. The fallback chain records it as the
    /// attempt's `error`, so it reaches the user through
    /// `summarize_fallback_attempts` — bytes, not a label.
    pub fn as_str(self) -> &'static str {
        match self {
            ProfileApiKeyFailure::NoApiKeyConfigured => "no-api-key-configured",
            ProfileApiKeyFailure::ApiKeyNotFound => "api-key-not-found",
        }
    }

    /// v4 `describeProfileApiKeyFailure` (`0506517d3` correction (d)): the
    /// user-facing sentence for a failed resolution, in ONE place. The Brahma
    /// one-shot console and the orchestrator used to spell it out separately and
    /// disagreed on the first letter — the one-shot said "no API key configured
    /// for this connection profile" where the orchestrator said "No …". v4
    /// collapsed both onto the capitalised form; so does this.
    ///
    /// v4's help chat is the third caller. That surface is unported (`p4.9i2`),
    /// so it has no v5 twin to route through here yet.
    pub fn describe(self) -> &'static str {
        match self {
            ProfileApiKeyFailure::NoApiKeyConfigured => {
                "No API key configured for this connection profile"
            }
            ProfileApiKeyFailure::ApiKeyNotFound => "API key not found",
        }
    }
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
/// The lookup is v4's UNSCOPED `findApiKeyById` — a fallback `safeQuery`, so
/// a read error logs `Error finding API key by ID` and answers `null`, which
/// is [`ProfileApiKeyFailure::ApiKeyNotFound`] (P4.136: the line had been
/// missing for every composite caller; the outcome was already v4's).
pub fn resolve_connection_profile_api_key<R: MainReads>(
    reads: &R,
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

    match read_api_key(reads, id) {
        Some(key) => ProfileApiKeyResolution::Ok(key.key_value),
        None => ProfileApiKeyResolution::Failed(ProfileApiKeyFailure::ApiKeyNotFound),
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

/// P4.139's shared test instance: a REAL provisioned instance (all three
/// partitions, v4's fresh schema + seed) with planted `api_keys` rows — for the
/// route- and service-level pins of the API-key read class, whose callers read
/// whole tables (`connection_profiles`, `image_profiles`, `characters`) a
/// reduced hand-rolled DDL would have to re-derive.
#[cfg(test)]
pub(crate) mod test_instance {
    use rusqlite::Connection;

    pub(crate) const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    /// The provisioned single user.
    pub(crate) const USER: &str = crate::services::provisioning::SINGLE_USER_ID;
    pub(crate) const NOW: &str = "2026-10-01T00:00:00.000Z";
    /// The planted CORRUPT key (a BLOB `key_value`) and a healthy one, both
    /// owned by [`USER`].
    pub(crate) const BAD_KEY: &str = "a0000139-0000-4000-8000-0000000000bd";
    pub(crate) const OK_KEY: &str = "a0000139-0000-4000-8000-00000000000c";

    /// Provision a fresh instance, plant [`BAD_KEY`] + [`OK_KEY`], run `seed`
    /// over the writable main partition, and open the pooled `Db` (main +
    /// mount index).
    pub(crate) fn provisioned(
        seed: impl FnOnce(&Connection),
    ) -> (tempfile::TempDir, crate::db::runtime::Db) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let main = data.join("quilltap.db");
        {
            let w = crate::db::Writer::open_writable(&main, PEPPER).unwrap();
            crate::db::fallback::test_plants::plant_api_key(w.connection(), BAD_KEY, USER, true);
            crate::db::fallback::test_plants::plant_api_key(w.connection(), OK_KEY, USER, false);
            seed(w.connection());
        }
        let db = crate::db::runtime::Db::open(
            crate::db::runtime::DbPaths {
                main,
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (dir, db)
    }

    /// Plant a connection profile owned by [`USER`].
    pub(crate) fn plant_connection_profile(
        conn: &Connection,
        id: &str,
        provider: &str,
        api_key_id: Option<&str>,
    ) {
        conn.execute(
            "INSERT INTO connection_profiles (id, userId, name, provider, modelName, apiKeyId, \
             createdAt, updatedAt) VALUES (?1, ?2, 'P4.139 probe', ?3, 'm', ?4, ?5, ?5)",
            rusqlite::params![id, USER, provider, api_key_id, NOW],
        )
        .unwrap();
    }

    /// The exact unscoped / scoped repository lines for `key_id` over the
    /// BLOB plant.
    pub(crate) fn unscoped_line(key_id: &str) -> String {
        format!(
            "ERROR quilltap::db Error finding API key by ID collection=connection_profiles keyId={key_id} error=Invalid column type Blob at index: 4, name: key_value"
        )
    }
    pub(crate) fn scoped_line(key_id: &str, user_id: &str) -> String {
        format!(
            "ERROR quilltap::db Error finding API key by ID and user ID collection=connection_profiles keyId={key_id} userId={user_id} error=Invalid column type Blob at index: 4, name: key_value"
        )
    }

    /// A pooled `Db` whose every checkout fails (the file is unlinked after
    /// the open; the writer kept no handle) — v4's in-`safeQuery`
    /// `getCollection()` failing. Asserted, so the plant cannot silently pass.
    /// P4.139: the ONE copy (`dangerous_content::understudy`'s byte-identical
    /// twin folded onto it).
    pub(crate) fn db_with_a_failing_read_pool() -> (tempfile::TempDir, crate::db::runtime::Db) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.db");
        drop(crate::db::Writer::open_writable(&path, PEPPER).unwrap());
        let db = crate::db::runtime::Db::open_main(&path, PEPPER).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(
            db.read_main(|_| Ok(())).is_err(),
            "the plant must fail the pool"
        );
        (dir, db)
    }

    /// The `quilltap::db` lines of a capture.
    pub(crate) fn db_lines(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.contains(" quilltap::db "))
            .cloned()
            .collect()
    }
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
            get_api_key_for_cheap_llm_selection(&conn, &sel, "u"),
            Some(String::new())
        );
    }

    use test_instance::db_with_a_failing_read_pool;

    /// P4.136: v4 `getApiKeyForConnectionProfile`'s FIRST read is the
    /// unscoped `_findById` — a failure logs `Error finding entity by ID` and
    /// answers `null`, so the key read is never reached (one line, not two).
    /// The canned seam still answers FIRST, before any read.
    #[test]
    fn a_failed_profile_read_logs_v4s_find_by_id_line_and_answers_none() {
        let (_dir, db) = db_with_a_failing_read_pool();
        let sel = CheapLlmSelection {
            provider: "OPENAI".to_string(),
            model_name: "m".to_string(),
            base_url: None,
            connection_profile_id: Some("cp-1".to_string()),
            is_local: false,
            profile_parameters: None,
        };
        let (key, lines) = crate::test_support::captured_with(|| {
            get_api_key_for_cheap_llm_selection(&db, &sel, "u-1")
        });
        assert_eq!(key, None);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "ERROR quilltap::db Error finding entity by ID collection=connection_profiles id=cp-1 error="
            ),
            "{}",
            lines[0]
        );
        let _canned = crate::test_support::CannedCheapLlmKey::install("k-canned");
        let (key, lines) = crate::test_support::captured_with(|| {
            get_api_key_for_cheap_llm_selection(&db, &sel, "u-1")
        });
        assert_eq!(key.as_deref(), Some("k-canned"));
        assert!(lines.is_empty(), "the canned seam reads nothing: {lines:?}");
    }

    /// P4.136: the gate+lookup composite's lookup is v4's UNSCOPED fallback
    /// `findApiKeyById` — a corrupt row logs the repository's line and the
    /// outcome stays `api-key-not-found`; a healthy row is silent.
    #[test]
    fn the_composites_failed_key_read_logs_v4s_line_and_refuses_not_found() {
        let conn = crate::db::fallback::test_plants::conn_with_api_keys();
        crate::db::fallback::test_plants::plant_api_key(&conn, "k-bad", "u-1", true);
        crate::db::fallback::test_plants::plant_api_key(&conn, "k-ok", "u-1", false);
        let (got, lines) = crate::test_support::captured_with(|| {
            resolve_connection_profile_api_key(&conn, "ANTHROPIC", Some("k-bad"))
        });
        assert_eq!(
            got,
            ProfileApiKeyResolution::Failed(ProfileApiKeyFailure::ApiKeyNotFound)
        );
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error finding API key by ID collection=connection_profiles keyId=k-bad error=Invalid column type Blob at index: 4, name: key_value".to_string()]
        );
        let (got, lines) = crate::test_support::captured_with(|| {
            resolve_connection_profile_api_key(&conn, "ANTHROPIC", Some("k-ok"))
        });
        assert_eq!(
            got,
            ProfileApiKeyResolution::Ok("synthetic-k-ok".to_string())
        );
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// P4.139: the two helpers ARE v4's two fallback reads — a corrupt cell
    /// logs the home's exact line (unscoped / scoped) and is `None`; a healthy
    /// row, a miss and (scoped) a foreign owner are silent.
    #[test]
    fn the_read_helpers_log_v4s_lines_and_answer_none() {
        use crate::db::fallback::test_plants::{conn_with_api_keys, plant_api_key};
        let conn = conn_with_api_keys();
        plant_api_key(&conn, "k-bad", "u-1", true);
        plant_api_key(&conn, "k-ok", "u-1", false);
        let blob = "error=Invalid column type Blob at index: 4, name: key_value";

        let (got, lines) = crate::test_support::captured_with(|| read_api_key(&conn, "k-bad"));
        assert!(got.is_none());
        assert_eq!(
            lines,
            vec![format!(
                "ERROR quilltap::db Error finding API key by ID collection=connection_profiles keyId=k-bad {blob}"
            )]
        );
        let (got, lines) =
            crate::test_support::captured_with(|| read_api_key_scoped(&conn, "k-bad", "u-1"));
        assert!(got.is_none());
        assert_eq!(
            lines,
            vec![format!(
                "ERROR quilltap::db Error finding API key by ID and user ID collection=connection_profiles keyId=k-bad userId=u-1 {blob}"
            )]
        );

        let (got, lines) = crate::test_support::captured_with(|| {
            (
                read_api_key(&conn, "k-ok").map(|k| k.key_value),
                read_api_key(&conn, "k-missing"),
                read_api_key_scoped(&conn, "k-ok", "u-1").map(|k| k.key_value),
                read_api_key_scoped(&conn, "k-ok", "u-2"),
                read_api_key_scoped(&conn, "k-missing", "u-1"),
            )
        });
        assert_eq!(got.0.as_deref(), Some("synthetic-k-ok"));
        assert!(got.1.is_none());
        assert_eq!(got.2.as_deref(), Some("synthetic-k-ok"));
        assert!(got.3.is_none(), "a foreign owner is a scoped miss");
        assert!(got.4.is_none());
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// P4.139: over the read POOL, a failed checkout lands inside the home's
    /// wrap — one line (the `MainReads` divergence: v4 logs `Failed to get
    /// API keys collection` first).
    #[test]
    fn a_failed_pool_is_the_helpers_line() {
        let (_dir, db) = db_with_a_failing_read_pool();
        let (got, lines) = crate::test_support::captured_with(|| read_api_key(&db, "k-1"));
        assert!(got.is_none());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "ERROR quilltap::db Error finding API key by ID collection=connection_profiles keyId=k-1 error="
            ),
            "{}",
            lines[0]
        );
    }

    /// P4.139: the generators' key idiom (external prompt, optimizer, wizard
    /// ×2, AI import) — a corrupt row is the SCOPED line and `''`, and the
    /// generation proceeds; a healthy row is its key; no / an empty `apiKeyId`
    /// reads nothing; a foreign owner is a silent `''`.
    #[test]
    fn the_generators_key_idiom_is_v4s_scoped_fallback() {
        use crate::db::fallback::test_plants::{conn_with_api_keys, plant_api_key};
        use serde_json::json;
        let conn = conn_with_api_keys();
        plant_api_key(&conn, "k-bad", "u-1", true);
        plant_api_key(&conn, "k-ok", "u-1", false);
        let (got, lines) = crate::test_support::captured_with(|| {
            profile_api_key_value_scoped(&conn, &json!({ "apiKeyId": "k-bad" }), "u-1")
        });
        assert_eq!(got, "");
        assert_eq!(lines, vec![test_instance::scoped_line("k-bad", "u-1")]);
        let (got, lines) = crate::test_support::captured_with(|| {
            (
                profile_api_key_value_scoped(&conn, &json!({ "apiKeyId": "k-ok" }), "u-1"),
                profile_api_key_value_scoped(&conn, &json!({ "apiKeyId": "k-ok" }), "u-2"),
                profile_api_key_value_scoped(&conn, &json!({}), "u-1"),
                profile_api_key_value_scoped(&conn, &json!({ "apiKeyId": "" }), "u-1"),
                profile_api_key_value_scoped(&conn, &json!({ "apiKeyId": null }), "u-1"),
            )
        });
        assert_eq!(
            got,
            (
                "synthetic-k-ok".to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new()
            )
        );
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// P4.139: `chat_enrichment::get_connection_profile` (the chat GET's
    /// profile summary) reads the key UNSCOPED through the fallback: a corrupt
    /// row is the line and `apiKey: null`, the profile still returned (v4
    /// `chat-enrichment.service.ts:392-397`). The test lives here because the
    /// lane owns only that file's key-read hunk.
    #[test]
    fn the_chat_enrichments_profile_summary_nulls_a_corrupt_key() {
        use test_instance::*;
        let (_dir, db) = provisioned(|c| {
            plant_connection_profile(c, "cp-bad", "OPENAI", Some(BAD_KEY));
            plant_connection_profile(c, "cp-ok", "OPENAI", Some(OK_KEY));
        });
        let (got, lines) = crate::test_support::captured_with(|| {
            db.read_main(|c| crate::services::chat_enrichment::get_connection_profile(c, "cp-bad"))
        });
        let got = got.unwrap().expect("the profile is still returned");
        assert_eq!(got.id, "cp-bad");
        assert!(got.api_key.is_none());
        assert_eq!(db_lines(&lines), vec![unscoped_line(BAD_KEY)]);
        let (got, lines) = crate::test_support::captured_with(|| {
            db.read_main(|c| crate::services::chat_enrichment::get_connection_profile(c, "cp-ok"))
        });
        assert_eq!(
            got.unwrap().unwrap().api_key.map(|k| k.id).as_deref(),
            Some(OK_KEY)
        );
        assert!(db_lines(&lines).is_empty(), "{lines:?}");
    }

    /// P4.139: the Scenario Builder prepare's profile read is v4's fallback
    /// `findById` — an unreadable (BLOB-named) profile logs `Error finding
    /// entity by ID {collection: 'connection_profiles', id, error}` and is the
    /// route's 404 (+ its DEBUG); a healthy foreign-less profile reads silent.
    /// (`scenario_builder_routes_equivalence` is the two-sided proof; this
    /// pins v5's `error` bytes on the test thread — the prepare is sync.)
    #[test]
    fn the_scenario_builder_prepares_unreadable_profile_is_the_line_and_the_404() {
        use test_instance::*;
        const BLOB_NAMED: &str = "a2000139-0000-4000-8000-0000000000b0";
        const HEALTHY: &str = "a2000139-0000-4000-8000-0000000000b1";
        let (_dir, db) = provisioned(|c| {
            plant_connection_profile(c, HEALTHY, "OLLAMA", None);
            c.execute(
                "INSERT INTO connection_profiles (id, userId, name, provider, modelName, \
                 createdAt, updatedAt) VALUES (?1, ?2, x'00', 'OLLAMA', 'm', ?3, ?3)",
                rusqlite::params![BLOB_NAMED, USER, NOW],
            )
            .unwrap();
        });
        let body = |pid: &str| {
            serde_json::json!({
                "mode": "in-world", "location": "The quay", "time": "dusk",
                "connectionProfileId": pid,
            })
        };
        let (got, lines) = crate::test_support::captured_with(|| {
            crate::api::scenario_builder::scenario_builder_prepare(&db, USER, &body(BLOB_NAMED))
        });
        match got {
            Err(crate::api::types::Response::Error(e)) => {
                assert_eq!(e.kind, crate::api::types::ErrorKind::NotFound);
                assert_eq!(e.message, "Connection profile not found");
            }
            other => panic!("expected the 404, got {other:?}"),
        }
        assert_eq!(
            db_lines(&lines),
            vec![format!(
                "ERROR quilltap::db Error finding entity by ID collection=connection_profiles id={BLOB_NAMED} error=Invalid column type Blob at index: 2, name: name"
            )]
        );
        let (got, lines) = crate::test_support::captured_with(|| {
            crate::api::scenario_builder::scenario_builder_prepare(&db, USER, &body(HEALTHY))
        });
        assert!(got.is_ok(), "the healthy profile prepares: {:?}", got.err());
        assert!(db_lines(&lines).is_empty(), "{lines:?}");
    }

    /// P4.139 (Tier 2 item 10): the lane-local by-user-id helper carries v4's
    /// exact bytes — a failed STATEMENT is `Error finding API keys by user ID
    /// {collection, userId, error}` and `[]`; a healthy list is silent. (On the
    /// union it folds onto `db::fallback::find_api_keys_by_user_id_or_empty`,
    /// §S.1, and this pin keeps holding.)
    #[test]
    fn the_by_user_id_helper_logs_v4s_line_and_answers_empty() {
        let (dir, db) = db_with_a_failing_read_pool();
        let (got, lines) =
            crate::test_support::captured_with(|| api_keys_by_user_id_or_empty(&db, "u-1"));
        assert!(got.is_empty());
        assert_eq!(
            lines,
            vec![format!(
                "ERROR quilltap::db Error finding API keys by user ID collection=connection_profiles userId=u-1 error=unable to open database file: {}",
                dir.path().join("main.db").display()
            )]
        );
        let conn = crate::db::fallback::test_plants::conn_with_api_keys();
        crate::db::fallback::test_plants::plant_api_key(&conn, "k-ok", "u-1", false);
        let (got, lines) =
            crate::test_support::captured_with(|| api_keys_by_user_id_or_empty(&conn, "u-1"));
        assert_eq!(
            got.into_iter().map(|k| k.id).collect::<Vec<_>>(),
            vec!["k-ok"]
        );
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// P4.139: the web search's key pick at v4's fallback — a bad row is
    /// dropped with its WARN and the OTHER key still found (the propagating
    /// original found nothing); a failed read is the line and `None`.
    #[test]
    fn the_search_key_pick_survives_a_bad_row_and_a_failed_read() {
        let conn = crate::db::fallback::test_plants::conn_with_api_keys();
        crate::db::fallback::test_plants::plant_api_key(&conn, "k-bad", "u-1", true);
        crate::db::fallback::test_plants::plant_api_key(&conn, "k-ok", "u-1", false);
        let (got, lines) = crate::test_support::captured_with(|| {
            find_active_api_key_for_provider_or_none(&conn, "u-1", "OPENAI")
        });
        assert_eq!(got.map(|k| k.id).as_deref(), Some("k-ok"));
        assert_eq!(
            lines,
            vec!["WARN quilltap::db API key validation failed keyId=k-bad userId=u-1 error=Invalid column type Blob at index: 4, name: key_value".to_string()]
        );
        let (_dir, db) = db_with_a_failing_read_pool();
        let (got, lines) = crate::test_support::captured_with(|| {
            find_active_api_key_for_provider_or_none(&db, "u-1", "OPENAI")
        });
        assert!(got.is_none());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with("ERROR quilltap::db Error finding API keys by user ID "));
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
        assert_eq!(get_api_key_for_cheap_llm_selection(&conn, &sel, "u"), None);
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

    /// **The spine half of v4 bug 81, now that v5 runs v4's gate** (P4.133,
    /// dogfood #133; this REPOINTS `provider_scan_is_capability_blind`, which
    /// pinned the old host provider scan as the reason v5 never had the bug —
    /// the scan no longer decides any model call's key).
    ///
    /// The Salon turn now asks v4's two questions over the participant's raw
    /// key (`orchestrator.service.ts:425-439`): an OpenAI-Compatible profile's
    /// key is FORWARDED (accepts, does not require — the bug-81 fix), an
    /// Ollama profile's stored key is DROPPED (accepts nothing), and a hosted
    /// provider with no key refuses. The turn-level arms are
    /// `orchestrator_tier3`'s `keyless_oac_sends_empty` /
    /// `keyless_requires_refuses` / `dangling_key_refuses`; this pins the two
    /// predicate answers the gate reads.
    #[test]
    fn the_salon_gate_forwards_an_oac_key_and_drops_an_ollama_one() {
        assert!(provider_accepts_api_key("OPENAI_COMPATIBLE"));
        assert!(!provider_requires_api_key("OPENAI_COMPATIBLE"));
        assert!(!provider_accepts_api_key("OLLAMA"));
        assert!(!provider_requires_api_key("OLLAMA"));
        assert!(provider_requires_api_key("ANTHROPIC"));
    }
}
