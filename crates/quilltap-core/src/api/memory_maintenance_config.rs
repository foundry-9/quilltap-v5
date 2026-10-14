//! The memory maintenance settings card's two verbs — P4.D288 (contract C2):
//! v4 `GET` / `POST /api/v1/memories?action=maintenance-config`
//! (`app/api/v1/memories/route.ts:864-899` at the `83334b9a5` pin), FILLED at
//! `KEYSTONE` because they need only C1-5's settings document; P4.D297 owns
//! them after (its maintenance routes module may take them over).
//!
//! - GET answers `{success: true, settings}` — the whole parsed settings.
//! - POST parses the body through v4's `maintenanceConfigSchema`
//!   ([`parse_memory_maintenance_patch`] — unknown keys such as the retired
//!   merge knobs stripped; a Zod failure is v4's `validationError` 400 with the
//!   issues as `details`), merges a sent `phases` INTO the stored toggles
//!   ("Per-phase toggles merge into the stored ones, so a card can send one"),
//!   lets every other sent key REPLACE its stored value (a sent
//!   `perCharacterCapOverrides` replaces the whole map) through
//!   [`set_memory_maintenance_settings`], logs v4's INFO `[Memories API]
//!   Memory maintenance settings updated {userId, enabled, perCharacterCap,
//!   capOverrides, phases}` (`phases` an object → `phasesJson`, the file-layer
//!   convention; the line's bytes are P4.D297's capture pin, §R.5), and answers
//!   `{success: true, settings}`.

use serde_json::{json, Value};

use crate::api::types::{ErrorKind, Response};
use crate::api::zod_issues::zod_issue_details;
use crate::db::instance_settings::{
    get_memory_maintenance_settings, parse_memory_maintenance_patch,
    set_memory_maintenance_settings,
};
use crate::db::runtime::Db;

/// v4 GET `?action=maintenance-config`.
pub fn memory_maintenance_config_get(db: &Db) -> Response {
    match db.read_main(|c| Ok(get_memory_maintenance_settings(c))) {
        Ok(settings) => {
            Response::Memory(json!({ "success": true, "settings": settings.to_json() }))
        }
        Err(e) => Response::error(ErrorKind::Internal, e.to_string()),
    }
}

/// v4 POST `?action=maintenance-config`.
pub async fn memory_maintenance_config_set(db: &Db, user_id: &str, config: Value) -> Response {
    let mut patch = match parse_memory_maintenance_patch(&config) {
        Ok(p) => p,
        Err(issues) => return Response::validation_error(zod_issue_details(&issues)),
    };
    let phases = patch.shift_remove("phases");
    let written = db
        .write(move |w| {
            let conn = w.main().connection();
            let mut value = patch;
            if let Some(Value::Object(sent)) = phases {
                // `{ ...current.phases, ...phases }`.
                let mut merged = match get_memory_maintenance_settings(conn).phases.to_json() {
                    Value::Object(m) => m,
                    _ => serde_json::Map::new(),
                };
                for (k, v) in sent {
                    merged.insert(k, v);
                }
                value.insert("phases".into(), Value::Object(merged));
            }
            set_memory_maintenance_settings(conn, &Value::Object(value))
        })
        .await;
    let settings = match written {
        Ok(s) => s,
        Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
    };
    tracing::info!(
        userId = %user_id,
        enabled = settings.enabled,
        perCharacterCap = settings.per_character_cap,
        capOverrides = settings.per_character_cap_overrides.len(),
        phasesJson = %settings.phases.to_json(),
        "[Memories API] Memory maintenance settings updated"
    );
    Response::Memory(json!({ "success": true, "settings": settings.to_json() }))
}
