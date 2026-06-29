//! The text-replacement-rules repository — the third Phase-2 repo port, after
//! `folders` and `tags`. Ports v4's
//! `lib/database/repositories/text-replacement-rules.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete`. Single-user (no `userId` scoping).
//! This is the first repo with **conflict detection**, so it is also the first
//! to need a repo-level *read*: `create`/`update` query the existing rows and
//! reject a duplicate `(fromText, caseSensitive)` pair before writing. It also
//! widens the tier-2 marshaling surface past `tags` with:
//!
//!   - a real **INTEGER number column** (`sortOrder`) — v4's `prepareForStorage`
//!     passes a JS number straight through, so it lands as a SQLite INTEGER;
//!   - **two boolean columns** (`caseSensitive`, `enabled`) → INTEGER 0/1, the
//!     same boolean→0/1 mapping `tags.quickHide` exercised, but now read back for
//!     the conflict check.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the form `folders`/`tags`
//! use.
//!
//! ## Conflict detection (the new behavior)
//!
//! v4's `assertNoConflict(fromText, caseSensitive, excludeId)` scans every rule
//! and flags a duplicate: a case-sensitive rule duplicates another iff
//! `row.fromText === fromText` (and both are case-sensitive); a case-insensitive
//! rule duplicates iff `row.fromText.toLowerCase() === fromText.toLowerCase()`.
//! The `caseSensitive` flag is part of the key, so the same text under different
//! sensitivities does NOT conflict. `excludeId` skips the row being updated.
//! `create` checks with `excludeId = None`; `update` only re-checks when the
//! next `(fromText, caseSensitive)` differs from the stored pair, with
//! `excludeId = Some(id)`. A conflict surfaces as [`TrrError::Conflict`] — v4
//! throws `TextReplacementRuleConflictError`, which the route maps to HTTP 409.
//!
//! v4 reads via `_findAll()` (full hydrate + Zod-validate each row); this port
//! reads only the three columns the check needs (`id, fromText, caseSensitive`),
//! which is behaviorally identical for the conflict *outcome* on valid data —
//! the dump (raw `SELECT *`) is what the differential compares, and a rejected
//! write leaves the table unchanged on both sides.
//!
//! Unicode seam (TRACKED DEFERRAL — must close before real, non-ASCII data): the
//! case-insensitive branch lowercases with Rust's `str::to_lowercase`; v4 uses JS
//! `String.prototype.toLowerCase`. They agree on ASCII but are not guaranteed
//! identical on locale/special-cased code points — the SAME `toLowerCase`
//! case-mapping seam `tags.nameLower` flagged, and a real correctness risk
//! because it gates duplicate rejection here. The ASCII corpus masks it. See
//! "Deferred seams — must revisit" in docs/developer/porting/phase-2-onramp.md.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a rule (the `Omit<Rule,'id'|timestamps>` shape).
pub struct TrrCreate {
    pub from_text: String,
    pub to_text: String,
    pub case_sensitive: bool,
    pub enabled: bool,
    pub sort_order: i64,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A rule update patch. Mirrors v4 `update` over `_update`: provided fields
/// overwrite, id and createdAt are preserved, `updatedAt` is set explicitly.
/// When `fromText` / `caseSensitive` change the conflict check re-runs.
pub struct TrrUpdate {
    pub from_text: Option<String>,
    pub to_text: Option<String>,
    pub case_sensitive: Option<bool>,
    pub enabled: Option<bool>,
    pub sort_order: Option<i64>,
    pub updated_at: String,
}

/// Error from the text-replacement-rules repo. Distinguishes a duplicate
/// `(fromText, caseSensitive)` conflict (v4's `TextReplacementRuleConflictError`)
/// from an underlying DB error, so the harness can assert the conflict path
/// fires without conflating it with a SQL failure.
#[derive(Debug)]
pub enum TrrError {
    Db(DbError),
    Conflict {
        from_text: String,
        case_sensitive: bool,
    },
}

impl std::fmt::Display for TrrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrrError::Db(e) => write!(f, "{e}"),
            TrrError::Conflict {
                from_text,
                case_sensitive,
            } => write!(
                f,
                "A {} rule for \"{from_text}\" already exists",
                if *case_sensitive {
                    "case-sensitive"
                } else {
                    "case-insensitive"
                }
            ),
        }
    }
}

impl std::error::Error for TrrError {}

impl From<DbError> for TrrError {
    fn from(e: DbError) -> Self {
        TrrError::Db(e)
    }
}

impl From<rusqlite::Error> for TrrError {
    fn from(e: rusqlite::Error) -> Self {
        TrrError::Db(DbError::Sqlite(e))
    }
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct TextReplacementRulesRepository<'c> {
    conn: &'c Connection,
}

impl<'c> TextReplacementRulesRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a rule with the given pinned id + timestamps, after asserting it
    /// does not duplicate an existing `(fromText, caseSensitive)` pair.
    pub fn create(&self, data: &TrrCreate, opts: &CreateOptions) -> Result<(), TrrError> {
        self.assert_no_conflict(&data.from_text, data.case_sensitive, None)?;

        self.conn.execute(
            "INSERT INTO text_replacement_rules \
               (id, fromText, toText, caseSensitive, enabled, sortOrder, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                opts.id,
                data.from_text,
                data.to_text,
                i64::from(data.case_sensitive),
                i64::from(data.enabled),
                data.sort_order,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the rule `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). Re-runs the conflict check only when
    /// the next `(fromText, caseSensitive)` differs from the stored pair. id and
    /// createdAt are never touched.
    pub fn update(&self, id: &str, patch: &TrrUpdate) -> Result<bool, TrrError> {
        // v4 `_findById`: the row must exist or the update is a no-op (-> null).
        let existing: Option<(String, i64)> = self
            .conn
            .query_row(
                "SELECT fromText, caseSensitive FROM text_replacement_rules WHERE id = ?1",
                params![id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let Some((existing_from, existing_cs_int)) = existing else {
            return Ok(false);
        };
        let existing_cs = existing_cs_int != 0;

        // The next pair (v4: `updateData.fromText ?? existing.fromText`, etc.).
        let next_from = patch.from_text.clone().unwrap_or(existing_from.clone());
        let next_cs = patch.case_sensitive.unwrap_or(existing_cs);
        if next_from != existing_from || next_cs != existing_cs {
            self.assert_no_conflict(&next_from, next_cs, Some(id))?;
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(from_text) = &patch.from_text {
            assignments.push(format!("fromText = ?{}", values.len() + 1));
            values.push(Box::new(from_text.clone()));
        }
        if let Some(to_text) = &patch.to_text {
            assignments.push(format!("toText = ?{}", values.len() + 1));
            values.push(Box::new(to_text.clone()));
        }
        if let Some(case_sensitive) = patch.case_sensitive {
            assignments.push(format!("caseSensitive = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(case_sensitive)));
        }
        if let Some(enabled) = patch.enabled {
            assignments.push(format!("enabled = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(enabled)));
        }
        if let Some(sort_order) = patch.sort_order {
            assignments.push(format!("sortOrder = ?{}", values.len() + 1));
            values.push(Box::new(sort_order));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE text_replacement_rules SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the rule `id`. Returns `false` when no row matched (v4's `_delete`
    /// "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, TrrError> {
        let affected = self.conn.execute(
            "DELETE FROM text_replacement_rules WHERE id = ?1",
            params![id],
        )?;
        Ok(affected > 0)
    }

    /// v4's `assertNoConflict`: scan every rule and reject a duplicate
    /// `(fromText, caseSensitive)` pair. `exclude_id` skips the row being
    /// updated. Case-sensitive rules compare `fromText` exactly; case-insensitive
    /// rules compare lowercased (see the Unicode seam note in the module header).
    fn assert_no_conflict(
        &self,
        from_text: &str,
        case_sensitive: bool,
        exclude_id: Option<&str>,
    ) -> Result<(), TrrError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, fromText, caseSensitive FROM text_replacement_rules")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;

        let from_lower = from_text.to_lowercase();
        for row in rows {
            let (row_id, row_from, row_cs_int) = row?;
            if exclude_id == Some(row_id.as_str()) {
                continue;
            }
            if (row_cs_int != 0) != case_sensitive {
                continue;
            }
            let matches = if case_sensitive {
                row_from == from_text
            } else {
                row_from.to_lowercase() == from_lower
            };
            if matches {
                return Err(TrrError::Conflict {
                    from_text: from_text.to_string(),
                    case_sensitive,
                });
            }
        }
        Ok(())
    }
}
