//! The migration-created index family, backfilled at boot (P4.160 — P4.153's
//! OPEN item).
//!
//! v4 has two index families: the generateDDL one its repositories make
//! (`fresh_schema.json`) and the one its MIGRATIONS make in PHASE 1 of every
//! first boot (`migration_indexes.json`, dumped from v4's REAL
//! `MigrationRunner`, D23). Since P4.153 `provision_fresh_instance` replays
//! both, but an instance `setup` BEFORE that round never gains the second: after
//! a boot it lacks 56 of the artifact's 60 statements (main 47 / mount-index 4
//! / llm-logs 5 — the boot's own ensures already make
//! `idx_files_generationKey`, `idx_folders_userId_projectId_path` and
//! `idx_help_doc_chunks_docId`) and holds `idx_doc_mount_folders_mp_path` PLAIN
//! (`fresh_schema.json`'s copy) where every v4 instance holds it UNIQUE. So its
//! UNIQUE write semantics and its query plans depended on WHEN it was set up
//! (a restore into one took 2 h 26 m against 9 m — dogfood #149).
//!
//! v4 itself would backfill almost none of it on such a file: most of the
//! index-creating migrations are ledgered or gate on a table that is already
//! there, so the family stays missing on BOTH apps until this pass runs. The
//! exception is `add-connection-profile-unique-name-index-v1`, whose
//! `shouldRun` is the index's absence — that one is PORTED here
//! ([`dedupe_connection_profile_names`]), a convergence: v4 would run exactly
//! it on a pre-round v5 file.
//!
//! ## What the pass does, per partition, per statement
//!
//! The statements are the artifact's and nothing else (D23 — never a
//! hand-written `CREATE INDEX`), with `IF NOT EXISTS ` spliced after `INDEX `.
//! SQLite stores an index's text WITHOUT that clause, so the backfilled
//! `sqlite_master` is byte-equal to a fresh provision's (R-D).
//!
//! - **The table is absent** → skipped silently and counted (R-E): the
//!   table-gated migrations would not have made the index either. Expected
//!   zero on a provisioned instance.
//! - **The name is present with the same UNIQUE-ness** → skipped (R-B: index
//!   PRESENCE is the once-only marker; no `migrations_state` row is written,
//!   which also keeps v4's index-gated `shouldRun`s false afterwards). A
//!   present UNIQUE copy of a name the artifact makes plain is never
//!   downgraded.
//! - **`idx_doc_mount_folders_mp_path` present PLAIN** (the one name the
//!   artifact makes UNIQUE over a plain generateDDL copy) → the duplicate
//!   pre-check, then `DROP INDEX` + the UNIQUE `CREATE` in ONE transaction. Never
//!   the window "plain dropped, UNIQUE not yet made": `builtin_mounts`'
//!   `CREATE UNIQUE INDEX IF NOT EXISTS` would make it on the next boot and
//!   FAIL THE BOOT on a duplicate path.
//! - **Absent** → created. For a UNIQUE name the duplicate pre-check runs
//!   first; for the connection-profile name v4's rename-dedupe runs first, in
//!   the same transaction as the `CREATE`.
//!
//! ## Duplicates under a UNIQUE name (R-A, ruled at planning)
//!
//! v4 dedupes before a UNIQUE create in exactly two migrations: the folder
//! collapse (already ported — `folders_unique_path_repair`, which runs earlier
//! in the same boot) and the connection-profile rename. For
//! `idx_chat_documents_unique`, the two group-join UNIQUEs and `mp_path` v4 has
//! NO dedupe, and its migration would fail the boot — but v4 never runs those
//! migrations on an existing file, so a v5 boot that died there would be a
//! v5-ONLY hazard on a shared instance. So a duplicate SKIPS that one index
//! with ONE v5-only WARN `Skipped a unique index backfill: duplicate rows
//! present {index, table, duplicates}` (a ruled divergence: v4 has no such
//! line) and the pass goes on; `mp_path` stays PLAIN (the status quo, not a
//! regression). Every later boot re-checks and re-warns until the rows are
//! fixed. `duplicates` is the number of rows beyond the first in each
//! colliding group — the rows a dedupe would have to touch.
//!
//! The pre-check is generic over the statement: its key terms are split out of
//! the artifact's own column list, and a row whose key holds a NULL in any term
//! is left out — SQLite treats NULLs as DISTINCT inside a UNIQUE index, while a
//! bare `GROUP BY` would call them equal (`idx_chat_documents_unique` includes
//! the nullable `mountPoint`). `GROUP BY` otherwise compares exactly as the
//! index does (each term's collation, numeric affinity).
//!
//! ## Failures (R-C)
//!
//! The pass is NON-FATAL and runs LAST in `seed_built_ins` — after
//! `create_missing_structural_tables`, so a table that step just created gets
//! its indexes the same boot. A statement whose `CREATE` (or conversion) fails
//! is rolled back, logged ERROR `Failed to backfill a migration-created index
//! {partition, index, error}` (v5-only) and counted; the rest still run. A
//! UNIQUE statement whose duplicate PRE-CHECK read fails is the same: one
//! failed statement, never the partition. A
//! partition whose `sqlite_master` cannot be read at all answers
//! [`PartitionOutcome::Failed`] after one ERROR `Migration index backfill
//! failed {partition, error}` and the next partition runs. Neither is a
//! structural failure for `/health`: the backfilled indexes are not v4's
//! repository ensures (no v4 `ensureTable` makes them), so recording one in
//! `EnsureFailures` would answer a `degraded` v4 never answers.
//!
//! INFO `Backfilled migration-created indexes {partition, created, skipped}`
//! (v5-only) when the pass made at least one index in a partition — `created`
//! counts the conversion too, `skipped` every family member still missing
//! afterwards (duplicates, absent tables, failures); silent otherwise, so the
//! second boot logs nothing.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};

use super::DbError;
use crate::services::profile_names::{make_unique_profile_name, normalize_profile_name};
use crate::services::provisioning::{index_name, index_table, migration_index_family};

/// v4's `INDEX_NAME` in `add-connection-profile-unique-name-index.ts`.
pub const PROFILE_NAME_INDEX: &str = "idx_connection_profiles_userId_name";

/// The one name the artifact makes UNIQUE over `fresh_schema.json`'s plain copy.
pub const FOLDER_PATH_INDEX: &str = "idx_doc_mount_folders_mp_path";

/// v4's migration-logger `context` on both of the rename-dedupe's lines — the
/// literal names the migration, which is what v4 ran.
const PROFILE_CONTEXT: &str = "migration.add-connection-profile-unique-name-index";

/// What the pass did in one partition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartitionBackfill {
    /// Absent indexes created.
    pub created: usize,
    /// Plain copies converted UNIQUE (`mp_path`).
    pub converted: usize,
    /// Already present in the artifact's shape.
    pub skipped_present: usize,
    /// On a table the partition does not have (R-E).
    pub skipped_absent_table: usize,
    /// UNIQUE names left absent (or plain) over duplicate rows (R-A).
    pub skipped_duplicates: usize,
    /// Statements whose create or conversion failed (logged, rolled back).
    pub failed: usize,
    /// Connection profiles v4's rename-dedupe renamed.
    pub profiles_renamed: usize,
}

impl PartitionBackfill {
    /// Family members still missing (or plain) after the pass.
    fn still_missing(&self) -> usize {
        self.skipped_duplicates + self.skipped_absent_table + self.failed
    }
}

/// One partition's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartitionOutcome {
    /// The instance has no such partition (or it is unavailable) — skipped.
    Absent,
    /// The pass ran.
    Ran(PartitionBackfill),
    /// The partition's `sqlite_master` could not be read; the bare error text.
    Failed(String),
}

/// What the pass did, per partition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexFamilyReport {
    pub main: PartitionOutcome,
    pub mount_index: PartitionOutcome,
    pub llm_logs: PartitionOutcome,
}

/// Backfill v4's migration-created index family on every partition the
/// instance has (see the module header). Errs only when the embedded artifact
/// does not parse (a build bug); every database failure is logged and answered
/// in the report, and the boot goes on.
pub fn ensure_migration_index_family(
    main: &Connection,
    mount_index: Option<&Connection>,
    llm_logs: Option<&Connection>,
) -> Result<IndexFamilyReport, DbError> {
    let family = migration_index_family().map_err(|e| DbError::Internal(e.to_string()))?;
    Ok(IndexFamilyReport {
        main: run_partition("main", Some(main), &family.main),
        mount_index: run_partition("mountIndex", mount_index, &family.mount_index),
        llm_logs: run_partition("llmLogs", llm_logs, &family.llm_logs),
    })
}

fn run_partition(
    partition: &'static str,
    conn: Option<&Connection>,
    statements: &[String],
) -> PartitionOutcome {
    let Some(conn) = conn else {
        return PartitionOutcome::Absent;
    };
    match backfill_partition(partition, conn, statements) {
        Ok(report) => {
            if report.created + report.converted > 0 {
                tracing::info!(
                    target: "quilltap::boot",
                    partition,
                    created = report.created + report.converted,
                    skipped = report.still_missing(),
                    "Backfilled migration-created indexes"
                );
            }
            PartitionOutcome::Ran(report)
        }
        Err(e) => {
            let error = super::fallback::error_text(&e);
            tracing::error!(
                target: "quilltap::boot",
                partition,
                error = %error,
                "Migration index backfill failed"
            );
            PartitionOutcome::Failed(error)
        }
    }
}

fn backfill_partition(
    partition: &'static str,
    conn: &Connection,
    statements: &[String],
) -> Result<PartitionBackfill, DbError> {
    let mut report = PartitionBackfill::default();
    for sql in statements {
        let (Some(name), Some(table)) = (index_name(sql), index_table(sql)) else {
            return Err(DbError::Internal(format!(
                "migration_indexes.json: not an index statement: {sql}"
            )));
        };
        if !table_exists(conn, table)? {
            report.skipped_absent_table += 1;
            continue;
        }
        let want_unique = is_unique(sql);
        let existing: Option<Option<String>> = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
                params![name],
                |r| r.get(0),
            )
            .optional()?;
        let create = if_not_exists(sql);
        match existing {
            // Present in the artifact's shape — or UNIQUE where the artifact is
            // plain (never downgraded).
            Some(have) if !want_unique || have.as_deref().is_some_and(is_unique) => {
                report.skipped_present += 1;
            }
            // Present PLAIN where the artifact is UNIQUE: `mp_path`.
            Some(_) => {
                match pre_check(partition, conn, name, table, sql, &mut report) {
                    PreCheck::Clear => {}
                    PreCheck::Duplicates | PreCheck::Failed => continue,
                }
                let converted = (|| {
                    let tx = conn.unchecked_transaction()?;
                    tx.execute_batch(&format!("DROP INDEX \"{name}\""))?;
                    tx.execute_batch(&create)?;
                    tx.commit()?;
                    Ok(())
                })();
                if succeeded(partition, name, converted, &mut report) {
                    report.converted += 1;
                }
            }
            None if name == PROFILE_NAME_INDEX => {
                match create_profile_name_index(conn, table, sql, &create) {
                    Ok(ProfileIndex::Created {
                        profile_count,
                        renamed,
                    }) => {
                        tracing::info!(
                            target: "quilltap::boot",
                            context = PROFILE_CONTEXT,
                            profileCount = profile_count,
                            renamed,
                            "Enforced unique connection-profile names"
                        );
                        report.profiles_renamed += renamed;
                        report.created += 1;
                    }
                    Ok(ProfileIndex::Duplicates) => {
                        // Counts the skip (or, should the re-read itself fail,
                        // the failure) — the WARN either way is per statement.
                        pre_check(partition, conn, name, table, sql, &mut report);
                    }
                    Err(e) => {
                        succeeded(partition, name, Err(e), &mut report);
                    }
                }
            }
            None => {
                if want_unique {
                    match pre_check(partition, conn, name, table, sql, &mut report) {
                        PreCheck::Clear => {}
                        PreCheck::Duplicates | PreCheck::Failed => continue,
                    }
                }
                let created = conn.execute_batch(&create).map_err(DbError::from);
                if succeeded(partition, name, created, &mut report) {
                    report.created += 1;
                }
            }
        }
    }
    Ok(report)
}

/// How a UNIQUE statement's duplicate pre-check came out.
enum PreCheck {
    /// No collisions — the `CREATE` may run.
    Clear,
    /// Rows collide: WARNed and counted in `skipped_duplicates`.
    Duplicates,
    /// The pre-check's own read failed (a key column the table lacks, …):
    /// logged and counted like a failed `CREATE` — ONE statement, never the
    /// partition (the `94fbb1ae3` boot-hardness unification's §3 review).
    Failed,
}

fn pre_check(
    partition: &'static str,
    conn: &Connection,
    index: &str,
    table: &str,
    sql: &str,
    report: &mut PartitionBackfill,
) -> PreCheck {
    match skipped_for_duplicates(conn, index, table, sql) {
        Ok(false) => PreCheck::Clear,
        Ok(true) => {
            report.skipped_duplicates += 1;
            PreCheck::Duplicates
        }
        Err(e) => {
            succeeded(partition, index, Err(e), report);
            PreCheck::Failed
        }
    }
}

/// `true` on `Ok`; on `Err` logs the v5-only ERROR, counts the failure and
/// answers `false`.
fn succeeded(
    partition: &'static str,
    index: &str,
    outcome: Result<(), DbError>,
    report: &mut PartitionBackfill,
) -> bool {
    match outcome {
        Ok(()) => true,
        Err(e) => {
            tracing::error!(
                target: "quilltap::boot",
                partition,
                index,
                error = %super::fallback::error_text(&e),
                "Failed to backfill a migration-created index"
            );
            report.failed += 1;
            false
        }
    }
}

/// How the connection-profile name index's transaction ended.
enum ProfileIndex {
    /// Renamed, created, verified, committed — v4's INFO fields.
    Created {
        profile_count: usize,
        renamed: usize,
    },
    /// Rows still collide under SQLite's own key after the rename: rolled back
    /// (renames included). Unreachable — JS's `trim().toLowerCase()` folds at
    /// least as much as SQLite's ASCII `lower(trim())`, so a JS-deduped set
    /// always satisfies the index (the survey's asymmetry note) — and kept so
    /// a surprise WARNs (R-A) instead of failing the `CREATE`.
    Duplicates,
}

/// v4's `add-connection-profile-unique-name-index-v1` `run()`: the
/// rename-dedupe, the `CREATE`, the presence check (v4 `:112-114` — `Unique
/// index was not created`, which v4's own catch turns into a failed migration),
/// in ONE transaction where v4 runs them bare: a failure leaves no half-renamed
/// table behind, and the absent index re-runs the whole step next boot.
fn create_profile_name_index(
    conn: &Connection,
    table: &str,
    sql: &str,
    create: &str,
) -> Result<ProfileIndex, DbError> {
    let tx = conn.unchecked_transaction()?;
    let (profile_count, renamed) = dedupe_connection_profile_names(&tx)?;
    if duplicate_rows(&tx, table, sql)? > 0 {
        return Ok(ProfileIndex::Duplicates);
    }
    tx.execute_batch(create)?;
    if !index_present(&tx, PROFILE_NAME_INDEX)? {
        return Err(DbError::Internal(
            "Unique index was not created".to_string(),
        ));
    }
    tx.commit()?;
    Ok(ProfileIndex::Created {
        profile_count,
        renamed,
    })
}

/// v4's `add-connection-profile-unique-name-index-v1` step 1
/// (`migrations/scripts/add-connection-profile-unique-name-index.ts:66-102`):
/// the oldest profile (by `createdAt`, `id` breaking ties) keeps its name;
/// every later one is re-minted through `makeUniqueProfileName` against the
/// names its user has already taken — so a colliding name gains `" (2)"`,
/// `" (3)"`, …, and (the trim quirk) a name with surrounding whitespace is
/// renamed to its trimmed form even with no collision at all, because
/// `uniqueName !== profile.name`. The "taken" set is scoped by `userId`, as the
/// index is. Each rename stamps its own `updatedAt` (v4 calls
/// `new Date().toISOString()` per row) and logs v4's DEBUG line.
///
/// The rename uses v4's JS normalization (`trim().toLowerCase()`, through
/// [`normalize_profile_name`]); the duplicate pre-check after it uses SQLite's
/// own `lower(trim(name))` — the index's semantics. They differ (JS folds
/// non-ASCII case and trims every JS whitespace; SQLite folds ASCII only and
/// trims spaces only), and JS's is the coarser, so the renamed set always
/// satisfies the index. Answers v4's INFO fields: `(profileCount, renamed)`.
fn dedupe_connection_profile_names(conn: &Connection) -> Result<(usize, usize), DbError> {
    let profiles: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, userId, name FROM connection_profiles ORDER BY createdAt ASC, id ASC",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let profile_count = profiles.len();
    let mut update =
        conn.prepare("UPDATE connection_profiles SET name = ?1, updatedAt = ?2 WHERE id = ?3")?;
    let mut taken_by_user: HashMap<String, HashSet<String>> = HashMap::new();
    let mut renamed = 0;
    for (id, user_id, name) in profiles {
        let taken = taken_by_user.entry(user_id).or_default();
        let unique_name = make_unique_profile_name(&name, taken);
        if unique_name != name {
            update.execute(params![unique_name, crate::clock::now_iso(), id])?;
            renamed += 1;
            tracing::debug!(
                target: "quilltap::boot",
                context = PROFILE_CONTEXT,
                profileId = %id,
                from = %name,
                to = %unique_name,
                "Renamed duplicate connection-profile name"
            );
        }
        taken.insert(normalize_profile_name(&unique_name));
    }
    Ok((profile_count, renamed))
}

/// The duplicate pre-check for a UNIQUE statement: `true` (after the R-A WARN)
/// when rows collide under its key.
fn skipped_for_duplicates(
    conn: &Connection,
    index: &str,
    table: &str,
    sql: &str,
) -> Result<bool, DbError> {
    let duplicates = duplicate_rows(conn, table, sql)?;
    if duplicates > 0 {
        tracing::warn!(
            target: "quilltap::boot",
            index,
            table,
            duplicates,
            "Skipped a unique index backfill: duplicate rows present"
        );
    }
    Ok(duplicates > 0)
}

/// Rows beyond the first in every group that collides under `sql`'s key, with
/// SQLite's UNIQUE semantics: a row with a NULL in any key term collides with
/// nothing, and the statement's own partial-index `WHERE` (if any) applies.
fn duplicate_rows(conn: &Connection, table: &str, sql: &str) -> Result<i64, DbError> {
    let (terms, partial) = key_terms(sql).ok_or_else(|| {
        DbError::Internal(format!("migration_indexes.json: no key terms in {sql}"))
    })?;
    let mut filter: Vec<String> = terms.iter().map(|t| format!("{t} IS NOT NULL")).collect();
    if let Some(partial) = partial {
        filter.push(format!("({partial})"));
    }
    let query = format!(
        "SELECT COALESCE(SUM(n - 1), 0) FROM (SELECT COUNT(*) AS n FROM \"{table}\" \
         WHERE {} GROUP BY {} HAVING COUNT(*) > 1)",
        filter.join(" AND "),
        terms.join(", ")
    );
    Ok(conn.query_row(&query, [], |r| r.get(0))?)
}

/// A `CREATE … INDEX … ON t (<terms>) [WHERE <partial>]` statement's key terms
/// (top-level commas split; a trailing `ASC`/`DESC` dropped — `GROUP BY` takes
/// neither, and order does not bear on equality) and its partial clause.
fn key_terms(sql: &str) -> Option<(Vec<String>, Option<String>)> {
    let (_, on) = sql.split_once(" ON ")?;
    let open = on.find('(')?;
    let mut depth = 0usize;
    let mut close = None;
    for (i, c) in on.char_indices().skip(open) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let inner = &on[open + 1..close];
    let mut terms = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (i, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                terms.push(inner[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    terms.push(inner[start..].to_string());
    let terms = terms
        .into_iter()
        .map(|t| {
            let t = t.trim();
            let t = t
                .strip_suffix(" DESC")
                .or_else(|| t.strip_suffix(" ASC"))
                .unwrap_or(t);
            t.trim().to_string()
        })
        .collect();
    let partial = on[close + 1..]
        .trim()
        .strip_prefix("WHERE ")
        .map(|w| w.trim().to_string());
    Some((terms, partial))
}

/// The artifact's statement with `IF NOT EXISTS ` spliced after `INDEX ` —
/// SQLite stores the text without it, so `sqlite_master` matches a fresh
/// provision's byte for byte (R-D).
fn if_not_exists(sql: &str) -> String {
    sql.replacen("INDEX ", "INDEX IF NOT EXISTS ", 1)
}

fn is_unique(sql: &str) -> bool {
    sql.starts_with("CREATE UNIQUE ")
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, DbError> {
    Ok(conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?
        .exists(params![table])?)
}

fn index_present(conn: &Connection, index: &str) -> Result<bool, DbError> {
    Ok(conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1")?
        .exists(params![index])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    /// The `94fbb1ae3` boot-hardness unification's §3 review: a UNIQUE
    /// statement whose duplicate PRE-CHECK read fails is ONE failed statement,
    /// logged and counted like a failed `CREATE`; the partition's later
    /// statements still run. Red before the fix: the pre-check's `?` ended the
    /// whole partition. (Reachability, measured: the artifact double-quotes its
    /// identifiers, so a MISSING column reads as SQLite's double-quoted-string
    /// fallback and the pre-check succeeds — the arm is reached by a read
    /// error such as a damaged page. The plant here uses a bare identifier,
    /// which errors `no such column`, to drive the same arm.)
    #[test]
    fn a_failed_duplicate_pre_check_fails_one_statement_not_the_partition() {
        let c = conn();
        c.execute_batch(
            "CREATE TABLE chat_documents (id TEXT, chatId TEXT); \
             CREATE TABLE chats (id TEXT, updatedAt TEXT);",
        )
        .unwrap();
        let statements = vec![
            "CREATE UNIQUE INDEX idx_chat_documents_unique ON chat_documents(chatId, mountPoint)"
                .to_string(),
            "CREATE INDEX \"idx_chats_updatedAt\" ON \"chats\" (\"updatedAt\")".to_string(),
        ];
        let report = backfill_partition("main", &c, &statements)
            .expect("a statement's failure never fails the partition");
        assert_eq!(report.failed, 1, "the pre-check failure is counted");
        assert_eq!(report.created, 1, "the next statement still runs");
    }

    #[test]
    fn key_terms_split_the_artifacts_column_lists() {
        assert_eq!(
            key_terms(
                "CREATE UNIQUE INDEX \"idx_connection_profiles_userId_name\" ON \
                 \"connection_profiles\" (\"userId\", lower(trim(\"name\")))"
            ),
            Some((
                vec!["\"userId\"".into(), "lower(trim(\"name\"))".into()],
                None
            ))
        );
        assert_eq!(
            key_terms(
                "CREATE INDEX idx_chats_autonomous_nextRunAt ON chats(scheduleNextRunAt) \
                 WHERE chatType = 'autonomous'"
            ),
            Some((
                vec!["scheduleNextRunAt".into()],
                Some("chatType = 'autonomous'".into())
            ))
        );
        assert_eq!(
            key_terms("CREATE INDEX \"i\" ON \"m\" (\"occurredAt\" DESC)"),
            Some((vec!["\"occurredAt\"".into()], None))
        );
    }

    #[test]
    fn if_not_exists_splices_after_index_in_both_shapes() {
        assert_eq!(
            if_not_exists("CREATE UNIQUE INDEX \"a\" ON \"t\" (\"x\")"),
            "CREATE UNIQUE INDEX IF NOT EXISTS \"a\" ON \"t\" (\"x\")"
        );
        assert_eq!(
            if_not_exists("CREATE INDEX a ON t(x) WHERE y = 'INDEX '"),
            "CREATE INDEX IF NOT EXISTS a ON t(x) WHERE y = 'INDEX '"
        );
    }

    /// The splice stores the artifact's text: SQLite drops `IF NOT EXISTS`.
    #[test]
    fn sqlite_master_keeps_the_artifacts_text() {
        let c = conn();
        c.execute_batch("CREATE TABLE chats (scheduleNextRunAt TEXT, chatType TEXT)")
            .unwrap();
        let sql = "CREATE INDEX idx_chats_autonomous_nextRunAt ON chats(scheduleNextRunAt) \
                   WHERE chatType = 'autonomous'";
        c.execute_batch(&if_not_exists(sql)).unwrap();
        let stored: String = c
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'idx_chats_autonomous_nextRunAt'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, sql);
    }

    #[test]
    fn nulls_never_collide_and_the_partial_clause_applies() {
        let c = conn();
        c.execute_batch(
            "CREATE TABLE t (a TEXT, b TEXT, k TEXT);
             INSERT INTO t VALUES ('x', NULL, 'on'), ('x', NULL, 'on'),
                                  ('y', 'z', 'off'), ('y', 'z', 'off'),
                                  ('w', 'v', 'on'), ('w', 'v', 'on'), ('w', 'v', 'on');",
        )
        .unwrap();
        let unique = "CREATE UNIQUE INDEX \"u\" ON \"t\" (\"a\", \"b\")";
        assert_eq!(duplicate_rows(&c, "t", unique).unwrap(), 3);
        let partial = "CREATE UNIQUE INDEX u ON t(a, b) WHERE k = 'on'";
        assert_eq!(duplicate_rows(&c, "t", partial).unwrap(), 2);
        // And the counts agree with SQLite's own verdict.
        assert!(c.execute_batch(unique).is_err());
        c.execute_batch(
            "DELETE FROM t WHERE a = 'w' AND rowid > (SELECT MIN(rowid) FROM t WHERE a = 'w')",
        )
        .unwrap();
        assert!(c.execute_batch(partial).is_ok());
    }

    /// R-E: a statement whose table is absent is skipped and counted — a
    /// table-less partition creates nothing and logs nothing.
    #[test]
    fn statements_on_absent_tables_are_skipped_and_counted() {
        let c = conn();
        let family = migration_index_family().unwrap();
        let report = backfill_partition("llmLogs", &c, &family.llm_logs).unwrap();
        assert_eq!(
            report,
            PartitionBackfill {
                skipped_absent_table: 5,
                ..Default::default()
            }
        );
    }

    /// A squatted name fails that ONE statement (rolled back, counted) and the
    /// pass goes on.
    #[test]
    fn a_failing_statement_is_counted_and_the_rest_still_run() {
        let c = conn();
        c.execute_batch(
            "CREATE TABLE llm_logs (autonomousRunId TEXT, chatId TEXT, connectionProfileId TEXT, \
             imageProfileId TEXT, type TEXT);
             CREATE TABLE idx_llm_logs_chatId (x TEXT);",
        )
        .unwrap();
        let family = migration_index_family().unwrap();
        let (report, lines) = crate::test_support::captured_with(|| {
            backfill_partition("llmLogs", &c, &family.llm_logs).unwrap()
        });
        assert_eq!((report.created, report.failed), (4, 1));
        assert_eq!(
            lines,
            [
                "ERROR quilltap::boot Failed to backfill a migration-created index \
              partition=llmLogs index=idx_llm_logs_chatId \
              error=there is already a table named idx_llm_logs_chatId"
            ]
        );
    }

    /// A UNIQUE copy is never downgraded to the artifact's plain text, and a
    /// present name is never re-created.
    #[test]
    fn present_names_are_left_alone() {
        let c = conn();
        c.execute_batch(
            "CREATE TABLE llm_logs (autonomousRunId TEXT, chatId TEXT, connectionProfileId TEXT, \
             imageProfileId TEXT, type TEXT);
             CREATE UNIQUE INDEX idx_llm_logs_type ON llm_logs (type, chatId);",
        )
        .unwrap();
        let family = migration_index_family().unwrap();
        let report = backfill_partition("llmLogs", &c, &family.llm_logs).unwrap();
        assert_eq!((report.created, report.skipped_present), (4, 1));
        let again = backfill_partition("llmLogs", &c, &family.llm_logs).unwrap();
        assert_eq!((again.created, again.skipped_present), (0, 5));
    }
}
