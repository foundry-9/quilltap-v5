//! v4 `import-document-stores.ts` — import Scriptorium document stores: mount
//! point configs plus, for database-backed mounts, folder structures, document
//! bodies, blobs, and project↔store links. Source mount-point ids map onto the
//! created/reused targets on `idMaps.mountPoints` for later reconciliation
//! (`characterDocumentMountPointId`).
//!
//! ## v4 quirks carried deliberately
//!
//! - **The overwrite clear leaves `doc_mount_blobs` rows behind.** v4's
//!   `docMountBlobs.deleteByMountPointId` deletes the LINK rows and the orphaned
//!   FILE rows — never a `doc_mount_blobs` row (`doc-mount-blobs.repository.ts:628`
//!   is a copy of the files variant). The subsequent
//!   `docMountFiles.deleteByMountPointId` then snapshots an already-emptied link
//!   set and is a no-op. [`overwrite_clear_mount`] reproduces the four calls'
//!   net effect in v4's exact order.
//! - **Folder `parentId` is NOT remapped** (`import-document-stores.ts:122-127`
//!   says so itself) — imported folders keep their source `parentId`, which
//!   dangles because every created folder row gets a fresh id.
//! - **Document `folderId` is ignored**: v4's `linkDocumentContent` derives the
//!   folder from `relativePath` and only logs when the caller's disagrees, so
//!   passing `doc.folderId ?? null` is observably a no-op — v5's
//!   `link_document_content` derives identically.

use base64::Engine;
use rusqlite::Connection;
use serde_json::Value;

use super::{ConflictStrategy, IdMaps, ImportOptions};
use crate::db::doc_mount_blobs::{CreateBlobInput, DocMountBlobsRepository};
use crate::db::doc_mount_file_links::{DocMountFileLinksRepository, LinkDocumentInput};
use crate::db::doc_mount_folders::{self, DocMountFoldersRepository};
use crate::db::doc_mount_points::{self, DmpCreate, DmpUpdate, DocMountPointsRepository};
use crate::db::ensure_official_store::next_unique_mount_point_name;
use crate::db::project_doc_mount_links::{self, PdmlCreate, ProjectDocMountLinksRepository};
use crate::db::DbError;

/// v4 `DocumentStoreImportCounts` (`types.ts:126`).
#[derive(Default)]
pub(super) struct DocStoreCounts {
    pub mount_points: u32,
    pub folders: u32,
    pub documents: u32,
    pub blobs: u32,
    pub project_links: u32,
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn opt_s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(|x| x.to_string())
}

fn str_array(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(|x| x.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// v4 `importDocumentStores` (`import-document-stores.ts:24`).
#[allow(clippy::too_many_arguments)]
pub(super) fn import_document_stores(
    mount: &Connection,
    mount_points: &[Value],
    folders: &[Value],
    documents: &[Value],
    blobs: &[Value],
    project_links: &[Value],
    options: &ImportOptions,
    id_maps: &mut IdMaps,
    warnings: &mut Vec<String>,
) -> Result<DocStoreCounts, DbError> {
    let mut counts = DocStoreCounts::default();
    let points_repo = DocMountPointsRepository::new(mount);

    // Existing stores: one case-insensitive namespace. v4's `new Map(existing
    // .map(s => [s.name.toLowerCase(), s]))` — a later duplicate key OVERWRITES,
    // so the LAST store with a given lowered name wins.
    let existing_stores = doc_mount_points::find_all_full_json(mount)?;
    let mut by_name: Vec<(String, String, String)> = Vec::new(); // (lower, id, stored name)
    for st in &existing_stores {
        let (Some(id), Some(name)) = (
            st.get("id").and_then(Value::as_str),
            st.get("name").and_then(Value::as_str),
        ) else {
            continue;
        };
        let key = name.to_lowercase();
        match by_name.iter_mut().find(|(k, _, _)| *k == key) {
            Some(slot) => {
                slot.1 = id.to_string();
                slot.2 = name.to_string();
            }
            None => by_name.push((key, id.to_string(), name.to_string())),
        }
    }
    // Track every name we see (pre-existing + created this run) so neither a
    // clash with an existing store nor a duplicate inside the import payload
    // mints a colliding name.
    let mut taken_names: std::collections::HashSet<String> = existing_stores
        .iter()
        .filter_map(|st| st.get("name").and_then(Value::as_str))
        .map(|n| n.to_string())
        .collect();

    for mp in mount_points {
        let mp_name = s(mp, "name");
        let source_id = s(mp, "id");
        let out: Result<(), DbError> = (|| {
            let existing = by_name
                .iter()
                .find(|(k, _, _)| *k == mp_name.to_lowercase())
                .map(|(_, id, _)| id.clone());
            let mount_type = s(mp, "mountType");
            let base_path = if mount_type == "database" {
                String::new()
            } else {
                s(mp, "basePath")
            };
            let store_type = opt_s(mp, "storeType").unwrap_or_else(|| "documents".to_string());
            let enabled = mp.get("enabled").and_then(Value::as_bool).unwrap_or(false);

            if let Some(existing_id) = &existing {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => {
                        id_maps
                            .mount_points
                            .set(source_id.clone(), existing_id.clone());
                        return Ok(());
                    }
                    ConflictStrategy::Overwrite => {
                        // Drop existing documents, links, orphaned files and chunks
                        // before replacing (v4's four repo calls, quirks included).
                        overwrite_clear_mount(mount, existing_id)?;
                        points_repo.update(
                            existing_id,
                            &DmpUpdate {
                                name: Some(mp_name.clone()),
                                base_path: Some(base_path),
                                mount_type: Some(mount_type),
                                store_type: Some(store_type),
                                include_patterns: Some(str_array(mp, "includePatterns")),
                                exclude_patterns: Some(str_array(mp, "excludePatterns")),
                                enabled: Some(enabled),
                                last_scanned_at: None,
                                scan_status: None,
                                last_scan_error: None,
                                conversion_status: None,
                                conversion_error: None,
                                file_count: None,
                                chunk_count: None,
                                total_size_bytes: None,
                                updated_at: crate::clock::now_iso(),
                            },
                        )?;
                        id_maps
                            .mount_points
                            .set(source_id.clone(), existing_id.clone());
                        counts.mount_points += 1;
                        return Ok(());
                    }
                    // 'duplicate' — fall through to create a freshly-named point.
                    ConflictStrategy::Duplicate => {}
                }
            }

            let desired =
                if existing.is_some() && options.conflict_strategy == ConflictStrategy::Duplicate {
                    format!("{mp_name} (imported)")
                } else {
                    mp_name.clone()
                };
            let name = next_unique_mount_point_name(&taken_names, &desired);
            taken_names.insert(name.clone());
            let now = crate::clock::now_iso();
            let new_id = uuid::Uuid::new_v4().to_string();
            points_repo.create(
                &DmpCreate {
                    name,
                    base_path,
                    mount_type,
                    store_type,
                    include_patterns: str_array(mp, "includePatterns"),
                    exclude_patterns: str_array(mp, "excludePatterns"),
                    enabled,
                    last_scanned_at: None,
                    scan_status: "idle".to_string(),
                    last_scan_error: None,
                    conversion_status: "idle".to_string(),
                    conversion_error: None,
                    file_count: 0.0,
                    chunk_count: 0.0,
                    total_size_bytes: 0.0,
                },
                &doc_mount_points::CreateOptions {
                    id: new_id.clone(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            )?;
            id_maps.mount_points.set(source_id.clone(), new_id);
            counts.mount_points += 1;
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!("Failed to import mount point \"{mp_name}\": {e}"));
            tracing::warn!(name = %mp_name, error = %e, "Failed to import mount point");
        }
    }

    // Folders — database-backed only; import before documents so document
    // folderId FKs resolve correctly (v4's stated intent; in practice folder ids
    // are minted fresh and `parentId` stays unremapped — the carried quirk).
    let folders_repo = DocMountFoldersRepository::new(mount);
    for folder in folders {
        let Some(target_mount_id) = id_maps.mount_points.get(&s(folder, "mountPointId")) else {
            continue;
        };
        let target_mount_id = target_mount_id.to_string();
        let path = s(folder, "path");
        let now = crate::clock::now_iso();
        let created = folders_repo.create(
            &doc_mount_folders::DmfCreate {
                mount_point_id: target_mount_id,
                parent_id: opt_s(folder, "parentId"),
                name: s(folder, "name"),
                path: path.clone(),
            },
            &doc_mount_folders::CreateOptions {
                id: uuid::Uuid::new_v4().to_string(),
                created_at: now.clone(),
                updated_at: now,
            },
        );
        match created {
            Ok(()) => counts.folders += 1,
            Err(e) => warnings.push(format!("Failed to import folder \"{path}\": {e}")),
        }
    }

    // Documents — database-backed only. `linkDocumentContent` handles file +
    // document + link in one shot (and derives the folder from relativePath).
    //
    // Hard-link groups are re-established in a SECOND pass, keyed by the group id
    // the export carried. The imported ids are never reused directly: they are
    // only a grouping token here, and re-binding through `bind_link_group` mints
    // a fresh id, so importing the same archive twice can't fuse the two copies
    // into one group (v4 `40319484`).
    let mut members_by_exported_group: Vec<(String, Vec<String>)> = Vec::new();
    let links_repo = DocMountFileLinksRepository::new(mount);
    for doc in documents {
        let Some(target_mount_id) = id_maps.mount_points.get(&s(doc, "mountPointId")) else {
            continue;
        };
        let target_mount_id = target_mount_id.to_string();
        let relative_path = s(doc, "relativePath");
        let content = s(doc, "content");
        let out = links_repo.link_document_content(&LinkDocumentInput {
            mount_point_id: target_mount_id,
            relative_path: relative_path.clone(),
            file_name: s(doc, "fileName"),
            file_type: s(doc, "fileType"),
            // v4 `fileSizeBytes: Buffer.byteLength(doc.content, 'utf-8')`;
            // `plainTextLength` rides the payload (JS UTF-16 units).
            file_size_bytes: content.len() as i64,
            plain_text_length: doc
                .get("plainTextLength")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
            content_sha256: s(doc, "contentSha256"),
            content,
            allow_embed: None,
            allow_character_read: None,
            allow_character_write: None,
        });
        match out {
            Ok(result) => {
                if let Some(exported_group) = opt_s(doc, "linkGroupId") {
                    // Insertion order preserved (v4 iterates a Map) — the anchor
                    // is the first member the document loop imported.
                    match members_by_exported_group
                        .iter_mut()
                        .find(|(g, _)| *g == exported_group)
                    {
                        Some((_, members)) => members.push(result.link_id),
                        None => {
                            members_by_exported_group.push((exported_group, vec![result.link_id]))
                        }
                    }
                }
                counts.documents += 1;
            }
            Err(e) => warnings.push(format!(
                "Failed to import document \"{relative_path}\": {e}"
            )),
        }
    }

    // A group whose other members fell outside the export's scope arrives with a
    // single member and is simply left un-linked — a group of one is not a link.
    for (exported_group_id, member_link_ids) in &members_by_exported_group {
        if member_link_ids.len() < 2 {
            continue;
        }
        let (anchor, rest) = member_link_ids.split_first().expect("non-empty group");
        for member_id in rest {
            if let Err(e) = links_repo.bind_link_group(anchor, member_id) {
                warnings.push(format!(
                    "Failed to restore hard link for group \"{exported_group_id}\": {e}"
                ));
            }
        }
    }

    // Blobs — universal across mount types.
    let blobs_repo = DocMountBlobsRepository::new(mount);
    for blob in blobs {
        let Some(target_mount_id) = id_maps.mount_points.get(&s(blob, "mountPointId")) else {
            continue;
        };
        let target_mount_id = target_mount_id.to_string();
        let relative_path = s(blob, "relativePath");
        let out: Result<(), String> = (|| {
            // v4 `Buffer.from(dataBase64, 'base64')` is lenient and never throws;
            // a genuinely undecodable payload can only come from a hand-corrupted
            // file, where surfacing the decode error as the per-item warning is
            // the closest faithful behavior.
            let data = base64::engine::general_purpose::STANDARD
                .decode(s(blob, "dataBase64"))
                .map_err(|e| format!("invalid blob base64: {e}"))?;
            let created = blobs_repo
                .create(&CreateBlobInput {
                    mount_point_id: target_mount_id,
                    relative_path: relative_path.clone(),
                    original_file_name: s(blob, "originalFileName"),
                    original_mime_type: s(blob, "originalMimeType"),
                    stored_mime_type: s(blob, "storedMimeType"),
                    sha256: s(blob, "sha256"),
                    data,
                    description: opt_s(blob, "description"),
                    file_name: None,
                    file_type: None,
                })
                .map_err(|e| e.to_string())?;
            // ⚠ v5 type gap, corrected here rather than at the source.
            //
            // v4 passes `blob.originalFileName` / `originalMimeType` straight
            // through to the link row, INCLUDING their `null` — and a
            // document-store export emits `null` for both on every blob (the
            // writer has no such fields to emit). v5's `CreateBlobInput` types
            // them `String`, so the absent key becomes `''` and the link's
            // upsert writes an empty string where v4 writes SQL NULL. The
            // observable difference is real: the export/import round-trip is the
            // only caller that has nothing to put there.
            //
            // The proper fix is widening `LinkBlobInput.original_file_name` /
            // `original_mime_type` to `Option<String>`, but
            // `db/doc_mount_file_links.rs` belongs to another lane this round
            // (P4.6BK, round-2 §Ownership), so this lane reproduces v4's exact
            // net state with a targeted correction instead of editing it. When
            // that type is widened this block collapses into the `create` call
            // above. Escalated in the lane record.
            mount
                .execute(
                    "UPDATE doc_mount_file_links SET originalFileName = ?1, \
                     originalMimeType = ?2 WHERE id = ?3",
                    rusqlite::params![
                        opt_s(blob, "originalFileName"),
                        opt_s(blob, "originalMimeType"),
                        created.link_id,
                    ],
                )
                .map_err(|e| DbError::from(e).to_string())?;
            // Restore the extractedText sidecar on imports from 4.3-dev+ exports.
            // Older exports omit these fields; keep the blob in the default 'none'
            // state so on-upload extraction has nothing to re-run.
            let has_extraction_metadata = blob.get("extractedText").is_some()
                || blob.get("extractionStatus").is_some()
                || blob.get("extractionError").is_some();
            if has_extraction_metadata {
                let extracted_text = opt_s(blob, "extractedText");
                let extracted_sha = opt_s(blob, "extractedTextSha256");
                let status = opt_s(blob, "extractionStatus").unwrap_or_else(|| "none".to_string());
                let error = opt_s(blob, "extractionError");
                blobs_repo
                    .update_extracted_text(
                        &created.id,
                        extracted_text.as_deref(),
                        extracted_sha.as_deref(),
                        &status,
                        error.as_deref(),
                        None,
                    )
                    .map_err(|e| e.to_string())?;
            }
            counts.blobs += 1;
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!("Failed to import blob \"{relative_path}\": {e}"));
        }
    }

    // Project ↔ mount-point links — remap both ids; skip any link whose project
    // or mount point didn't survive the import; dedupe against links that
    // already exist after an overwrite/skip on an existing mount point.
    let pdml_repo = ProjectDocMountLinksRepository::new(mount);
    let mut existing_link_keys: std::collections::HashSet<String> = if project_links.is_empty() {
        Default::default()
    } else {
        project_doc_mount_links::find_all_pairs(mount)?
            .into_iter()
            .map(|(p, m)| format!("{p}::{m}"))
            .collect()
    };
    for link in project_links {
        let Some(target_mount_id) = id_maps.mount_points.get(&s(link, "mountPointId")) else {
            continue;
        };
        let Some(target_project_id) = id_maps.projects.get(&s(link, "projectId")) else {
            continue;
        };
        let (target_mount_id, target_project_id) =
            (target_mount_id.to_string(), target_project_id.to_string());
        let key = format!("{target_project_id}::{target_mount_id}");
        if existing_link_keys.contains(&key) {
            // Already linked after an overwrite/skip on an existing mount point.
            continue;
        }
        let now = crate::clock::now_iso();
        let out = pdml_repo.create(
            &PdmlCreate {
                project_id: target_project_id.clone(),
                mount_point_id: target_mount_id.clone(),
            },
            &project_doc_mount_links::CreateOptions {
                id: uuid::Uuid::new_v4().to_string(),
                created_at: now.clone(),
                updated_at: now,
            },
        );
        match out {
            Ok(()) => {
                existing_link_keys.insert(key);
                counts.project_links += 1;
            }
            Err(e) => warnings.push(format!(
                "Failed to link project {target_project_id} to mount point {target_mount_id}: {e}"
            )),
        }
    }

    Ok(counts)
}

/// v4's overwrite clear — the four `deleteByMountPointId` calls in order, net
/// effect preserved (see the module header for the blob quirk):
///
/// 1. `docMountDocuments` — documents joined via the links.
/// 2. `docMountBlobs` — actually deletes the LINKS + orphaned FILES.
/// 3. `docMountChunks` — chunks by `mountPointId`.
/// 4. `docMountFiles` — a second links+files pass over an already-empty set.
///
/// ## P4.31 — the folder gap: MEASURED, PINNED, and ESCALATED (not fixed)
///
/// v4's clear list (`import-document-stores.ts:62-67`) omits
/// `doc_mount_folders`, so an overwrite replaces every file while leaving the
/// previous contents' folder tree standing. Measured on v4 at `49769ec4`
/// (`system_import_state`'s `execute_folder_overwrite` arm): importing an
/// archive carrying only `gamma` over a store that held `alpha` +
/// `alpha/beta` leaves ALL THREE folders, and the two the archive no longer
/// mentions are then permanent — the same orphan shape this lane closes at the
/// delete end, arriving by a different door. Re-importing an IDENTICAL archive
/// surfaces it instead as `Failed to import folder …UNIQUE constraint failed`
/// warnings, because the survivors block the re-insert.
///
/// **The obvious repair — clear the folders too — is WRONG, and that is why
/// this is escalated rather than landed.** The stores an overwrite most often
/// lands on are character vaults and project stores, whose folders
/// (`Outfits` / `Prompts` / `Scenarios` / `Wardrobe` / `images` / `files`) are
/// SCAFFOLDING, not archive content: `system-data`'s Lorian Character Vault
/// carries six of them, and a blanket clear deletes the lot. Choosing between
/// "clear only the folders the archive replaces", "clear all non-scaffold
/// folders", and "leave them and reap the unreachable ones later" is a
/// semantics call of the same kind as the P4.9G5 restore-bug ruling, so v5
/// stays v4-faithful here until it is made.
///
/// The gap is not merely written down: `execute_folder_overwrite` pins the
/// stale-folder set in BOTH directions, so it fires the moment either app's
/// behavior moves.
fn overwrite_clear_mount(mount: &Connection, mount_point_id: &str) -> Result<(), DbError> {
    // 1. Documents whose file is linked from this mount.
    mount.execute(
        "DELETE FROM doc_mount_documents WHERE fileId IN (\
           SELECT DISTINCT fileId FROM doc_mount_file_links WHERE mountPointId = ?1)",
        [mount_point_id],
    )?;

    // 2 + 4. The links-then-orphaned-files pass, run twice in v4 (the second is
    // a no-op because the first already emptied the link set). One faithful pass
    // suffices for the same net state.
    let mut stmt = mount
        .prepare("SELECT DISTINCT fileId FROM doc_mount_file_links WHERE mountPointId = ?1")?;
    let affected: Vec<String> = stmt
        .query_map([mount_point_id], |r| r.get::<_, String>(0))?
        .collect::<Result<_, _>>()?;
    drop(stmt);
    mount.execute(
        "DELETE FROM doc_mount_file_links WHERE mountPointId = ?1",
        [mount_point_id],
    )?;
    for file_id in &affected {
        mount.execute(
            "DELETE FROM doc_mount_files WHERE id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM doc_mount_file_links l WHERE l.fileId = ?1)",
            [file_id.as_str()],
        )?;
    }

    // 3. Chunks by mountPointId.
    mount.execute(
        "DELETE FROM doc_mount_chunks WHERE mountPointId = ?1",
        [mount_point_id],
    )?;

    Ok(())
}
