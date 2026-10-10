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
//! - store (mount-point) names: UNIQUE (name COLLATE NOCASE) — v4
//!   `ed8b15b50` (bug 186) REVERSED the old "no DB index, because backup
//!   restore must be able to recreate legacy rows verbatim" rule: every write
//!   chokepoint now refuses a taken name (`DocMountPointsRepository::create` /
//!   `update`), the restore and the import pick free names before inserting,
//!   and the index ([`ensure_mount_point_name_unique_index`]) is the backstop.
//!   (v4's own module header still carries the old sentence — a stale comment
//!   in v4, a filing candidate; the section comment there is current.)
//!
//! The `ensure_*_nocase_unique_index` helpers are wired into the boot hook
//! [`crate::services::builtin_mounts`] (`ensure_mount_index_tables`), which runs
//! once per process at startup. v4 calls them from EACH repo's lazy table-init
//! block (folders / file-links / mount-points repos). On CADENCE that collapse
//! is a non-divergence — each v4 site fires once per process once it succeeds.
//! On FAILURE it was not (dogfood #134(b)): v4's `ensureTable` logs ERROR
//! `Failed to ensure <table> table in mount index database` and rethrows into
//! the caller's fallback read, PER ACCESS, and the boot never touches it — a
//! damaged column degrades reads, it never stops the server. Since P4.134 the
//! boot hook answers a failure here with that line (`db::fallback::
//! ensure_table_or_log`) and continues; v5 logs it once per boot where v4 logs
//! it per access (the recorded cadence divergence). Each call renames any rows
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
use crate::jsstr::js_trim;

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
            // v4 `:183-189` — WARN with camelCase keys (P4.D274 R-G; v5 had
            // logged INFO with snake_case keys).
            tracing::warn!(
                target: "quilltap::mount_repair",
                mountPointId = %loser.mount_point_id,
                keptFolder = %keeper.name,
                oldName = %loser.name,
                newName = %new_name,
                oldPath = %loser.path,
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
            // v4 `:261-266` — WARN, camelCase (P4.D274 R-G).
            tracing::warn!(
                target: "quilltap::mount_repair",
                mountPointId = %loser.mount_point_id,
                keptPath = %keeper.relative_path,
                oldPath = %loser.relative_path,
                newPath = %new_rel,
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

pub const LINK_GROUP_INDEX: &str = "idx_doc_mount_file_links_linkGroupId";

/// Ensure `doc_mount_file_links.linkGroupId` (deliberate hard-link groups)
/// exists, along with its partial index — v4 `ensureLinkGroupColumn`
/// (`mount-index-case-repair.ts:299`).
///
/// v4 calls this from BOTH repositories that touch the column (links, and
/// documents through its joined view) because its `safeQuery` turns the
/// resulting "no such column" into a null result — a missing column would
/// present as *every document silently not existing*. **That premise does not
/// transfer to v5** (a documented non-divergence): v5 has no such swallow, so a
/// missing column is a hard `rusqlite` error, loud by construction. The two v4
/// lazy-init call sites therefore collapse to the single once-per-startup boot
/// hook in `services::builtin_mounts`, exactly as the three
/// `ensure*NocaseUniqueIndex` helpers already do.
///
/// v5 has no migration runner (a locked deferral), so this — plus the
/// `generateDDL` column on fresh instances — is the whole of v4's
/// `add-doc-mount-link-groups-v1` step 1.
///
/// Idempotent; one PRAGMA on the happy path.
pub fn ensure_link_group_column(db: &Connection) -> Result<(), DbError> {
    // On a fresh database the links table may not exist yet — only the links
    // repository creates it (with the column already in it, from the schema).
    if !table_exists(db, "doc_mount_file_links")? {
        return Ok(());
    }

    let has_column = {
        let mut stmt = db.prepare("PRAGMA table_info(\"doc_mount_file_links\")")?;
        let mut found = false;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            if row.get::<_, String>(1)? == "linkGroupId" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_column {
        db.execute_batch(
            "ALTER TABLE \"doc_mount_file_links\" ADD COLUMN \"linkGroupId\" TEXT DEFAULT NULL",
        )?;
    }
    db.execute_batch(&format!(
        "CREATE INDEX IF NOT EXISTS \"{LINK_GROUP_INDEX}\" \
         ON \"doc_mount_file_links\" (\"linkGroupId\") WHERE \"linkGroupId\" IS NOT NULL"
    ))?;
    Ok(())
}

// ============================================================================
// Store (mount-point) names: UNIQUE (name COLLATE NOCASE)
// ============================================================================

/// v4 `MOUNT_POINT_NAME_NOCASE_INDEX` (`ed8b15b50`).
pub const MOUNT_POINT_NAME_NOCASE_INDEX: &str = "idx_doc_mount_points_name_nocase";

/// The index's CREATE text — byte-identical to the line v4's `generateDDL`
/// emits (P4.D265's re-dumped `fresh_schema.json`, pinned in this module's
/// tests), so a fresh instance and a healed one carry the same definition.
const MOUNT_POINT_NAME_NOCASE_INDEX_SQL: &str =
    "CREATE UNIQUE INDEX \"idx_doc_mount_points_name_nocase\" ON \"doc_mount_points\" (\"name\" COLLATE NOCASE)";

/// Repair surviving name collisions, then make sure the unique name index is in
/// place — v4 `ensureMountPointNameUniqueIndex` (`mount-index-case-repair.ts:
/// 343-352`). Every write chokepoint already refuses a taken name
/// (`DocMountPointsRepository::create` / `update`), and a restore picks free
/// names before inserting; the index is the backstop for anything that slips
/// past them. NOCASE folds ASCII only — the JS-side checks fold the rest.
///
/// The repair runs FIRST and unconditionally (the index cannot be created over
/// a collision); then a genuine UNIQUE + NOCASE definition is trusted, anything
/// else (absent, or a tampered stand-in under the name) is dropped and
/// recreated — two statements, no transaction, as v4's two `db.exec`s — and v4's
/// INFO (root logger, no fields) is logged. On a fresh instance (P4.D265's DDL
/// carries the index) it is a no-op scan and logs nothing.
pub fn ensure_mount_point_name_unique_index(db: &Connection) -> Result<(), DbError> {
    repair_mount_point_name_collisions(db)?;
    if nocase_unique_index_is_valid(db, MOUNT_POINT_NAME_NOCASE_INDEX)? {
        return Ok(());
    }
    db.execute_batch(&format!(
        "DROP INDEX IF EXISTS \"{MOUNT_POINT_NAME_NOCASE_INDEX}\""
    ))?;
    db.execute_batch(MOUNT_POINT_NAME_NOCASE_INDEX_SQL)?;
    tracing::info!(
        target: "quilltap::mount_repair",
        "Mount-index: created the unique document-store name index"
    );
    Ok(())
}

struct MountPointNameRow {
    id: String,
    name: String,
    created_at: String,
}

/// Rename stores whose names collide with a peer's except for casing (or exactly
/// — legacy rows predate any uniqueness at all). Keep-oldest wins; losers get the
/// standard ` (N)` suffix. Cheap no-op scan when the invariant already holds;
/// runs on every boot ahead of the unique index, which cannot be created over a
/// collision. The character-aware naming (live vault names, retired vaults) is
/// `db::store_name_reconcile` (P4.D274), which needs the main database and so
/// runs later in boot. Returns 0 if the table is absent. The grouping key, the
/// taken set and the loser name are JS-trimmed ([`js_trim`]), as v4's `.trim()`.
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
        let key = js_trim(&row.name).to_lowercase();
        groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Vec::new()
        });
        groups.get_mut(&key).unwrap().push(idx);
    }

    let mut renamed = 0u64;
    let mut taken_lower: HashSet<String> = rows
        .iter()
        .map(|r| js_trim(&r.name).to_lowercase())
        .collect();
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
            let new_name = next_suffixed_name(&mut taken_lower, js_trim(&loser.name));
            db.execute(
                "UPDATE doc_mount_points SET name = ?1 WHERE id = ?2",
                params![new_name, loser.id],
            )?;
            renamed += 1;
            // v4 `:395-400` — WARN, camelCase (P4.D274 R-G).
            tracing::warn!(
                target: "quilltap::mount_repair",
                mountPointId = %loser.id,
                keptMountPointId = %keeper_id,
                oldName = %loser.name,
                newName = %new_name,
                "Mount-index case repair: renamed document store whose name collided with a peer except for casing",
            );
        }
    }
    Ok(renamed)
}

#[cfg(test)]
mod link_group_column_tests {
    use super::*;

    /// A pre-`40319484` mount index: the links table as `generateDDL` emitted it
    /// before the column existed (only the columns the helper reads matter).
    fn pre_column_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE \"doc_mount_file_links\" (\
               \"id\" TEXT PRIMARY KEY NOT NULL, \"fileId\" TEXT NOT NULL, \
               \"mountPointId\" TEXT NOT NULL, \"relativePath\" TEXT NOT NULL)",
        )
        .unwrap();
        db
    }

    fn has_column(db: &Connection, name: &str) -> bool {
        let mut stmt = db
            .prepare("PRAGMA table_info(\"doc_mount_file_links\")")
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            if row.get::<_, String>(1).unwrap() == name {
                return true;
            }
        }
        false
    }

    fn index_sql(db: &Connection) -> Option<String> {
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
            params![LINK_GROUP_INDEX],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .unwrap()
        .flatten()
    }

    #[test]
    fn adds_the_column_and_its_partial_index_to_an_existing_instance() {
        let db = pre_column_db();
        assert!(!has_column(&db, "linkGroupId"));

        ensure_link_group_column(&db).unwrap();

        assert!(has_column(&db, "linkGroupId"));
        let sql = index_sql(&db).expect("partial index created");
        // v4 creates it PARTIAL — the vast majority of links are ungrouped.
        assert!(sql.contains("WHERE \"linkGroupId\" IS NOT NULL"), "{sql}");
    }

    #[test]
    fn is_idempotent_across_boots() {
        let db = pre_column_db();
        ensure_link_group_column(&db).unwrap();
        // A second boot must not attempt the ALTER again (SQLite would error
        // with "duplicate column name").
        ensure_link_group_column(&db).unwrap();
        assert!(has_column(&db, "linkGroupId"));
        assert!(index_sql(&db).is_some());
    }

    #[test]
    fn is_a_no_op_before_the_links_table_exists() {
        // v4: whichever repository initializes first calls this, and only the
        // links repository creates the table.
        let db = Connection::open_in_memory().unwrap();
        ensure_link_group_column(&db).unwrap();
        assert!(index_sql(&db).is_none());
    }
}

#[cfg(test)]
mod name_index_tests {
    use super::*;

    fn db_with(names: &[(&str, &str, &str)]) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE doc_mount_points (id TEXT PRIMARY KEY, name TEXT NOT NULL, createdAt TEXT NOT NULL)",
        )
        .unwrap();
        for (id, name, created) in names {
            db.execute(
                "INSERT INTO doc_mount_points (id, name, createdAt) VALUES (?1, ?2, ?3)",
                params![id, name, created],
            )
            .unwrap();
        }
        db
    }

    fn index_sql(db: &Connection) -> Option<String> {
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
            params![MOUNT_POINT_NAME_NOCASE_INDEX],
            |r| r.get(0),
        )
        .optional()
        .unwrap()
    }

    /// The ensure's CREATE text is byte-identical to the line P4.D265's D23
    /// re-dump carries, so a fresh instance and a healed one hold one
    /// definition.
    #[test]
    fn create_text_matches_the_fresh_schema_dump() {
        let dump: serde_json::Value =
            serde_json::from_str(include_str!("../services/provisioning/fresh_schema.json"))
                .unwrap();
        let lines: Vec<&str> = dump["mountIndex"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .filter(|s| s.contains(MOUNT_POINT_NAME_NOCASE_INDEX))
            .collect();
        assert_eq!(lines, vec![MOUNT_POINT_NAME_NOCASE_INDEX_SQL]);
    }

    #[test]
    fn repairs_then_creates_the_index_once() {
        let db = db_with(&[
            ("a", "Tester Character Vault", "2024-01-01T00:00:00.000Z"),
            ("b", "tester character vault", "2024-02-01T00:00:00.000Z"),
        ]);
        let ((), lines) = crate::test_support::captured_with(|| {
            ensure_mount_point_name_unique_index(&db).unwrap();
        });
        assert_eq!(
            index_sql(&db).as_deref(),
            Some(MOUNT_POINT_NAME_NOCASE_INDEX_SQL)
        );
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].starts_with(
            "WARN quilltap::mount_repair Mount-index case repair: renamed document store"
        ));
        assert!(lines[0].ends_with(
            "mountPointId=b keptMountPointId=a oldName=tester character vault newName=tester character vault (2)"
        ));
        assert_eq!(
            lines[1],
            "INFO quilltap::mount_repair Mount-index: created the unique document-store name index"
        );
        let err = db
            .execute(
                "INSERT INTO doc_mount_points (id, name, createdAt) VALUES ('c', 'TESTER CHARACTER VAULT', 'x')",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().contains("UNIQUE"), "{err}");
        // Idempotent: a second pass renames nothing and logs nothing.
        let ((), again) = crate::test_support::captured_with(|| {
            ensure_mount_point_name_unique_index(&db).unwrap();
        });
        assert!(again.is_empty(), "{again:?}");
    }

    #[test]
    fn a_tampered_stand_in_is_replaced_and_a_valid_index_is_trusted() {
        let db = db_with(&[("a", "Lore", "2024-01-01T00:00:00.000Z")]);
        db.execute_batch(&format!(
            "CREATE INDEX \"{MOUNT_POINT_NAME_NOCASE_INDEX}\" ON \"doc_mount_points\" (\"name\")"
        ))
        .unwrap();
        ensure_mount_point_name_unique_index(&db).unwrap();
        assert_eq!(
            index_sql(&db).as_deref(),
            Some(MOUNT_POINT_NAME_NOCASE_INDEX_SQL)
        );

        let fresh = db_with(&[]);
        fresh
            .execute_batch(MOUNT_POINT_NAME_NOCASE_INDEX_SQL)
            .unwrap();
        let ((), lines) = crate::test_support::captured_with(|| {
            ensure_mount_point_name_unique_index(&fresh).unwrap();
        });
        assert!(lines.is_empty(), "{lines:?}");
    }
}
