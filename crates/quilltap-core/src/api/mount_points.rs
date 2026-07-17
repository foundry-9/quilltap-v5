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

/// v4 `DELETE /api/v1/mount-points/[id]` — 404 / the ordered cascade (detach seam
/// → chunks → files [+ orphaned-file GC via the link snapshot] → documents →
/// blobs → folders → per-link project-link delete → the point) → `{message}`.
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

/// The exact v4 route cascade (the detach watcher is a fire-and-forget host seam,
/// omitted). Each repo's `deleteByMountPointId` is replicated by its SQL: chunks +
/// folders + project-links are plain `WHERE mountPointId` deletes; `files`
/// snapshots the mount's file ids, drops the link rows, then GCs orphaned files;
/// `documents`/`blobs` GC keys off the (now-empty) link table, exactly as v4.
fn cascade_delete(conn: &Connection, mount_point_id: &str) -> Result<(), DbError> {
    // 1. chunks.
    conn.execute(
        "DELETE FROM doc_mount_chunks WHERE mountPointId = ?1",
        rusqlite::params![mount_point_id],
    )?;
    // 2. files.deleteByMountPointId — snapshot file ids, drop links, GC orphans.
    let affected: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT DISTINCT fileId FROM doc_mount_file_links WHERE mountPointId = ?1")?;
        let rows = stmt.query_map(rusqlite::params![mount_point_id], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    conn.execute(
        "DELETE FROM doc_mount_file_links WHERE mountPointId = ?1",
        rusqlite::params![mount_point_id],
    )?;
    for fid in &affected {
        conn.execute(
            "DELETE FROM doc_mount_files WHERE id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM doc_mount_file_links l WHERE l.fileId = ?1)",
            rusqlite::params![fid],
        )?;
    }
    // 3. documents.deleteByMountPointId — keyed off the (now-empty) link table.
    conn.execute(
        "DELETE FROM doc_mount_documents WHERE fileId IN \
         (SELECT DISTINCT fileId FROM doc_mount_file_links WHERE mountPointId = ?1)",
        rusqlite::params![mount_point_id],
    )?;
    // 4. blobs.deleteByMountPointId — snapshot keyed off the (now-empty) link table.
    let blob_affected: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT DISTINCT fileId FROM doc_mount_file_links WHERE mountPointId = ?1")?;
        let rows = stmt.query_map(rusqlite::params![mount_point_id], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for fid in &blob_affected {
        conn.execute(
            "DELETE FROM doc_mount_files WHERE id = ?1 \
             AND NOT EXISTS (SELECT 1 FROM doc_mount_file_links l WHERE l.fileId = ?1)",
            rusqlite::params![fid],
        )?;
    }
    // 5. folders (explicit — no FK cascade).
    conn.execute(
        "DELETE FROM doc_mount_folders WHERE mountPointId = ?1",
        rusqlite::params![mount_point_id],
    )?;
    // 6. project links (v4 finds then per-link deletes → net a scoped delete).
    conn.execute(
        "DELETE FROM project_doc_mount_links WHERE mountPointId = ?1",
        rusqlite::params![mount_point_id],
    )?;
    // 7. the point itself.
    conn.execute(
        "DELETE FROM doc_mount_points WHERE id = ?1",
        rusqlite::params![mount_point_id],
    )?;
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
    use super::derive_mount_capabilities;
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
}
