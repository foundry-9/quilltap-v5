//! The Almanack dispatch surface (P4.37) — v4's four `capabilities-report-*`
//! actions on `app/api/v1/system/tools/route.ts`.
//!
//! The action strings keep v4's `capabilities-report-*` names verbatim at the
//! web edge: v4 froze them deliberately when the feature was renamed, because
//! "renaming them would break bookmarks to buy nothing". The dispatch verbs use
//! the feature's real name.
//!
//! ## Where phase 7 lives
//!
//! v4's `generateAndSaveAlmanack` takes a `persist` callback so the route can
//! create the `files` row. v5's read collectors are synchronous pooled reads
//! while both writes (the Uploads-mount blob and the `files` row) must go
//! through the writer task, so the split falls here instead: the core gathers
//! and renders, and [`almanack_generate`] does phase 7 — announce, filename,
//! mount write, files row — and returns the FILES-ROW id as `reportId`.
//!
//! That id is load-bearing and is v4's own 404 fix: list/get/delete all key off
//! the files row, and the pre-4.9 code minted a throwaway UUID here, so the
//! dialog opened straight after generating handed the download link an id
//! nothing could resolve.
//!
//! ## The progress side-channel
//!
//! No `capabilities-report-progress` action is ported. v5 serves ONE global
//! `/api/events` stream, so the run's `phase` frames ride it scope-tagged by
//! `progressId` — the standing D3-family divergence
//! ([`crate::services::creation_progress`]). The POST's own JSON body remains
//! authoritative for the result.

use serde_json::{json, Map, Value};

use crate::almanack::{
    announce_binding, generate_almanack_data, render_almanack_markdown, report_filename,
    AlmanackContext,
};
use crate::db::files::{CreateOptions, FileCreate, FileEntry, FilesRepository};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::creation_progress::CreationProgressEmitter;
use crate::services::file_storage::{
    delete_file, download_file, write_user_upload_to_mount_store, NotConfiguredPixelCodec,
    StorageBackend,
};

use super::types::{ErrorKind, Response};

/// Everything the report needs from the host process (P4.0: the core discovers
/// no paths, no clock and no runtime facts of its own). `None` on
/// [`super::engine::EngineAssembly`] → the four arms answer the loud
/// not-assembled refusal. Wired LIVE by `quilltap-host`'s
/// `HostAlmanackServices` (P4.37 resume item 3) — the production spine always
/// supplies it.
pub trait AlmanackHost: Send + Sync {
    /// The disk storage backend (`<base>/files`), for the download/delete legs.
    /// A report's own bytes live in the Uploads mount, so this only matters for
    /// a legacy disk-keyed row.
    fn storage(&self) -> std::sync::Arc<dyn StorageBackend>;
    /// The three database files, the data dir and the backups dir
    /// (v4 `lib/paths`).
    fn paths(&self) -> crate::almanack::AlmanackPaths;
    /// v4's `process`/`os` block — see the phase-1 divergence note.
    fn runtime_facts(&self) -> crate::almanack::RuntimeFacts;
    /// v4 `getHasUserPassphrase()`.
    fn passphrase_protected(&self) -> bool;
    /// v4 `packageJson.version`.
    fn app_version(&self) -> String;
    /// v4 `process.env.NODE_ENV || 'development'`.
    fn node_env(&self) -> String;
    /// Wall clock, injected so the differential can freeze `generatedAt`.
    fn now_ms(&self) -> i64;
}

/// v4's route arms answer FIXED sentences (`serverError('Failed to …')`) and
/// log the detail server-side (`route.ts` catch blocks) — leaking `e` into the
/// body was the P4.D29 wording-drift class (§3 review, 2026-08-06).
fn internal(action_sentence: &str, e: impl std::fmt::Display) -> Response {
    tracing::error!("[Almanack] {action_sentence}: {e}");
    Response::error(ErrorKind::Internal, action_sentence)
}

/// v4 `JSON.stringify` of an integral JS number has no fraction digits —
/// `"size":46061`, never `46061.0` (the P4.6an "the `1` not `1.0`" rule). The
/// row's column reads as REAL, so the value arrives as `f64`.
fn js_size_number(size: f64) -> Value {
    if size.is_finite() && size.fract() == 0.0 {
        json!(size as i64)
    } else {
        json!(size)
    }
}

/// One listed report (v4's `{id, filename, storageKey, createdAt, size}`).
struct ReportRow {
    id: String,
    filename: String,
    storage_key: String,
    created_at: String,
    size: f64,
}

/// v4's `files.findByCategory('DOCUMENT')` filtered to `folderPath === '/reports'`.
/// The category + folder pair is the whole identity of a report row.
fn read_report_rows(db: &Db, user_id: &str) -> Result<Vec<ReportRow>, DbError> {
    db.read_main(|conn| {
        let mut stmt = conn.prepare(
            r#"SELECT "id", "originalFilename", "storageKey", "createdAt", "size"
               FROM "files"
               WHERE "userId" = ?1 AND "category" = 'DOCUMENT' AND "folderPath" = '/reports'"#,
        )?;
        let rows = stmt.query_map([user_id], |r| {
            Ok(ReportRow {
                id: r.get::<_, String>(0)?,
                filename: r.get::<_, String>(1)?,
                // v4 `file.storageKey || ''`.
                storage_key: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                created_at: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                // v4 `file.size || 0`.
                size: r.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    })
}

/// v4 `?action=capabilities-report-list` — newest first.
pub fn almanack_list(db: &Db, user_id: &str) -> Response {
    let mut rows = match read_report_rows(db, user_id) {
        Ok(r) => r,
        Err(e) => return internal("Failed to list reports", e),
    };
    // v4 sorts by `new Date(createdAt).getTime()` descending. An unparseable or
    // absent stamp becomes NaN there, which `sort` treats as "no opinion"; here
    // it sorts as 0, i.e. oldest — the only reachable difference is the order of
    // rows whose `createdAt` is corrupt, which the writer never produces.
    rows.sort_by(|a, b| {
        let ka = crate::clock::iso_to_ms(&a.created_at).unwrap_or(0);
        let kb = crate::clock::iso_to_ms(&b.created_at).unwrap_or(0);
        kb.cmp(&ka)
    });
    let reports: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mut m = Map::new();
            m.insert("id".into(), Value::String(r.id.clone()));
            m.insert("filename".into(), Value::String(r.filename.clone()));
            m.insert("storageKey".into(), Value::String(r.storage_key.clone()));
            m.insert("createdAt".into(), Value::String(r.created_at.clone()));
            m.insert("size".into(), js_size_number(r.size));
            Value::Object(m)
        })
        .collect();
    Response::System(json!({ "reports": reports }))
}

/// The `files` row for a report id, or `None` when it is not a report of this
/// user's (v4 finds within the DOCUMENT/`/reports` set, so a real file that is
/// not a report is a 404 here — deliberately).
fn find_report_entry(
    db: &Db,
    user_id: &str,
    report_id: &str,
) -> Result<Option<FileEntry>, DbError> {
    let rows = read_report_rows(db, user_id)?;
    if !rows.iter().any(|r| r.id == report_id) {
        return Ok(None);
    }
    db.read_main(|conn| FilesRepository::new(conn).find_by_id(report_id))
}

/// The bytes + filename of a stored report — shared by the JSON `get` and the
/// web edge's `download=true` leg.
pub fn almanack_get_bytes(
    db: &Db,
    backend: &dyn StorageBackend,
    user_id: &str,
    report_id: &str,
) -> Result<Option<(String, Vec<u8>)>, Response> {
    let entry = match find_report_entry(db, user_id, report_id) {
        Ok(e) => e,
        Err(e) => return Err(internal("Failed to get report", e)),
    };
    let Some(entry) = entry else {
        return Ok(None);
    };
    match download_file(db, backend, &entry) {
        Ok(bytes) => Ok(Some((entry.original_filename.clone(), bytes))),
        Err(e) => Err(internal("Failed to get report", e)),
    }
}

/// v4 `?action=capabilities-report-get&reportId=…` (the non-download leg).
pub fn almanack_get(
    db: &Db,
    backend: &dyn StorageBackend,
    user_id: &str,
    report_id: &str,
) -> Response {
    match almanack_get_bytes(db, backend, user_id, report_id) {
        Err(r) => r,
        Ok(None) => Response::error(ErrorKind::NotFound, "Report not found"),
        Ok(Some((filename, bytes))) => {
            // v4 `buffer.toString('utf-8')` — lossy on invalid UTF-8, never a
            // throw. The writer only ever stores rendered markdown.
            let content = String::from_utf8_lossy(&bytes).into_owned();
            Response::System(json!({
                "reportId": report_id,
                "filename": filename,
                "content": content,
                "size": bytes.len(),
            }))
        }
    }
}

/// v4 `?action=capabilities-report-delete` — storage delete then the files row.
pub async fn almanack_delete(
    db: &Db,
    backend: &dyn StorageBackend,
    user_id: &str,
    report_id: &str,
) -> Response {
    let entry = match find_report_entry(db, user_id, report_id) {
        Ok(e) => e,
        Err(e) => return internal("Failed to delete report", e),
    };
    let Some(entry) = entry else {
        return Response::error(ErrorKind::NotFound, "Report not found");
    };
    if let Err(e) = delete_file(db, backend, &entry).await {
        return internal("Failed to delete report", e);
    }
    let id = entry.id.clone();
    match db
        .write(move |writers| FilesRepository::new(writers.main().connection()).delete(&id))
        .await
    {
        Ok(_) => Response::System(json!({ "success": true })),
        Err(e) => internal("Failed to delete report", e),
    }
}

/// v4 `?action=capabilities-report-generate` + `generateAndSaveAlmanack`.
///
/// Blocking: the JSON body carries the whole rendered report, exactly as v4's
/// does. `progress` narrates the seven phases on the global event stream and
/// takes v4's `finish()` / `fail(message)` terminal frames.
pub async fn almanack_generate(
    ctx: &AlmanackContext<'_>,
    progress: &CreationProgressEmitter,
) -> Response {
    let data = generate_almanack_data(ctx, progress);

    // --- Phase 7: Binding the volume ---------------------------------------
    announce_binding(progress);
    let markdown = render_almanack_markdown(&data);
    let filename = report_filename(ctx.now_ms);
    let bytes = markdown.clone().into_bytes();
    let size = bytes.len() as f64;
    let sha256 = sha256_hex(&bytes);

    // Always non-project; routed into the Quilltap Uploads mount under
    // `diagnostics/` rather than the catch-all `_general/`.
    let db = ctx.db;
    let upload = {
        let filename = filename.clone();
        let bytes = bytes.clone();
        // Both halves are mount-index writes, so they run on the writer thread.
        // The codec is irrelevant here — `text/markdown` is not a convertible
        // image type, so `store_mount_blob`'s transcode step never reaches it.
        let stored = db
            .write(move |writers| {
                let Some(mount) = writers.mount_index() else {
                    return Err(DbError::Internal(
                        "Quilltap Uploads mount has not been provisioned".to_string(),
                    ));
                };
                let mount_conn = mount.connection();
                // v4's `writeUserUploadToMountStore` takes no description —
                // the link row keeps its empty default; the files-row
                // description below is the route's own (v4 route :983-1002).
                write_user_upload_to_mount_store(
                    writers.main().connection(),
                    mount_conn,
                    &NotConfiguredPixelCodec,
                    &filename,
                    &bytes,
                    "text/markdown",
                    "diagnostics",
                    None,
                )
            })
            .await;
        match stored {
            Ok(stored) => stored,
            Err(e) => {
                progress.fail(e.to_string());
                return internal("Failed to generate report", e);
            }
        }
    };

    let storage_key = upload.storage_key();
    let report_id = uuid::Uuid::new_v4().to_string();
    let create = FileCreate {
        user_id: ctx.user_id.to_string(),
        sha256,
        original_filename: filename.clone(),
        mime_type: "text/markdown".to_string(),
        size,
        width: None,
        height: None,
        is_plain_text: None,
        linked_to: Vec::new(),
        source: "SYSTEM".to_string(),
        category: "DOCUMENT".to_string(),
        generation_prompt: None,
        generation_model: None,
        generation_revised_prompt: None,
        description: Some("The Almanack — system report".to_string()),
        tags: Vec::new(),
        project_id: None,
        folder_path: Some("/reports".to_string()),
        storage_key: Some(storage_key.clone()),
        file_status: "ok".to_string(),
    };
    let stamp = crate::clock::iso_from_unix_ms(ctx.now_ms);
    let opts = CreateOptions {
        id: report_id.clone(),
        created_at: stamp.clone(),
        updated_at: stamp,
    };
    if let Err(e) = db
        .write(move |writers| {
            FilesRepository::new(writers.main().connection()).create(&create, &opts)
        })
        .await
    {
        progress.fail(e.to_string());
        return internal("Failed to generate report", e);
    }

    progress.finish();
    Response::System(json!({
        "success": true,
        "reportId": report_id,
        "filename": filename,
        "storageKey": storage_key,
        "size": js_size_number(size),
        "content": markdown,
    }))
}

/// v4's `createHash('sha256').update(content, 'utf-8').digest('hex')`.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
