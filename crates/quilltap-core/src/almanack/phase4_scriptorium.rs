//! The Almanack — Phase 4, "Touring the Scriptorium"
//! (v4 `lib/tools/almanack/phase4-scriptorium.ts`).
//!
//! The mount-index database — the half of the application v4's old capabilities
//! report never opened. Since the 4.4–4.9 cutovers this is where character
//! content, wardrobe, custom tools, photos, mail and scenarios actually live,
//! and where nearly all the bytes are.
//!
//! Everything reads through [`mount_rows`] (the mount-index door), never the
//! main connection — that addresses the main DB and would silently return
//! nothing for every `doc_mount_*` table (dogfood #16's exact shape).
//!
//! Tier classification (character vault / project store / group store / general)
//! is a cross-database question: the mount rows themselves don't know what they
//! belong to, so the owning ids are read from the main DB first and carried in.

use rusqlite::types::Value as SqlValue;
use rusqlite::ToSql;

use crate::db::runtime::Db;
use crate::db::{instance_settings, DbError};
use crate::pascal::custom_tool_types::TOOLS_FOLDER;
use crate::pascal::roster::list_all_custom_tools;
use crate::photos::photos_paths::PHOTOS_FOLDER;
use crate::post_office::mailbox::MAIL_FOLDER;
use crate::wardrobe_instructions::WARDROBE_INSTRUCTIONS_PATH;

use super::db::{
    in_clause, is_mount_index_available, main_rows, mount_row, mount_rows, num_col, opt_text, text,
};
use super::types::*;

/// Root-level folder each character vault keeps its wardrobe in.
const WARDROBE_FOLDER: &str = "Wardrobe";
/// Root-level folder holding one markdown file per scenario.
const SCENARIOS_FOLDER: &str = "Scenarios";
/// The keystone file: a vault without it reads as broken (a hard 503).
const VAULT_KEYSTONE: &str = "properties.json";
/// Optional per-character free-form metadata document.
const VAULT_METADATA: &str = "metadata.json";

/// Which mount ids belong to which tier, read from the main database
/// (v4 `MountOwnership`).
struct MountOwnership {
    character_vaults: Vec<String>,
    project_stores: Vec<String>,
    group_stores: Vec<String>,
    general_mount_id: Option<String>,
    uploads_mount_id: Option<String>,
    lantern_mount_id: Option<String>,
}

fn collect_mount_ownership(db: &Db, user_id: &str) -> MountOwnership {
    let character_vaults = main_rows(
        db,
        r#"SELECT "characterDocumentMountPointId" AS id FROM "characters"
         WHERE "userId" = ? AND "characterDocumentMountPointId" IS NOT NULL"#,
        &[&user_id],
        "scriptorium.characterVaultIds",
        |r| text(r, 0),
    );
    let project_stores = main_rows(
        db,
        r#"SELECT "officialMountPointId" AS id FROM "projects" WHERE "officialMountPointId" IS NOT NULL"#,
        &[],
        "scriptorium.projectStoreIds",
        |r| text(r, 0),
    );
    let group_stores = main_rows(
        db,
        r#"SELECT "officialMountPointId" AS id FROM "groups" WHERE "officialMountPointId" IS NOT NULL"#,
        &[],
        "scriptorium.groupStoreIds",
        |r| text(r, 0),
    );

    MountOwnership {
        // v4 builds `Set`s here; the ids come from a UNIQUE-ish column per row
        // and are only ever used as an `IN (…)` list, so dedup preserves both
        // the semantics and the placeholder count.
        character_vaults: dedup(character_vaults),
        project_stores: dedup(project_stores),
        group_stores: dedup(group_stores),
        general_mount_id: db
            .read_main(instance_settings::get_general_mount_point_id)
            .ok()
            .flatten(),
        uploads_mount_id: db
            .read_main(instance_settings::get_user_uploads_mount_point_id)
            .ok()
            .flatten(),
        lantern_mount_id: db
            .read_main(instance_settings::get_lantern_backgrounds_mount_point_id)
            .ok()
            .flatten(),
    }
}

/// `new Set(...)` — first-seen order, which is what iterating a JS `Set` yields.
fn dedup(ids: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

/// Bind an owned id list plus trailing scalars as query params.
fn params_with<'a>(ids: &'a [SqlValue], extra: &'a [SqlValue]) -> Vec<&'a dyn ToSql> {
    ids.iter()
        .chain(extra.iter())
        .map(|v| v as &dyn ToSql)
        .collect()
}

fn sql_values(ids: &[String]) -> Vec<SqlValue> {
    ids.iter().map(|v| SqlValue::Text(v.clone())).collect()
}

/// v4 `countLinksInFolder` — links whose path sits under `folder/`.
fn count_links_in_folder(db: &Db, mount_ids: &[String], folder: &str) -> f64 {
    count_links_in_folder_excluding(db, mount_ids, folder, None)
}

/// v4 `countLinksInFolder(mountIds, folder, excludeRelativePath?)` (`b86bb1a5`).
/// `exclude_relative_path` drops one exact (lowercased) path from the count —
/// used to keep `Wardrobe/instructions.md` out of the garment figures.
fn count_links_in_folder_excluding(
    db: &Db,
    mount_ids: &[String],
    folder: &str,
    exclude_relative_path: Option<&str>,
) -> f64 {
    let ids = sql_values(mount_ids);
    let mut extra = vec![SqlValue::Text(format!("{}/%", folder.to_lowercase()))];
    let exclusion = match exclude_relative_path {
        Some(path) => {
            extra.push(SqlValue::Text(path.to_lowercase()));
            r#" AND lower("relativePath") != ?"#
        }
        None => "",
    };
    let sql = format!(
        r#"SELECT COUNT(*) AS n FROM "doc_mount_file_links"
     WHERE "mountPointId" IN {} AND lower("relativePath") LIKE ?{}"#,
        in_clause(mount_ids),
        exclusion
    );
    mount_row(
        db,
        &sql,
        &params_with(&ids, &extra),
        &format!("scriptorium.countLinksInFolder:{folder}"),
        |r| num_col(r, 0),
    )
    .unwrap_or(0.0)
}

/// v4 `countLinksMatchingContent` — the cheap way to ask a frontmatter question
/// in SQL.
///
/// Approximate by construction: a substring match on the whole document, not a
/// YAML parse. Used only for coarse figures (archived wardrobe items,
/// unannounced letters) where an occasional false positive in a body is
/// immaterial and parsing every document would not be.
fn count_links_matching_content(db: &Db, mount_ids: &[String], folder: &str, pattern: &str) -> f64 {
    let ids = sql_values(mount_ids);
    let extra = [
        SqlValue::Text(format!("{}/%", folder.to_lowercase())),
        SqlValue::Text(pattern.to_string()),
    ];
    let sql = format!(
        r#"SELECT COUNT(*) AS n
     FROM "doc_mount_file_links" l
     JOIN "doc_mount_documents" d ON d."fileId" = l."fileId"
     WHERE l."mountPointId" IN {}
       AND lower(l."relativePath") LIKE ?
       AND d."content" LIKE ?"#,
        in_clause(mount_ids)
    );
    mount_row(
        db,
        &sql,
        &params_with(&ids, &extra),
        &format!("scriptorium.countLinksMatchingContent:{folder}"),
        |r| num_col(r, 0),
    )
    .unwrap_or(0.0)
}

/// v4 `bytesInFolder` — total bytes of links under `folder/` (blobs + text).
fn bytes_in_folder(db: &Db, mount_ids: &[String], folder: &str) -> f64 {
    let ids = sql_values(mount_ids);
    let extra = [SqlValue::Text(format!("{}/%", folder.to_lowercase()))];
    let sql = format!(
        r#"SELECT COALESCE(SUM(f."fileSizeBytes"), 0) AS bytes
     FROM "doc_mount_file_links" l
     JOIN "doc_mount_files" f ON f."id" = l."fileId"
     WHERE l."mountPointId" IN {} AND lower(l."relativePath") LIKE ?"#,
        in_clause(mount_ids)
    );
    mount_row(
        db,
        &sql,
        &params_with(&ids, &extra),
        &format!("scriptorium.bytesInFolder:{folder}"),
        |r| num_col(r, 0),
    )
    .unwrap_or(0.0)
}

/// v4 `countStateFiles` — stores carrying a non-empty root `state.json`.
fn count_state_files(db: &Db, mount_ids: &[String]) -> f64 {
    let ids = sql_values(mount_ids);
    let sql = format!(
        r#"SELECT COUNT(DISTINCT l."mountPointId") AS n
     FROM "doc_mount_file_links" l
     JOIN "doc_mount_documents" d ON d."fileId" = l."fileId"
     WHERE l."mountPointId" IN {}
       AND lower(l."relativePath") = 'state.json'
       AND trim(d."content") NOT IN ('', '{{}}')"#,
        in_clause(mount_ids)
    );
    mount_row(
        db,
        &sql,
        &params_with(&ids, &[]),
        "scriptorium.stateFiles",
        |r| num_col(r, 0),
    )
    .unwrap_or(0.0)
}

/// v4 `emptyScriptorium()` — an empty tour, for when the mount index is
/// unreachable or a collector fails.
pub fn empty_scriptorium() -> ScriptoriumInfo {
    ScriptoriumInfo {
        available: false,
        mount_points: ScriptoriumMountPoints {
            total: 0.0,
            enabled: 0.0,
            by_kind: Vec::new(),
            scan_errors: 0.0,
            conversion_errors: 0.0,
            scan_statuses: Vec::new(),
            conversion_statuses: Vec::new(),
            well_known: Vec::new(),
        },
        content: ScriptoriumContent {
            file_rows: 0.0,
            link_rows: 0.0,
            document_rows: 0.0,
            document_text_bytes: 0.0,
            blob_rows: 0.0,
            blob_bytes: 0.0,
            blobs_by_mime_type: Vec::new(),
            chunk_rows: 0.0,
            unembedded_chunks: 0.0,
            chunk_tokens: 0.0,
        },
        links: ScriptoriumLinks {
            dedup_ratio: 0.0,
            hard_link_groups: 0.0,
            hard_linked_links: 0.0,
            extraction_errors: 0.0,
            conversion_errors: 0.0,
            policy_embed_denied: 0.0,
            policy_character_read_denied: 0.0,
            policy_character_write_denied: 0.0,
        },
        character_vaults: CharacterVaultHealth {
            total: 0.0,
            with_keystone: 0.0,
            missing_keystone: 0.0,
            with_metadata: 0.0,
        },
        wardrobe: Vec::new(),
        custom_tools: CustomToolsInfo {
            total: 0.0,
            by_store: Vec::new(),
            with_presets: 0.0,
            preset_files: 0.0,
            with_llm_consult: 0.0,
            with_effects: 0.0,
            metadata_gated: 0.0,
            parse_failures: 0.0,
            parse_failure_detail: Vec::new(),
        },
        post_office: PostOfficeInfo {
            letters: 0.0,
            unannounced: 0.0,
            mailboxes: 0.0,
        },
        photos: PhotosInfo {
            character_vault_photos: 0.0,
            character_vault_bytes: 0.0,
            user_gallery_photos: 0.0,
            user_gallery_bytes: 0.0,
        },
        scenarios: Vec::new(),
        state_cascade: StateCascadeInfo {
            chats_with_state: 0.0,
            projects_with_state: 0.0,
            groups_with_state: 0.0,
            general_state_present: false,
        },
    }
}

/// v4 `collectScriptorium(userId, chatsWithNonEmptyState)` — the whole tour.
pub fn collect_scriptorium(
    db: &Db,
    user_id: &str,
    chats_with_state: f64,
) -> Result<ScriptoriumInfo, DbError> {
    if !is_mount_index_available(db) {
        tracing::warn!("Mount index unavailable; Scriptorium section will report as unreachable");
        return Ok(empty_scriptorium());
    }

    let ownership = collect_mount_ownership(db, user_id);

    // --- Mount points -------------------------------------------------------

    let by_kind = mount_rows(
        db,
        r#"SELECT "mountType"                                          AS mountType,
            "storeType"                                          AS storeType,
            COUNT(*)                                             AS count,
            SUM(CASE WHEN "enabled" = 1 THEN 1 ELSE 0 END)       AS enabled,
            COALESCE(SUM("fileCount"), 0)                        AS fileCount,
            COALESCE(SUM("chunkCount"), 0)                       AS chunkCount,
            COALESCE(SUM("totalSizeBytes"), 0)                   AS totalSizeBytes
     FROM "doc_mount_points"
     GROUP BY mountType, storeType
     ORDER BY count DESC"#,
        &[],
        "scriptorium.mountsByKind",
        |r| {
            Ok(MountPointSummaryRow {
                mount_type: text(r, 0)?,
                store_type: text(r, 1)?,
                count: num_col(r, 2)?,
                enabled: num_col(r, 3)?,
                file_count: num_col(r, 4)?,
                chunk_count: num_col(r, 5)?,
                total_size_bytes: num_col(r, 6)?,
            })
        },
    );

    let scan_statuses = mount_rows(
        db,
        r#"SELECT COALESCE("scanStatus", 'idle') AS status, COUNT(*) AS count
     FROM "doc_mount_points" GROUP BY status ORDER BY count DESC"#,
        &[],
        "scriptorium.scanStatuses",
        |r| {
            Ok(StatusCountRow {
                status: text(r, 0)?,
                count: num_col(r, 1)?,
            })
        },
    );

    let conversion_statuses = mount_rows(
        db,
        r#"SELECT COALESCE("conversionStatus", 'idle') AS status, COUNT(*) AS count
     FROM "doc_mount_points" GROUP BY status ORDER BY count DESC"#,
        &[],
        "scriptorium.conversionStatuses",
        |r| {
            Ok(StatusCountRow {
                status: text(r, 0)?,
                count: num_col(r, 1)?,
            })
        },
    );

    let mount_totals = mount_row(
        db,
        r#"SELECT COUNT(*)                                                        AS total,
            SUM(CASE WHEN "enabled" = 1 THEN 1 ELSE 0 END)                  AS enabled,
            SUM(CASE WHEN "lastScanError" IS NOT NULL
                      AND "lastScanError" != '' THEN 1 ELSE 0 END)          AS scanErr,
            SUM(CASE WHEN "conversionStatus" = 'error' THEN 1 ELSE 0 END)   AS convErr
     FROM "doc_mount_points""#,
        &[],
        "scriptorium.mountTotals",
        |r| {
            Ok((
                num_col(r, 0)?,
                num_col(r, 1)?,
                num_col(r, 2)?,
                num_col(r, 3)?,
            ))
        },
    )
    .unwrap_or((0.0, 0.0, 0.0, 0.0));

    let well_known_specs: [(&str, &str, Option<&String>); 3] = [
        (
            "Quilltap Uploads",
            "userUploadsMountPointId",
            ownership.uploads_mount_id.as_ref(),
        ),
        (
            "Quilltap General",
            "generalMountPointId",
            ownership.general_mount_id.as_ref(),
        ),
        (
            "Lantern Backgrounds",
            "lanternBackgroundsMountPointId",
            ownership.lantern_mount_id.as_ref(),
        ),
    ];

    let well_known: Vec<WellKnownMountInfo> = well_known_specs
        .iter()
        .map(|(label, setting_key, id)| {
            let Some(id) = id else {
                return WellKnownMountInfo {
                    label: (*label).to_string(),
                    setting_key: (*setting_key).to_string(),
                    mount_point_id: None,
                    name: None,
                    resolved: false,
                };
            };
            let row = mount_row(
                db,
                r#"SELECT "name" FROM "doc_mount_points" WHERE "id" = ?"#,
                &[id],
                "scriptorium.wellKnownMount",
                |r| opt_text(r, 0),
            );
            WellKnownMountInfo {
                label: (*label).to_string(),
                setting_key: (*setting_key).to_string(),
                mount_point_id: Some((*id).clone()),
                name: row.clone().flatten(),
                // A pointer that resolves to nothing means the provisioning
                // migration ran against a database that has since lost the row.
                resolved: row.is_some(),
            }
        })
        .collect();

    // --- Content totals -----------------------------------------------------

    let file_rows = mount_row(
        db,
        r#"SELECT COUNT(*) AS n FROM "doc_mount_files""#,
        &[],
        "scriptorium.files",
        |r| num_col(r, 0),
    )
    .unwrap_or(0.0);
    let link_rows = mount_row(
        db,
        r#"SELECT COUNT(*) AS n FROM "doc_mount_file_links""#,
        &[],
        "scriptorium.links",
        |r| num_col(r, 0),
    )
    .unwrap_or(0.0);

    let document_totals = mount_row(
        db,
        r#"SELECT COUNT(*) AS rows, COALESCE(SUM(length("content")), 0) AS bytes
     FROM "doc_mount_documents""#,
        &[],
        "scriptorium.documents",
        |r| Ok((num_col(r, 0)?, num_col(r, 1)?)),
    )
    .unwrap_or((0.0, 0.0));

    let blob_totals = mount_row(
        db,
        r#"SELECT COUNT(*) AS rows, COALESCE(SUM("sizeBytes"), 0) AS bytes FROM "doc_mount_blobs""#,
        &[],
        "scriptorium.blobs",
        |r| Ok((num_col(r, 0)?, num_col(r, 1)?)),
    )
    .unwrap_or((0.0, 0.0));

    let blobs_by_mime_type = mount_rows(
        db,
        r#"SELECT "storedMimeType" AS storedMimeType, COUNT(*) AS count, COALESCE(SUM("sizeBytes"), 0) AS bytes
     FROM "doc_mount_blobs"
     GROUP BY storedMimeType ORDER BY bytes DESC"#,
        &[],
        "scriptorium.blobsByMimeType",
        |r| {
            Ok(BlobMimeRow {
                stored_mime_type: text(r, 0)?,
                count: num_col(r, 1)?,
                bytes: num_col(r, 2)?,
            })
        },
    );

    let chunk_totals = mount_row(
        db,
        r#"SELECT COUNT(*)                                                    AS rows,
            SUM(CASE WHEN "embedding" IS NULL THEN 1 ELSE 0 END)        AS unembedded,
            COALESCE(SUM("tokenCount"), 0)                              AS tokens
     FROM "doc_mount_chunks""#,
        &[],
        "scriptorium.chunks",
        |r| Ok((num_col(r, 0)?, num_col(r, 1)?, num_col(r, 2)?)),
    )
    .unwrap_or((0.0, 0.0, 0.0));

    // --- Links, hard links and per-document policy --------------------------

    let link_totals = mount_row(
        db,
        r#"SELECT COUNT(DISTINCT "linkGroupId")                                   AS groups,
            SUM(CASE WHEN "linkGroupId" IS NOT NULL THEN 1 ELSE 0 END)      AS grouped,
            SUM(CASE WHEN "extractionStatus" = 'error' THEN 1 ELSE 0 END)   AS extractionErrors,
            SUM(CASE WHEN "conversionStatus" = 'error' THEN 1 ELSE 0 END)   AS conversionErrors,
            SUM(CASE WHEN "allowEmbed" = 0 THEN 1 ELSE 0 END)               AS noEmbed,
            SUM(CASE WHEN "allowCharacterRead" = 0 THEN 1 ELSE 0 END)       AS noRead,
            SUM(CASE WHEN "allowCharacterWrite" = 0 THEN 1 ELSE 0 END)      AS noWrite
     FROM "doc_mount_file_links""#,
        &[],
        "scriptorium.linkTotals",
        |r| {
            Ok((
                num_col(r, 0)?,
                num_col(r, 1)?,
                num_col(r, 2)?,
                num_col(r, 3)?,
                num_col(r, 4)?,
                num_col(r, 5)?,
                num_col(r, 6)?,
            ))
        },
    )
    .unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0));

    // --- Character-vault health --------------------------------------------

    let vault_ids = &ownership.character_vaults;
    let vault_sql_ids = sql_values(vault_ids);

    let count_vaults_with = |relative_path: &str, context: &str| -> f64 {
        let extra = [SqlValue::Text(relative_path.to_lowercase())];
        let sql = format!(
            r#"SELECT COUNT(DISTINCT "mountPointId") AS n FROM "doc_mount_file_links"
       WHERE "mountPointId" IN {} AND lower("relativePath") = ?"#,
            in_clause(vault_ids)
        );
        mount_row(
            db,
            &sql,
            &params_with(&vault_sql_ids, &extra),
            context,
            |r| num_col(r, 0),
        )
        .unwrap_or(0.0)
    };
    let with_keystone = count_vaults_with(VAULT_KEYSTONE, "scriptorium.vaultKeystones");
    let with_metadata = count_vaults_with(VAULT_METADATA, "scriptorium.vaultMetadata");

    // --- Wardrobe by tier ---------------------------------------------------

    let general_ids: Vec<String> = ownership.general_mount_id.iter().cloned().collect();
    let tiers: [(&str, &Vec<String>); 4] = [
        ("character", vault_ids),
        ("project", &ownership.project_stores),
        ("group", &ownership.group_stores),
        ("general", &general_ids),
    ];

    let wardrobe: Vec<WardrobeTierRow> = tiers
        .iter()
        .map(|(tier, ids)| WardrobeTierRow {
            tier: (*tier).to_string(),
            // The dressing-instructions file lives in the folder but is not a
            // garment.
            items: count_links_in_folder_excluding(
                db,
                ids,
                WARDROBE_FOLDER,
                Some(WARDROBE_INSTRUCTIONS_PATH),
            ),
            // v4 `b86bb1a5` gave the sibling `archived` LIKE-count NO exclusion:
            // an instructions file containing the literal `archived: true` still
            // counts as an archived garment. Carried, asymmetry and all.
            archived: count_links_matching_content(db, ids, WARDROBE_FOLDER, "%archived: true%"),
        })
        .collect();

    // --- Custom tools (Pascal's Workbench) ----------------------------------

    let custom_tools = collect_custom_tools(db);

    // --- The Post Office ----------------------------------------------------

    let letters = count_links_in_folder(db, vault_ids, MAIL_FOLDER);
    let unannounced = count_links_matching_content(db, vault_ids, MAIL_FOLDER, "%alerted: false%");
    let mailboxes = {
        let extra = [SqlValue::Text(format!("{}/%", MAIL_FOLDER.to_lowercase()))];
        let sql = format!(
            r#"SELECT COUNT(DISTINCT "mountPointId") AS n FROM "doc_mount_file_links"
       WHERE "mountPointId" IN {} AND lower("relativePath") LIKE ?"#,
            in_clause(vault_ids)
        );
        mount_row(
            db,
            &sql,
            &params_with(&vault_sql_ids, &extra),
            "scriptorium.mailboxes",
            |r| num_col(r, 0),
        )
        .unwrap_or(0.0)
    };

    // --- Photos -------------------------------------------------------------

    let gallery_ids: Vec<String> = ownership.uploads_mount_id.iter().cloned().collect();

    // --- Scenarios by tier --------------------------------------------------

    let scenarios: Vec<ScenarioTierRow> = tiers
        .iter()
        .map(|(tier, ids)| ScenarioTierRow {
            tier: (*tier).to_string(),
            count: count_links_in_folder(db, ids, SCENARIOS_FOLDER),
            // The same LIKE match the wardrobe tally uses — the flag is
            // frontmatter, so an archived scenario is one whose file body
            // contains `archived: true`.
            archived: count_links_matching_content(db, ids, SCENARIOS_FOLDER, "%archived: true%"),
        })
        .collect();

    // --- State cascade ------------------------------------------------------

    let projects_with_state = count_state_files(db, &ownership.project_stores);
    let groups_with_state = count_state_files(db, &ownership.group_stores);
    let general_state_present =
        ownership.general_mount_id.is_some() && count_state_files(db, &general_ids) > 0.0;

    Ok(ScriptoriumInfo {
        available: true,
        mount_points: ScriptoriumMountPoints {
            total: mount_totals.0,
            enabled: mount_totals.1,
            by_kind,
            scan_errors: mount_totals.2,
            conversion_errors: mount_totals.3,
            scan_statuses,
            conversion_statuses,
            well_known,
        },
        content: ScriptoriumContent {
            file_rows,
            link_rows,
            document_rows: document_totals.0,
            document_text_bytes: document_totals.1,
            blob_rows: blob_totals.0,
            blob_bytes: blob_totals.1,
            blobs_by_mime_type,
            chunk_rows: chunk_totals.0,
            unembedded_chunks: chunk_totals.1,
            chunk_tokens: chunk_totals.2,
        },
        links: ScriptoriumLinks {
            dedup_ratio: if file_rows > 0.0 {
                link_rows / file_rows
            } else {
                0.0
            },
            hard_link_groups: link_totals.0,
            hard_linked_links: link_totals.1,
            extraction_errors: link_totals.2,
            conversion_errors: link_totals.3,
            policy_embed_denied: link_totals.4,
            policy_character_read_denied: link_totals.5,
            policy_character_write_denied: link_totals.6,
        },
        character_vaults: CharacterVaultHealth {
            total: vault_ids.len() as f64,
            with_keystone,
            // A linked vault with no `properties.json` is precisely the state
            // that makes a character read throw — a 503 for that character.
            missing_keystone: (vault_ids.len() as f64 - with_keystone).max(0.0),
            with_metadata,
        },
        wardrobe,
        custom_tools,
        post_office: PostOfficeInfo {
            letters,
            unannounced,
            mailboxes,
        },
        photos: PhotosInfo {
            character_vault_photos: count_links_in_folder(db, vault_ids, PHOTOS_FOLDER),
            character_vault_bytes: bytes_in_folder(db, vault_ids, PHOTOS_FOLDER),
            user_gallery_photos: count_links_in_folder(db, &gallery_ids, PHOTOS_FOLDER),
            user_gallery_bytes: bytes_in_folder(db, &gallery_ids, PHOTOS_FOLDER),
        },
        scenarios,
        state_cascade: StateCascadeInfo {
            chats_with_state,
            projects_with_state,
            groups_with_state,
            general_state_present,
        },
    })
}

/// v4's custom-tools inventory block — the whole thing is inside a `try/catch`
/// there, so a failing library read degrades to the empty shape rather than
/// taking the tour down.
fn collect_custom_tools(db: &Db) -> CustomToolsInfo {
    let Ok(library) = db.read_mount_index(|conn| Ok(list_all_custom_tools(conn))) else {
        tracing::warn!("Failed to inventory custom tools");
        return empty_scriptorium().custom_tools;
    };

    let mut by_store_counts: Vec<(String, f64)> = Vec::new();
    let mut with_llm_consult = 0.0;
    let mut with_effects = 0.0;
    let mut metadata_gated = 0.0;

    for entry in &library.entries {
        match by_store_counts
            .iter_mut()
            .find(|(s, _)| *s == entry.mount_name)
        {
            Some((_, n)) => *n += 1.0,
            None => by_store_counts.push((entry.mount_name.clone(), 1.0)),
        }
        if entry.definition.llm.is_some() {
            with_llm_consult += 1.0;
        }
        if entry
            .definition
            .effects
            .as_ref()
            .is_some_and(|e| !e.is_empty())
        {
            with_effects += 1.0;
        }
        if entry.definition.available_when.is_some() || entry.definition.withheld_when.is_some() {
            metadata_gated += 1.0;
        }
    }

    // Presets are sibling `Tools/{tool}.{preset}.settings.json` documents, so
    // they are counted from the file links rather than the definitions.
    let preset_rows: Vec<String> = mount_rows(
        db,
        r#"SELECT "relativePath" AS relativePath FROM "doc_mount_file_links"
       WHERE lower("relativePath") LIKE ? AND lower("relativePath") LIKE '%.settings.json'"#,
        &[&format!("{}/%", TOOLS_FOLDER.to_lowercase())],
        "scriptorium.toolPresets",
        |r| text(r, 0),
    );
    let prefix_len = TOOLS_FOLDER.len() + 1;
    let mut tools_with_presets: Vec<String> = Vec::new();
    for path in &preset_rows {
        // v4 slices by the CONSTANT's length, not by locating the separator, so
        // a path that merely matched case-insensitively is sliced the same way.
        let filename: String = path.chars().skip(prefix_len).collect();
        let Some(dot) = filename.find('.').filter(|d| *d > 0) else {
            continue;
        };
        let name = filename[..dot].to_string();
        if !tools_with_presets.contains(&name) {
            tools_with_presets.push(name);
        }
    }

    // v4 sorts by count DESC then store name ascending (`localeCompare`).
    by_store_counts.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| crate::collation::locale_compare(&a.0, &b.0))
    });

    CustomToolsInfo {
        total: library.entries.len() as f64,
        by_store: by_store_counts
            .into_iter()
            .map(|(store, count)| CustomToolStoreRow { store, count })
            .collect(),
        with_presets: tools_with_presets.len() as f64,
        preset_files: preset_rows.len() as f64,
        with_llm_consult,
        with_effects,
        metadata_gated,
        parse_failures: library.errors.len() as f64,
        parse_failure_detail: library
            .errors
            .iter()
            .take(20)
            .map(|e| CustomToolParseFailure {
                store: e.mount_name.clone(),
                path: e.definition_path.clone(),
                reason: e.reason.clone(),
            })
            .collect(),
    }
}
