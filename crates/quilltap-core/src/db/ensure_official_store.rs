//! Official document-store provisioning — v4's `ensureOfficialStore`
//! (`lib/mount-index/ensure-official-store.ts`) + the group wrapper
//! `ensureGroupOfficialStore`, plus the pure naming leaf
//! `nextUniqueMountPointName`.
//!
//! Find / adopt / create a store-backed entity's canonical "official" document
//! store and persist the FK on the slim row. Idempotent. The slim entity row
//! lives in the **main** DB (`groups`); the mount point + link live in the
//! **mount-index** DB — so provisioning spans both connections.
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
//! ## Scope for the groups pilot
//!
//! `create()` always provisions fresh (it nulls any incoming FK), so the groups
//! tier-2 differential exercises **step 3** end-to-end (plus step 1's existence
//! guard via the re-ensure idempotency). **Step 2 (adopt a hand-linked store)**
//! is the startup-heal heuristic (`pickPrimaryGroupStore`) and is NOT ported here
//! — it needs a richer `doc_mount_points` read (name/mountType/storeType) and
//! lands with the startup-backfill slice; the corpus never has a pre-existing
//! link, so nothing in the verified path depends on it. This is a tracked
//! deferral, not a stub on a reachable path.

use std::collections::HashSet;

use rusqlite::Connection;

use super::doc_mount_points::{CreateOptions as DmpCreateOptions, DmpCreate};
use super::groups::GroupsRepository;
use super::DbError;

/// The name prefix for a group's auto-created own store (v4
/// `GROUP_OWN_STORE_NAME_PREFIX`).
pub const GROUP_OWN_STORE_NAME_PREFIX: &str = "Group Files: ";

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

/// What [`ensure_group_official_store`] resolved/created.
pub struct EnsureResult {
    pub mount_point_id: String,
    pub created: bool,
}

/// Find or create the group's canonical "group-official" document store and
/// persist the FK on the group row (v4 `ensureGroupOfficialStore`). Returns
/// `None` when the group row does not exist. See the module header for the
/// resolution order and the step-2 deferral.
pub fn ensure_group_official_store(
    main: &Connection,
    mount: &Connection,
    group_id: &str,
    group_name: &str,
) -> Result<Option<EnsureResult>, DbError> {
    let groups = GroupsRepository::new(main, mount);

    // Read the RAW slim row (never the overlay-applied read — the store files may
    // not exist yet). We need only existence + the current FK.
    let Some(row) = groups.find_by_id_raw(group_id)? else {
        return Ok(None);
    };
    let current_fk = row
        .get("officialMountPointId")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let points = mount_points_repo(mount);

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

    // 2. Adopt a hand-linked store — deferred (see module header). The corpus
    //    never has a pre-existing link, so the create branch is taken.

    // 3. Mint a fresh `Group Files: <name>` store, link it, set the FK.
    let trimmed = {
        let n = group_name.trim();
        if n.is_empty() {
            "Untitled"
        } else {
            n
        }
    };
    let desired = truncate_chars(&format!("{GROUP_OWN_STORE_NAME_PREFIX}{trimmed}"), 200);
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

    GroupDocMountLinksRepository::new(mount).link(group_id, &mount_point_id)?;
    groups.set_official_mount_point_id(group_id, &mount_point_id)?;

    Ok(Some(EnsureResult {
        mount_point_id,
        created: true,
    }))
}

use super::doc_mount_points::DocMountPointsRepository;
use super::group_doc_mount_links::GroupDocMountLinksRepository;

fn mount_points_repo(mount: &Connection) -> DocMountPointsRepository<'_> {
    DocMountPointsRepository::new(mount)
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
