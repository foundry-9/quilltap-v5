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
//! ## Lines
//!
//! v4's four lines, with the entity's label (`project` / `group`) and its
//! id key (`projectId` / `groupId`): the WARN `ensure<Label>OfficialStore:
//! <label> not found`, the INFO `<Label> officialMountPointId points to a
//! missing mount point; will heal`, the INFO `Adopted existing linked store as
//! <label> official`, the INFO `Created <label>-official document store`
//! (P4.171 — D273-21 / C-9, pinned by `scenario_location_equivalence`).

use std::collections::HashSet;

use rusqlite::Connection;

use super::doc_mount_points::{
    CreateOptions as DmpCreateOptions, DmpCreate, DocMountPointsRepository,
};
use super::document_store_overlay::StoreEntity;
use super::store_backed::StoreBackedRepository;
use super::DbError;
use crate::jsstr::{js_trim, utf16_truncate};

/// Returns `desired` if absent from `taken`, else the first of `desired (2)`,
/// `desired (3)`, … that is absent — v4 `nextUniqueMountPointName`. Numbering
/// starts at `(2)` (there is no `(1)`); the suffix is ` (N)` (one leading space,
/// parens around the number).
///
/// Matching is **case-insensitive (trimmed)** — mount-point names form one
/// namespace regardless of casing (v4 `0a0419f5`), so `lore` counts as taken
/// when `Lore` exists. Callers pass names in their stored casing; folding
/// happens here. `desired` comes back UNTRIMMED when it is free, as v4 returns
/// it.
///
/// The trim is JS's `String.prototype.trim` ([`js_trim`]: U+FEFF trims, U+0085
/// does not), not Rust's `str::trim` — converged at P4.D274, pinned by the
/// `store_names_equivalence` suffixer rows (which also cover restore 22a's
/// free-name pick, the C1 §14 caller).
pub fn next_unique_mount_point_name(taken: &HashSet<String>, desired: &str) -> String {
    let taken_lower: HashSet<String> = taken.iter().map(|n| js_trim(n).to_lowercase()).collect();
    if !taken_lower.contains(&js_trim(desired).to_lowercase()) {
        return desired.to_string();
    }
    let mut suffix = 2u32;
    loop {
        let candidate = format!("{desired} ({suffix})");
        if !taken_lower.contains(&js_trim(&candidate).to_lowercase()) {
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

/// One of v4's provisioning lines, keyed by the entity's own id key
/// (`projectId` / `groupId` — v4's `config.entityIdLogKey`). A tracing field
/// name is static, so the key is chosen by matching the entity label.
macro_rules! entity_line {
    ($lvl:ident, $label:expr, $id:expr, $msg:expr $(, $k:ident = $v:expr)*) => {
        if $label == "project" {
            tracing::$lvl!(projectId = %$id $(, $k = %$v)*, "{}", $msg)
        } else {
            tracing::$lvl!(groupId = %$id $(, $k = %$v)*, "{}", $msg)
        }
    };
}

/// v4's capitalised entity noun (`config.entityLabelCapitalized`).
fn capitalized(label: &str) -> String {
    let mut c = label.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Find or create the entity's canonical official document store and persist the
/// FK on the slim row (v4 `ensureOfficialStore`). Returns `None` when the slim
/// row does not exist. See the module header for the resolution order and the
/// lines.
pub fn ensure_official_store<E: StoreEntity>(
    main: &Connection,
    mount: &Connection,
    entity_id: &str,
    entity_name: &str,
) -> Result<Option<EnsureResult>, DbError> {
    let repo = StoreBackedRepository::<E>::new(main, mount);
    let label = E::entity_label();

    // Read the RAW slim row (never the overlay-applied read — the store files may
    // not exist yet). We need only existence + the current FK.
    let Some(row) = repo.find_by_id_raw(entity_id)? else {
        entity_line!(
            warn,
            label,
            entity_id,
            format!(
                "ensure{}OfficialStore: {label} not found",
                capitalized(label)
            )
        );
        return Ok(None);
    };
    // v4 `if (entity.officialMountPointId)` — truthy, so an empty FK is no FK.
    let current_fk = row
        .get("officialMountPointId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
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
        entity_line!(
            info,
            label,
            entity_id,
            format!(
                "{} officialMountPointId points to a missing mount point; will heal",
                capitalized(label)
            ),
            staleMountPointId = fk
        );
    }

    // 2. Adopt an existing linked store if one matches: a database-backed
    //    `documents` store (`storeType ?? 'documents'`), preferring one whose
    //    name carries the entity's own prefix (v4 `isOwnStoreName` —
    //    `name.startsWith(prefix)`), else the first eligible in link order.
    //    Raw FK write — must not re-read the store overlay: on the create path
    //    the adopted store may have no properties.json yet, and the
    //    overlay-applying `update()` would throw `Unavailable`.
    let links = E::find_store_links(mount, entity_id)?;
    if !links.is_empty() {
        let mut eligible = Vec::new();
        for link in &links {
            if let Some(s) = points.find_store_naming_by_id(link)? {
                if s.mount_type == "database"
                    && s.store_type.as_deref().unwrap_or("documents") == "documents"
                {
                    eligible.push(s);
                }
            }
        }
        let adoptable = eligible
            .iter()
            .find(|s| s.name.starts_with(E::store_name_prefix()))
            .or_else(|| eligible.first());
        if let Some(adoptable) = adoptable {
            repo.set_official_mount_point_id(entity_id, &adoptable.id)?;
            entity_line!(
                info,
                label,
                entity_id,
                format!("Adopted existing linked store as {label} official"),
                mountPointId = adoptable.id,
                storeName = adoptable.name
            );
            return Ok(Some(EnsureResult {
                mount_point_id: adoptable.id.clone(),
                created: false,
            }));
        }
    }

    // 3. Mint a fresh `<prefix><name>` store, link it, set the FK. v4's
    //    `(entityName || 'Untitled').trim()`: only an EMPTY name is falsy, so a
    //    whitespace-only name trims to the bare prefix; then `.slice(0, 200)`
    //    over UTF-16 units.
    let named = if entity_name.is_empty() {
        "Untitled"
    } else {
        js_trim(entity_name)
    };
    let desired = utf16_truncate(&format!("{}{named}", E::store_name_prefix()), 200);
    let taken: HashSet<String> = points.find_all_names()?.into_iter().collect();
    let final_name = next_unique_mount_point_name(&taken, &desired);

    let now = crate::clock::now_iso();
    let mount_point_id = uuid::Uuid::new_v4().to_string();
    points.create(
        &DmpCreate {
            name: final_name.clone(),
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

    entity_line!(
        info,
        label,
        entity_id,
        format!("Created {label}-official document store"),
        mountPointId = mount_point_id,
        storeName = final_name
    );

    Ok(Some(EnsureResult {
        mount_point_id,
        created: true,
    }))
}

/// The two owners that have an official store (v4 `StoreOwnerKind`,
/// `ensure-owner-store.ts`, `9ecebf986`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreOwnerKind {
    Project,
    Group,
}

impl StoreOwnerKind {
    /// v4's wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            StoreOwnerKind::Project => "project",
            StoreOwnerKind::Group => "group",
        }
    }
}

/// v4 `ensureOwnerOfficialStore(kind, ownerId, ownerName)`
/// (`lib/mount-index/ensure-owner-store.ts`, `9ecebf986`) — find / adopt /
/// create the official document store of a project or a group and persist the
/// FK on the owner's row. The two owners differ only in their repository wiring
/// and naming ([`StoreEntity`]), which is all the kind selects; the
/// find/adopt/create flow itself is [`ensure_official_store`]. Idempotent.
/// `None` when the owner row is missing.
pub fn ensure_owner_official_store(
    main: &Connection,
    mount: &Connection,
    kind: StoreOwnerKind,
    owner_id: &str,
    owner_name: &str,
) -> Result<Option<EnsureResult>, DbError> {
    match kind {
        StoreOwnerKind::Project => ensure_official_store::<super::projects::ProjectEntity>(
            main, mount, owner_id, owner_name,
        ),
        StoreOwnerKind::Group => {
            ensure_official_store::<super::groups::GroupEntity>(main, mount, owner_id, owner_name)
        }
    }
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

    /// P4.D274: the trim is JS's — U+FEFF trims (so `"Store\u{FEFF}"` holds
    /// `store`), U+0085 does not (so `"Store\u{85}"` leaves `store` free).
    /// Rust's `str::trim` had both backwards.
    #[test]
    fn next_unique_name_trims_as_js_does() {
        let feff: HashSet<String> = ["Store\u{FEFF}".to_string()].into();
        assert_eq!(next_unique_mount_point_name(&feff, "store"), "store (2)");
        let nel: HashSet<String> = ["Store\u{85}".to_string()].into();
        assert_eq!(next_unique_mount_point_name(&nel, "store"), "store");
    }
}
