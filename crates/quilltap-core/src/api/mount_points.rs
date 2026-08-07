//! The global mount-points dispatch handlers (P4.6p) — a differential port of v4's
//! `mount-points/route.ts` + `[id]/route.ts` (list / get / create / patch /
//! delete-cascade), composed over the ported `db::doc_mount_points` repo + the two
//! new embedded-count reads + the pure `derive_mount_capabilities`. The mount-index
//! partition backs every table here (reads via [`Db::read_mount_index`]; writes on
//! the mount-index writer). The action verbs + `semantic-search` live in
//! [`super::mount_files`] (P4.6y — D7 closed; `convert`/`deconvert` are the
//! remaining loud refusal arms there).

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::db::character_vault::scaffold_character_mount;
use crate::db::doc_mount_chunks::{
    count_embedded_by_mount_point_ids, count_nonempty_embeddings_by_mount_point_id,
};
use crate::db::doc_mount_points::{
    find_all_full_json, CreateOptions, DmpCreate, DmpUpdate, DocMountPointsRepository,
};
use crate::db::runtime::Db;
use crate::db::DbError;

use super::types::{ErrorKind, Response};

// ===========================================================================
// Response helpers
// ===========================================================================

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}
fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn conflict(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Conflict, msg)
}

/// The stored-casing name of the store (if any) whose trimmed/lowercased name
/// matches `desired`, excluding `exclude_id` (for renames). Store names form one
/// case-insensitive namespace (v4 `0a0419f5`).
fn find_store_name_clash(
    db: &Db,
    desired: &str,
    exclude_id: Option<&str>,
) -> Result<Option<String>, crate::db::DbError> {
    let desired_lower = desired.trim().to_lowercase();
    let exclude = exclude_id.map(str::to_string);
    db.read_mount_index(move |conn| {
        let mut stmt = conn.prepare("SELECT id, name FROM doc_mount_points")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<(String, String)>, _>>()?;
        Ok(rows.into_iter().find_map(|(id, name)| {
            let excluded = exclude.as_deref() == Some(id.as_str());
            (!excluded && name.trim().to_lowercase() == desired_lower).then_some(name)
        }))
    })
}

const DEFAULT_INCLUDE: [&str; 4] = ["*.md", "*.txt", "*.pdf", "*.docx"];
const DEFAULT_EXCLUDE: [&str; 4] = [".git", "node_modules", ".obsidian", ".trash"];

// ===========================================================================
// deriveMountCapabilities (pure)
// ===========================================================================

/// v4 `deriveMountCapabilities` — the non-persisted per-mount capability flags.
/// `quiescent = enabled && !midConversion`; every write/move flag is `quiescent`;
/// `canConvert` additionally requires `scanStatus !== 'scanning'`.
fn derive_mount_capabilities(mp: &Value) -> Value {
    let conversion = mp
        .get("conversionStatus")
        .and_then(Value::as_str)
        .unwrap_or("idle");
    let mid_conversion = conversion == "converting" || conversion == "deconverting";
    let enabled = mp.get("enabled").and_then(Value::as_bool).unwrap_or(false);
    let quiescent = enabled && !mid_conversion;
    let scan = mp
        .get("scanStatus")
        .and_then(Value::as_str)
        .unwrap_or("idle");
    json!({
        "canWrite": quiescent,
        "canDelete": quiescent,
        "canCreateFolder": quiescent,
        "canMoveIn": quiescent,
        "canMoveOut": quiescent,
        "canConvert": quiescent && scan != "scanning",
    })
}

// ===========================================================================
// Handlers
// ===========================================================================

/// v4 `GET /api/v1/mount-points` — `findAll` (unscoped) → createdAt DESC → the
/// CHEAP GROUP-BY embedded count → `{mountPoints: [DocMountPoint &
/// {embeddedChunkCount}]}`.
pub fn mount_point_list(db: &Db) -> Response {
    let result = db.read_mount_index(|conn| {
        let mut mounts = find_all_full_json(conn)?;
        mounts.sort_by(|a, b| {
            let ta = a.get("createdAt").and_then(Value::as_str).unwrap_or("");
            let tb = b.get("createdAt").and_then(Value::as_str).unwrap_or("");
            tb.cmp(ta)
        });
        let ids: Vec<String> = mounts
            .iter()
            .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
            .collect();
        let counts = count_embedded_by_mount_point_ids(conn, &ids)?;
        let enriched: Vec<Value> = mounts
            .into_iter()
            .map(|mut m| {
                let id = m
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let c = counts.get(&id).copied().unwrap_or(0);
                if let Some(obj) = m.as_object_mut() {
                    obj.insert("embeddedChunkCount".into(), json!(c));
                }
                m
            })
            .collect();
        Ok(json!({ "mountPoints": enriched }))
    });
    match result {
        Ok(v) => Response::MountPoint(v),
        Err(e) => internal(e),
    }
}

/// v4 `GET /api/v1/mount-points/[id]` — 404 / the EXPENSIVE hydrate-and-filter
/// embedded count + derived capabilities → `{mountPoint: {…, embeddedChunkCount,
/// capabilities}}`.
pub fn mount_point_get(db: &Db, mount_point_id: &str) -> Response {
    let result = db.read_mount_index(|conn| {
        let repo = DocMountPointsRepository::new(conn);
        match repo.find_full_json_by_id(mount_point_id)? {
            Some(mut mp) => {
                let count = count_nonempty_embeddings_by_mount_point_id(conn, mount_point_id)?;
                let capabilities = derive_mount_capabilities(&mp);
                if let Some(obj) = mp.as_object_mut() {
                    obj.insert("embeddedChunkCount".into(), json!(count));
                    obj.insert("capabilities".into(), capabilities);
                }
                Ok(Some(mp))
            }
            None => Ok(None),
        }
    });
    match result {
        Ok(Some(mp)) => Response::MountPoint(json!({ "mountPoint": mp })),
        Ok(None) => not_found("Mount point"),
        Err(e) => internal(e),
    }
}

/// The validated create fields (v4 `createMountPointSchema`).
struct CreateFields {
    name: String,
    base_path: String,
    mount_type: String,
    store_type: String,
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
    enabled: bool,
}

fn parse_string_array(v: &Value) -> Option<Vec<String>> {
    v.as_array().map(|a| {
        a.iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    })
}

/// v4 `POST /api/v1/mount-points` — `createMountPointSchema.parse` (no try/catch,
/// so a Zod failure → the middleware `Validation error` 400), then create, the
/// `verifyBasePath` warning seam, and the character-scaffold arm; `{mountPoint,
/// warning?}`.
pub async fn mount_point_create(db: &Db, body: Value) -> Response {
    let fields = match validate_create(&body) {
        Ok(f) => f,
        Err(r) => return r,
    };
    // Document-store names form one case-insensitive namespace: no store may
    // share a name with a peer, even in a different casing (v4 `0a0419f5`).
    match find_store_name_clash(db, &fields.name, None) {
        Ok(Some(clash_name)) => {
            return conflict(format!(
                "A document store named \"{clash_name}\" already exists. Names are matched without regard to case — please choose a different name."
            ));
        }
        Ok(None) => {}
        Err(e) => return internal(e),
    }
    let id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();
    let create = DmpCreate {
        name: fields.name.clone(),
        base_path: fields.base_path.clone(),
        mount_type: fields.mount_type.clone(),
        store_type: fields.store_type.clone(),
        include_patterns: fields.include_patterns.clone(),
        exclude_patterns: fields.exclude_patterns.clone(),
        enabled: fields.enabled,
        last_scanned_at: None,
        scan_status: "idle".to_string(),
        last_scan_error: None,
        conversion_status: "idle".to_string(),
        conversion_error: None,
        file_count: 0.0,
        chunk_count: 0.0,
        total_size_bytes: 0.0,
    };
    let opts = CreateOptions {
        id: id.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let scaffold = fields.store_type == "character" && fields.mount_type == "database";
    let scaffold_id = id.clone();
    let write = db
        .write(move |ws| {
            let conn = mount_conn(ws)?;
            DocMountPointsRepository::new(conn).create(&create, &opts)?;
            // v4's post-create character-scaffold (best-effort — never fails create).
            if scaffold {
                let _ = scaffold_character_mount(conn, &scaffold_id);
            }
            Ok(())
        })
        .await;
    if let Err(e) = write {
        return internal(e);
    }

    // The created DocMountPoint (in-memory `validate(entityInput)` — every field
    // present, the three nullables rendered as `null`).
    let mut mp = Map::new();
    mp.insert("id".into(), Value::String(id));
    mp.insert("name".into(), Value::String(fields.name));
    mp.insert("basePath".into(), Value::String(fields.base_path.clone()));
    mp.insert("mountType".into(), Value::String(fields.mount_type.clone()));
    mp.insert("storeType".into(), Value::String(fields.store_type));
    mp.insert("includePatterns".into(), json!(fields.include_patterns));
    mp.insert("excludePatterns".into(), json!(fields.exclude_patterns));
    mp.insert("enabled".into(), Value::Bool(fields.enabled));
    mp.insert("lastScannedAt".into(), Value::Null);
    mp.insert("scanStatus".into(), Value::String("idle".into()));
    mp.insert("lastScanError".into(), Value::Null);
    mp.insert("conversionStatus".into(), Value::String("idle".into()));
    mp.insert("conversionError".into(), Value::Null);
    mp.insert("fileCount".into(), json!(0));
    mp.insert("chunkCount".into(), json!(0));
    mp.insert("totalSizeBytes".into(), json!(0));
    mp.insert("createdAt".into(), Value::String(now.clone()));
    mp.insert("updatedAt".into(), Value::String(now));
    let mount_point = Value::Object(mp);

    // verifyBasePath seam: a database mount skips the check; every other mount type
    // is treated as inaccessible (the injected deterministic default — the
    // differential drives v4 with a nonexistent path, so both produce the warning).
    if fields.mount_type != "database" {
        let warning = format!(
            "Base path '{}' is not currently accessible. The mount point was created but scanning will fail until the path is available.",
            fields.base_path
        );
        Response::MountPoint(json!({ "mountPoint": mount_point, "warning": warning }))
    } else {
        Response::MountPoint(json!({ "mountPoint": mount_point }))
    }
}

fn validate_create(body: &Value) -> Result<CreateFields, Response> {
    let validation_error = || bad_request("Validation error");
    let name = match body.get("name").and_then(Value::as_str) {
        Some(s) if (1..=200).contains(&crate::jsstr::utf16_len(s)) => s.to_string(),
        _ => return Err(validation_error()),
    };
    let base_path = body
        .get("basePath")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mount_type = match body.get("mountType") {
        None | Some(Value::Null) => "filesystem".to_string(),
        Some(Value::String(s)) if ["filesystem", "obsidian", "database"].contains(&s.as_str()) => {
            s.clone()
        }
        Some(_) => return Err(validation_error()),
    };
    let store_type = match body.get("storeType") {
        None | Some(Value::Null) => "documents".to_string(),
        Some(Value::String(s)) if ["documents", "character"].contains(&s.as_str()) => s.clone(),
        Some(_) => return Err(validation_error()),
    };
    let include_patterns = match body.get("includePatterns") {
        None | Some(Value::Null) => DEFAULT_INCLUDE.iter().map(|s| s.to_string()).collect(),
        Some(v) => parse_string_array(v).ok_or_else(validation_error)?,
    };
    let exclude_patterns = match body.get("excludePatterns") {
        None | Some(Value::Null) => DEFAULT_EXCLUDE.iter().map(|s| s.to_string()).collect(),
        Some(v) => parse_string_array(v).ok_or_else(validation_error)?,
    };
    let enabled = match body.get("enabled") {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(validation_error()),
    };
    // The `.refine`: basePath required unless database.
    if mount_type != "database" && base_path.is_empty() {
        return Err(validation_error());
    }
    Ok(CreateFields {
        name,
        base_path,
        mount_type,
        store_type,
        include_patterns,
        exclude_patterns,
        enabled,
    })
}

/// v4 `PATCH /api/v1/mount-points/[id]` — 404 / the whole-handler-try/catch quirk
/// (a Zod validation failure surfaces as a **500** `Failed to update mount point`,
/// NOT 400) / the storeType-flip character scaffold; `{mountPoint}` (NO count /
/// capabilities on the echo).
pub async fn mount_point_update(db: &Db, mount_point_id: &str, body: Value) -> Response {
    let existing = db.read_mount_index(|conn| {
        DocMountPointsRepository::new(conn).find_full_json_by_id(mount_point_id)
    });
    let existing = match existing {
        Ok(Some(m)) => m,
        Ok(None) => return not_found("Mount point"),
        Err(e) => return internal(e),
    };
    // The whole PATCH handler is one try/catch → a validation failure is a 500,
    // not a 400.
    let update_failed = || internal("Failed to update mount point");
    let Some(body_obj) = body.as_object() else {
        return update_failed();
    };

    let mut merged = existing.clone();
    let mo = merged.as_object_mut().unwrap();
    let mut patch = DmpUpdate::default();

    // Each field: validate the type (a failure → 500), set the merged echo + patch.
    if let Some(v) = body_obj.get("name") {
        match v.as_str() {
            Some(s) if (1..=200).contains(&crate::jsstr::utf16_len(s)) => {
                mo.insert("name".into(), Value::String(s.to_string()));
                patch.name = Some(s.to_string());
            }
            _ => return update_failed(),
        }
    }
    if let Some(v) = body_obj.get("basePath") {
        match v.as_str() {
            Some(s) => {
                mo.insert("basePath".into(), Value::String(s.to_string()));
                patch.base_path = Some(s.to_string());
            }
            None => return update_failed(),
        }
    }
    if let Some(v) = body_obj.get("mountType") {
        match v.as_str() {
            Some(s) if ["filesystem", "obsidian", "database"].contains(&s) => {
                mo.insert("mountType".into(), Value::String(s.to_string()));
                patch.mount_type = Some(s.to_string());
            }
            _ => return update_failed(),
        }
    }
    if let Some(v) = body_obj.get("storeType") {
        match v.as_str() {
            Some(s) if ["documents", "character"].contains(&s) => {
                mo.insert("storeType".into(), Value::String(s.to_string()));
                patch.store_type = Some(s.to_string());
            }
            _ => return update_failed(),
        }
    }
    if let Some(v) = body_obj.get("includePatterns") {
        match parse_string_array(v) {
            Some(a) => {
                mo.insert("includePatterns".into(), json!(a));
                patch.include_patterns = Some(a);
            }
            None => return update_failed(),
        }
    }
    if let Some(v) = body_obj.get("excludePatterns") {
        match parse_string_array(v) {
            Some(a) => {
                mo.insert("excludePatterns".into(), json!(a));
                patch.exclude_patterns = Some(a);
            }
            None => return update_failed(),
        }
    }
    if let Some(v) = body_obj.get("enabled") {
        match v.as_bool() {
            Some(b) => {
                mo.insert("enabled".into(), Value::Bool(b));
                patch.enabled = Some(b);
            }
            None => return update_failed(),
        }
    }

    let now = crate::clock::now_iso();
    patch.updated_at = now.clone();
    mo.insert("updatedAt".into(), Value::String(now));

    let existing_store = existing
        .get("storeType")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let new_store = merged
        .get("storeType")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let new_type = merged
        .get("mountType")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let flipped =
        existing_store != "character" && new_store == "character" && new_type == "database";

    // Renames stay inside the case-insensitive name namespace: no store may take
    // a name a peer already holds, even in a different casing (v4 `0a0419f5`).
    // Only when `name` is present; the clash excludes the store itself.
    if let Some(desired) = patch.name.clone() {
        match find_store_name_clash(db, &desired, Some(mount_point_id)) {
            Ok(Some(clash_name)) => {
                return conflict(format!(
                    "A document store named \"{clash_name}\" already exists. Names are matched without regard to case — please choose a different name."
                ));
            }
            Ok(None) => {}
            Err(e) => return internal(e),
        }
    }

    let id = mount_point_id.to_string();
    let write = db
        .write(move |ws| {
            let conn = mount_conn(ws)?;
            let updated = DocMountPointsRepository::new(conn).update(&id, &patch)?;
            if !updated {
                return Ok(false);
            }
            if flipped {
                let _ = scaffold_character_mount(conn, &id);
            }
            Ok(true)
        })
        .await;
    match write {
        Ok(true) => Response::MountPoint(json!({ "mountPoint": merged })),
        // update returning false → v4 logs + serverError (500).
        Ok(false) => update_failed(),
        Err(e) => internal(e),
    }
}

/// v4 `DELETE /api/v1/mount-points/[id]` — 404 / the cascade → `{message}`.
///
/// The wire is v4's byte-for-byte; the WRITES deliberately are not. See
/// [`cascade_delete`].
pub async fn mount_point_delete(db: &Db, mount_point_id: &str) -> Response {
    let existing = db.read_mount_index(|conn| {
        DocMountPointsRepository::new(conn).find_full_json_by_id(mount_point_id)
    });
    match existing {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Mount point"),
        Err(e) => return internal(e),
    }
    let id = mount_point_id.to_string();
    let out = db
        .write(move |ws| {
            let conn = mount_conn(ws)?;
            cascade_delete(conn, &id)
        })
        .await;
    match out {
        Ok(()) => Response::MountPoint(json!({ "message": "Mount point deleted successfully" })),
        Err(e) => internal(e),
    }
}

/// THE store-delete chokepoint (P4.31) — every removal of a `doc_mount_points`
/// row in production goes through here, inside ONE transaction, and takes the
/// store's children with it.
///
/// This is dogfood finding #58's root cause. The measured damage on the real
/// instance (2026-08-03) was 43 orphaned `doc_mount_file_links` + 118 orphaned
/// `doc_mount_folders` across 21 vanished character vaults; P4.28 taught the
/// RESTORE to survive them, and this is the end that stops minting them. Nothing
/// else enforces it: read connections set no `PRAGMA foreign_keys`, and a
/// `generateDDL` link table declares no foreign keys at all (only the
/// migration-vintage schema carries the cascade), so a partial cascade leaves
/// rows nothing will ever collect.
///
/// The detach watcher is a fire-and-forget host seam and is omitted, as before.
/// Chunks / links / folders / project-links stay exactly v4's scoped deletes.
/// **Three fixes v5 made first, which v4 has since CONVERGED to** (`3bb664f0`,
/// bug 9: `delete-store-cascade.ts`) — so `store_delete_equivalence` now
/// compares them as plain equalities (the both-directions divergence pins are
/// retired). The history, kept because the code shape follows it:
///
///  1. **The whole cascade is one transaction.** v4 runs seven independent
///     awaited repo calls; a failure at any of them leaves the store half-gone
///     and the survivors unreachable. `unchecked_transaction` is the same shape
///     `sweep_orphaned_link_content` already uses on this connection.
///  2. **Content dies through [`gc_orphaned_file_row`], not through v4's dead
///     document/blob steps.** v4's `docMountDocuments.deleteByMountPointId`
///     (`doc-mount-documents.repository.ts:327-343`) selects the file ids from
///     `doc_mount_file_links` — *after* `docMountFiles.deleteByMountPointId` has
///     emptied that table — so it matches nothing and `doc_mount_documents`
///     (which has no foreign key on either schema vintage) leaks forever. Its
///     `docMountBlobs.deleteByMountPointId` (`doc-mount-blobs.repository.ts:
///     628-655`) is a copy-paste of the files step: it re-deletes links and
///     files and never names `doc_mount_blobs`. That one is currently invisible
///     — `doc_mount_blobs` is the one mount table whose hand-written DDL carries
///     `ON DELETE CASCADE`, on BOTH vintages, and both apps enable
///     `foreign_keys = ON` on their writers — but relying on that is relying on
///     a pragma this cascade does not set for itself, so the collect is
///     explicit. `gc_orphaned_file_row` is the existing chokepoint that already
///     deletes documents + blobs + the file row for exactly this reason, and it
///     spares a file another store still hard-links.
///  3. **`group_doc_mount_links` is deleted alongside `project_doc_mount_links`.**
///     v4 deletes only the project links; its sole group-link delete is
///     `deleteByGroupId`, which no store delete calls. A group that had linked
///     the store keeps a join row pointing at nothing.
///
/// A quieter fourth difference remains: v4's repos wrap these deletes in
/// `safeQuery`, so a mount index MISSING one of the named tables still answers
/// 200; v5 names them without a gate, so the same delete 500s and rolls back
/// whole. Deliberate — both tables are in `fresh_schema.json` and referenced
/// all over v5, so the arm is unreachable on any ensured schema, and a
/// half-vintage index failing LOUDLY beats it half-cascading.
fn cascade_delete(conn: &Connection, mount_point_id: &str) -> Result<(), DbError> {
    let tx = conn.unchecked_transaction()?;

    // 1. chunks.
    tx.execute(
        "DELETE FROM doc_mount_chunks WHERE mountPointId = ?1",
        rusqlite::params![mount_point_id],
    )?;

    // 2. links, then the content each link was the last reference to. The
    // snapshot must be taken BEFORE the link delete — that ordering is exactly
    // what v4's documents step gets wrong.
    let affected: Vec<String> = {
        let mut stmt =
            tx.prepare("SELECT DISTINCT fileId FROM doc_mount_file_links WHERE mountPointId = ?1")?;
        let rows = stmt.query_map(rusqlite::params![mount_point_id], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    tx.execute(
        "DELETE FROM doc_mount_file_links WHERE mountPointId = ?1",
        rusqlite::params![mount_point_id],
    )?;
    for fid in &affected {
        crate::db::doc_mount_file_links::gc_orphaned_file_row(&tx, fid)?;
    }

    // 3. folders (explicit — no FK cascade on either vintage).
    tx.execute(
        "DELETE FROM doc_mount_folders WHERE mountPointId = ?1",
        rusqlite::params![mount_point_id],
    )?;

    // 4. project links (v4 finds then per-link deletes → net a scoped delete).
    tx.execute(
        "DELETE FROM project_doc_mount_links WHERE mountPointId = ?1",
        rusqlite::params![mount_point_id],
    )?;

    // 5. group links — divergence 3 above.
    tx.execute(
        "DELETE FROM group_doc_mount_links WHERE mountPointId = ?1",
        rusqlite::params![mount_point_id],
    )?;

    // 6. the point itself.
    DocMountPointsRepository::new(&tx).delete_row_only(mount_point_id)?;

    tx.commit()?;
    Ok(())
}

/// The mount-index write connection (this whole family is mount-index-partitioned).
fn mount_conn(ws: &crate::db::runtime::WriterSet) -> Result<&Connection, DbError> {
    Ok(ws
        .mount_index()
        .ok_or_else(|| DbError::Key("mount-points require the mount-index database".to_string()))?
        .connection())
}

#[cfg(test)]
mod tests {
    use super::{cascade_delete, derive_mount_capabilities};
    use serde_json::json;

    #[test]
    fn capabilities_match_v4() {
        let quiescent =
            json!({ "enabled": true, "conversionStatus": "idle", "scanStatus": "idle" });
        let c = derive_mount_capabilities(&quiescent);
        assert_eq!(c["canWrite"], true);
        assert_eq!(c["canConvert"], true);

        let scanning =
            json!({ "enabled": true, "conversionStatus": "idle", "scanStatus": "scanning" });
        assert_eq!(derive_mount_capabilities(&scanning)["canWrite"], true);
        assert_eq!(derive_mount_capabilities(&scanning)["canConvert"], false);

        let converting =
            json!({ "enabled": true, "conversionStatus": "converting", "scanStatus": "idle" });
        assert_eq!(derive_mount_capabilities(&converting)["canWrite"], false);

        let disabled =
            json!({ "enabled": false, "conversionStatus": "idle", "scanStatus": "idle" });
        assert_eq!(derive_mount_capabilities(&disabled)["canWrite"], false);
    }

    /// The mount-index tables the two cascade tests below build, in their
    /// generateDDL shape MINUS every foreign key — including `doc_mount_blobs`'
    /// `ON DELETE CASCADE`, which is the only one the real schema has. The
    /// cascade must not depend on a pragma it does not set for itself.
    fn fk_free_mount_index(with_project_links: bool) -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        conn.execute_batch(
            "CREATE TABLE doc_mount_points (id TEXT PRIMARY KEY, name TEXT NOT NULL);\
             CREATE TABLE doc_mount_files (id TEXT PRIMARY KEY, sha256 TEXT NOT NULL);\
             CREATE TABLE doc_mount_documents (id TEXT PRIMARY KEY, fileId TEXT NOT NULL);\
             CREATE TABLE doc_mount_blobs (id TEXT PRIMARY KEY, fileId TEXT NOT NULL);\
             CREATE TABLE doc_mount_file_links (id TEXT PRIMARY KEY, fileId TEXT NOT NULL, \
               mountPointId TEXT NOT NULL, linkGroupId TEXT);\
             CREATE TABLE doc_mount_folders (id TEXT PRIMARY KEY, mountPointId TEXT NOT NULL);\
             CREATE TABLE doc_mount_chunks (id TEXT PRIMARY KEY, mountPointId TEXT NOT NULL);\
             CREATE TABLE group_doc_mount_links (id TEXT PRIMARY KEY, mountPointId TEXT NOT NULL);\
             INSERT INTO doc_mount_points VALUES ('mp','Doomed'),('other','Keeper');\
             INSERT INTO doc_mount_files VALUES ('f1','aa'),('shared','bb');\
             INSERT INTO doc_mount_documents VALUES ('d1','f1'),('dshared','shared');\
             INSERT INTO doc_mount_blobs VALUES ('b1','f1');\
             INSERT INTO doc_mount_file_links VALUES \
               ('l1','f1','mp',NULL),('l2','shared','mp','g'),('l3','shared','other','g');\
             INSERT INTO doc_mount_folders VALUES ('fo1','mp');\
             INSERT INTO doc_mount_chunks VALUES ('c1','mp');\
             INSERT INTO group_doc_mount_links VALUES ('g1','mp');",
        )
        .unwrap();
        if with_project_links {
            conn.execute_batch(
                "CREATE TABLE project_doc_mount_links (id TEXT PRIMARY KEY, mountPointId TEXT NOT NULL);\
                 INSERT INTO project_doc_mount_links VALUES ('p1','mp');",
            )
            .unwrap();
        }
        conn
    }

    /// P4.31 — the half of the cascade no fixture built by v4 can prove.
    ///
    /// `doc_mount_blobs` is the one mount table whose DDL carries
    /// `fileId … ON DELETE CASCADE`, and both apps enable `foreign_keys = ON` on
    /// their writers, so `store_delete_equivalence` sees the blob die on both
    /// sides however the delete is written — v4's dead blobs step included.
    /// Here the foreign key is absent and the pragma off, and the blob (and the
    /// document, which has no FK on any vintage) is collected anyway. Replace
    /// the `gc_orphaned_file_row` call with v4's raw file delete and this goes
    /// red where the differential stays green.
    #[test]
    fn cascade_collects_blobs_and_documents_without_any_foreign_key() {
        let conn = fk_free_mount_index(true);
        cascade_delete(&conn, "mp").unwrap();

        let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_blobs"),
            0,
            "blob collected"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_documents WHERE id='d1'"),
            0,
            "document"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_files WHERE id='f1'"),
            0,
            "file"
        );
        assert_eq!(count("SELECT COUNT(*) FROM doc_mount_chunks"), 0, "chunks");
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_folders"),
            0,
            "folders"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM project_doc_mount_links"),
            0,
            "project links"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM group_doc_mount_links"),
            0,
            "group links"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_points WHERE id='mp'"),
            0,
            "the store"
        );

        // The hard-linked survivor: `other` still links it, so neither the file
        // nor its document may be collected.
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_files WHERE id='shared'"),
            1
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_documents WHERE id='dshared'"),
            1
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_file_links"),
            1,
            "only l3 remains"
        );
    }

    /// P4.31 — a failure anywhere in the cascade must leave the store WHOLE
    /// rather than half-deleted (v4 runs seven independent awaited repo calls,
    /// which is how a partial cascade mints permanent orphans). With
    /// `project_doc_mount_links` absent, step 4 throws after chunks, links,
    /// content and folders have already gone; the transaction rolls all of it
    /// back. Run the cascade's statements loose on `conn` and this goes red.
    #[test]
    fn cascade_is_all_or_nothing() {
        let conn = fk_free_mount_index(false);
        assert!(cascade_delete(&conn, "mp").is_err());

        let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_chunks"),
            1,
            "chunks rolled back"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_file_links"),
            3,
            "links rolled back"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_files"),
            2,
            "content rolled back"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_documents"),
            2,
            "documents rolled back"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_folders"),
            1,
            "folders rolled back"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_points"),
            2,
            "the store survives"
        );
    }
}
