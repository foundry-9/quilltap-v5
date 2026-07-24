//! Mount-index case-collision repair — the port of v4's
//! `lib/database/repositories/mount-index-case-repair.ts` (the `0a0419f5`
//! case-insensitive mount-namespace fix).
//!
//! Database-backed vaults treat names case-insensitively on the read side (path
//! lookups go through `LOWER()`/`COLLATE NOCASE`), so two siblings whose names
//! differ only by casing — `Lore` vs `lore`, `Notes.md` vs `notes.md` — are
//! ambiguous: readers resolve whichever row sorts first and the other is
//! shadowed. The uniqueness invariant is therefore case-INSENSITIVE:
//!
//! - folder siblings: UNIQUE (mountPointId, parentId, name COLLATE NOCASE)
//! - file links: UNIQUE (mountPointId, relativePath COLLATE NOCASE)
//! - vault names: app-level (see the mount-points routes and
//!   `next_unique_mount_point_name`) plus the repair below — no DB index,
//!   because backup restore must be able to recreate legacy rows verbatim
//!   before repair runs.
//!
//! The `ensure_*_nocase_unique_index` helpers are wired into the boot hook
//! [`crate::services::builtin_mounts`] (`ensure_mount_index_tables`), which runs
//! once per process at startup. v4 calls them from EACH repo's lazy table-init
//! block (folders / file-links / mount-points repos) — three call sites that in
//! v4's architecture each fire effectively once per process. v5 has no per-repo
//! lazy `getCollection()` init; the single boot hook is the same effective
//! once-per-startup cadence, so the three v4 call sites collapse to it here (a
//! documented non-divergence, not a behavior change). Each call renames any rows
//! that violate the invariant (keep-oldest wins, losers get the standard ` (2)`,
//! ` (3)`, … suffix) and then makes sure a *genuine* unique NOCASE index is in
//! place, replacing the legacy case-sensitive one or any tampered stand-in.
//!
//! The repair scan runs UNCONDITIONALLY, not just when the index is missing: the
//! index blocks violations while it is present and honest, but the database is a
//! file the operator can open directly, so rows can drift in from outside — an
//! index dropped and duplicates inserted, an index recreated under the right
//! name with the wrong definition, or non-ASCII case-variants (which ASCII-only
//! NOCASE tolerates but the `to_lowercase()` grouping here catches). The scan is
//! one narrow SELECT plus in-memory grouping — a no-op boot cost when the
//! invariant holds.
//!
//! Deliberately NOT wrapped in an explicit transaction: every rename is an
//! individually-consistent step (row + descendants + links together), the whole
//! pass is idempotent, and the new-index-exists check re-runs it on the next
//! boot if the process dies partway through.
//!
//! NOTE: SQLite's NOCASE collation (and `LOWER()`) only fold ASCII. Non-ASCII
//! case-variants (e.g. `Ärger` vs `ärger`) are not folded at the DB level — the
//! JS-side write guards use `toLowerCase()` and catch most of those, but the
//! hard constraint is ASCII-only. That matches the `LOWER()`-based read paths,
//! which have the same limitation.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};

use super::DbError;
use crate::collation::locale_compare;

pub const FOLDER_NOCASE_INDEX: &str = "idx_doc_mount_folders_mp_parent_name_nocase";
const FOLDER_LEGACY_INDEX: &str = "idx_doc_mount_folders_mp_parent_name";
pub const LINK_NOCASE_INDEX: &str = "idx_doc_mount_file_links_mp_path_nocase";
const LINK_LEGACY_INDEX: &str = "idx_doc_mount_file_links_mp_path";

// ---------------------------------------------------------------------------
// Node `path.posix` helpers, scoped to this module's needs (all inputs are
// normalised relative paths / basenames — no leading or trailing slashes).
// ---------------------------------------------------------------------------

/// Node `path.posix.dirname`: everything before the final `/`, or `"."` when the
/// path has no directory component.
fn posix_dirname(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..i].to_string(),
        None => ".".to_string(),
    }
}

/// Node `path.posix.parse(fileName)` split into `(name, ext)`: `ext` includes the
/// leading dot, or `""` when the segment has no dot or is a dotfile (the only dot
/// at index 0). `name` is the remainder.
fn split_name_ext(file_name: &str) -> (&str, &str) {
    match file_name.rfind('.') {
        None | Some(0) => (file_name, ""),
        Some(i) => (&file_name[..i], &file_name[i..]),
    }
}

// ---------------------------------------------------------------------------
// Index-validity + table-existence probes.
// ---------------------------------------------------------------------------

/// True only when an index with this name exists AND its stored CREATE statement
/// declares both UNIQUE and NOCASE. A plain existence check would be satisfiable
/// by a hand-made non-unique (or non-NOCASE) index under the same name, silently
/// disabling the constraint.
fn nocase_unique_index_is_valid(db: &Connection, name: &str) -> Result<bool, DbError> {
    let sql: Option<Option<String>> = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
            params![name],
            |row| row.get(0),
        )
        .optional()?;
    // Row absent, or present with a NULL sql → not valid.
    let Some(Some(sql)) = sql else {
        return Ok(false);
    };
    let upper = sql.to_uppercase();
    Ok(upper.contains("UNIQUE") && upper.contains("NOCASE"))
}

fn table_exists(db: &Connection, name: &str) -> Result<bool, DbError> {
    let found: Option<String> = db
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

// ---------------------------------------------------------------------------
// Suffix minting.
// ---------------------------------------------------------------------------

/// First `${base} (2)`, `${base} (3)`, … whose lowercased form is absent from
/// `taken_lower`. The chosen name's lowercased form is added to the set so
/// successive calls in the same sibling group stay mutually unique.
fn next_suffixed_name(taken_lower: &mut HashSet<String>, base: &str) -> String {
    let mut n = 2;
    while taken_lower.contains(&format!("{base} ({n})").to_lowercase()) {
        n += 1;
    }
    let chosen = format!("{base} ({n})");
    taken_lower.insert(chosen.to_lowercase());
    chosen
}

/// Same policy for file names: the suffix goes before the extension.
fn next_suffixed_file_name(
    taken_lower_rel: &mut HashSet<String>,
    dir: &str,
    file_name: &str,
) -> String {
    let (name, ext) = split_name_ext(file_name);
    let mut n = 2;
    loop {
        let candidate = format!("{name} ({n}){ext}");
        let candidate_rel = if dir.is_empty() {
            candidate.clone()
        } else {
            format!("{dir}/{candidate}")
        };
        n += 1;
        if !taken_lower_rel.contains(&candidate_rel.to_lowercase()) {
            taken_lower_rel.insert(candidate_rel.to_lowercase());
            return candidate;
        }
    }
}

/// Keep-oldest comparator: `createdAt.localeCompare(...) || id.localeCompare(...)`
/// — createdAt ascending, id ascending as the tiebreak, both via ICU
/// `localeCompare` (v4 uses `String.prototype.localeCompare`).
fn keep_oldest_less(
    a_created: &str,
    a_id: &str,
    b_created: &str,
    b_id: &str,
) -> std::cmp::Ordering {
    locale_compare(a_created, b_created).then_with(|| locale_compare(a_id, b_id))
}

// ============================================================================
// Folders: UNIQUE (mountPointId, COALESCE(parentId,''), name COLLATE NOCASE)
// ============================================================================

struct FolderRow {
    id: String,
    mount_point_id: String,
    parent_id: Option<String>,
    name: String,
    path: String,
    created_at: String,
}

/// Rename a folder row and repair everything that denormalises its path:
/// descendant folder rows (walked via parentId, NOT by path prefix, so exact
/// duplicate paths from very old databases can't cross-contaminate) and every
/// link row inside the subtree (matched by folderId, relativePath rebuilt from
/// the folder's repaired path).
fn rename_folder_row(db: &Connection, folder: &FolderRow, new_name: &str) -> Result<(), DbError> {
    let dir = posix_dirname(&folder.path);
    let new_path = if dir == "." {
        new_name.to_string()
    } else {
        format!("{dir}/{new_name}")
    };
    db.execute(
        "UPDATE doc_mount_folders SET name = ?1, path = ?2 WHERE id = ?3",
        params![new_name, new_path, folder.id],
    )?;

    // Breadth-first walk of the subtree, repairing each child's path from its
    // parent's (already repaired) path.
    let mut repaired: Vec<(String, String)> = vec![(folder.id.clone(), new_path)];
    let mut i = 0;
    while i < repaired.len() {
        let (parent_id, parent_path) = repaired[i].clone();
        let children: Vec<(String, String)> = {
            let mut stmt = db.prepare(
                "SELECT id, name FROM doc_mount_folders WHERE mountPointId = ?1 AND parentId = ?2",
            )?;
            let rows = stmt.query_map(params![folder.mount_point_id, parent_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (child_id, child_name) in children {
            let child_path = format!("{parent_path}/{child_name}");
            db.execute(
                "UPDATE doc_mount_folders SET path = ?1 WHERE id = ?2",
                params![child_path, child_id],
            )?;
            repaired.push((child_id, child_path));
        }
        i += 1;
    }

    if !table_exists(db, "doc_mount_file_links")? {
        return Ok(());
    }
    for (node_id, node_path) in &repaired {
        let links: Vec<(String, String)> = {
            let mut stmt = db.prepare(
                "SELECT id, fileName FROM doc_mount_file_links WHERE mountPointId = ?1 AND folderId = ?2",
            )?;
            let rows = stmt.query_map(params![folder.mount_point_id, node_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (link_id, file_name) in links {
            db.execute(
                "UPDATE doc_mount_file_links SET relativePath = ?1 WHERE id = ?2",
                params![format!("{node_path}/{file_name}"), link_id],
            )?;
        }
    }
    Ok(())
}

fn repair_folder_case_collisions(db: &Connection) -> Result<u64, DbError> {
    let rows: Vec<FolderRow> = {
        let mut stmt = db.prepare(
            "SELECT id, mountPointId, parentId, name, path, createdAt FROM doc_mount_folders",
        )?;
        let mapped = stmt.query_map([], |row| {
            Ok(FolderRow {
                id: row.get(0)?,
                mount_point_id: row.get(1)?,
                parent_id: row.get(2)?,
                name: row.get(3)?,
                path: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        mapped.collect::<Result<Vec<_>, _>>()?
    };

    // Group by (mountPointId, parentId ?? '', name.toLowerCase()) preserving
    // first-seen order so the differential's row-set match is order-independent
    // anyway, but the keep-oldest sort below makes it deterministic.
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, row) in rows.iter().enumerate() {
        let key = format!(
            "{} {} {}",
            row.mount_point_id,
            row.parent_id.as_deref().unwrap_or(""),
            row.name.to_lowercase()
        );
        groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Vec::new()
        });
        groups.get_mut(&key).unwrap().push(idx);
    }

    let mut renamed = 0u64;
    for key in &order {
        let mut group: Vec<usize> = groups[key].clone();
        if group.len() < 2 {
            continue;
        }
        group.sort_by(|&a, &b| {
            keep_oldest_less(
                &rows[a].created_at,
                &rows[a].id,
                &rows[b].created_at,
                &rows[b].id,
            )
        });
        let keeper = &rows[group[0]];
        let mut sibling_lower: HashSet<String> = rows
            .iter()
            .filter(|r| {
                r.mount_point_id == keeper.mount_point_id
                    && r.parent_id.as_deref().unwrap_or("")
                        == keeper.parent_id.as_deref().unwrap_or("")
            })
            .map(|r| r.name.to_lowercase())
            .collect();
        for &loser_idx in &group[1..] {
            let loser = &rows[loser_idx];
            let new_name = next_suffixed_name(&mut sibling_lower, &loser.name);
            rename_folder_row(db, loser, &new_name)?;
            renamed += 1;
            tracing::info!(
                target: "quilltap::mount_repair",
                mount_point_id = %loser.mount_point_id,
                kept_folder = %keeper.name,
                old_name = %loser.name,
                new_name = %new_name,
                old_path = %loser.path,
                "Mount-index case repair: renamed folder that collided with a sibling except for casing",
            );
        }
    }
    Ok(renamed)
}

/// Ensure the case-insensitive sibling-uniqueness index on `doc_mount_folders`,
/// repairing case-collisions first. The repair scan is unconditional (manual
/// edits can slip past or remove the index — see the module docs), and the index
/// is recreated unless a genuine unique NOCASE definition is already in place.
pub fn ensure_folder_nocase_unique_index(db: &Connection) -> Result<(), DbError> {
    repair_folder_case_collisions(db)?;
    if nocase_unique_index_is_valid(db, FOLDER_NOCASE_INDEX)? {
        return Ok(());
    }
    db.execute_batch(&format!(
        "DROP INDEX IF EXISTS \"{FOLDER_LEGACY_INDEX}\";\
         DROP INDEX IF EXISTS \"{FOLDER_NOCASE_INDEX}\";"
    ))?;
    // COALESCE because SQLite treats each NULL as distinct in UNIQUE indexes.
    db.execute_batch(&format!(
        "CREATE UNIQUE INDEX \"{FOLDER_NOCASE_INDEX}\" \
         ON \"doc_mount_folders\" (\"mountPointId\", COALESCE(\"parentId\", ''), \"name\" COLLATE NOCASE)"
    ))?;
    Ok(())
}

// ============================================================================
// File links: UNIQUE (mountPointId, relativePath COLLATE NOCASE)
// ============================================================================

struct LinkRow {
    id: String,
    mount_point_id: String,
    relative_path: String,
    file_name: String,
    created_at: String,
}

fn repair_link_case_collisions(db: &Connection) -> Result<u64, DbError> {
    let rows: Vec<LinkRow> = {
        let mut stmt = db.prepare(
            "SELECT id, mountPointId, relativePath, fileName, createdAt FROM doc_mount_file_links",
        )?;
        let mapped = stmt.query_map([], |row| {
            Ok(LinkRow {
                id: row.get(0)?,
                mount_point_id: row.get(1)?,
                relative_path: row.get(2)?,
                file_name: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        mapped.collect::<Result<Vec<_>, _>>()?
    };

    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, row) in rows.iter().enumerate() {
        let key = format!(
            "{} {}",
            row.mount_point_id,
            row.relative_path.to_lowercase()
        );
        groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Vec::new()
        });
        groups.get_mut(&key).unwrap().push(idx);
    }

    let mut renamed = 0u64;
    for key in &order {
        let mut group: Vec<usize> = groups[key].clone();
        if group.len() < 2 {
            continue;
        }
        group.sort_by(|&a, &b| {
            keep_oldest_less(
                &rows[a].created_at,
                &rows[a].id,
                &rows[b].created_at,
                &rows[b].id,
            )
        });
        let keeper = &rows[group[0]];
        let mut taken_lower_rel: HashSet<String> = rows
            .iter()
            .filter(|r| r.mount_point_id == keeper.mount_point_id)
            .map(|r| r.relative_path.to_lowercase())
            .collect();
        for &loser_idx in &group[1..] {
            let loser = &rows[loser_idx];
            let dir = posix_dirname(&loser.relative_path);
            let normal_dir = if dir == "." { "" } else { dir.as_str() };
            let new_file_name =
                next_suffixed_file_name(&mut taken_lower_rel, normal_dir, &loser.file_name);
            let new_rel = if normal_dir.is_empty() {
                new_file_name.clone()
            } else {
                format!("{normal_dir}/{new_file_name}")
            };
            db.execute(
                "UPDATE doc_mount_file_links SET relativePath = ?1, fileName = ?2 WHERE id = ?3",
                params![new_rel, new_file_name, loser.id],
            )?;
            renamed += 1;
            tracing::info!(
                target: "quilltap::mount_repair",
                mount_point_id = %loser.mount_point_id,
                kept_path = %keeper.relative_path,
                old_path = %loser.relative_path,
                new_path = %new_rel,
                "Mount-index case repair: renamed file that collided with a sibling except for casing",
            );
        }
    }
    Ok(renamed)
}

/// Ensure the case-insensitive one-file-per-location index on
/// `doc_mount_file_links`, repairing case-collisions first. The repair scan is
/// unconditional, and the index is recreated unless a genuine unique NOCASE
/// definition is already in place (see the module docs for why existence alone
/// is not trusted).
pub fn ensure_link_nocase_unique_index(db: &Connection) -> Result<(), DbError> {
    repair_link_case_collisions(db)?;
    if nocase_unique_index_is_valid(db, LINK_NOCASE_INDEX)? {
        return Ok(());
    }
    db.execute_batch(&format!(
        "DROP INDEX IF EXISTS \"{LINK_LEGACY_INDEX}\";\
         DROP INDEX IF EXISTS \"{LINK_NOCASE_INDEX}\";"
    ))?;
    db.execute_batch(&format!(
        "CREATE UNIQUE INDEX \"{LINK_NOCASE_INDEX}\" \
         ON \"doc_mount_file_links\" (\"mountPointId\", \"relativePath\" COLLATE NOCASE)"
    ))?;
    Ok(())
}

// ============================================================================
// Vault (mount-point) names — app-level uniqueness, repaired at boot
// ============================================================================

struct MountPointNameRow {
    id: String,
    name: String,
    created_at: String,
}

/// Rename vaults whose names collide with a peer's except for casing (or exactly
/// — legacy rows predate any uniqueness at all). Keep-oldest wins; losers get the
/// standard ` (N)` suffix. Cheap no-op scan when the invariant already holds, so
/// the boot hook runs it on every startup — that also mops up duplicates
/// reintroduced by a backup restore. Returns 0 if the table is absent.
pub fn repair_mount_point_name_collisions(db: &Connection) -> Result<u64, DbError> {
    if !table_exists(db, "doc_mount_points")? {
        return Ok(0);
    }
    let rows: Vec<MountPointNameRow> = {
        let mut stmt = db.prepare("SELECT id, name, createdAt FROM doc_mount_points")?;
        let mapped = stmt.query_map([], |row| {
            Ok(MountPointNameRow {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        mapped.collect::<Result<Vec<_>, _>>()?
    };

    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, row) in rows.iter().enumerate() {
        // trim applies to vault names only (folders/links are not trimmed).
        let key = row.name.trim().to_lowercase();
        groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Vec::new()
        });
        groups.get_mut(&key).unwrap().push(idx);
    }

    let mut renamed = 0u64;
    let mut taken_lower: HashSet<String> =
        rows.iter().map(|r| r.name.trim().to_lowercase()).collect();
    for key in &order {
        let mut group: Vec<usize> = groups[key].clone();
        if group.len() < 2 {
            continue;
        }
        group.sort_by(|&a, &b| {
            keep_oldest_less(
                &rows[a].created_at,
                &rows[a].id,
                &rows[b].created_at,
                &rows[b].id,
            )
        });
        let keeper_id = rows[group[0]].id.clone();
        for &loser_idx in &group[1..] {
            let loser = &rows[loser_idx];
            let new_name = next_suffixed_name(&mut taken_lower, loser.name.trim());
            db.execute(
                "UPDATE doc_mount_points SET name = ?1 WHERE id = ?2",
                params![new_name, loser.id],
            )?;
            renamed += 1;
            tracing::info!(
                target: "quilltap::mount_repair",
                mount_point_id = %loser.id,
                kept_mount_point_id = %keeper_id,
                old_name = %loser.name,
                new_name = %new_name,
                "Mount-index case repair: renamed document store whose name collided with a peer except for casing",
            );
        }
    }
    Ok(renamed)
}
