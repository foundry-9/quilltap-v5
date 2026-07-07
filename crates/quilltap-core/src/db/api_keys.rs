//! The `api_keys` repository — the **last unported repo** (W4.7d).
//!
//! v4 hosts this collection INSIDE `ConnectionProfilesRepository`
//! (`getApiKeysCollection()` → `ensureCollection('api_keys', ApiKeySchema)`); it
//! does NOT ride the base-repository `_create`/`_update`/`_delete`, so the
//! marshaling boundary is the table itself and this port lives in its own
//! `db::api_keys` module (a plain main-DB table).
//!
//! ## Plaintext — the DB cipher is the only protection
//!
//! `key_value` holds the **raw** secret. The old AES-GCM columns were dropped by
//! migration (`decrypt-api-key-values.ts` is history, not live code); there is no
//! read-time decryption anywhere. Every differential fixture / canned seam uses
//! **synthetic keys only** — a real key must never enter a fixture, a dump, a log,
//! or a recorded envelope.
//!
//! ## Schema (DDL transcribed verbatim; the schema never changes mid-port)
//!
//! ```sql
//! CREATE TABLE "api_keys" (
//!   "id" TEXT PRIMARY KEY,
//!   "userId" TEXT NOT NULL,
//!   "label" TEXT NOT NULL,
//!   "provider" TEXT NOT NULL,     -- ⚠ ProviderEnum = z.string().min(1): FREE-FORM
//!   "key_value" TEXT NOT NULL,
//!   "isActive" INTEGER DEFAULT 1, -- boolean, Zod default true
//!   "lastUsed" TEXT,              -- nullable-optional ISO timestamp
//!   "createdAt" TEXT NOT NULL,
//!   "updatedAt" TEXT NOT NULL
//! );
//! ```
//!
//! `provider` is a **free-form string**, not an enum (`ProviderEnum =
//! z.string().min(1)`); matching everywhere is exact string equality.
//!
//! ## Methods (v4 `connection-profiles.repository.ts`, :205–403)
//!
//!   - [`get_api_keys_by_user_id`] — ⚠ per-row `safeParse` that **DROPS** invalid
//!     rows (and v4's `safeQuery` returns `[]` on any error). The corpus exercises
//!     the drop with a raw row whose `provider` is empty (`z.string().min(1)`
//!     fails). Full Zod validation (UUID/ISO formats) is a documented seam — the
//!     drop is keyed on the exercised invalidity (empty `provider`).
//!   - [`find_by_id`] — **UNSCOPED** (no ownership check). v4 wraps
//!     `ApiKeySchema.parse` in `safeQuery` (a parse throw → `null`); this port
//!     marshals the row directly (the corpus never reads a malformed row by id).
//!   - [`find_by_id_and_user_id`] — the primary resolver (`findOne({id,userId})`).
//!   - [`ApiKeysRepository::create`] — mints id + timestamps (no `CreateOptions`;
//!     v4 `createApiKey` takes none), so the differential is the minted-values
//!     remap form.
//!   - [`ApiKeysRepository::update`] — read-modify-write (`{...existing, ...data,
//!     id, createdAt, updatedAt: now}` → full `$set`), preserving `id`/`createdAt`,
//!     minting `updatedAt`.
//!   - [`ApiKeysRepository::delete`].
//!   - [`ApiKeysRepository::record_usage`] — sets `lastUsed` THROUGH `update`, so
//!     it also bumps `updatedAt` (two `getCurrentTimestamp()` calls, as in v4).
//!
//! The user-scoped wrapper semantics (`UserScopedConnectionsRepository`) live in
//! [`crate::services::api_key_service`] (the service-facing functions).

use rusqlite::{params, Connection};

use super::DbError;
use crate::clock::now_iso;

/// A hydrated `api_keys` row (v4 `ApiKey`). `is_active` is the INTEGER 0/1 column
/// coerced to bool; `last_used` is the nullable timestamp.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiKey {
    pub id: String,
    pub user_id: String,
    pub label: String,
    /// Free-form provider string (v4 `ProviderEnum = z.string().min(1)`).
    pub provider: String,
    /// The raw secret (plaintext — SYNTHETIC in every fixture/seam).
    pub key_value: String,
    pub is_active: bool,
    pub last_used: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// The create shape (v4 `Omit<ApiKey, 'id'|'createdAt'|'updatedAt'>`). `is_active`
/// is `Option` so an absent value takes the Zod default (`true`); `last_used` is
/// the nullable-optional timestamp (absent → SQL NULL).
pub struct AkCreate {
    pub user_id: String,
    pub label: String,
    pub provider: String,
    pub key_value: String,
    /// `None` → Zod default `true`.
    pub is_active: Option<bool>,
    pub last_used: Option<String>,
}

/// An update patch (v4 `Partial<ApiKey>` after the user-scoped `userId` strip). A
/// `Some` field overwrites; `last_used` is a double-`Option` (outer `None` = not
/// in the patch → keep existing; `Some(inner)` = set to `inner`, incl. NULL).
#[derive(Default)]
pub struct AkUpdate {
    pub label: Option<String>,
    pub provider: Option<String>,
    pub key_value: Option<String>,
    pub is_active: Option<bool>,
    pub last_used: Option<Option<String>>,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct ApiKeysRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ApiKeysRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// v4 `createApiKey` — mint id + `createdAt`/`updatedAt` (both `now`), default
    /// `isActive` to `true` when absent, insert, and return the created row.
    pub fn create(&self, data: &AkCreate) -> Result<ApiKey, DbError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_iso();
        let is_active = data.is_active.unwrap_or(true);

        self.conn.execute(
            "INSERT INTO api_keys \
               (id, userId, label, provider, key_value, isActive, lastUsed, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                data.user_id,
                data.label,
                data.provider,
                data.key_value,
                i64::from(is_active),
                data.last_used,
                now,
                now,
            ],
        )?;

        Ok(ApiKey {
            id,
            user_id: data.user_id.clone(),
            label: data.label.clone(),
            provider: data.provider.clone(),
            key_value: data.key_value.clone(),
            is_active,
            last_used: data.last_used.clone(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    /// v4 `updateApiKey` — read existing (`None` → not found), merge the patch,
    /// preserve `id`/`createdAt`, mint `updatedAt`, and write the full row (v4's
    /// `$set: validated`). Returns the merged row, or `None` when no row matched.
    pub fn update(&self, id: &str, patch: &AkUpdate) -> Result<Option<ApiKey>, DbError> {
        let Some(existing) = find_by_id(self.conn, id)? else {
            return Ok(None);
        };

        let merged = ApiKey {
            id: existing.id.clone(),
            user_id: existing.user_id.clone(),
            label: patch.label.clone().unwrap_or(existing.label),
            provider: patch.provider.clone().unwrap_or(existing.provider),
            key_value: patch.key_value.clone().unwrap_or(existing.key_value),
            is_active: patch.is_active.unwrap_or(existing.is_active),
            last_used: match &patch.last_used {
                Some(v) => v.clone(),
                None => existing.last_used,
            },
            created_at: existing.created_at,
            updated_at: now_iso(),
        };

        self.conn.execute(
            "UPDATE api_keys SET \
               userId = ?1, label = ?2, provider = ?3, key_value = ?4, isActive = ?5, \
               lastUsed = ?6, createdAt = ?7, updatedAt = ?8 \
             WHERE id = ?9",
            params![
                merged.user_id,
                merged.label,
                merged.provider,
                merged.key_value,
                i64::from(merged.is_active),
                merged.last_used,
                merged.created_at,
                merged.updated_at,
                merged.id,
            ],
        )?;

        Ok(Some(merged))
    }

    /// v4 `deleteApiKey` — `deletedCount === 0 -> false`.
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM api_keys WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// v4 `recordApiKeyUsage` — set `lastUsed = now` THROUGH `update`, so
    /// `updatedAt` is bumped too (v4 mints one `now` for `lastUsed` here and a
    /// second `now` for `updatedAt` inside `update`; reproduced with two
    /// [`now_iso`] calls). Returns `None` when no row matched.
    pub fn record_usage(&self, id: &str) -> Result<Option<ApiKey>, DbError> {
        self.update(
            id,
            &AkUpdate {
                last_used: Some(Some(now_iso())),
                ..Default::default()
            },
        )
    }
}

/// The shared column list every `api_keys` read selects (marshal order).
const AK_COLUMNS: &str =
    "id, userId, label, provider, key_value, isActive, lastUsed, createdAt, updatedAt";

fn marshal_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ApiKey> {
    Ok(ApiKey {
        id: r.get(0)?,
        user_id: r.get(1)?,
        label: r.get(2)?,
        provider: r.get(3)?,
        key_value: r.get(4)?,
        is_active: r.get::<_, i64>(5)? != 0,
        last_used: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

/// v4 `findApiKeyById` — **UNSCOPED** by id. `None` when no row exists. Marshals
/// the row directly; v4's `ApiKeySchema.parse`-throw → `null` on a malformed row
/// is a seam (the corpus never reads a malformed row by id).
pub fn find_by_id(conn: &Connection, id: &str) -> Result<Option<ApiKey>, DbError> {
    conn.query_row(
        &format!("SELECT {AK_COLUMNS} FROM api_keys WHERE id = ?1"),
        params![id],
        marshal_row,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
}

/// v4 `findApiKeyByIdAndUserId` — the primary resolver (`findOne({id, userId})`).
/// `None` when no matching row exists (wrong owner → `None`).
pub fn find_by_id_and_user_id(
    conn: &Connection,
    id: &str,
    user_id: &str,
) -> Result<Option<ApiKey>, DbError> {
    conn.query_row(
        &format!("SELECT {AK_COLUMNS} FROM api_keys WHERE id = ?1 AND userId = ?2"),
        params![id, user_id],
        marshal_row,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
}

/// v4 `getApiKeysByUserId` — all keys for a user, in insertion (rowid) order,
/// **dropping** rows that fail `ApiKeySchema.safeParse` (v4 logs + filters).
///
/// The exercised invalidity is an empty `provider` (`ProviderEnum =
/// z.string().min(1)`); the drop is keyed on that. Full Zod validation
/// (UUID/ISO-datetime formats of the other fields) is a documented seam — no
/// corpus row exercises it. v4's `safeQuery` fallback (`[]` on a query error)
/// maps to the `Err` propagating here; a corrupt table is not in scope.
pub fn get_api_keys_by_user_id(conn: &Connection, user_id: &str) -> Result<Vec<ApiKey>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {AK_COLUMNS} FROM api_keys WHERE userId = ?1"
    ))?;
    let rows = stmt.query_map(params![user_id], marshal_row)?;
    let mut out = Vec::new();
    for r in rows {
        let key = r?;
        // Per-row safeParse drop (exercised invalidity: empty provider).
        if key.provider.is_empty() {
            continue;
        }
        out.push(key);
    }
    Ok(out)
}
