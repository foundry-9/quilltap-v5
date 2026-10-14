//! The Memories API's consolidation actions — v4
//! `app/api/v1/memories/route.ts` (`d58548051`, byte-identical at the
//! `01a83539d` pin): GET/POST `?action=consolidation-config`, GET/POST
//! `?action=extraction-mode-config`, POST `?action=consolidate`. They fill the
//! five C2 wire arms P4.D265 landed with refusal bodies.
//!
//! **The settings POSTs port v4's clobber FAITHFULLY (P4.D271 R-A, the named
//! both-ways table `PARTIAL_DEFAULTS_CLOBBER`).** v4 parses with
//! `<Schema>.partial()` and "drops undefined keys so a partial write never
//! clobbers stored values" — but under Zod 4, `.partial()` over `.default()`
//! fields still fills the default for an ABSENT key, so nothing is ever
//! `undefined`: every POST rewrites EVERY field, and a tuned stored value is
//! lost to its default. v5 reproduces it — the patch is the FULL default-filled
//! parse (measured equal to v4's `.partial()` parse on every corpus input),
//! merged `{...current, ...patch}` (= the patch), validated, written, re-read;
//! the INFO's `fields` always lists every key in schema order. A v4 fix turns
//! the table's second half red — the convergence signal.
//!
//! **Validation (R-K):** a Zod failure answers v5's house class for the
//! memories config surface — 400 `Validation error`, message only.
//!
//! **The dry run (R-E):** in-process, through core's process-global
//! [`consolidation_runner`] the host installs. An engine with no runner
//! installed answers a loud typed refusal AFTER v4's validation and 404 arms.

use serde_json::{json, Value};

use crate::api::types::{ErrorKind, Response};
use crate::db::instance_settings::{
    get_memory_consolidation_settings, get_memory_extraction_mode_settings,
    parse_memory_consolidation_settings, parse_memory_extraction_mode_settings,
    set_memory_consolidation_settings, set_memory_extraction_mode_settings,
};
use crate::db::runtime::Db;

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}

fn validation_error() -> Response {
    Response::error(ErrorKind::BadRequest, "Validation error")
}

fn fields_json(settings_json: &Value) -> String {
    let keys: Vec<&String> = settings_json
        .as_object()
        .map(|o| o.keys().collect())
        .unwrap_or_default();
    serde_json::to_string(&keys).unwrap_or_default()
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

/// v4 POST `?action=consolidation-config` (R-A: the faithful clobber).
pub async fn memory_consolidation_config_set(db: &Db, config: Value) -> Response {
    // `.partial()` over `.default()` fields = the full default-filled parse.
    let parsed = match parse_memory_consolidation_settings(&config) {
        Ok(p) => p,
        Err(_) => return validation_error(),
    };
    let patch = parsed.to_json();
    let fields = fields_json(&patch);
    let wrote = db
        .write(move |w| set_memory_consolidation_settings(w.main().connection(), &patch))
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

/// v4 POST `?action=extraction-mode-config` (R-A: the faithful clobber).
pub async fn memory_extraction_mode_config_set(db: &Db, config: Value) -> Response {
    let parsed = match parse_memory_extraction_mode_settings(&config) {
        Ok(p) => p,
        Err(_) => return validation_error(),
    };
    let patch = parsed.to_json();
    let fields = fields_json(&patch);
    let wrote = db
        .write(move |w| set_memory_extraction_mode_settings(w.main().connection(), &patch))
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
