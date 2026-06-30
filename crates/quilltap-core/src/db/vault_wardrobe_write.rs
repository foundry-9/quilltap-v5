//! The character vault **wardrobe write projection** — ports v4's
//! `projectVaultWardrobe` (`vault-overlay/wardrobe-sync.ts`) +
//! `projectArrayIntoVaultFolder` (`vault-overlay/vault-projection.ts`), the final
//! wardrobe write piece. Re-projects an authoritative `WardrobeItem` list into a
//! vault store's `Wardrobe/` folder: every item is written as `Wardrobe/<title>.md`
//! (filename collisions disambiguated with `-1`, `-2`, … suffixes), any file in the
//! folder NOT produced by the current list is swept, and the legacy
//! `wardrobe.json` is deleted so the folder layout is the single on-disk source.
//!
//! Composes the already-ported pure leaves — [`build_slug_by_item_id_map`],
//! [`build_wardrobe_item_file`] (the Decision-A YAML emitter),
//! [`sanitize_file_name`] — over the document-store write primitive
//! ([`DocMountFileLinksRepository::write_database_document`]) and its GC delete
//! ([`DocMountFileLinksRepository::delete_database_document`]).
//!
//! Out of scope (matches the storage primitive's existing boundary): v4's
//! post-write `reindexSingleFile` chunk pass (the differential drives v4 with the
//! reindex running and pins the link `chunkCount` / excludes `doc_mount_chunks`,
//! exactly as the groups/projects store-backed tests do).

use std::collections::{HashMap, HashSet};

use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::DbError;
use crate::vault_overlay::{
    build_slug_by_item_id_map, build_wardrobe_item_file, sanitize_file_name, WardrobeItem,
};

const WARDROBE_FOLDER: &str = "Wardrobe";
const WARDROBE_JSON_PATH: &str = "wardrobe.json";

/// Replace a vault folder's `.md` contents with a fresh projection of `items`
/// (v4 `projectArrayIntoVaultFolder`). `mapper` turns each item into its
/// `(fileName, content)`; filename collisions (case-insensitive) get `-1`/`-2`/…
/// suffixes. Files present in the folder but not produced this pass are swept.
pub fn project_array_into_vault_folder<T>(
    links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    folder: &str,
    items: &[T],
    mapper: impl Fn(&T) -> (String, String),
) -> Result<(), DbError> {
    let existing =
        docs.find_many_by_mount_points_in_folder(&[mount_point_id.to_string()], folder, ".md")?;
    // The relative paths currently in the folder, to sweep what we don't rewrite.
    let existing_paths: Vec<String> = existing.into_iter().map(|d| d.relative_path).collect();

    // v4 calls `ensureFolderPath` when items > 0; the write primitive already
    // find-or-creates the folder segments on each write, so an explicit ensure is
    // redundant (and an empty list correctly creates no folder). Match: write-only.

    let mut written_paths: HashSet<String> = HashSet::new();
    let mut seen: HashSet<String> = HashSet::new(); // lowercased candidate file names
    for item in items {
        let (file_name, content) = mapper(item);
        // Disambiguate: while the lowercased candidate is taken, append `-n` before
        // the extension (n from the ORIGINAL file name, matching v4).
        let mut candidate = file_name.clone();
        let mut n = 1u64;
        while seen.contains(&candidate.to_lowercase()) {
            let (base, ext) = match file_name.rfind('.') {
                Some(dot) => (&file_name[..dot], &file_name[dot..]),
                None => (file_name.as_str(), ""),
            };
            candidate = format!("{base}-{n}{ext}");
            n += 1;
        }
        seen.insert(candidate.to_lowercase());
        let rel_path = format!("{folder}/{candidate}");
        written_paths.insert(rel_path.clone());
        links.write_database_document(mount_point_id, &rel_path, &content)?;
    }

    for rel_path in &existing_paths {
        if written_paths.contains(rel_path) {
            continue;
        }
        links.delete_database_document(mount_point_id, rel_path)?;
    }
    Ok(())
}

/// Project an authoritative wardrobe-item list into a vault store's `Wardrobe/`
/// folder (v4 `projectVaultWardrobe`). Composite items emit their `componentItems:`
/// slug arrays via the slug map built here; the legacy `wardrobe.json` is deleted
/// after a successful projection so it can't drift back to authoritative-on-read.
pub fn project_vault_wardrobe(
    links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    items: &[WardrobeItem],
) -> Result<(), DbError> {
    let id_titles: Vec<(String, String)> = items
        .iter()
        .map(|it| (it.id.clone(), it.title.clone()))
        .collect();
    let slug_by_item_id: HashMap<String, String> =
        build_slug_by_item_id_map(&id_titles).into_iter().collect();

    project_array_into_vault_folder(
        links,
        docs,
        mount_point_id,
        WARDROBE_FOLDER,
        items,
        |item| {
            (
                format!("{}.md", sanitize_file_name(&item.title)),
                build_wardrobe_item_file(item, &slug_by_item_id),
            )
        },
    )?;

    // Clean up the legacy single-JSON file (NOT_FOUND tolerated → false).
    links.delete_database_document(mount_point_id, WARDROBE_JSON_PATH)?;
    Ok(())
}
