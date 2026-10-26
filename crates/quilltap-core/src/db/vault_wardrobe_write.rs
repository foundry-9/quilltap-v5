//! The character vault **wardrobe write projection** — ports v4's
//! `projectVaultWardrobe` (`vault-overlay/wardrobe-sync.ts`) +
//! `projectArrayIntoVaultFolder` (`vault-overlay/vault-projection.ts`), the final
//! wardrobe write piece. Re-projects an authoritative `WardrobeItem` list into a
//! vault store's `Wardrobe/` folder: every item is written as
//! `Wardrobe/<folder>/<title>.md` (filename collisions disambiguated with `-1`,
//! `-2`, … suffixes, PER FOLDER), any item file in the folder NOT produced by
//! the current list is swept, and the legacy `wardrobe.json` is deleted so the
//! folder layout is the single on-disk source.
//!
//! **`4a38222e6` (P4.D279): the projection is RECURSIVE for the wardrobe.** The
//! operator's subfolders (`Wardrobe/Formal/Evening/`) are folders of items; an
//! item's `folder` is its path and nothing else, so a moved item is written at
//! its new path while the sweep removes the old one. Three rules keep the sweep
//! honest ([`ProjectArrayOptions`]): the sweep removes ONLY files the caller
//! OWNS (`owns_file` — for the wardrobe, files that parse as items), at every
//! depth, so a note or a half-written garment kept under `Wardrobe/` is never
//! deleted; `Wardrobe/images/` (the reserved first-level segment) is never
//! swept; and paths are compared LOWER-CASED, because the store's upsert is
//! case-insensitive and keeps the stored casing — an exact compare could delete
//! a file just rewritten with only a case change (that fix applies to the FLAT
//! callers too). The four flat callers (prompts / scenarios) pass the old
//! behaviour explicitly and see exactly the top-level semantics they always had.
//!
//! Composes the already-ported pure leaves — [`build_slug_by_item_id_map`],
//! [`build_wardrobe_item_file`] (the Decision-A YAML emitter),
//! [`sanitize_file_name`], [`is_wardrobe_item_document`] — over the
//! document-store write primitive
//! ([`DocMountFileLinksRepository::write_database_document`]), its GC delete
//! ([`DocMountFileLinksRepository::delete_database_document`]) and the folder
//! ensure ([`DocMountFileLinksRepository::ensure_folder_path`]).
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
    build_slug_by_item_id_map, build_wardrobe_item_file, is_wardrobe_item_document,
    sanitize_file_name, WardrobeItem,
};
use crate::wardrobe_folders::{join_folder, WARDROBE_RESERVED_FOLDER_SEGMENTS};
use crate::wardrobe_instructions::WARDROBE_INSTRUCTIONS_FILENAME;

const WARDROBE_FOLDER: &str = "Wardrobe";
const WARDROBE_JSON_PATH: &str = "wardrobe.json";

/// Options for [`project_array_into_vault_folder`] (v4 `ProjectArrayOptions`,
/// `4a38222e6`).
#[derive(Clone, Copy)]
pub struct ProjectArrayOptions<'a> {
    /// File names (case-insensitive) exempt from both the sweep and the
    /// projected name pool (v4 `preserveFileNames`, `b86bb1a5`). Matched against
    /// the file segment at any depth in recursive mode, at the folder's root
    /// otherwise. An empty list is byte-identical to the pre-`b86bb1a5`
    /// behaviour (v4 pins that omitting the option still sweeps
    /// `instructions.md`).
    pub preserve_file_names: &'a [&'a str],
    /// Recursive projection (v4 `recursive: { excludeSubfolders }`): the mapper
    /// may return a file name with `/` separators (a folder path under
    /// `folder`), the existing-file listing is recursive, and the sweep deletes
    /// stale `.md` at any depth except under these first-level subfolders
    /// (compared case-insensitively). Folders themselves are never deleted.
    /// `None` = flat (the prompts / scenarios callers).
    pub recursive: Option<&'a [&'a str]>,
    /// Which existing files the projection owns and may sweep (v4 `ownsFile`,
    /// called with `(relativePath, content)`). Without it every stale `.md` is
    /// swept (the flat callers' behaviour); with it, a file it rejects — a
    /// note kept beside the items — is left alone.
    #[allow(clippy::type_complexity)]
    pub owns_file: Option<&'a dyn Fn(&str, Option<&str>) -> bool>,
    /// The extension of the files the projection lists and may sweep (v4
    /// `ext`, `4fbdad186`; default `.md` there — every markdown caller passes
    /// it explicitly here, so no `Default` hides the choice). The Inventory
    /// projects `.json` objects. Only files with this extension are ever
    /// considered; the mapper's content is written as given.
    pub ext: &'a str,
}

impl ProjectArrayOptions<'_> {
    /// The flat projection with no preserved names and no ownership filter —
    /// what the prompts / scenarios callers always did.
    pub const FLAT: ProjectArrayOptions<'static> = ProjectArrayOptions {
        preserve_file_names: &[],
        recursive: None,
        owns_file: None,
        ext: ".md",
    };
}

/// Replace a vault folder's `.md` contents with a fresh projection of `items`
/// (v4 `projectArrayIntoVaultFolder`). `mapper` turns each item into its
/// `(fileName, content)` — in recursive mode the file name may carry `/`
/// separators; filename collisions (case-insensitive, per folder) get
/// `-1`/`-2`/… suffixes. Files present in the folder but not produced this pass
/// — and owned by the caller, see [`ProjectArrayOptions`] — are swept.
/// `scope_id` is v4's `characterId` argument: the parse / log scope the lines
/// name (a character id for a vault, the mount id for a shared store).
#[allow(clippy::too_many_arguments)]
pub fn project_array_into_vault_folder<T>(
    links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    folder: &str,
    items: &[T],
    mapper: impl Fn(&T) -> (String, String),
    scope_id: &str,
    opts: ProjectArrayOptions<'_>,
) -> Result<(), DbError> {
    let recursive = opts.recursive.is_some();
    let preserved: HashSet<String> = opts
        .preserve_file_names
        .iter()
        .map(|n| n.to_lowercase())
        .collect();
    let excluded: HashSet<String> = opts
        .recursive
        .unwrap_or_default()
        .iter()
        .map(|n| n.to_lowercase())
        .collect();

    let existing = docs.find_many_by_mount_points_in_folder_opts_or_empty(
        &[mount_point_id.to_string()],
        folder,
        opts.ext,
        recursive,
    )?;

    let mapped: Vec<(String, String)> = items.iter().map(&mapper).collect();

    // Every folder the projection writes into, created once each before the
    // writes (first spelling per lower-cased key). Flat mode only ever writes
    // into `folder` itself.
    let mut subfolders: Vec<String> = Vec::new();
    let mut subfolder_keys: HashSet<String> = HashSet::new();
    for (file_name, _) in &mapped {
        let sub = match (recursive, file_name.rfind('/')) {
            (true, Some(slash)) => file_name[..slash].to_string(),
            _ => String::new(),
        };
        if subfolder_keys.insert(sub.to_lowercase()) {
            subfolders.push(sub);
        }
    }
    // A folder already holding a document exists; ensure only the others (a
    // write would create them too, but an EMPTY projection target must exist).
    // Flat mode keeps its one unconditional ensure.
    let occupied: HashSet<String> = existing
        .iter()
        .map(|d| match d.relative_path.rfind('/') {
            Some(slash) => d.relative_path[..slash].to_lowercase(),
            None => String::new(),
        })
        .collect();
    for sub in &subfolders {
        let target = if sub.is_empty() {
            folder.to_string()
        } else {
            format!("{folder}/{sub}")
        };
        if recursive && occupied.contains(&target.to_lowercase()) {
            continue;
        }
        links.ensure_folder_path(mount_point_id, &target)?;
    }
    if subfolders.len() > 1 || (recursive && subfolders.len() == 1 && !subfolder_keys.contains(""))
    {
        tracing::debug!(
            characterId = %scope_id,
            mountPointId = %mount_point_id,
            folder = %folder,
            subfolderCount = subfolders.len(),
            "Recursive vault projection ensured subfolders"
        );
    }

    // The name pool is keyed by the full lower-cased path under `folder`, so
    // suffixing is per folder. Preserved names are claimed in every folder the
    // projection writes into (just the root in flat mode).
    let mut seen: HashSet<String> = HashSet::new();
    for key in &subfolder_keys {
        for name in &preserved {
            seen.insert(if key.is_empty() {
                name.clone()
            } else {
                format!("{key}/{name}")
            });
        }
    }
    let mut written_paths: HashSet<String> = HashSet::new();
    for (file_name, content) in &mapped {
        // Disambiguate: while the lowercased candidate is taken, append `-n`
        // before the extension (n from the ORIGINAL file name, matching v4).
        // `hasExt` is `dot > slash`, so `A.B/Coat` has no extension.
        let mut candidate = file_name.clone();
        let mut n = 1u64;
        while seen.contains(&candidate.to_lowercase()) {
            let slash = file_name.rfind('/').map(|i| i as i64).unwrap_or(-1);
            let dot = file_name.rfind('.').map(|i| i as i64).unwrap_or(-1);
            let (base, ext) = if dot > slash {
                let d = dot as usize;
                (&file_name[..d], &file_name[d..])
            } else {
                (file_name.as_str(), "")
            };
            candidate = format!("{base}-{n}{ext}");
            n += 1;
        }
        seen.insert(candidate.to_lowercase());
        let rel_path = format!("{folder}/{candidate}");
        written_paths.insert(rel_path.to_lowercase());
        links.write_database_document(mount_point_id, &rel_path, content)?;
    }

    // The sweep. Paths are compared LOWER-CASED (the store's upsert keeps the
    // stored casing, so an exact compare could delete a file just rewritten
    // with only a case change).
    let folder_prefix_len = folder.len() + 1;
    for doc in &existing {
        let rel_path = doc.relative_path.as_str();
        if written_paths.contains(&rel_path.to_lowercase()) {
            continue;
        }
        // v4 slices at the LAST `/` — with no slash the whole path is the
        // segment, so a root-level file name still matches.
        let file_segment = match rel_path.rfind('/') {
            Some(i) => &rel_path[i + 1..],
            None => rel_path,
        }
        .to_lowercase();
        if preserved.contains(&file_segment) {
            continue;
        }
        if let Some(owns) = opts.owns_file {
            if !owns(rel_path, Some(doc.content.as_str())) {
                continue;
            }
        }
        if recursive {
            let rest = rel_path.get(folder_prefix_len..).unwrap_or("");
            if let Some(first_slash) = rest.find('/') {
                if excluded.contains(&rest[..first_slash].to_lowercase()) {
                    continue;
                }
            }
        }
        // v4 catches a failed delete and WARNs `Failed to delete stale vault
        // file during folder projection` {characterId, mountPointId,
        // relativePath, docId, error}, then carries on. v5's delete answers
        // `Ok(false)` for a vanished link (v4's NOT_FOUND throw) and `Err` only
        // for a storage failure, which is rendered here with the bare message;
        // the document id is not in the listing row, so the line carries the
        // fields it has.
        if let Err(e) = links.delete_database_document(mount_point_id, rel_path) {
            tracing::warn!(
                characterId = %scope_id,
                mountPointId = %mount_point_id,
                relativePath = %rel_path,
                error = %crate::db::fallback::error_text(&e),
                "Failed to delete stale vault file during folder projection"
            );
        }
    }
    Ok(())
}

/// Project an authoritative wardrobe-item list into a vault store's `Wardrobe/`
/// folder (v4 `projectVaultWardrobe(mountPointId, characterId, items)`) — each
/// item at `Wardrobe/<folder>/<title>.md`. Composite items emit their
/// `componentItems:` slug arrays via the slug map built here; the legacy
/// `wardrobe.json` is deleted after a successful projection so it can't drift
/// back to authoritative-on-read. `scope_id` is v4's `characterId` (the mount
/// id for a shared store — `characterId ?? mountPointId` upstream).
pub fn project_vault_wardrobe(
    links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    scope_id: &str,
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
                // The folder is the path: an item filed in `Formal/Evening` is
                // written to `Wardrobe/Formal/Evening/<title>.md`, and a moved
                // item is written at its new path while the sweep removes the
                // old one.
                join_folder(&[
                    item.folder.as_str(),
                    &format!("{}.md", sanitize_file_name(&item.title)),
                ]),
                build_wardrobe_item_file(item, &slug_by_item_id),
            )
        },
        scope_id,
        ProjectArrayOptions {
            // The dressing-instructions file is not a garment: the sweep must
            // never delete it, and an item titled "Instructions" must land on a
            // suffix.
            preserve_file_names: &[WARDROBE_INSTRUCTIONS_FILENAME],
            // Subfolders are the operator's filing; `images/` holds item
            // pictures and is never swept.
            recursive: Some(WARDROBE_RESERVED_FOLDER_SEGMENTS),
            // Only item files are the wardrobe's to sweep: a note, or a file
            // that doesn't parse as an item, kept under `Wardrobe/` (at any
            // depth) is left exactly where the operator put it.
            owns_file: Some(&is_wardrobe_item_document),
            ext: ".md",
        },
    )?;

    // Clean up the legacy single-JSON file (NOT_FOUND tolerated → false).
    links.delete_database_document(mount_point_id, WARDROBE_JSON_PATH)?;
    Ok(())
}
