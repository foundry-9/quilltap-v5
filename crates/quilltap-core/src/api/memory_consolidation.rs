//! The Memories API's consolidation-settings and extraction-MODE actions — v4
//! `app/api/v1/memories/route.ts:974-1020` at the `83334b9a5` pin: GET/POST
//! `?action=consolidation-config`, GET/POST `?action=extraction-mode-config`.
//! (The `consolidate` verb is RETIRED — `b045dd986`; `maintain` covers it,
//! [`super::memory_maintenance_routes`].)
//!
//! **Bug 244 (`b045dd986`, `patchSchemaOf`).** v4's write-side schema strips
//! every `.default()` before making each key optional, "so a partial write
//! never clobbers stored values": the POST keeps only the keys it was SENT
//! (C1-5's [`parse_memory_consolidation_patch`] /
//! [`parse_memory_extraction_mode_patch`] — present keys only, schema order,
//! bounds / enums / nullability kept, unknown keys stripped), the setter merges
//! them over the stored settings, and the INFO's `fields` names only the sent
//! keys. This retired P4.D271 R-A's faithful clobber and its ruled
//! `PARTIAL_DEFAULTS_CLOBBER` table: v4 converged (measured by P4.D297 at the
//! pin — `PATCH_PRESERVES_STORED` in `memory_consolidation_routes_equivalence`
//! now pins the preserved values both ways).
//!
//! **Validation (R-K):** a Zod failure answers v5's house class for the
//! memories config surface — 400 `Validation error`, message only.

use serde_json::{json, Value};

use crate::api::types::{ErrorKind, Response};
use crate::db::instance_settings::{
    get_memory_consolidation_settings, get_memory_extraction_mode_settings,
    parse_memory_consolidation_patch, parse_memory_extraction_mode_patch,
    set_memory_consolidation_settings, set_memory_extraction_mode_settings,
};
use crate::db::runtime::Db;

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}

fn validation_error() -> Response {
    Response::error(ErrorKind::BadRequest, "Validation error")
}

/// v4 `Object.keys(patch)` as the INFO's `fieldsJson`.
fn fields_json(patch: &serde_json::Map<String, Value>) -> String {
    serde_json::to_string(&patch.keys().collect::<Vec<_>>()).unwrap_or_default()
}

/// v4 GET `?action=consolidation-config` — `{success, settings}`.
pub fn memory_consolidation_config_get(db: &Db) -> Response {
    match db.read_main(|c| Ok(get_memory_consolidation_settings(c))) {
        Ok(settings) => {
            Response::Memory(json!({ "success": true, "settings": settings.to_json() }))
        }
        Err(e) => internal(e),
    }
}

/// v4 POST `?action=consolidation-config` (bug 244): parse the patch, merge
/// the sent keys, re-read, INFO `[Memories API] Consolidation settings updated
/// (instance-wide) {enabled, fields}`.
pub async fn memory_consolidation_config_set(db: &Db, config: Value) -> Response {
    let patch = match parse_memory_consolidation_patch(&config) {
        Ok(p) => p,
        Err(_) => return validation_error(),
    };
    let fields = fields_json(&patch);
    let value = Value::Object(patch);
    let wrote = db
        .write(move |w| set_memory_consolidation_settings(w.main().connection(), &value))
        .await;
    if let Err(e) = wrote {
        return internal(e);
    }
    let settings = match db.read_main(|c| Ok(get_memory_consolidation_settings(c))) {
        Ok(s) => s,
        Err(e) => return internal(e),
    };
    tracing::info!(
        enabled = settings.enabled,
        fieldsJson = %fields,
        "[Memories API] Consolidation settings updated (instance-wide)"
    );
    Response::Memory(json!({ "success": true, "settings": settings.to_json() }))
}

/// v4 GET `?action=extraction-mode-config` — `{success, settings}`.
pub fn memory_extraction_mode_config_get(db: &Db) -> Response {
    match db.read_main(|c| Ok(get_memory_extraction_mode_settings(c))) {
        Ok(settings) => {
            Response::Memory(json!({ "success": true, "settings": settings.to_json() }))
        }
        Err(e) => internal(e),
    }
}

/// v4 POST `?action=extraction-mode-config` (bug 244): as the consolidation
/// write; INFO `[Memories API] Extraction-mode settings updated
/// (instance-wide) {otherPass, fields}`.
pub async fn memory_extraction_mode_config_set(db: &Db, config: Value) -> Response {
    let patch = match parse_memory_extraction_mode_patch(&config) {
        Ok(p) => p,
        Err(_) => return validation_error(),
    };
    let fields = fields_json(&patch);
    let value = Value::Object(patch);
    let wrote = db
        .write(move |w| set_memory_extraction_mode_settings(w.main().connection(), &value))
        .await;
    if let Err(e) = wrote {
        return internal(e);
    }
    let settings = match db.read_main(|c| Ok(get_memory_extraction_mode_settings(c))) {
        Ok(s) => s,
        Err(e) => return internal(e),
    };
    tracing::info!(
        otherPass = %settings.other_pass,
        fieldsJson = %fields,
        "[Memories API] Extraction-mode settings updated (instance-wide)"
    );
    Response::Memory(json!({ "success": true, "settings": settings.to_json() }))
}
