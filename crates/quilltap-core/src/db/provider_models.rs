//! The provider-models repository — a Phase-2 repo port, after `folders`,
//! `tags`, `text_replacement_rules`, `prompt_templates`, and
//! `conversation_annotations`. Ports
//! v4's `lib/database/repositories/provider-models.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete` (the abstract methods over the base
//! repo), plus `upsertModel` / `upsertModelsForProvider` and the private
//! `findByProviderAndModelId` lookup they ride on (the minted-values / remap
//! path — see [`ProviderModelsRepository::upsert_model`]). The remaining custom
//! helpers — `findBy*`/`deleteByProvider` — are out of scope. `provider_models`
//! is a
//! system-wide collection (no `userId` scoping) cataloguing the models available
//! per provider. It widens the tier-2 marshaling surface past
//! `text_replacement_rules`/`prompt_templates` with:
//!
//!   - **two nullable REAL number columns** (`contextWindow`, `maxOutputTokens`)
//!     — both are bare `z.number().nullable().optional()` with NO min/max, so
//!     v4's schema translator (`mapToSQLiteType`) maps them to **REAL** (INTEGER
//!     affinity requires both an integer min AND max). They are bound as
//!     `Option<f64>`: `None` → SQL NULL, `Some(n)` → an 8-byte float. An
//!     integer-valued REAL (e.g. `128000.0`) renders back as `128000` in the
//!     canonical dump via [`super::js_number_to_json`], matching v4's
//!     better-sqlite3 → `JSON.stringify` path byte-for-byte.
//!   - **two boolean-default columns** (`deprecated`, `experimental`) → INTEGER
//!     0/1, the same boolean→0/1 mapping `tags.quickHide` / the TRR booleans
//!     used (`i64::from(bool)`).
//!   - **enum TEXT columns** (`provider`, `modelType`) — stored as plain text;
//!     v4's `ProviderEnum` is `z.string().min(1)` and `ModelTypeEnum` is
//!     `z.enum(['chat','image','embedding'])`, both lowering to TEXT.
//!
//! There is no read-then-guard behavior here (no conflict check, no built-in
//! guard): the three abstract ops delegate straight to the base repo's
//! `_create`/`_update`/`_delete`. To avoid relying on Zod defaults (`modelType`
//! defaults to `'chat'`, `deprecated`/`experimental` to `false`), the tier-2
//! corpus supplies every column explicitly on create.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in each update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the form
//! `folders`/`tags`/`text_replacement_rules`/`prompt_templates` use.
//!
//! Deferred (not in the corpus): setting a nullable column **to NULL** via
//! `update` (the patch models a provided field as "set to this value"; clearing
//! a column to NULL lands when an op needs it), and Zod's create-time defaults.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;
use crate::clock::now_iso;

/// Fields for creating a provider model (the `Omit<ProviderModel,'id'|
/// timestamps>` shape). `base_url` is the one nullable string column;
/// `context_window`/`max_output_tokens` are the nullable REAL columns;
/// `deprecated`/`experimental` are the boolean columns.
pub struct PmCreate {
    pub provider: String,
    pub model_id: String,
    pub model_type: String,
    pub display_name: String,
    /// `None` => SQL NULL (custom-endpoint base URL).
    pub base_url: Option<String>,
    /// `None` => SQL NULL; `Some` => an 8-byte REAL (integer-valued collapses in
    /// the dump).
    pub context_window: Option<f64>,
    /// `None` => SQL NULL; `Some` => an 8-byte REAL.
    pub max_output_tokens: Option<f64>,
    pub deprecated: bool,
    pub experimental: bool,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A provider-model update patch. Mirrors v4 `update` over `_update`: provided
/// fields overwrite, id and createdAt are preserved, `updatedAt` is set
/// explicitly. Each `Some` field sets that column; clearing a nullable column to
/// NULL is deferred (not in the corpus).
#[derive(Default)]
pub struct PmUpdate {
    pub display_name: Option<String>,
    pub context_window: Option<f64>,
    pub max_output_tokens: Option<f64>,
    pub deprecated: Option<bool>,
    pub experimental: Option<bool>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct ProviderModelsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> ProviderModelsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a provider model with the given pinned id + timestamps. All 12
    /// columns are written explicitly; the REAL columns bind `Option<f64>`, the
    /// boolean columns bind `i64::from(bool)`.
    pub fn create(&self, data: &PmCreate, opts: &CreateOptions) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO provider_models \
               (id, provider, modelId, modelType, displayName, baseUrl, contextWindow, \
                maxOutputTokens, deprecated, experimental, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                opts.id,
                data.provider,
                data.model_id,
                data.model_type,
                data.display_name,
                data.base_url,
                data.context_window,
                data.max_output_tokens,
                i64::from(data.deprecated),
                i64::from(data.experimental),
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the model `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id and createdAt are never touched.
    pub fn update(&self, id: &str, patch: &PmUpdate) -> Result<bool, DbError> {
        // v4 `_findById`: the row must exist or the update is a no-op (-> null).
        if !self.row_exists(id)? {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(display_name) = &patch.display_name {
            assignments.push(format!("displayName = ?{}", values.len() + 1));
            values.push(Box::new(display_name.clone()));
        }
        if let Some(context_window) = patch.context_window {
            assignments.push(format!("contextWindow = ?{}", values.len() + 1));
            values.push(Box::new(context_window));
        }
        if let Some(max_output_tokens) = patch.max_output_tokens {
            assignments.push(format!("maxOutputTokens = ?{}", values.len() + 1));
            values.push(Box::new(max_output_tokens));
        }
        if let Some(deprecated) = patch.deprecated {
            assignments.push(format!("deprecated = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(deprecated)));
        }
        if let Some(experimental) = patch.experimental {
            assignments.push(format!("experimental = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(experimental)));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE provider_models SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the model `id`. Returns `Ok(false)` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM provider_models WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// Upsert a provider model — v4 `upsertModel`: find an existing row by
    /// `(provider, modelId, modelType ?? 'chat', baseUrl ?? undefined)` via
    /// [`Self::find_by_provider_and_model_id`]; if found, overwrite it with the
    /// FULL `data` (id + createdAt preserved, updatedAt minted to `now`); else
    /// create a fresh row (minting id + createdAt + updatedAt all = `now`).
    /// Returns the id of the affected row.
    ///
    /// The minted-values path: nothing is pinned, so the caller normalizes the
    /// id + timestamps in the differential (the remap form). v4 derives the
    /// match `modelType` from `data.model_type ?? 'chat'` — but `model_type` is a
    /// required `String` on [`PmCreate`] here (the corpus always supplies it), so
    /// the `?? 'chat'` default never engages; we pass it through verbatim.
    pub fn upsert_model(&self, data: &PmCreate) -> Result<String, DbError> {
        let now = now_iso();

        // v4: `data.baseUrl ?? undefined` — only a TRUTHY baseUrl constrains the
        // find (matching v4's `if (baseUrl) query.baseUrl = baseUrl`). An empty
        // string is falsy in v4, so it would NOT constrain; mirror that.
        let base_url_filter = match &data.base_url {
            Some(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };

        let existing = self.find_by_provider_and_model_id(
            &data.provider,
            &data.model_id,
            &data.model_type,
            base_url_filter.as_deref(),
        )?;

        match existing {
            Some(id) => {
                // v4 `update(existing.id, data)` -> `_update`: the FULL `data`
                // overwrites every column, id + createdAt are preserved, and
                // updatedAt is minted (`data` carries no `updatedAt`).
                self.conn.execute(
                    "UPDATE provider_models SET \
                       provider = ?1, modelId = ?2, modelType = ?3, displayName = ?4, \
                       baseUrl = ?5, contextWindow = ?6, maxOutputTokens = ?7, \
                       deprecated = ?8, experimental = ?9, updatedAt = ?10 \
                     WHERE id = ?11",
                    params![
                        data.provider,
                        data.model_id,
                        data.model_type,
                        data.display_name,
                        data.base_url,
                        data.context_window,
                        data.max_output_tokens,
                        i64::from(data.deprecated),
                        i64::from(data.experimental),
                        now,
                        id,
                    ],
                )?;
                Ok(id)
            }
            None => {
                // v4 `create(data)` (no options) -> mint id + createdAt + updatedAt.
                let id = uuid::Uuid::new_v4().to_string();
                self.create(
                    data,
                    &CreateOptions {
                        id: id.clone(),
                        created_at: now.clone(),
                        updated_at: now,
                    },
                )?;
                Ok(id)
            }
        }
    }

    /// Bulk upsert — v4 `upsertModelsForProvider`: a thin loop over
    /// [`Self::upsert_model`], one row per element. Returns the ids of the
    /// affected rows in input order. (v4 returns `{created, updated}` counts and
    /// swallows per-row errors; the port keeps the loop thin and propagates
    /// errors — the differential drives it through `upsert_model`, so the count
    /// bookkeeping is out of scope.)
    pub fn upsert_model_for_provider(&self, models: &[PmCreate]) -> Result<Vec<String>, DbError> {
        let mut ids = Vec::with_capacity(models.len());
        for data in models {
            ids.push(self.upsert_model(data)?);
        }
        Ok(ids)
    }

    /// v4 `findByProviderAndModelId`: the existing-row lookup behind `upsertModel`.
    /// Always constrains `provider = ? AND modelId = ? AND modelType = ?`; adds
    /// `AND baseUrl = ?` ONLY when `base_url` is `Some` (v4: `if (baseUrl)
    /// query.baseUrl = baseUrl`, so a null/undefined/empty baseUrl leaves the
    /// column UNCONSTRAINED — it does NOT mean "match a NULL baseUrl"). No ORDER
    /// BY (v4's `findOne` issues none), so a `LIMIT 1` over an unconstrained
    /// baseUrl returns the first row in rowid order. Returns the matched id.
    fn find_by_provider_and_model_id(
        &self,
        provider: &str,
        model_id: &str,
        model_type: &str,
        base_url: Option<&str>,
    ) -> Result<Option<String>, DbError> {
        let found: Result<String, rusqlite::Error> = match base_url {
            Some(url) => self.conn.query_row(
                "SELECT id FROM provider_models \
                 WHERE provider = ?1 AND modelId = ?2 AND modelType = ?3 AND baseUrl = ?4 \
                 LIMIT 1",
                params![provider, model_id, model_type, url],
                |row| row.get::<_, String>(0),
            ),
            None => self.conn.query_row(
                "SELECT id FROM provider_models \
                 WHERE provider = ?1 AND modelId = ?2 AND modelType = ?3 \
                 LIMIT 1",
                params![provider, model_id, model_type],
                |row| row.get::<_, String>(0),
            ),
        };
        match found {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(other) => Err(other.into()),
        }
    }

    /// True iff a row with this id exists — v4's `_update` `findById` precondition
    /// (a missing target makes the update a no-op returning `null`).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM provider_models WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(found.is_some())
    }
}
