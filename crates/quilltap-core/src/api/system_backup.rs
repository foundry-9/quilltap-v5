//! The Backup & Restore dispatch surface (P4.9G5).
//!
//! Landed: `systemBackupCreate` — v4 `POST /api/v1/system/backup`
//! (`app/api/v1/system/backup/route.ts:21`): run `createBackup`, mint a backup
//! id, register the zip in the host's single-use temp store, and answer
//! `{success, backupId, manifest}`. The DOWNLOAD is a web-edge-only leg (`GET
//! /api/v1/system/backup/{id}`), because it streams bytes rather than JSON —
//! `quilltap-web::backup_routes`.
//!
//! Also landed: `systemRestorePreview` / `systemRestoreExecute` — v4 `POST
//! /api/v1/system/restore` (`system/restore/route.ts:145` / `:186`) over the
//! host's pending-upload store. The UPLOAD that fills that store is a
//! web-edge-only leg (octet-stream body → temp zip), like the download.

use serde_json::{Map, Value};

use crate::db::runtime::Db;
use crate::services::backup::restore::{preview_restore, restore, RestoreMode};
use crate::services::backup::{create_backup, BackupHost};

use super::types::{ErrorKind, Response};

/// v4 `POST /api/v1/system/backup`. v4 answers **201** with
/// `{success:true, backupId, manifest}`; the REST edge sets the status, the
/// dispatch envelope carries the body.
///
/// v4 catches every failure and answers its generic `serverError('Failed to
/// create backup')` rather than leaking the cause; the port keeps that wording
/// and puts the detail nowhere the client can see it (matching v4's shape) —
/// but the cause is logged at the swallow site (the P4.18 arm-(a) pattern), so
/// a failing backup is diagnosable from the server log instead of being a bare
/// 500. Found at the round's unification: the first live walk of this verb hit
/// exactly that wall, with nothing anywhere saying why.
pub fn backup_create(db: &Db, host: &dyn BackupHost, compact: bool) -> Response {
    let created_at = iso_from_millis(host.now_ms());
    let created = match create_backup(
        db,
        host,
        super::engine::SINGLE_USER_ID,
        &created_at,
        compact,
    ) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "createBackup failed");
            return Response::error(ErrorKind::Internal, "Failed to create backup");
        }
    };

    let backup_id = uuid::Uuid::new_v4().to_string();
    host.store_backup(&backup_id, &created.zip_path);

    let mut body = Map::new();
    body.insert("success".into(), Value::Bool(true));
    body.insert("backupId".into(), Value::String(backup_id));
    body.insert("manifest".into(), created.manifest);
    // ## ⚠ DELIBERATE DIVERGENCE (dogfood #59, 2026-08-03) — say what was left out
    //
    // v4's body is `{success, backupId, manifest}` and nothing else: a file whose
    // bytes could not be read is warned to the module logger and dropped, so the
    // operator is told at RESTORE time, possibly months later, as
    // `File not found in backup: <name>`. That is what the 2026-08-03 walk hit —
    // 19 of them, from `files` rows whose legacy disk keys outlived their bytes.
    //
    // Under the standing 2026-08-03 backup/restore ruling the names come back
    // with the backup. The key is OMITTED when nothing was skipped, so the happy
    // path's body stays byte-identical to v4's and the divergence exists only on
    // an instance that actually has the problem.
    //
    // Blind spot, stated: no differential covers this route's BODY (the backup
    // family diffs the archive tree), so `system_backup_equivalence`'s two-armed assertion on
    // `StageReport::skipped_files` is what holds the shape it is built from.
    if !created.staged.skipped_files.is_empty() {
        body.insert(
            "skippedFiles".into(),
            Value::Array(
                created
                    .staged
                    .skipped_files
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
    Response::System(Value::Object(body))
}

/// v4's `UUID_REGEX` (`system/restore/route.ts:35`) — the shape check
/// `getPendingUpload` runs before it even looks in the map, so a non-UUID
/// `uploadId` answers "Upload not found or expired" rather than a 404.
fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    b.iter().enumerate().all(|(i, c)| match i {
        8 | 13 | 18 | 23 => *c == b'-',
        _ => c.is_ascii_hexdigit(),
    })
}

/// v4 `POST /api/v1/system/restore?action=preview` (`route.ts:145`).
///
/// `{uploadId}` → `{success:true, preview}`. Both failure arms are v4's,
/// verbatim: an unknown/expired/ill-shaped id is `badRequest('Upload not found
/// or expired')`, and a parse failure is `serverError(error.message)` — this
/// handler **leaks the message**, unlike backup create, because the malformed-
/// archive wording is what tells the user their file is not a backup.
pub fn restore_preview(host: &dyn BackupHost, upload_id: &str) -> Response {
    if !is_uuid(upload_id) {
        return Response::error(ErrorKind::BadRequest, "Upload not found or expired");
    }
    let Some(zip_path) = host.get_upload(upload_id) else {
        return Response::error(ErrorKind::BadRequest, "Upload not found or expired");
    };
    match preview_restore(&zip_path, &host.temp_dir()) {
        Ok(preview) => {
            let mut body = Map::new();
            body.insert("success".into(), Value::Bool(true));
            body.insert(
                "preview".into(),
                serde_json::to_value(preview).unwrap_or(Value::Null),
            );
            Response::System(Value::Object(body))
        }
        Err(e) => {
            tracing::error!(error = %e, "Error generating restore preview");
            Response::error(ErrorKind::Internal, &e)
        }
    }
}

/// v4 `POST /api/v1/system/restore` with no action (`route.ts:186`).
///
/// `{uploadId, mode}` → `{success:true, summary}`. v4's three `badRequest`s are
/// reproduced (including the mode guard, which the REST edge also applies before
/// it ever reaches here), and — v4 has **no `catch`** around the restore itself,
/// only a `finally` that removes the pending upload, so a throw propagates to
/// the generic handler wrapper. The `finally` moves HERE rather than staying at
/// the route, because the SPA reaches this verb through dispatch, not the REST
/// edge: leaving the removal at the edge would leak every upload the app's own
/// client makes.
pub async fn restore_execute(
    db: &Db,
    host: &dyn BackupHost,
    upload_id: &str,
    mode: &str,
) -> Response {
    let Some(mode) = RestoreMode::parse(mode) else {
        return Response::error(
            ErrorKind::BadRequest,
            "mode must be \"replace\" or \"new-account\"",
        );
    };
    if !is_uuid(upload_id) {
        return Response::error(ErrorKind::BadRequest, "Upload not found or expired");
    }
    let Some(zip_path) = host.get_upload(upload_id) else {
        return Response::error(ErrorKind::BadRequest, "Upload not found or expired");
    };

    let result = restore(db, host, &zip_path, mode, super::engine::SINGLE_USER_ID).await;
    host.remove_upload(upload_id); // v4's `finally removePendingUpload` (`:238`).

    match result {
        Ok(summary) => {
            let mut body = Map::new();
            body.insert("success".into(), Value::Bool(true));
            body.insert(
                "summary".into(),
                serde_json::to_value(summary).unwrap_or(Value::Null),
            );
            Response::System(Value::Object(body))
        }
        Err(e) => {
            tracing::error!(error = %e, "Restore failed");
            Response::error(ErrorKind::Internal, &e)
        }
    }
}

/// `new Date(ms).toISOString()` — the manifest's `createdAt` and (with `[:.]`
/// replaced) the staging folder name. Written out rather than pulled from a
/// date crate so the format is pinned to v4's exactly:
/// `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn iso_from_millis(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// Howard Hinnant's `civil_from_days` — days since 1970-01-01 → (y, m, d).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::iso_from_millis;

    #[test]
    fn iso_matches_js_to_iso_string() {
        assert_eq!(iso_from_millis(0), "1970-01-01T00:00:00.000Z");
        // `new Date('2026-07-24T18:14:39.863Z').getTime()` === 1784916879863
        assert_eq!(
            iso_from_millis(1_784_916_879_863),
            "2026-07-24T18:14:39.863Z"
        );
        // A leap day.
        assert_eq!(
            iso_from_millis(1_709_164_800_000),
            "2024-02-29T00:00:00.000Z"
        );
    }
}
