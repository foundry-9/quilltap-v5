//! The post-reconcile wardrobe picture re-mint (v4 `lib/import/quilltap-import/
//! import-wardrobe-images.ts`, `7c8572869` #82, unchanged through `f5e953a3f`;
//! P4.D264).
//!
//! v4's header, carried: a character's `.qtap` bundle carries each owned
//! wardrobe item's picture metadata (`_imageFiles`) beside the item, and the
//! picture BYTES as blobs in the character's vault under
//! `Wardrobe/images/<itemId>/`. Those blobs land with the vault in the
//! document-store phase; this pass re-creates the `files` rows that make them
//! pictures again — linked to the item, pointing at the imported vault's blob
//! — and points each item at its own copy of its current picture.
//!
//! It never writes BYTES and never calls the image bridge's write: it reads
//! the FROZEN path helper (C1 §7, [`wardrobe_item_image_path`]) and re-mints
//! rows against blobs the document-store phase already landed (§R.4(f)). The
//! item id it uses is the SOURCE id — valid because the vault's frontmatter
//! carries it verbatim (a bundle with no vault never reaches the per-item
//! loop). The `FileCreate` carries NO `generationKey`: v4's `_imageFiles`
//! projection (twelve keys) never exports one (the harness's generation-key
//! census pins the `None`).

use rusqlite::Connection;
use serde_json::Value;

use super::IdMaps;
use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::files::{CreateOptions, FileCreate, FilesRepository};
use crate::db::vault_wardrobe_public::{
    update_vault_wardrobe_item, WardrobePatch, WardrobePublicError,
};
use crate::services::file_storage::build_mount_blob_storage_key;
use crate::services::wardrobe_image_paths::wardrobe_item_image_path;

/// JS `${value}` for the item title in v4's sentences (`String(undefined)` is
/// `"undefined"`).
fn title_of(item: &Value) -> String {
    match item.get("title") {
        None => "undefined".to_string(),
        Some(v) => crate::pascal::js_value::to_js_string(v),
    }
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// The tail of a picture warning — v4's `error.message`.
fn error_tail(e: &WardrobePublicError) -> String {
    match e {
        WardrobePublicError::Db(d) => super::item_error_text(d),
        WardrobePublicError::NoMount => format!(
            "Cannot update wardrobe item: {}",
            crate::db::vault_wardrobe_public::NO_MOUNT_MESSAGE
        ),
        WardrobePublicError::Cycle(msg) => msg.clone(),
    }
}

/// v4 `importWardrobeItemImages(userId, characters, idMaps, warnings)` —
/// "Returns the number of picture rows created. Never throws; failures become
/// warnings and the item is left without that picture." The caller discards
/// the count, as v4's executor does.
pub(crate) fn import_wardrobe_item_images(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    characters: &[Value],
    id_maps: &IdMaps,
    warnings: &mut Vec<String>,
) -> usize {
    let links = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);
    let blobs = DocMountBlobsRepository::new(mount);
    let files = FilesRepository::new(main);
    let mut created = 0usize;

    for character in characters {
        let source_id = character.get("id").and_then(Value::as_str).unwrap_or("");
        let Some(new_character_id) = id_maps.characters.get(source_id) else {
            continue;
        };
        let items: Vec<&Value> = character
            .get("wardrobeItems")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter(|i| {
                        i.get("_imageFiles")
                            .and_then(Value::as_array)
                            .is_some_and(|f| !f.is_empty())
                    })
                    .collect()
            })
            .unwrap_or_default();
        if items.is_empty() {
            continue;
        }

        let mount_point_id = id_maps
            .character_vault_mounts
            .get(new_character_id)
            .and_then(|exported_vault| id_maps.mount_points.get(exported_vault));
        let Some(mount_point_id) = mount_point_id else {
            tracing::debug!(
                characterId = %new_character_id,
                itemCount = items.len(),
                "No imported vault for character; wardrobe pictures not carried"
            );
            continue;
        };

        for item in items {
            let item_id = item.get("id").and_then(Value::as_str).unwrap_or("");
            let title = title_of(item);
            // Exported picture id → the id it was re-minted under.
            let mut file_id_map: Vec<(String, String)> = Vec::new();
            for image in item["_imageFiles"].as_array().into_iter().flatten() {
                let original_filename = opt_str(image, "originalFilename").unwrap_or_default();
                let outcome = (|| -> Result<bool, String> {
                    let relative_path = wardrobe_item_image_path(item_id, &original_filename);
                    let link = links
                        .find_by_mount_point_and_path(mount_point_id, &relative_path)
                        .map_err(|e| super::item_error_text(&e))?;
                    let blob = match &link {
                        Some(l) => blobs
                            .find_by_file_id(&l.file_id)
                            .map_err(|e| super::item_error_text(&e))?,
                        None => None,
                    };
                    let (Some(link), Some(blob)) = (link, blob) else {
                        return Ok(false);
                    };
                    let exported_id = opt_str(image, "id").unwrap_or_default();
                    // "the EXPORTED id is kept when free"
                    let file_id = if files
                        .find_by_id(&exported_id)
                        .map_err(|e| super::item_error_text(&e))?
                        .is_some()
                    {
                        uuid::Uuid::new_v4().to_string()
                    } else {
                        exported_id.clone()
                    };
                    let create = FileCreate {
                        user_id: user_id.to_string(),
                        sha256: link.sha256.clone(),
                        original_filename: original_filename.clone(),
                        mime_type: blob.stored_mime_type.clone(),
                        size: link.file_size_bytes as f64,
                        width: image.get("width").and_then(Value::as_f64),
                        height: image.get("height").and_then(Value::as_f64),
                        is_plain_text: None,
                        linked_to: vec![item_id.to_string()],
                        source: opt_str(image, "source").unwrap_or_default(),
                        category: "IMAGE".to_string(),
                        generation_prompt: opt_str(image, "generationPrompt"),
                        generation_model: opt_str(image, "generationModel"),
                        generation_revised_prompt: opt_str(image, "generationRevisedPrompt"),
                        generation_key: None,
                        description: opt_str(image, "description"),
                        tags: vec![item_id.to_string()],
                        project_id: None,
                        folder_path: None,
                        storage_key: Some(build_mount_blob_storage_key(mount_point_id, &blob.id)),
                        file_status: "ok".to_string(),
                    };
                    files
                        .create(
                            &create,
                            &CreateOptions {
                                id: file_id.clone(),
                                created_at: opt_str(image, "createdAt")
                                    .unwrap_or_else(crate::clock::now_iso),
                                updated_at: crate::clock::now_iso(),
                            },
                        )
                        .map_err(|e| super::item_error_text(&e))?;
                    file_id_map.push((exported_id, file_id));
                    Ok(true)
                })();
                match outcome {
                    Ok(true) => created += 1,
                    Ok(false) => warnings.push(format!(
                        "Wardrobe item \"{title}\" lost a picture whose bytes were not in the bundle ({original_filename})."
                    )),
                    Err(msg) => warnings.push(format!(
                        "Failed to import a picture of wardrobe item \"{title}\": {msg}"
                    )),
                }
            }

            // "Point the item at its own copy of its current picture — or at
            // none, when that picture did not come along." Written ONLY when
            // the pointer moves (an exported id kept free needs no write).
            // `item.imageFileId ?? null`, then `exportedCurrent ? map.get(…) ??
            // null : null` — an `''` pointer is kept as `''` and so DOES move
            // (to `null`), exactly as v4's `!==` sees it.
            let exported_current = opt_str(item, "imageFileId");
            let current = exported_current
                .as_ref()
                .filter(|exported| !exported.is_empty())
                .and_then(|exported| {
                    file_id_map
                        .iter()
                        .find(|(k, _)| k == exported)
                        .map(|(_, v)| v.clone())
                });
            if current != exported_current {
                if let Err(e) = update_vault_wardrobe_item(
                    main,
                    &links,
                    &docs,
                    item_id,
                    &WardrobePatch {
                        image_file_id: Some(current),
                        ..Default::default()
                    },
                    Some(new_character_id),
                ) {
                    warnings.push(format!(
                        "Failed to repoint the picture of wardrobe item \"{title}\": {}",
                        error_tail(&e)
                    ));
                }
            }
        }
    }

    if created > 0 {
        tracing::info!(count = created, "Imported wardrobe item pictures");
    }
    created
}
