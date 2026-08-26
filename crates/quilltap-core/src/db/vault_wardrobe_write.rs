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
use crate::wardrobe_instructions::WARDROBE_INSTRUCTIONS_FILENAME;

const WARDROBE_FOLDER: &str = "Wardrobe";
const WARDROBE_JSON_PATH: &str = "wardrobe.json";

/// Replace a vault folder's `.md` contents with a fresh projection of `items`
/// (v4 `projectArrayIntoVaultFolder`). `mapper` turns each item into its
/// `(fileName, content)`; filename collisions (case-insensitive) get `-1`/`-2`/…
/// suffixes. Files present in the folder but not produced this pass are swept.
///
/// `preserve_file_names` (v4 `opts.preserveFileNames`, `b86bb1a5`) exempts
/// specific root-level file names (case-insensitive) from both the sweep and the
/// projected name pool, so a hand-kept file like `Wardrobe/instructions.md`
/// survives every projection and an item whose title maps to the same name lands
/// on a `-1` suffix instead of overwriting it. An empty list is byte-identical
/// to the pre-`b86bb1a5` behaviour (v4 pins that omitting the option still
/// sweeps `instructions.md`).
pub fn project_array_into_vault_folder<T>(
    links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    folder: &str,
    items: &[T],
    mapper: impl Fn(&T) -> (String, String),
    preserve_file_names: &[&str],
) -> Result<(), DbError> {
    let existing =
        docs.find_many_by_mount_points_in_folder(&[mount_point_id.to_string()], folder, ".md")?;
    // The relative paths currently in the folder, to sweep what we don't rewrite.
    let existing_paths: Vec<String> = existing.into_iter().map(|d| d.relative_path).collect();

    // v4 calls `ensureFolderPath` when items > 0; the write primitive already
    // find-or-creates the folder segments on each write, so an explicit ensure is
    // redundant (and an empty list correctly creates no folder). Match: write-only.

    let preserved: HashSet<String> = preserve_file_names
        .iter()
        .map(|n| n.to_lowercase())
        .collect();

    let mut written_paths: HashSet<String> = HashSet::new();
    // Seeded with the preserved names so a garment titled "Instructions"
    // disambiguates to `instructions-1.md` rather than overwriting the file.
    let mut seen: HashSet<String> = preserved.clone(); // lowercased candidate file names
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
        // v4 slices at the LAST `/` — with no slash the whole path is the
        // segment, so a root-level file name still matches.
        let file_segment = match rel_path.rfind('/') {
            Some(i) => &rel_path[i + 1..],
            None => rel_path.as_str(),
        }
        .to_lowercase();
        if preserved.contains(&file_segment) {
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
        // The dressing-instructions file is not a garment: the sweep must never
        // delete it, and an item titled "Instructions" must land on a suffix.
        &[WARDROBE_INSTRUCTIONS_FILENAME],
    )?;

    // Clean up the legacy single-JSON file (NOT_FOUND tolerated → false).
    links.delete_database_document(mount_point_id, WARDROBE_JSON_PATH)?;
    Ok(())
}
