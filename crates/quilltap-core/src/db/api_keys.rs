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
//!   "isActive" INTEGER DEFAULT 1, -- boolean, Zod default true (decoded as v4's hydrate)
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
//!   - [`get_api_keys_by_user_id`] — ⚠ per-row `safeParse` that **DROPS** an
//!     invalid row with v4's WARN `API key validation failed` and lists the rest
//!     (P4.139: a row the marshal refuses — a BLOB `key_value` — is dropped the
//!     same way; it had failed the WHOLE list). The statement's own failure still
//!     propagates: v4's `safeQuery` fallback (`[]` + `Error finding API keys by
//!     user ID`) is the CALLER's home, not this fn's. Full Zod validation
//!     (UUID/ISO formats) is a documented seam — the drop is keyed on the
//!     exercised invalidities (an empty `provider`, a cell the marshal refuses).
//!   - [`find_by_id`] — **UNSCOPED** (no ownership check). v4 wraps
//!     `ApiKeySchema.parse` in a fallback `safeQuery` (a parse throw → its line +
//!     `null`); this port's marshal failure is the `Err` every caller hands to
//!     `db::fallback`'s API-key homes (P4.136 / P4.139), which log that line.
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

/// A hydrated `api_keys` row (v4 `ApiKey`). `is_active` is the `isActive` cell
/// decoded as v4's SQLite hydrate decodes it (see [`is_active_cell`]);
/// `last_used` is the nullable timestamp.
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

/// The `isActive` cell as v4 reads it (P4.139). v4's backend hydrates every
/// `is*` column through its boolean branch (`lib/database/backends/sqlite/
/// backend.ts:412-450`): NULL → `undefined`, which `ApiKeySchema`'s
/// `isActive: z.boolean().default(true)` (`lib/schemas/profile.types.ts:30`)
/// turns into `true`; a number → `value === 1`; anything else →
/// `Boolean(value)` (a non-empty string and any Buffer are truthy). So a NULL,
/// a text or a BLOB cell is a key v4 reads and USES, and `2` is an INACTIVE
/// one — v5's old `i64 != 0` had refused the first three (the whole read an
/// `Err`) and read `2` as active. v4 writes only booleans (0/1); the other
/// cells are a foreign or hand-edited row's.
fn is_active_cell(cell: rusqlite::types::ValueRef<'_>) -> bool {
    use rusqlite::types::ValueRef;
    match cell {
        ValueRef::Null => true,
        ValueRef::Integer(n) => n == 1,
        ValueRef::Real(f) => f == 1.0,
        ValueRef::Text(t) => !t.is_empty(),
        ValueRef::Blob(_) => true,
    }
}

fn marshal_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ApiKey> {
    Ok(ApiKey {
        id: r.get(0)?,
        user_id: r.get(1)?,
        label: r.get(2)?,
        provider: r.get(3)?,
        key_value: r.get(4)?,
        is_active: is_active_cell(r.get_ref(5)?),
        last_used: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

/// v4 `findApiKeyById` — **UNSCOPED** by id. `None` when no row exists. A row
/// the marshal refuses is an `Err` — v4's `ApiKeySchema.parse` throw inside its
/// fallback `safeQuery`; every caller hands it to `db::fallback::find_api_key_
/// by_id_or_none` (or `services::api_key_service::read_api_key`), which logs
/// v4's line and answers `None`.
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
/// **dropping** rows that fail `ApiKeySchema.safeParse`, each with v4's WARN
/// (`connection-profiles.repository.ts:224-237`):
/// `logger.warn('API key validation failed', { keyId: (doc as any).id, userId,
/// error: result.error.message })` — a direct WARN, not a `safeQuery` line, so
/// it carries NO `collection` field — and listing the rest.
///
/// Two invalidities are exercised (`api_keys_tier2_equivalence`):
///   - an empty `provider` (`ProviderEnum = z.string().min(1, 'Provider is
///     required')`) — the row marshals, and the WARN's `error` is Zod's own
///     message, byte-exact through [`crate::api::zod_issues`];
///   - a cell the marshal refuses (a BLOB `key_value`, which v4 decodes as
///     Float32 and `z.string()` rejects) — P4.139: this had failed the WHOLE
///     list (`let key = r?`), so one bad row cost a user every key: the
///     api-keys list answered 500, `delete_all` deleted none, the web-search
///     pick found none. The WARN's `error` is rusqlite's sentence where v4's is
///     a ZodError message — the one recorded byte divergence of the drop.
///
/// Full Zod validation (UUID/ISO-datetime formats of the other fields) is a
/// documented seam — no corpus row exercises it. A STATEMENT failure (the
/// prepare, the query) still propagates: v4's `safeQuery` fallback (`[]` +
/// `Error finding API keys by user ID`) is the caller's home.
pub fn get_api_keys_by_user_id(conn: &Connection, user_id: &str) -> Result<Vec<ApiKey>, DbError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {AK_COLUMNS} FROM api_keys WHERE userId = ?1"
    ))?;
    let mut rows = stmt.query(params![user_id])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        match marshal_row(row) {
            // Per-row safeParse drop (exercised invalidity: empty provider).
            Ok(key) if key.provider.is_empty() => {
                let issues = [crate::api::zod_issues::ZodIssue::too_small_string_message(
                    serde_json::json!(1),
                    vec![crate::api::zod_issues::key("provider")],
                    "Provider is required",
                )];
                warn_validation_failed(
                    Some(&key.id),
                    user_id,
                    &crate::api::zod_issues::zod_error_message(&issues),
                );
            }
            Ok(key) => out.push(key),
            Err(e) => {
                // v4's `(doc as any).id` — an unreadable id is `undefined`,
                // which the logger omits.
                let id = match row.get_ref(0) {
                    Ok(rusqlite::types::ValueRef::Text(t)) => {
                        Some(String::from_utf8_lossy(t).into_owned())
                    }
                    _ => None,
                };
                warn_validation_failed(id.as_deref(), user_id, &e.to_string());
            }
        }
    }
    Ok(out)
}

/// The per-row drop's WARN (see [`get_api_keys_by_user_id`]).
fn warn_validation_failed(key_id: Option<&str>, user_id: &str, error: &str) {
    match key_id {
        Some(key_id) => tracing::warn!(
            target: "quilltap::db",
            keyId = %key_id,
            userId = %user_id,
            error = %error,
            "API key validation failed"
        ),
        None => tracing::warn!(
            target: "quilltap::db",
            userId = %user_id,
            error = %error,
            "API key validation failed"
        ),
    }
}

/// v4 `connections.findApiKeyById(id)?.label` — the label alone (the `.qtap`
/// export's `_apiKeyLabel` resolver, P4.9G4). `None` when the key is absent.
pub fn find_label_by_id(conn: &Connection, id: &str) -> Result<Option<String>, DbError> {
    conn.query_row(
        "SELECT label FROM api_keys WHERE id = ?1",
        params![id],
        |row| row.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::fallback::test_plants::{conn_with_api_keys, plant_api_key};

    /// Plant one healthy row (user `u-1`) whose `isActive` cell is the SQL
    /// literal `cell`.
    fn plant_is_active(conn: &Connection, id: &str, cell: &str) {
        conn.execute(
            &format!(
                "INSERT INTO api_keys (id, userId, label, provider, key_value, isActive, \
                 createdAt, updatedAt) VALUES (?1, 'u-1', 'k', 'OPENAI', 'synthetic', {cell}, \
                 '2026-10-01T00:00:00.000Z', '2026-10-01T00:00:00.000Z')"
            ),
            params![id],
        )
        .unwrap();
    }

    /// P4.139: v4's hydrate over the six cells v4 never writes but reads
    /// (`api_keys_tier2_equivalence`'s `readIsActive` rows are the two-sided
    /// proof); 0 and 1 for the rows it does write.
    #[test]
    fn is_active_decodes_every_cell_as_v4s_hydrate() {
        let conn = conn_with_api_keys();
        let cells = [
            ("null", "NULL", true),
            ("zero", "0", false),
            ("one", "1", true),
            ("two", "2", false),
            ("minus-one", "-1", false),
            ("text-x", "'x'", true),
            ("text-empty", "''", false),
            ("real-one-half", "1.5", false),
            ("blob", "x'00'", true),
        ];
        for (id, cell, _) in &cells {
            plant_is_active(&conn, id, cell);
        }
        for (id, cell, want) in cells {
            let key = find_by_id(&conn, id)
                .unwrap_or_else(|e| panic!("{cell}: {e}"))
                .unwrap();
            assert_eq!(key.is_active, want, "isActive = {cell}");
        }
    }

    /// P4.139: a row the marshal refuses is DROPPED with v4's WARN (no
    /// `collection` — a direct `logger.warn`, not a `safeQuery` line) and the
    /// rest listed; an empty `provider` is dropped with Zod's own message.
    #[test]
    fn a_bad_row_is_dropped_with_v4s_warn_and_the_rest_listed() {
        let conn = conn_with_api_keys();
        plant_api_key(&conn, "k-ok", "u-1", false);
        plant_api_key(&conn, "k-bad", "u-1", true);
        conn.execute(
            "INSERT INTO api_keys (id, userId, label, provider, key_value, isActive, \
             createdAt, updatedAt) VALUES ('k-empty', 'u-1', 'k', '', 'synthetic', 1, \
             '2026-10-01T00:00:00.000Z', '2026-10-01T00:00:00.000Z')",
            [],
        )
        .unwrap();
        plant_api_key(&conn, "k-other", "u-2", true);
        let (keys, lines) =
            crate::test_support::captured_with(|| get_api_keys_by_user_id(&conn, "u-1"));
        let ids: Vec<String> = keys.unwrap().into_iter().map(|k| k.id).collect();
        assert_eq!(ids, vec!["k-ok".to_string()]);
        assert_eq!(
            lines,
            vec![
                "WARN quilltap::db API key validation failed keyId=k-bad userId=u-1 error=Invalid column type Blob at index: 4, name: key_value".to_string(),
                "WARN quilltap::db API key validation failed keyId=k-empty userId=u-1 error=[\n  {\n    \"origin\": \"string\",\n    \"code\": \"too_small\",\n    \"minimum\": 1,\n    \"inclusive\": true,\n    \"path\": [\n      \"provider\"\n    ],\n    \"message\": \"Provider is required\"\n  }\n]".to_string(),
            ]
        );
        // A healthy list is silent.
        let (keys, lines) =
            crate::test_support::captured_with(|| get_api_keys_by_user_id(&conn, "u-3"));
        assert!(keys.unwrap().is_empty());
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// An unreadable `id` cell is v4's `undefined` — the field is omitted.
    #[test]
    fn an_unreadable_id_omits_the_key_id_field() {
        let conn = conn_with_api_keys();
        conn.execute(
            "INSERT INTO api_keys (id, userId, label, provider, key_value, isActive, \
             createdAt, updatedAt) VALUES (NULL, 'u-1', 'k', 'OPENAI', 'synthetic', 1, \
             '2026-10-01T00:00:00.000Z', '2026-10-01T00:00:00.000Z')",
            [],
        )
        .unwrap();
        let (keys, lines) =
            crate::test_support::captured_with(|| get_api_keys_by_user_id(&conn, "u-1"));
        assert!(keys.unwrap().is_empty());
        assert_eq!(
            lines,
            vec!["WARN quilltap::db API key validation failed userId=u-1 error=Invalid column type Null at index: 0, name: id".to_string()]
        );
    }
}
