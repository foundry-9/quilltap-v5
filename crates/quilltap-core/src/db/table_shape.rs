//! The boot-time structural table check's SHAPE half — v4
//! `lib/database/table-shape.ts` (`findTableShapeProblem`, new at `e5c6bd0c0`,
//! bug 176 — this port's own filing from P4.135) and the per-repository problem
//! templates its PHASE 3.1 pass reports (`dedicated-db.repository.ts:176-200`,
//! `help-doc-chunks.repository.ts:45-56`).
//!
//! Why a shape check at all: the generated DDL is `CREATE TABLE IF NOT EXISTS`,
//! so running it against a damaged table proves nothing — a column renamed in
//! place, or a table swapped for a view, passes untouched unless an index
//! happens to name the missing column. This check does not depend on that luck.
//! Extra columns are fine (repositories add them in `onTableEnsured`); a missing
//! schema column, or a name that is not a table at all, is damage. Missing
//! columns are listed in ZOD SCHEMA order (v4 filters
//! `extractSchemaMetadata(...).fields`, `schema-translator.ts:233-246`), never
//! `PRAGMA` order — the two agree on every `generateDDL`-born table and differ on
//! an `ALTER … ADD COLUMN`-healed or migration-born one.
//!
//! [`STRUCTURAL_TABLES`] is v4's verifiable-repository census in the container's
//! insertion order (`lib/database/repositories/index.ts:174-216` — `llmLogs`
//! FIRST), each with its schema's field list. It is GENERATED, never typed: the
//! `census` rows of `harness/oracle/cases/table-shape.ts` (v4's REAL container +
//! schemas) at `e5c6bd0c0`, and `table_shape_equivalence` diffs it
//! field-for-field — on a mismatch that test prints the regenerated literal.
//! `services/provisioning/fresh_schema.json` is NOT the source: it is SQL text
//! and may carry columns no schema names.

use rusqlite::Connection;

use super::fallback::log_partition_structural_unavailable;
use super::DbError;

/// Which partition a structural table lives in (v4's `dbTarget`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Partition {
    Main,
    MountIndex,
    LlmLogs,
}

impl Partition {
    /// v4's `dbTarget` spelling — the `Verified dedicated-database table
    /// structure` line's field value.
    pub fn db_target(self) -> &'static str {
        match self {
            Partition::Main => "main",
            Partition::MountIndex => "mountIndex",
            Partition::LlmLogs => "llmLogs",
        }
    }

    /// v4's database label in a problem string: `DB_LABELS`
    /// (`dedicated-db.repository.ts:58-61`) for the two dedicated targets, and
    /// the help-chunks repository's literal `main` (`help-doc-chunks.repository.ts:54`).
    pub fn label(self) -> &'static str {
        match self {
            Partition::Main => "main",
            Partition::MountIndex => "mount index",
            Partition::LlmLogs => "LLM logs",
        }
    }
}

/// One structure-verifiable repository's table, as v4's pass meets it.
#[derive(Debug)]
pub struct StructuralTable {
    /// v4's repository-container key (the `repository` field of the
    /// per-problem ERROR).
    pub repository: &'static str,
    pub collection: &'static str,
    pub partition: Partition,
    /// The schema's columns, in schema order.
    pub fields: &'static [&'static str],
}

/// v4's 11 structure-verifiable repositories in container order (generated —
/// see the module doc; `checked` is 11 on every v4 boot).
pub const STRUCTURAL_TABLES: &[StructuralTable] = &[
    StructuralTable {
        repository: "llmLogs",
        collection: "llm_logs",
        partition: Partition::LlmLogs,
        fields: &[
            "id",
            "userId",
            "type",
            "messageId",
            "chatId",
            "characterId",
            "autonomousRunId",
            "provider",
            "modelName",
            "connectionProfileId",
            "imageProfileId",
            "request",
            "response",
            "usage",
            "cacheUsage",
            "rawProviderUsage",
            "requestHashes",
            "durationMs",
            "createdAt",
            "updatedAt",
        ],
    },
    StructuralTable {
        repository: "helpDocChunks",
        collection: "help_doc_chunks",
        partition: Partition::Main,
        fields: &[
            "id",
            "docId",
            "chunkIndex",
            "heading",
            "content",
            "embedding",
            "createdAt",
            "updatedAt",
        ],
    },
    StructuralTable {
        repository: "docMountPoints",
        collection: "doc_mount_points",
        partition: Partition::MountIndex,
        fields: &[
            "id",
            "name",
            "basePath",
            "mountType",
            "storeType",
            "includePatterns",
            "excludePatterns",
            "enabled",
            "lastScannedAt",
            "scanStatus",
            "lastScanError",
            "conversionStatus",
            "conversionError",
            "fileCount",
            "chunkCount",
            "totalSizeBytes",
            "createdAt",
            "updatedAt",
        ],
    },
    StructuralTable {
        repository: "docMountFiles",
        collection: "doc_mount_files",
        partition: Partition::MountIndex,
        fields: &[
            "id",
            "sha256",
            "fileSizeBytes",
            "fileType",
            "source",
            "createdAt",
            "updatedAt",
        ],
    },
    StructuralTable {
        repository: "docMountFileLinks",
        collection: "doc_mount_file_links",
        partition: Partition::MountIndex,
        fields: &[
            "id",
            "fileId",
            "linkGroupId",
            "mountPointId",
            "relativePath",
            "fileName",
            "folderId",
            "originalFileName",
            "originalMimeType",
            "description",
            "descriptionUpdatedAt",
            "conversionStatus",
            "conversionError",
            "plainTextLength",
            "extractedText",
            "extractedTextSha256",
            "extractionStatus",
            "extractionError",
            "chunkCount",
            "allowEmbed",
            "allowCharacterRead",
            "allowCharacterWrite",
            "lastModified",
            "createdAt",
            "updatedAt",
        ],
    },
    StructuralTable {
        repository: "docMountFolders",
        collection: "doc_mount_folders",
        partition: Partition::MountIndex,
        fields: &[
            "id",
            "mountPointId",
            "parentId",
            "name",
            "path",
            "createdAt",
            "updatedAt",
        ],
    },
    StructuralTable {
        repository: "docMountChunks",
        collection: "doc_mount_chunks",
        partition: Partition::MountIndex,
        fields: &[
            "id",
            "linkId",
            "mountPointId",
            "chunkIndex",
            "content",
            "tokenCount",
            "headingContext",
            "embedding",
            "createdAt",
            "updatedAt",
        ],
    },
    StructuralTable {
        repository: "docMountDocuments",
        collection: "doc_mount_documents",
        partition: Partition::MountIndex,
        fields: &[
            "id",
            "fileId",
            "content",
            "contentSha256",
            "plainTextLength",
            "createdAt",
            "updatedAt",
        ],
    },
    StructuralTable {
        repository: "projectDocMountLinks",
        collection: "project_doc_mount_links",
        partition: Partition::MountIndex,
        fields: &["id", "projectId", "mountPointId", "createdAt", "updatedAt"],
    },
    StructuralTable {
        repository: "groupDocMountLinks",
        collection: "group_doc_mount_links",
        partition: Partition::MountIndex,
        fields: &["id", "groupId", "mountPointId", "createdAt", "updatedAt"],
    },
    StructuralTable {
        repository: "groupCharacterMembers",
        collection: "group_character_members",
        partition: Partition::MountIndex,
        fields: &["id", "groupId", "characterId", "createdAt", "updatedAt"],
    },
];

/// v4 `findTableShapeProblem` (`lib/database/table-shape.ts`): describe what is
/// wrong with `name`'s shape against `fields`, or `None` when it matches. v4's
/// two statements verbatim.
pub fn find_table_shape_problem(
    conn: &Connection,
    name: &str,
    fields: &[&str],
) -> Result<Option<String>, DbError> {
    let object_type: Option<String> = {
        let mut stmt = conn.prepare(
            "SELECT type FROM sqlite_master WHERE name = ? AND type IN ('table', 'view')",
        )?;
        let mut rows = stmt.query([name])?;
        match rows.next()? {
            Some(row) => Some(row.get(0)?),
            None => None,
        }
    };
    let object_type = match object_type {
        None => return Ok(Some(format!("table {name} does not exist"))),
        Some(t) => t,
    };
    if object_type != "table" {
        return Ok(Some(format!("{name} is a {object_type}, not a table")));
    }

    let present: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{name}\")"))?;
        let names = stmt
            .query_map([], |r| r.get::<_, String>("name"))?
            .collect::<Result<_, _>>()?;
        names
    };
    let missing: Vec<&str> = fields
        .iter()
        .copied()
        .filter(|column| !present.contains(*column))
        .collect();
    if !missing.is_empty() {
        return Ok(Some(format!(
            "table {name} is missing column{} {}",
            if missing.len() == 1 { "" } else { "s" },
            missing.join(", ")
        )));
    }
    Ok(None)
}

/// v4 `verifyStructure`'s unavailable form (`dedicated-db.repository.ts:179-183`):
/// `acquireDb()` threw (the database is degraded or not initialized).
pub fn unavailable(label: &str, message: &str) -> String {
    format!("{label} database unavailable: {message}")
}

/// v4 `verifyStructure`'s ensure-error form (`dedicated-db.repository.ts:197-199`,
/// `help-doc-chunks.repository.ts:54`): the ensure — or the shape check after
/// it — threw.
pub fn ensure_failed(collection: &str, label: &str, message: &str) -> String {
    format!("{collection} in {label} database: {message}")
}

/// `/api/health`'s `structure.message` on a sound boot (`app/api/health/route.ts:121`).
pub const STRUCTURE_HEALTHY_MESSAGE: &str = "All structural tables verified";

/// `/api/health`'s `structure.message` for `n` damaged tables (`route.ts:127-131`,
/// v4's pluralization transcribed — the route imports `next/server` and its
/// helper is not exported, so this is a recorded transcription, unit-pinned
/// below, not an oracle row).
pub fn structure_message(n: usize) -> String {
    format!(
        "{n} damaged table{}; reads through {} answer empty",
        if n == 1 { "" } else { "s" },
        if n == 1 { "it" } else { "them" }
    )
}

/// This boot's ensure failures, by collection: the FIRST failure text each
/// collection met (v4's `ensureTable` throws on its first failing statement, so
/// one repository has one ensure error). Filled by the lazy-home sub-steps in
/// `services::builtin_mounts` and by [`create_missing_structural_tables`];
/// read by [`verify_structural_tables`], which REUSES the text rather than
/// re-running an ensure (a second ensure would log `Failed to ensure …` twice
/// per boot).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnsureFailures(Vec<(&'static str, String)>);

impl EnsureFailures {
    /// Record `text` for `collection` unless it already failed this boot.
    pub fn record(&mut self, collection: &'static str, text: String) {
        if self.get(collection).is_none() {
            self.0.push((collection, text));
        }
    }

    /// The first failure text recorded for `collection`.
    pub fn get(&self, collection: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(c, _)| *c == collection)
            .map(|(_, t)| t.as_str())
    }

    /// Fold another collector in (first failure per collection still wins).
    pub fn extend(&mut self, other: EnsureFailures) {
        for (collection, text) in other.0 {
            self.record(collection, text);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// v4's `generateDDL` dump for the structural tables — the same artifact
/// fresh-instance provisioning replays (`services/provisioning`).
const FRESH_SCHEMA_JSON: &str = include_str!("../services/provisioning/fresh_schema.json");

/// The dump, parsed once per process (the boot walks ten tables).
static FRESH_SCHEMA: std::sync::LazyLock<serde_json::Value> = std::sync::LazyLock::new(|| {
    serde_json::from_str(FRESH_SCHEMA_JSON).expect("fresh_schema.json parses")
});

/// The dump's statements for `collection` in `partition`, in dump order: its
/// `CREATE TABLE`, then every `CREATE INDEX` on it.
///
/// The index target is matched in EITHER spelling: v4's repositories write one
/// structural index unquoted (`doc-mount-files.repository.ts`'s
/// `CREATE INDEX idx_doc_mount_files_sha256 ON doc_mount_files (sha256)`), and
/// the dump keeps v4's text. A quoted-only match created `doc_mount_files`
/// WITHOUT its sha256 index — a v5-only schema difference, caught at the
/// `e5c6bd0c0` unification (the tier-1 substrate test had filtered v4's side
/// the same way, so both sides dropped it).
fn fresh_ddl_for(partition: Partition, collection: &str) -> Vec<String> {
    let key = match partition {
        Partition::Main => "main",
        Partition::MountIndex => "mountIndex",
        Partition::LlmLogs => "llmLogs",
    };
    let table_prefix = format!("CREATE TABLE \"{collection}\" (");
    let on_quoted = format!(" ON \"{collection}\" (");
    let on_bare = format!(" ON {collection} (");
    FRESH_SCHEMA[key]
        .as_array()
        .expect("fresh_schema.json partition array")
        .iter()
        .filter_map(|v| v.as_str())
        .filter(|sql| {
            sql.starts_with(&table_prefix)
                || ((sql.starts_with("CREATE INDEX ") || sql.starts_with("CREATE UNIQUE INDEX "))
                    && (sql.contains(&on_quoted) || sql.contains(&on_bare)))
        })
        .map(str::to_string)
        .collect()
}

/// Does any table or view hold `name`? (The shape check's own first question.)
fn name_is_taken(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE name = ? AND type IN ('table', 'view')")?;
    Ok(stmt.exists([name])?)
}

/// v4's PHASE 3.1 ensure, for the one case v5's own boot ensures do not cover:
/// a dedicated structural table that does not exist at all. v4's
/// `verifyStructure` runs the repository's `ensureTable` first, whose
/// `CREATE TABLE IF NOT EXISTS` (+ the default and `onTableEnsured` indexes)
/// CREATES an absent table, which the shape check then reports sound
/// (`dedicated-db.repository.ts:138-156`, `:186-195`). v4 creates its tables
/// LAZILY, so an instance — and every committed fixture whose builder never
/// touched groups — can lack the link tables with nothing wrong at all.
///
/// RULED 2026-10-03 (the human, at P4.D248's STOP — the order's R3 had said
/// "report it", which the fixture census showed would answer a false
/// `degraded` 503 on healthy instances): create it, from v4's own DDL dump,
/// ONLY when no table or view holds the name. An existing table is never
/// touched (its damage is the shape check's to report), and a view standing in
/// for the table is left for the shape check too — a recorded divergence: v4's
/// index DDL fails on the view (`views may not be indexed`, the ensure form)
/// where v5 reports `X is a view, not a table`.
///
/// A failed creation logs v4's `ensureTable` line once (the `db::fallback`
/// home) and is recorded for the pass's ensure form. The main partition is not
/// walked: its one structural table, `help_doc_chunks`, has its own (fatal)
/// boot ensure. `None` partitions are skipped (R4).
pub fn create_missing_structural_tables(
    mount_index: Option<&Connection>,
    llm_logs: Option<&Connection>,
    failures: &mut EnsureFailures,
) {
    for table in STRUCTURAL_TABLES {
        let conn = match table.partition {
            Partition::Main => continue,
            Partition::MountIndex => mount_index,
            Partition::LlmLogs => llm_logs,
        };
        let Some(conn) = conn else { continue };
        if failures.get(table.collection).is_some() {
            continue;
        }
        let mut error = None;
        crate::db::fallback::ensure_table_or_log(table.collection, table.partition.label(), || {
            if name_is_taken(conn, table.collection)? {
                return Ok(());
            }
            // One SAVEPOINT per table: a later `CREATE INDEX` failing (a
            // squatted index name) rolls the table back too, so the next boot
            // retries and re-reports — v4's `IF NOT EXISTS` index DDL re-fails
            // on every boot. Without it the table would survive index-less and
            // read SOUND forever after (the shape check reads columns only).
            let created = conn
                .execute_batch("SAVEPOINT qt_structural_create")
                .and_then(|()| {
                    for sql in fresh_ddl_for(table.partition, table.collection) {
                        if let Err(e) = conn.execute_batch(&sql) {
                            let _ = conn.execute_batch(
                                "ROLLBACK TO qt_structural_create; RELEASE qt_structural_create",
                            );
                            return Err(e);
                        }
                    }
                    conn.execute_batch("RELEASE qt_structural_create")
                });
            created.map_err(|e| {
                let e = DbError::from(e);
                error = Some(crate::db::fallback::error_text(&e));
                e
            })
        });
        if let Some(text) = error {
            failures.record(table.collection, text);
        }
    }
}

/// One table's read in the pass, from the caller's connection pool.
pub enum TableRead {
    /// The table's partition file is absent (`None`) — a v5-only state, skipped
    /// and not counted (R4).
    PartitionAbsent,
    /// The table's partition opened DEGRADED (P4.159, dogfood #150 — v4's
    /// degraded mode): COUNTED, v4's `<label> database unavailable: <guard
    /// sentence>` problem, and no `Verified …` DEBUG (v4's `verifyStructure`
    /// returns before the shape check).
    PartitionDegraded,
    /// The shape check's answer, or the read's own failure.
    Read(Result<Option<String>, DbError>),
}

/// One problem the pass found, with v4's repository key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralProblem {
    pub repository: &'static str,
    pub problem: String,
}

/// v4's `context` on every pass line (`lib/startup/verify-structural-tables.ts`'s
/// child logger — v4's merge puts it FIRST).
const PASS_CONTEXT: &str = "startup.verify-structural-tables";

/// v4's PHASE 3.1 pass (`verifyStructuralTables`, `e5c6bd0c0`) over
/// [`STRUCTURAL_TABLES`] in container order, with v4's lines:
///
/// - a collection whose ensure already failed THIS boot reports that failure in
///   the ensure form — no shape check, no DEBUG, and never a second ensure;
/// - otherwise `read` runs the shape check: an absent partition is skipped and
///   not counted (R4); a DEGRADED partition is counted with v4's unavailable
///   form (P4.159); a read failure is v4's catch, the ensure form; a dedicated
///   table's answer logs v4's DEBUG `Verified dedicated-database table
///   structure` `{collection, dbTarget, ok}` (v4's root logger — no `context`;
///   `help_doc_chunks` logs none, as v4's own `verifyStructure` there does not);
/// - each problem logs ERROR `Structural table check failed; reads through this
///   repository will come back empty` `{context, repository, problem}`;
/// - then ERROR `Structural tables damaged; /api/health will report degraded
///   until repaired` `{context, checked, damaged}` or DEBUG `Structural tables
///   verified` `{context, checked}`.
pub fn verify_structural_tables(
    failures: &EnsureFailures,
    mut read: impl FnMut(&StructuralTable) -> TableRead,
) -> Vec<StructuralProblem> {
    let mut problems = Vec::new();
    let mut checked = 0usize;
    for table in STRUCTURAL_TABLES {
        let label = table.partition.label();
        let problem = if let Some(text) = failures.get(table.collection) {
            checked += 1;
            Some(ensure_failed(table.collection, label, text))
        } else {
            match read(table) {
                TableRead::PartitionAbsent => continue,
                TableRead::PartitionDegraded => {
                    checked += 1;
                    Some(log_partition_structural_unavailable(
                        table.partition,
                        table.repository,
                    ))
                }
                TableRead::Read(Err(error)) => {
                    checked += 1;
                    Some(ensure_failed(
                        table.collection,
                        label,
                        &crate::db::fallback::error_text(&error),
                    ))
                }
                TableRead::Read(Ok(problem)) => {
                    checked += 1;
                    if table.partition != Partition::Main {
                        tracing::debug!(
                            target: "quilltap::db",
                            collection = table.collection,
                            dbTarget = table.partition.db_target(),
                            ok = problem.is_none(),
                            "Verified dedicated-database table structure"
                        );
                    }
                    problem
                }
            }
        };
        if let Some(problem) = problem {
            tracing::error!(
                target: "quilltap::boot",
                context = PASS_CONTEXT,
                repository = table.repository,
                problem = problem.as_str(),
                "Structural table check failed; reads through this repository will come back empty"
            );
            problems.push(StructuralProblem {
                repository: table.repository,
                problem,
            });
        }
    }
    if problems.is_empty() {
        tracing::debug!(
            target: "quilltap::boot",
            context = PASS_CONTEXT,
            checked,
            "Structural tables verified"
        );
    } else {
        tracing::error!(
            target: "quilltap::boot",
            context = PASS_CONTEXT,
            checked,
            damaged = problems.len(),
            "Structural tables damaged; /api/health will report degraded until repaired"
        );
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_health_message_pluralizes_as_v4() {
        assert_eq!(
            structure_message(1),
            "1 damaged table; reads through it answer empty"
        );
        assert_eq!(
            structure_message(2),
            "2 damaged tables; reads through them answer empty"
        );
        assert_eq!(
            structure_message(3),
            "3 damaged tables; reads through them answer empty"
        );
    }

    /// The help-chunks template is transcribed (the oracle case cannot drive
    /// that repository without opening a main backend): v4's
    /// `` `${this.collectionName} in main database: ${extractErrorMessage(error)}` ``.
    #[test]
    fn the_help_chunks_template_is_v4s() {
        assert_eq!(
            ensure_failed(
                "help_doc_chunks",
                Partition::Main.label(),
                "no such column: docId"
            ),
            "help_doc_chunks in main database: no such column: docId"
        );
    }

    /// C1 item 2's strings — v4's guard sentences under `DB_LABELS` (the
    /// `degraded` rows of `table_shape_equivalence` compare them against v4's
    /// REAL guards; this pins the lane-local copy until it folds).
    #[test]
    fn a_degraded_partitions_problem_is_v4s_unavailable_form() {
        assert_eq!(
            log_partition_structural_unavailable(Partition::MountIndex, "docMountPoints"),
            "mount index database unavailable: Mount index database is in degraded mode"
        );
        assert_eq!(
            log_partition_structural_unavailable(Partition::LlmLogs, "llmLogs"),
            "LLM logs database unavailable: LLM logs database is in degraded mode"
        );
    }

    /// A degraded partition is COUNTED (`checked`) and reported per table; an
    /// absent one is skipped (R4) — the two must never collapse (R-B).
    #[test]
    fn a_degraded_partition_is_counted_where_an_absent_one_is_skipped() {
        let (problems, lines) = crate::test_support::captured_with(|| {
            verify_structural_tables(&EnsureFailures::default(), |t| match t.partition {
                Partition::LlmLogs => TableRead::PartitionDegraded,
                Partition::MountIndex => TableRead::PartitionAbsent,
                Partition::Main => TableRead::Read(Ok(None)),
            })
        });
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].repository, "llmLogs");
        assert!(
            lines.iter().any(|l| l.ends_with("checked=2 damaged=1")),
            "{lines:#?}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Verified dedicated-database")),
            "{lines:#?}"
        );
    }

    #[test]
    fn the_census_is_eleven_tables_led_by_llm_logs() {
        assert_eq!(STRUCTURAL_TABLES.len(), 11);
        assert_eq!(STRUCTURAL_TABLES[0].collection, "llm_logs");
        assert_eq!(STRUCTURAL_TABLES[1].collection, "help_doc_chunks");
    }

    /// Found at the `e5c6bd0c0` unification: `doc_mount_files`' sha256 index is
    /// written UNQUOTED in v4's dump; a created table must carry it.
    #[test]
    fn a_created_doc_mount_files_carries_its_unquoted_sha256_index() {
        let mount = Connection::open_in_memory().unwrap();
        let mut failures = EnsureFailures::default();
        create_missing_structural_tables(Some(&mount), None, &mut failures);
        assert!(failures.is_empty(), "{failures:?}");
        let has: bool = mount
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' \
                 AND name = 'idx_doc_mount_files_sha256' AND tbl_name = 'doc_mount_files')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            has,
            "doc_mount_files was created without idx_doc_mount_files_sha256"
        );
    }

    /// A later index failing rolls the table back with it, so the next boot
    /// retries and re-reports instead of reading an index-less table SOUND.
    #[test]
    fn a_failed_index_rolls_the_created_table_back_and_is_recorded() {
        let mount = Connection::open_in_memory().unwrap();
        // Squat the sha256 index's name with a table.
        mount
            .execute_batch("CREATE TABLE idx_doc_mount_files_sha256 (x)")
            .unwrap();
        let mut failures = EnsureFailures::default();
        create_missing_structural_tables(Some(&mount), None, &mut failures);
        let text = failures
            .get("doc_mount_files")
            .expect("the failure is recorded");
        assert!(text.contains("idx_doc_mount_files_sha256"), "{text}");
        assert!(
            !name_is_taken(&mount, "doc_mount_files").unwrap(),
            "the table must roll back with its index"
        );
        // Every other mount table still created.
        assert!(name_is_taken(&mount, "doc_mount_file_links").unwrap());
        // Squatter gone → a second pass creates it whole.
        mount
            .execute_batch("DROP TABLE idx_doc_mount_files_sha256")
            .unwrap();
        let mut again = EnsureFailures::default();
        create_missing_structural_tables(Some(&mount), None, &mut again);
        assert!(again.is_empty(), "{again:?}");
        assert!(name_is_taken(&mount, "doc_mount_files").unwrap());
    }
}
