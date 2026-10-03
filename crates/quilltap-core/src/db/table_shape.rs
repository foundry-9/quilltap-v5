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

    #[test]
    fn the_census_is_eleven_tables_led_by_llm_logs() {
        assert_eq!(STRUCTURAL_TABLES.len(), 11);
        assert_eq!(STRUCTURAL_TABLES[0].collection, "llm_logs");
        assert_eq!(STRUCTURAL_TABLES[1].collection, "help_doc_chunks");
    }
}
