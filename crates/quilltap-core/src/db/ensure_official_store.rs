//! Official document-store provisioning — v4's `ensureOfficialStore`
//! (`lib/mount-index/ensure-official-store.ts`) ported as a Rust generic over a
//! [`StoreEntity`], plus the pure naming leaf `nextUniqueMountPointName`. The
//! group/project wrappers (`ensureGroupOfficialStore` / `ensureProjectOfficialStore`)
//! collapse into one call: the only per-entity differences (the store-name
//! prefix and the entity↔store link table) are expressed through `StoreEntity`.
//!
//! Find / adopt / create a store-backed entity's canonical "official" document
//! store and persist the FK on the slim row. Idempotent. The slim entity row
//! lives in the **main** DB; the mount point + link live in the **mount-index**
//! DB — so provisioning spans both connections.
//!
//! ## Resolution order (v4)
//!
//!   1. If `officialMountPointId` is set and the mount point still exists → use it.
//!   2. If a linked store matches the adopt heuristic → adopt it (raw FK write).
//!   3. Otherwise mint a fresh `<prefix><name>` store, link it, set the FK.
//!
//! The raw FK write (`set_official_mount_point_id`) is deliberate: provisioning
//! runs BEFORE the store files exist, so the overlay-applying `update()` would
//! throw `Unavailable` on its closing re-read.
//!
//! ## Scope
//!
//! `create()` always provisions fresh (it nulls any incoming FK), so the tier-2
//! differentials exercise **step 3** end-to-end (plus step 1's existence guard via
//! re-ensure idempotency). **Step 2 (adopt a hand-linked store)** is the
//! startup-heal heuristic (`pickPrimary*Store`) and is NOT ported here — it needs
//! a richer `doc_mount_points` read (name/mountType/storeType) and lands with the
//! startup-backfill slice; the corpora never have a pre-existing link, so nothing
//! in the verified path depends on it. A tracked deferral, not a stub on a
//! reachable path.

use std::collections::HashSet;

use rusqlite::Connection;

use super::doc_mount_points::{
    CreateOptions as DmpCreateOptions, DmpCreate, DocMountPointsRepository,
};
use super::document_store_overlay::StoreEntity;
use super::store_backed::StoreBackedRepository;
use super::DbError;

/// Returns `desired` if absent from `taken`, else the first of `desired (2)`,
/// `desired (3)`, … that is absent — v4 `nextUniqueMountPointName`. Numbering
/// starts at `(2)` (there is no `(1)`); the suffix is ` (N)` (one leading space,
/// parens around the number).
pub fn next_unique_mount_point_name(taken: &HashSet<String>, desired: &str) -> String {
    if !taken.contains(desired) {
        return desired.to_string();
    }
    let mut suffix = 2u32;
    loop {
        let candidate = format!("{desired} ({suffix})");
        if !taken.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

/// What [`ensure_official_store`] resolved/created.
pub struct EnsureResult {
    pub mount_point_id: String,
    pub created: bool,
}

/// Find or create the entity's canonical official document store and persist the
/// FK on the slim row (v4 `ensureOfficialStore`). Returns `None` when the slim
/// row does not exist. See the module header for the resolution order + the
/// step-2 deferral.
pub fn ensure_official_store<E: StoreEntity>(
    main: &Connection,
    mount: &Connection,
    entity_id: &str,
    entity_name: &str,
) -> Result<Option<EnsureResult>, DbError> {
    let repo = StoreBackedRepository::<E>::new(main, mount);

    // Read the RAW slim row (never the overlay-applied read — the store files may
    // not exist yet). We need only existence + the current FK.
    let Some(row) = repo.find_by_id_raw(entity_id)? else {
        return Ok(None);
    };
    let current_fk = row
        .get("officialMountPointId")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let points = DocMountPointsRepository::new(mount);

    // 1. Existing FK still valid?
    if let Some(fk) = current_fk {
        if points.exists(&fk)? {
            return Ok(Some(EnsureResult {
                mount_point_id: fk,
                created: false,
            }));
        }
        // A stale FK falls through to (re)provisioning.
    }

    // 2. Adopt a hand-linked store — deferred (see module header). The adopt
    //    branch would consult `E::find_store_links` here; the corpora never have a
    //    pre-existing link, so the create branch is always taken.

    // 3. Mint a fresh `<prefix><name>` store, link it, set the FK.
    let trimmed = {
        let n = entity_name.trim();
        if n.is_empty() {
            "Untitled"
        } else {
            n
        }
    };
    let desired = truncate_chars(&format!("{}{trimmed}", E::store_name_prefix()), 200);
    let taken: HashSet<String> = points.find_all_names()?.into_iter().collect();
    let final_name = next_unique_mount_point_name(&taken, &desired);

    let now = crate::clock::now_iso();
    let mount_point_id = uuid::Uuid::new_v4().to_string();
    points.create(
        &DmpCreate {
            name: final_name,
            base_path: String::new(),
            mount_type: "database".into(),
            store_type: "documents".into(),
            include_patterns: Vec::new(),
            exclude_patterns: vec![
                ".git".into(),
                "node_modules".into(),
                ".obsidian".into(),
                ".trash".into(),
            ],
            enabled: true,
            last_scanned_at: None,
            scan_status: "idle".into(),
            last_scan_error: None,
            conversion_status: "idle".into(),
            conversion_error: None,
            file_count: 0.0,
            chunk_count: 0.0,
            total_size_bytes: 0.0,
        },
        &DmpCreateOptions {
            id: mount_point_id.clone(),
            created_at: now.clone(),
            updated_at: now,
        },
    )?;

    E::link_store(mount, entity_id, &mount_point_id)?;
    repo.set_official_mount_point_id(entity_id, &mount_point_id)?;

    Ok(Some(EnsureResult {
        mount_point_id,
        created: true,
    }))
}

/// `String.prototype.slice(0, n)` over UTF-16 code units would be the exact v4
/// match; the store names are ASCII (the prefix + a user name), so a char-count
/// truncation suffices and the corpus stays within ASCII. (Documented alongside
/// the UTF-16 `plainTextLength` seam.)
fn truncate_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_unique_name_unused_passthrough() {
        let taken: HashSet<String> = HashSet::new();
        assert_eq!(
            next_unique_mount_point_name(&taken, "Group Files: A"),
            "Group Files: A"
        );
    }

    #[test]
    fn next_unique_name_starts_at_two_and_climbs() {
        let mut taken: HashSet<String> = HashSet::new();
        taken.insert("Store".into());
        assert_eq!(next_unique_mount_point_name(&taken, "Store"), "Store (2)");
        taken.insert("Store (2)".into());
        taken.insert("Store (3)".into());
        assert_eq!(next_unique_mount_point_name(&taken, "Store"), "Store (4)");
    }
}
