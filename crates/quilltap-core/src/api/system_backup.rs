//! The Backup & Restore dispatch surface (P4.9G5).
//!
//! Landed: `systemBackupCreate` — v4 `POST /api/v1/system/backup`
//! (`app/api/v1/system/backup/route.ts:21`): run `createBackup`, mint a backup
//! id, register the zip in the host's single-use temp store, and answer
//! `{success, backupId, manifest}`. The DOWNLOAD is a web-edge-only leg (`GET
//! /api/v1/system/backup/{id}`), because it streams bytes rather than JSON —
//! `quilltap-web::backup_routes`.
//!
//! NOT landed (they keep the loud "recognized but not yet available" refusal in
//! `engine.rs`): `systemRestorePreview` / `systemRestoreExecute` and the
//! octet-stream upload leg. The order's status header carries the resume list.

use serde_json::{Map, Value};

use crate::db::runtime::Db;
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
pub fn backup_create(db: &Db, host: &dyn BackupHost) -> Response {
    let created_at = iso_from_millis(host.now_ms());
    let created = match create_backup(db, host, super::engine::SINGLE_USER_ID, &created_at) {
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
    Response::System(Value::Object(body))
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
