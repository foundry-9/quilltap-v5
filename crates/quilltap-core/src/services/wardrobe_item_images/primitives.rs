//! The item-image primitives (contract C1 §7; signatures FROZEN at
//! `KEYSTONE`): v4 `item-images.ts`'s `listWardrobeItemImages` (`:174-182`),
//! `assertItemImageChoice` (`:189-199`), `removeImageFile` (`:305-311`) and
//! `cleanupItemImages` (`:355-397`), plus the bridge's
//! `deleteWardrobeItemImageLink` (`wardrobe-image-bridge.ts:181-211`) that
//! `removeImageFile` calls.
//!
//! The history is every IMAGE `files` row `linkedTo` the item — never a
//! frontmatter list — newest first. A deleted item's pictures go with it: each
//! mount link through `deleteWithGC` (the blob only when no sibling still links
//! it) and each `files` row, ALWAYS, even when the storage key does not parse.
//!
//! The link delete is the bridge's ([`delete_wardrobe_item_image_link`],
//! P4.D263 — with v4's best-effort `refreshStats`; the realtime side is the
//! bridge header's note).

use rusqlite::Connection;

use crate::db::files::{FileRow, FilesRepository};
use crate::db::DbError;
use crate::services::file_storage::parse_mount_blob_storage_key;
use crate::services::wardrobe_image_bridge::delete_wardrobe_item_image_link;

/// v4 `listWardrobeItemImages(repos, itemId)` — the item's pictures, newest
/// first: `files.findByLinkedTo(itemId)` → `category === 'IMAGE'` → sorted by
/// `createdAt` DESC (string compare; a STABLE sort, so ties keep rowid order as
/// V8's does).
pub fn find_item_image_rows(main: &Connection, item_id: &str) -> Result<Vec<FileRow>, DbError> {
    let mut rows: Vec<FileRow> = FilesRepository::new(main)
        .find_by_linked_to(item_id)?
        .into_iter()
        .filter(|f| f.category == "IMAGE")
        .collect();
    rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(rows)
}

/// v4's `ForeignWardrobeImageError` — a requested picture is not one of the
/// item's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemImageChoiceError {
    /// The file is not an IMAGE `files` row linked to the item.
    Foreign { item_id: String, file_id: String },
    /// The history read itself failed.
    Read(String),
}

impl ItemImageChoiceError {
    /// The error's OWN message — v4 `ForeignWardrobeImageError`'s
    /// `` `File ${fileId} is not an image of wardrobe item ${itemId}` ``
    /// (`item-images.ts:49-54`).
    pub fn message(&self) -> String {
        match self {
            ItemImageChoiceError::Foreign { item_id, file_id } => {
                format!("File {file_id} is not an image of wardrobe item {item_id}")
            }
            ItemImageChoiceError::Read(m) => m.clone(),
        }
    }
}

/// The ROUTE's sentence — v4 `imageChoiceError` (`lib/wardrobe/
/// item-route-steps.ts:33-47`) answers `badRequest` with it for a foreign
/// pick. A read failure renders its own message (v4 rethrows it).
impl std::fmt::Display for ItemImageChoiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemImageChoiceError::Foreign { .. } => {
                f.write_str("imageFileId must name one of this item's own pictures")
            }
            ItemImageChoiceError::Read(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for ItemImageChoiceError {}

/// v4 `assertItemImageChoice(repos, itemId, fileId)` — refuse an
/// `imageFileId` that is not one of the item's own pictures. `None` (absent
/// or `null` — clear the picture) always passes.
pub fn assert_item_image_choice(
    main: &Connection,
    item_id: &str,
    file_id: Option<&str>,
) -> Result<(), ItemImageChoiceError> {
    let Some(file_id) = file_id else {
        return Ok(());
    };
    let images = find_item_image_rows(main, item_id)
        .map_err(|e| ItemImageChoiceError::Read(crate::db::fallback::error_text(&e)))?;
    if images.iter().any(|f| f.id == file_id) {
        Ok(())
    } else {
        Err(ItemImageChoiceError::Foreign {
            item_id: item_id.to_string(),
            file_id: file_id.to_string(),
        })
    }
}

/// The meta a delete route hands [`cleanup_item_images`] — v4's `meta` object,
/// one variant per caller shape (its keys LEAD every line, `itemId` after
/// them unless the meta already placed it).
#[derive(Debug, Clone, Copy)]
pub enum ItemImageCleanupMeta<'a> {
    /// The character route: `{ characterId: id, itemId }`.
    Character { character_id: &'a str },
    /// The General (archetype) route: `{ itemId }`.
    Archetype,
    /// The project store factory: `{ projectId: id, mountPointId, context: 'wardrobe' }`.
    Project {
        project_id: &'a str,
        mount_point_id: &'a str,
    },
    /// The group store factory: `{ groupId: id, mountPointId, context: 'wardrobe' }`.
    Group {
        group_id: &'a str,
        mount_point_id: &'a str,
    },
}

/// One cleanup line in each of the four meta shapes (tracing field names are
/// static, so the shape is the match).
macro_rules! cleanup_line {
    ($level:ident, $meta:expr, $item_id:expr, $message:expr; $($rest:tt)*) => {
        match $meta {
            ItemImageCleanupMeta::Character { character_id } => tracing::$level!(
                characterId = %character_id, itemId = %$item_id, $($rest)* "{}", $message
            ),
            ItemImageCleanupMeta::Archetype => tracing::$level!(
                itemId = %$item_id, $($rest)* "{}", $message
            ),
            ItemImageCleanupMeta::Project { project_id, mount_point_id } => tracing::$level!(
                projectId = %project_id, mountPointId = %mount_point_id, context = "wardrobe",
                itemId = %$item_id, $($rest)* "{}", $message
            ),
            ItemImageCleanupMeta::Group { group_id, mount_point_id } => tracing::$level!(
                groupId = %group_id, mountPointId = %mount_point_id, context = "wardrobe",
                itemId = %$item_id, $($rest)* "{}", $message
            ),
        }
    };
}

/// v4 `cleanupItemImages(repos, itemId, logTag, meta)` — drop every picture
/// of a deleted item (each mount link, each `files` row). Called by the
/// delete routes AFTER the item is gone; a composite's deletion takes only
/// its OWN pictures. Never returns an error: a list failure WARNs and stops;
/// a per-picture failure WARNs and continues (the item is already gone, so
/// nothing could retry the cleanup); when there were pictures, one INFO with
/// `removed` / `failed`. Answers the removed count.
pub fn cleanup_item_images(
    main: &Connection,
    mount: &Connection,
    item_id: &str,
    log_tag: &str,
    meta: ItemImageCleanupMeta<'_>,
) -> usize {
    let images = match find_item_image_rows(main, item_id) {
        Ok(images) => images,
        Err(e) => {
            let error = crate::db::fallback::error_text(&e);
            cleanup_line!(
                warn,
                meta,
                item_id,
                format!("{log_tag} Could not list wardrobe item images for cleanup");
                error = %error,
            );
            return 0;
        }
    };
    let mut removed = 0usize;
    for file in &images {
        match remove_image_file(main, mount, item_id, file) {
            Ok(()) => removed += 1,
            Err(e) => {
                let error = crate::db::fallback::error_text(&e);
                cleanup_line!(
                    warn,
                    meta,
                    item_id,
                    format!("{log_tag} Failed to remove a wardrobe item image; continuing");
                    fileId = %file.id,
                    error = %error,
                );
            }
        }
    }
    if !images.is_empty() {
        cleanup_line!(
            info,
            meta,
            item_id,
            format!("{log_tag} Removed wardrobe item images with the item");
            removed = removed,
            failed = images.len() - removed,
        );
    }
    removed
}

/// v4 `removeImageFile` — the picture's mount link (when the storage key
/// parses), then its `files` row ALWAYS. Never fails on a missing link.
pub(crate) fn remove_image_file(
    main: &Connection,
    mount: &Connection,
    item_id: &str,
    file: &FileRow,
) -> Result<(), DbError> {
    if let Some((mount_point_id, _blob_id)) =
        parse_mount_blob_storage_key(file.storage_key.as_deref().unwrap_or(""))
    {
        delete_wardrobe_item_image_link(mount, &mount_point_id, item_id, &file.original_filename);
    }
    FilesRepository::new(main).delete(&file.id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::wardrobe_image_paths::wardrobe_item_image_path;
    use crate::test_support::captured_with;
    use rusqlite::params;

    const ITEM: &str = "7d8c4a4e-5b1d-4f3a-9a2e-1c2b3d4e5f60";
    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const MP: &str = "aaaaaaaa-5b1d-4f3a-9a2e-1c2b3d4e5f60";

    fn fresh(partition: &str) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../provisioning/fresh_schema.json")).unwrap();
        for sql in schema[partition].as_array().unwrap() {
            let sql = sql.as_str().unwrap();
            if sql.starts_with("CREATE TABLE") {
                conn.execute_batch(sql).unwrap();
            }
        }
        conn
    }

    fn plant_file(
        main: &Connection,
        id: &str,
        category: &str,
        created: &str,
        storage_key: Option<&str>,
    ) {
        main.execute(
            "INSERT INTO files (id, userId, sha256, originalFilename, mimeType, size, linkedTo, \
             source, category, tags, storageKey, createdAt, updatedAt) VALUES (?1, 'u', ?2, ?3, \
             'image/webp', 12, ?4, 'GENERATED', ?5, '[]', ?6, ?7, ?7)",
            params![
                id,
                SHA,
                format!("{id}.webp"),
                format!("[\"{ITEM}\"]"),
                category,
                storage_key,
                created
            ],
        )
        .unwrap();
    }

    /// Plant the mount-side trio for one picture (file + blob-less link).
    fn plant_link(mount: &Connection, link_id: &str, leaf: &str) {
        mount
            .execute(
                "INSERT INTO doc_mount_files (id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt) \
                 VALUES (?1, ?2, 12, 'blob', 'database', 'x', 'x')",
                params![format!("f-{link_id}"), SHA],
            )
            .unwrap();
        mount
            .execute(
                "INSERT INTO doc_mount_file_links (id, fileId, mountPointId, relativePath, fileName, lastModified, \
                 createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?5, 'x', 'x', 'x')",
                params![
                    link_id,
                    format!("f-{link_id}"),
                    MP,
                    wardrobe_item_image_path(ITEM, leaf),
                    leaf
                ],
            )
            .unwrap();
    }

    #[test]
    fn the_history_is_images_newest_first() {
        let main = fresh("main");
        plant_file(
            &main,
            "11111111-1111-4111-8111-111111111111",
            "IMAGE",
            "2026-01-01T00:00:00.000Z",
            None,
        );
        plant_file(
            &main,
            "22222222-2222-4222-8222-222222222222",
            "DOCUMENT",
            "2026-01-03T00:00:00.000Z",
            None,
        );
        plant_file(
            &main,
            "33333333-3333-4333-8333-333333333333",
            "IMAGE",
            "2026-01-02T00:00:00.000Z",
            None,
        );
        let ids: Vec<String> = find_item_image_rows(&main, ITEM)
            .unwrap()
            .into_iter()
            .map(|f| f.id)
            .collect();
        assert_eq!(
            ids,
            vec![
                "33333333-3333-4333-8333-333333333333",
                "11111111-1111-4111-8111-111111111111"
            ]
        );
    }

    #[test]
    fn the_choice_check_carries_both_texts() {
        let main = fresh("main");
        plant_file(
            &main,
            "11111111-1111-4111-8111-111111111111",
            "IMAGE",
            "2026-01-01T00:00:00.000Z",
            None,
        );
        assert_eq!(assert_item_image_choice(&main, ITEM, None), Ok(()));
        assert_eq!(
            assert_item_image_choice(&main, ITEM, Some("11111111-1111-4111-8111-111111111111")),
            Ok(())
        );
        let err =
            assert_item_image_choice(&main, ITEM, Some("99999999-9999-4999-8999-999999999999"))
                .unwrap_err();
        assert_eq!(
            err.message(),
            format!(
                "File 99999999-9999-4999-8999-999999999999 is not an image of wardrobe item {ITEM}"
            )
        );
        assert_eq!(
            err.to_string(),
            "imageFileId must name one of this item's own pictures"
        );
    }

    /// Two pictures, one with a parseable mount-blob key and a link, one
    /// whose key does not parse: both `files` rows go, the link goes, one
    /// INFO with the caller's meta first; then the silence legs.
    #[test]
    fn cleanup_removes_links_and_rows_and_logs_once() {
        let main = fresh("main");
        let mount = fresh("mountIndex");
        let a = "11111111-1111-4111-8111-111111111111";
        let b = "22222222-2222-4222-8222-222222222222";
        plant_file(
            &main,
            a,
            "IMAGE",
            "2026-01-02T00:00:00.000Z",
            Some(&format!("mount-blob:{MP}:blob-a")),
        );
        plant_file(
            &main,
            b,
            "IMAGE",
            "2026-01-01T00:00:00.000Z",
            Some("files/legacy.webp"),
        );
        plant_link(&mount, "link-a", &format!("{a}.webp"));
        let (removed, lines) = captured_with(|| {
            cleanup_item_images(
                &main,
                &mount,
                ITEM,
                "[Wardrobe v1]",
                ItemImageCleanupMeta::Character {
                    character_id: "char-1",
                },
            )
        });
        assert_eq!(removed, 2);
        let left: i64 = main
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
        let links: i64 = mount
            .query_row("SELECT COUNT(*) FROM doc_mount_file_links", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(links, 0);
        let info: Vec<&String> = lines.iter().filter(|l| l.starts_with("INFO")).collect();
        assert_eq!(info.len(), 1, "{lines:#?}");
        assert!(
            info[0].ends_with(
                "[Wardrobe v1] Removed wardrobe item images with the item characterId=char-1 itemId=7d8c4a4e-5b1d-4f3a-9a2e-1c2b3d4e5f60 removed=2 failed=0"
            ),
            "{}",
            info[0]
        );
        assert!(lines
            .iter()
            .any(|l| l.contains("[WardrobeImageBridge] Deleted wardrobe item image link")));
        // Silence leg: no pictures, no INFO.
        let (removed, lines) = captured_with(|| {
            cleanup_item_images(
                &main,
                &mount,
                ITEM,
                "[Wardrobe Archetypes v1]",
                ItemImageCleanupMeta::Archetype,
            )
        });
        assert_eq!(removed, 0);
        assert!(lines.is_empty(), "{lines:#?}");
    }

    /// A per-picture failure WARNs and continues (a planted trigger refuses
    /// one `files` DELETE).
    #[test]
    fn a_failing_picture_does_not_strand_the_rest() {
        let main = fresh("main");
        let mount = fresh("mountIndex");
        let a = "11111111-1111-4111-8111-111111111111";
        let b = "22222222-2222-4222-8222-222222222222";
        plant_file(&main, a, "IMAGE", "2026-01-02T00:00:00.000Z", None);
        plant_file(&main, b, "IMAGE", "2026-01-01T00:00:00.000Z", None);
        main.execute_batch(&format!(
            "CREATE TRIGGER refuse BEFORE DELETE ON files WHEN OLD.id = '{a}' \
             BEGIN SELECT RAISE(ABORT, 'planted'); END;"
        ))
        .unwrap();
        let (removed, lines) = captured_with(|| {
            cleanup_item_images(
                &main,
                &mount,
                ITEM,
                "[Projects v1]",
                ItemImageCleanupMeta::Project {
                    project_id: "p-1",
                    mount_point_id: MP,
                },
            )
        });
        assert_eq!(removed, 1);
        assert!(lines.iter().any(|l| l.starts_with("WARN")
            && l.contains("[Projects v1] Failed to remove a wardrobe item image; continuing projectId=p-1")
            && l.contains(&format!("fileId={a} error=planted"))), "{lines:#?}");
        assert!(
            lines.iter().any(|l| l.ends_with("removed=1 failed=1")),
            "{lines:#?}"
        );
    }
}
