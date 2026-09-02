//! The folders repository — the Phase-2 pilot port of v4's
//! `lib/database/repositories/folders.repository.ts` (+ the `_create`/`_update`
//! internals of `base.repository.ts`).
//!
//! Scope (the on-ramp's "repos directly first"): `create` and `update`. v4's
//! folders repo is pure single-table CRUD — `create`/`update` just wrap
//! `_create`/`_update` with logging — so the resulting row state is what these
//! reproduce. Tier-2 verified against the v4 oracle (`folders-tier2`).
//!
//! Determinism: the pilot pins the id and timestamps (v4 honors
//! `CreateOptions.{id,createdAt,updatedAt}` on create and an explicit
//! `updatedAt` on update), so the persisted rows match v4's byte-for-byte with
//! no normalization.
//!
//! The **unpinned** create path (the normal, non-sync app path) is also ported:
//! when an id / timestamp is not supplied, `_create` mints them
//! (`options?.id || generateId()`, `createdAt/updatedAt || now`). `create`
//! returns the id actually used so a caller can wire it into a dependent op
//! (e.g. a child folder's `parentFolderId`). That path is verified by the
//! tier-2 *remap* case, which normalizes the legitimately-nondeterministic
//! generated ids (first-seen remap) and timestamps (placeholder) on both sides.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use crate::clock::now_iso;

use super::DbError;

/// Fields for creating a folder (the `Omit<FolderInput,'id'|timestamps>` shape).
pub struct FolderCreate {
    pub user_id: String,
    pub path: String,
    pub name: String,
    /// `None` => root level (stored as SQL NULL).
    pub parent_folder_id: Option<String>,
    /// `None` => general files, not in a project (stored as SQL NULL).
    pub project_id: Option<String>,
}

/// Id + timestamps (v4's `CreateOptions`). Each field is optional and mirrors
/// `_create`'s defaults: `id = options?.id || generateId()`,
/// `createdAt = options?.createdAt || now`, `updatedAt = options?.updatedAt ||
/// now`. The tier-2 pilot supplies all three (fully deterministic); the remap
/// case supplies none (the minted-values path).
#[derive(Default)]
pub struct CreateOptions {
    pub id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// A folder update patch. Mirrors v4 `_update`: provided fields overwrite, id
/// and createdAt are preserved, updatedAt is set explicitly. The pilot patches
/// `name` + `path` + `updatedAt`; the remaining columns and v4's "updatedAt =
/// now when absent" fallback land when an op needs them.
pub struct FolderUpdate {
    pub name: Option<String>,
    pub path: Option<String>,
    pub updated_at: String,
}

/// A `folders` row hydrated in full (v4 `Folder`) — the shape the files-family
/// folders routes read (`findByUserId` GET map, `findByPath`, the rename/delete
/// arms).
#[derive(Debug, Clone)]
pub struct FolderRow {
    pub id: String,
    pub user_id: String,
    pub path: String,
    pub name: String,
    pub parent_folder_id: Option<String>,
    pub project_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const FOLDER_SELECT_ALL: &str = "SELECT id, userId, path, name, parentFolderId, projectId, \
     createdAt, updatedAt FROM folders";

fn map_folder_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FolderRow> {
    Ok(FolderRow {
        id: row.get(0)?,
        user_id: row.get(1)?,
        path: row.get(2)?,
        name: row.get(3)?,
        parent_folder_id: row.get(4)?,
        project_id: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct FoldersRepository<'c> {
    conn: &'c Connection,
}

impl<'c> FoldersRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a folder, minting id / timestamps that `opts` leaves unset
    /// (v4 `_create`: `id = options?.id || generateId()`, timestamps `|| now`).
    /// Returns the id actually persisted so a caller can reference it.
    pub fn create(&self, data: &FolderCreate, opts: &CreateOptions) -> Result<String, DbError> {
        Ok(self.create_returning(data, opts)?.id)
    }

    /// [`Self::create`] with the persisted row handed back whole — v4's `create`
    /// resolves to the created `Folder`, and [`Self::ensure_by_path`] must return
    /// one. Identical SQL and identical minting; `create` is the id-only view of
    /// it, so there is no second insert path to keep in step.
    fn create_returning(
        &self,
        data: &FolderCreate,
        opts: &CreateOptions,
    ) -> Result<FolderRow, DbError> {
        let id = opts
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let now = now_iso();
        let created_at = opts.created_at.clone().unwrap_or_else(|| now.clone());
        let updated_at = opts.updated_at.clone().unwrap_or(now);

        self.conn.execute(
            "INSERT INTO folders \
               (id, userId, path, name, parentFolderId, projectId, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                data.user_id,
                data.path,
                data.name,
                data.parent_folder_id,
                data.project_id,
                created_at,
                updated_at,
            ],
        )?;
        Ok(FolderRow {
            id,
            user_id: data.user_id.clone(),
            path: data.path.clone(),
            name: data.name.clone(),
            parent_folder_id: data.parent_folder_id.clone(),
            project_id: data.project_id.clone(),
            created_at,
            updated_at,
        })
    }

    /// v4 `ensureByPath` (`folders.repository.ts`, v4 `a5df98b3f`, bug 114) —
    /// find-or-create the folder at `path`, and **the only sanctioned way to
    /// bring a folder row into being for a path that may already have one.**
    ///
    /// Every caller used to hand-roll `findByPath` → `create`, and each copy had
    /// the same two holes:
    ///
    ///   - **The read can fail soft.** v4's `findByPath` swallows query errors
    ///     and returns `null` (its `safeQuery` fallback), which a hand-rolled
    ///     guard cannot tell apart from "no such folder" — so a read failure
    ///     mints a duplicate instead of surfacing. Until v4 `c180246b1`
    ///     (2026-04-17) `FolderSchema.parentFolderId` was `.nullable()` without
    ///     `.optional()` while the SQLite hydrator turns a NULL column into
    ///     `undefined`, so *every* root-level folder failed validation on read
    ///     and every image generation appended another row (607 rows describing
    ///     24 folders on the real instance). **v5 never had this half** —
    ///     [`Self::find_by_path`] validates nothing and propagates every error
    ///     but `QueryReturnedNoRows`, so the duplicates v5 meets are v4-written.
    ///   - **The check and the insert are not atomic.** Two background jobs
    ///     generating images into the same project run concurrently (v4's global
    ///     in-flight cap is 4), both read absent, and both insert. That half is
    ///     live in v5 too.
    ///
    /// The `(userId, COALESCE(projectId, ''), path)` unique index closes both:
    /// the loser of a race takes a constraint violation and resolves to the
    /// winning row rather than adding to the pile. A conflict that resolves to
    /// nothing is re-raised rather than answered with a folder that does not
    /// exist, and a non-constraint failure is propagated untouched.
    ///
    /// v4 normalizes `data.projectId ?? null` before BOTH the read and the
    /// write; v5's `project_id` is already `Option<String>`, so that
    /// normalization is structural here — the same value reaches the `IS NULL`
    /// read branch and the SQL NULL column.
    pub fn ensure_by_path(&self, data: &FolderCreate) -> Result<FolderRow, DbError> {
        self.ensure_by_path_hooked(data, &|| {})
    }

    /// [`Self::ensure_by_path`] with a seam between the read and the insert.
    ///
    /// The lost-race arm is unreachable from a single-threaded test otherwise:
    /// any row a second connection plants BEFORE the call is simply found by the
    /// first read, and any row planted after is too late. `before_insert` is the
    /// one instant a competing writer could commit in; production passes a
    /// no-op, so this costs nothing.
    fn ensure_by_path_hooked(
        &self,
        data: &FolderCreate,
        before_insert: &dyn Fn(),
    ) -> Result<FolderRow, DbError> {
        let project_id = data.project_id.as_deref();

        if let Some(existing) = self.find_by_path(&data.user_id, &data.path, project_id)? {
            return Ok(existing);
        }

        before_insert();

        match self.create_returning(data, &CreateOptions::default()) {
            Ok(created) => Ok(created),
            Err(error) => {
                if !crate::db::sqlite_errors::is_unique_constraint_error(&error) {
                    return Err(error);
                }
                // Someone committed this path between our read and our insert.
                // The winning row is committed and visible, so resolve to it.
                let winner = self.find_by_path(&data.user_id, &data.path, project_id)?;
                let Some(winner) = winner else {
                    // Unique conflict with nothing to reconcile to — surface it
                    // rather than silently returning a folder that does not
                    // exist.
                    return Err(error);
                };
                tracing::debug!(
                    target: "quilltap::db",
                    user_id = %data.user_id,
                    path = %data.path,
                    project_id = ?data.project_id,
                    folder_id = %winner.id,
                    "Reconciled concurrent folder create to existing folder",
                );
                Ok(winner)
            }
        }
    }

    /// Apply an update patch to the folder `id`. Returns `false` when no row
    /// matched (v4's "not found -> null"). id and createdAt are never touched.
    pub fn update(&self, id: &str, patch: &FolderUpdate) -> Result<bool, DbError> {
        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(name) = &patch.name {
            assignments.push(format!("name = ?{}", values.len() + 1));
            values.push(Box::new(name.clone()));
        }
        if let Some(path) = &patch.path {
            assignments.push(format!("path = ?{}", values.len() + 1));
            values.push(Box::new(path.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE folders SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// v4 base `findByUserId(userId)` — every folder owned by the user, in
    /// natural/rowid order (the folders GET route sorts by `path.localeCompare`
    /// afterward).
    pub fn find_by_user_id(&self, user_id: &str) -> Result<Vec<FolderRow>, DbError> {
        let sql = format!("{FOLDER_SELECT_ALL} WHERE userId = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![user_id], map_folder_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `findByPath(userId, path, projectId)` — the single folder at a path in a
    /// scope (`findOneByFilter`). Nullable `projectId` via v4's `createNullableFilter`
    /// (null → `IS NULL`, else `= ?`). `None` when absent.
    pub fn find_by_path(
        &self,
        user_id: &str,
        path: &str,
        project_id: Option<&str>,
    ) -> Result<Option<FolderRow>, DbError> {
        let result = match project_id {
            Some(pid) => self.conn.query_row(
                &format!("{FOLDER_SELECT_ALL} WHERE userId = ?1 AND path = ?2 AND projectId = ?3 LIMIT 1"),
                params![user_id, path, pid],
                map_folder_row,
            ),
            None => self.conn.query_row(
                &format!("{FOLDER_SELECT_ALL} WHERE userId = ?1 AND path = ?2 AND projectId IS NULL LIMIT 1"),
                params![user_id, path],
                map_folder_row,
            ),
        };
        result.map(Some).or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other.into()),
        })
    }

    /// v4 `updatePathPrefix(userId, oldPrefix, newPrefix, projectId)` — rewrite
    /// every folder in-scope whose `path` starts with `old_prefix`, replacing the
    /// FIRST occurrence (v4 `String.replace` semantics; the prefix is at index 0)
    /// and minting a fresh `updatedAt` via [`Self::update`]. Returns the count
    /// updated. Called AFTER the renamed folder itself is updated, so it counts only
    /// descendants (the route adds `+1`).
    pub fn update_path_prefix(
        &self,
        user_id: &str,
        old_prefix: &str,
        new_prefix: &str,
        project_id: Option<&str>,
    ) -> Result<usize, DbError> {
        let all = self.find_all_in_scope(user_id, project_id)?;
        let mut updated = 0usize;
        for folder in all.into_iter().filter(|f| f.path.starts_with(old_prefix)) {
            let new_path = folder.path.replacen(old_prefix, new_prefix, 1);
            self.update(
                &folder.id,
                &FolderUpdate {
                    name: None,
                    path: Some(new_path),
                    updated_at: now_iso(),
                },
            )?;
            updated += 1;
        }
        Ok(updated)
    }

    /// v4 `hasChildren(folderId)` — `count({parentFolderId: folderId}) > 0`.
    pub fn has_children(&self, folder_id: &str) -> Result<bool, DbError> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM folders WHERE parentFolderId = ?1",
            params![folder_id],
            |row| row.get(0),
        )?;
        Ok(n > 0)
    }

    /// v4 `delete(folderId)` — `_delete`. Returns `false` when no row matched.
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM folders WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// v4 `findAllInProject(userId, projectId)` reduced to the rows the rename
    /// prefix-rewrite needs (nullable `projectId`). Rowid order (the caller filters
    /// + counts, so order is not observable).
    fn find_all_in_scope(
        &self,
        user_id: &str,
        project_id: Option<&str>,
    ) -> Result<Vec<FolderRow>, DbError> {
        match project_id {
            Some(pid) => {
                let sql = format!("{FOLDER_SELECT_ALL} WHERE userId = ?1 AND projectId = ?2");
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![user_id, pid], map_folder_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
            None => {
                let sql = format!("{FOLDER_SELECT_ALL} WHERE userId = ?1 AND projectId IS NULL");
                let mut stmt = self.conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params![user_id], map_folder_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            }
        }
    }
}

#[cfg(test)]
mod ensure_by_path_tests {
    //! v4's `__tests__/unit/lib/repositories/folders-ensure-by-path.test.ts`
    //! (six cases) carried over. v4 verifies the chokepoint by spying on
    //! `findByPath` and `create`; v5 cannot spy a repo, so each case is driven
    //! against a REAL `folders` table carrying the bug-114 unique index, and the
    //! spy assertions become row-state assertions (no second row inserted, the
    //! stored `projectId` is SQL NULL, …) plus, for the race arm, the
    //! `before_insert` seam standing in for the concurrent writer.

    use super::*;

    const USER_ID: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
    const PROJECT_ID: &str = "f29e2112-e609-48c1-977c-8843c0f1be0f";

    /// The `folders` DDL v4's own migration test hand-builds, plus the unique
    /// index bug 114 adds.
    fn folders_db(with_index: bool) -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE \"folders\" (\
               \"id\" TEXT PRIMARY KEY, \"userId\" TEXT NOT NULL, \"path\" TEXT NOT NULL, \
               \"name\" TEXT NOT NULL, \"parentFolderId\" TEXT, \"projectId\" TEXT, \
               \"createdAt\" TEXT NOT NULL, \"updatedAt\" TEXT NOT NULL);",
        )
        .unwrap();
        if with_index {
            c.execute_batch(
                "CREATE UNIQUE INDEX \"idx_folders_userId_projectId_path\" \
                 ON \"folders\" (\"userId\", COALESCE(\"projectId\", ''), \"path\")",
            )
            .unwrap();
        }
        c
    }

    fn input(project_id: Option<&str>) -> FolderCreate {
        FolderCreate {
            user_id: USER_ID.to_string(),
            path: "/story-backgrounds/".to_string(),
            name: "story-backgrounds".to_string(),
            parent_folder_id: None,
            project_id: project_id.map(str::to_string),
        }
    }

    fn seed(c: &Connection, id: &str, project_id: Option<&str>, path: &str) {
        c.execute(
            "INSERT INTO folders (id, userId, path, name, parentFolderId, projectId, \
               createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, 'seeded', NULL, ?4, '2026-02-11T21:38:02.655Z', \
               '2026-02-11T21:38:02.655Z')",
            params![id, USER_ID, path, project_id],
        )
        .unwrap();
    }

    fn row_count(c: &Connection) -> i64 {
        c.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn returns_the_existing_folder_without_inserting_a_second_row() {
        // Deliberately WITHOUT the index — v4's spy test asserts `create` was
        // never called, and the read-first branch is the only thing that can
        // make that true. With the index present a dropped early return still
        // converges (the insert violates, the re-read resolves to the same
        // row), so an index-backed table cannot tell the two apart; the sibling
        // case below covers that arm.
        let c = folders_db(false);
        seed(&c, "existing-id", Some(PROJECT_ID), "/story-backgrounds/");
        let repo = FoldersRepository::new(&c);

        let got = repo.ensure_by_path(&input(Some(PROJECT_ID))).unwrap();

        assert_eq!(got.id, "existing-id");
        assert_eq!(got.name, "seeded", "the STORED row, not the input");
        assert_eq!(row_count(&c), 1, "no second row inserted");
    }

    #[test]
    fn returns_the_existing_folder_under_the_index_too() {
        let c = folders_db(true);
        seed(&c, "existing-id", Some(PROJECT_ID), "/story-backgrounds/");
        let repo = FoldersRepository::new(&c);

        let got = repo.ensure_by_path(&input(Some(PROJECT_ID))).unwrap();

        assert_eq!(got.id, "existing-id");
        assert_eq!(row_count(&c), 1);
    }

    #[test]
    fn creates_the_folder_when_the_path_is_genuinely_absent() {
        let c = folders_db(true);
        let repo = FoldersRepository::new(&c);

        let got = repo.ensure_by_path(&input(Some(PROJECT_ID))).unwrap();

        assert_eq!(got.path, "/story-backgrounds/");
        assert_eq!(got.name, "story-backgrounds");
        assert_eq!(got.project_id.as_deref(), Some(PROJECT_ID));
        assert_eq!(row_count(&c), 1);
        let stored: String = c
            .query_row("SELECT id FROM folders", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored, got.id, "the returned row is the one persisted");
    }

    #[test]
    fn an_absent_project_id_is_null_on_both_the_read_and_the_write() {
        // v4's `data.projectId ?? null`. The read must take the `IS NULL`
        // branch (so a same-path row in a PROJECT is not mistaken for this
        // one) and the write must store SQL NULL.
        let c = folders_db(true);
        seed(&c, "in-a-project", Some(PROJECT_ID), "/reports/");
        let repo = FoldersRepository::new(&c);

        let got = repo
            .ensure_by_path(&FolderCreate {
                user_id: USER_ID.to_string(),
                path: "/reports/".to_string(),
                name: "reports".to_string(),
                parent_folder_id: None,
                project_id: None,
            })
            .unwrap();

        assert_ne!(
            got.id, "in-a-project",
            "the project row is a different folder"
        );
        assert_eq!(got.project_id, None);
        assert_eq!(row_count(&c), 2);
        let is_null: bool = c
            .query_row(
                "SELECT projectId IS NULL FROM folders WHERE id = ?1",
                params![got.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(is_null, "stored as SQL NULL, not the empty string");
    }

    #[test]
    fn resolves_to_the_winning_row_when_a_concurrent_create_won_the_race() {
        let c = folders_db(true);
        let repo = FoldersRepository::new(&c);

        let planted = std::cell::Cell::new(false);
        let got = repo
            .ensure_by_path_hooked(&input(Some(PROJECT_ID)), &|| {
                if !planted.replace(true) {
                    seed(&c, "winner-id", Some(PROJECT_ID), "/story-backgrounds/");
                }
            })
            .unwrap();

        assert!(
            planted.get(),
            "the seam fired — the read found nothing first"
        );
        assert_eq!(got.id, "winner-id");
        assert_eq!(row_count(&c), 1, "the loser's row was never committed");
    }

    #[test]
    fn rethrows_a_unique_conflict_that_cannot_be_reconciled_to_a_row() {
        // The index keys on COALESCE(projectId, ''), so a stored empty-string
        // projectId collides with an absent one — while `find_by_path(None)`
        // reads `projectId IS NULL` and legitimately finds nothing, before AND
        // after. That is v4's "conflict with no winner" arm, reachable without
        // any seam at all.
        let c = folders_db(true);
        seed(&c, "empty-string-project", Some(""), "/story-backgrounds/");
        let repo = FoldersRepository::new(&c);

        let err = repo.ensure_by_path(&input(None)).unwrap_err();

        assert!(
            crate::db::sqlite_errors::is_unique_constraint_error(&err),
            "the ORIGINAL constraint error is re-raised: {err}"
        );
        assert_eq!(row_count(&c), 1, "nothing written");
    }

    #[test]
    fn rethrows_a_non_constraint_create_failure_instead_of_swallowing_it() {
        // v4 asserts the ORIGINAL error surfaces and `findByPath` was called
        // exactly ONCE — a spy assertion v5 cannot make. It becomes an error
        // IDENTITY assertion instead: the seam reshapes the table so that the
        // INSERT and a hypothetical second SELECT fail with DIFFERENT SQLite
        // sentences, so "the create's own error, un-recovered" is
        // distinguishable from "recovered, then re-read".
        let c = folders_db(true);
        let repo = FoldersRepository::new(&c);

        let err = repo
            .ensure_by_path_hooked(&input(Some(PROJECT_ID)), &|| {
                c.execute_batch("DROP TABLE folders; CREATE TABLE folders (id TEXT);")
                    .unwrap();
            })
            .unwrap_err();

        assert!(
            !crate::db::sqlite_errors::is_unique_constraint_error(&err),
            "not a constraint error: {err}"
        );
        assert!(
            err.to_string().contains("has no column named userId"),
            "the INSERT's own error, propagated untouched — a recovery attempt \
             would surface the re-read's `no such column` instead; got {err}"
        );
    }

    #[test]
    fn a_read_failure_propagates_before_any_create_is_attempted() {
        // No `folders` table at all: v4's `findByPath` throws (v5 propagates;
        // v4's `safeQuery` would have swallowed it into the `null` that started
        // bug 114 — see the method header).
        let c = Connection::open_in_memory().unwrap();
        let repo = FoldersRepository::new(&c);

        let err = repo.ensure_by_path(&input(Some(PROJECT_ID))).unwrap_err();

        assert!(err.to_string().contains("no such table"), "got {err}");
    }

    #[test]
    fn without_the_index_a_lost_race_still_duplicates() {
        // The chokepoint is only half the fix: the atomicity comes from the
        // index. On a pre-collapse instance the same seam mints a second row —
        // which is what the boot ensure exists to prevent.
        let c = folders_db(false);
        let repo = FoldersRepository::new(&c);

        let planted = std::cell::Cell::new(false);
        let got = repo
            .ensure_by_path_hooked(&input(Some(PROJECT_ID)), &|| {
                if !planted.replace(true) {
                    seed(&c, "winner-id", Some(PROJECT_ID), "/story-backgrounds/");
                }
            })
            .unwrap();

        assert_ne!(got.id, "winner-id");
        assert_eq!(row_count(&c), 2, "no index, no atomicity");
    }
}
