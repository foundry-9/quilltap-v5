//! The `.qtap` export schema validator (P4.86 — v4
//! `lib/validation/qtap-schema-validator.ts`).
//!
//! v4 validates an assembled export against `public/schemas/qtap-export.
//! schema.json` with ajv:
//!
//! ```text
//! new Ajv2020({ allErrors: true, strict: false, validateFormats: true })
//! addFormats(ajv)
//! ```
//!
//! and renders each failure as `` `${err.instancePath || '/'}: ${err.message
//! || 'unknown error'}` ``. Any throw inside `validateQtapExport` becomes
//! `{valid: false, errors: ['Schema validation error: <msg>']}` plus an
//! `error` log line.
//!
//! # The schema is a VENDORED v5 artifact
//!
//! `qtap-export.schema.json` beside this file is a byte-copy of v4's
//! `public/schemas/qtap-export.schema.json` at the oracle baseline
//! (89,769 bytes, `$schema` draft 2020-12), embedded with `include_str!` —
//! the `help/**` arrangement. **A v4 commit that touches that file is a
//! re-vendor obligation**, and `qtap_schema_embed_guard` (harness) fires the
//! moment the two diverge. Its 37 distinct `$ref`s are ALL local
//! (`#/$defs/…`), so the engine never resolves anything remotely.
//!
//! # Engine parity — what agrees and what is RECORDED
//!
//! The engine is the `jsonschema` crate, not ajv, so the two agree on the
//! decision and disagree on the prose:
//!
//! * `valid` — **exact**. Both implement draft 2020-12 over the same schema.
//! * the SET of sections derived from the error paths (v4
//!   `ai-import.service.ts`'s `/^\/data\/(\w+)\//`, the input to the repair
//!   pass) — **exact**, and the thing the port actually depends on.
//! * the error COUNT, ORDER and message TEXT — a **RECORDED DIVERGENCE by
//!   construction**: ajv reports one error per failing keyword occurrence
//!   with its own wording (`must be string`), `jsonschema` reports its own
//!   set with its own wording. The count reaches the wire in two places —
//!   the `step_error validation` frame (`N validation error(s)`) and the
//!   `errors.validation` sentence — and those two carry the divergence with
//!   it. `qtap_schema_validate_equivalence` measures both sides per corpus
//!   row and prints the table.
//!
//! `allErrors: true` is `iter_errors` (every error, not the first);
//! `strict: false` is the crate's default (unknown keywords are annotations,
//! not failures); `validateFormats: true` + `ajv-formats` is
//! `should_validate_formats(true)` — draft 2020-12 makes `format` an
//! annotation unless asserted, and this schema asserts exactly two formats,
//! `uuid` and `date-time`, both engine built-ins.

use std::sync::OnceLock;

use serde_json::Value;

/// The vendored copy of v4's `public/schemas/qtap-export.schema.json`.
pub const QTAP_EXPORT_SCHEMA_JSON: &str = include_str!("qtap-export.schema.json");

/// The tracing target the `[QtapSchemaValidator]` lines are emitted under.
pub const QTAP_SCHEMA_LOG_TARGET: &str = "quilltap::qtap_schema";

/// v4's `ValidationResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
}

/// The compiled validator, built once (v4 caches both the parsed schema and
/// the compiled ajv validator in module-level `let`s). A parse or compile
/// failure is held as the message v4's `catch` would render, so the refusal
/// is answered on every call exactly as v4's cached-throw path would be.
fn validator() -> Result<&'static jsonschema::Validator, &'static String> {
    static COMPILED: OnceLock<Result<jsonschema::Validator, String>> = OnceLock::new();
    COMPILED
        .get_or_init(|| {
            // v4's `loadSchema` throws `Failed to load qtap-export schema: …`
            // when the file cannot be read or parsed; the embedded copy can
            // only fail the PARSE half, and it takes the same sentence.
            let schema: Value = serde_json::from_str(QTAP_EXPORT_SCHEMA_JSON)
                .map_err(|e| format!("Failed to load qtap-export schema: {e}"))?;
            jsonschema::options()
                // ajv `validateFormats: true` + `addFormats(ajv)`.
                .should_validate_formats(true)
                .build(&schema)
                .map_err(|e| e.to_string())
        })
        .as_ref()
}

/// v4 `validateQtapExport(data)`.
pub fn validate_qtap_export(data: &Value) -> ValidationResult {
    let validator = match validator() {
        Ok(v) => v,
        Err(message) => {
            // v4's `catch`: the `error` log line, then the one-error result.
            tracing::error!(
                target: QTAP_SCHEMA_LOG_TARGET,
                context = %serde_json::json!({ "error": message }),
                "[QtapSchemaValidator] Validation error"
            );
            return ValidationResult {
                valid: false,
                errors: vec![format!("Schema validation error: {message}")],
            };
        }
    };
    // ajv `allErrors: true` — every failure, not the first.
    let errors: Vec<String> = validator
        .iter_errors(data)
        .map(|err| {
            // `err.instancePath || '/'` — ajv's root pointer is `''`, and so
            // is this engine's.
            let path = err.instance_path().to_string();
            let path = if path.is_empty() { "/" } else { path.as_str() };
            // ajv's `err.message || 'unknown error'`; this engine's Display
            // is never empty, so the fallback cannot render (recorded).
            format!("{path}: {err}")
        })
        .collect();
    ValidationResult {
        valid: errors.is_empty(),
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal_characters_export() -> Value {
        json!({
            "manifest": {
                "format": "quilltap-export",
                "version": "1.0",
                "exportType": "characters",
                "createdAt": "2026-03-02T12:00:30.000Z",
                "appVersion": "4.10.0-dev.1",
                "settings": {
                    "includeMemories": true,
                    "scope": "selected",
                    "selectedIds": ["11111111-0000-4000-8000-000000000001"]
                },
                "counts": { "characters": 1 }
            },
            "data": {
                "characters": [{
                    "id": "11111111-0000-4000-8000-000000000001",
                    "userId": "11111111-0000-4000-8000-0000000000ff",
                    "name": "Mira Lanternwright",
                    "createdAt": "2026-03-02T12:00:30.000Z",
                    "updatedAt": "2026-03-02T12:00:30.000Z"
                }]
            }
        })
    }

    #[test]
    fn the_embedded_schema_compiles() {
        assert!(validator().is_ok(), "the vendored schema must compile");
        // P4.D171: 92,797 at v4 `78b381a96` (was 89,769 at `2f4254b42`).
        assert_eq!(QTAP_EXPORT_SCHEMA_JSON.len(), 92_797);
    }

    #[test]
    fn a_well_formed_export_validates() {
        let r = validate_qtap_export(&minimal_characters_export());
        assert!(r.valid, "unexpected errors: {:?}", r.errors);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn a_non_object_root_is_reported_at_the_root_pointer() {
        let r = validate_qtap_export(&json!("nope"));
        assert!(!r.valid);
        assert!(
            r.errors.iter().all(|e| e.starts_with("/: ")),
            "root errors render `/`: {:?}",
            r.errors
        );
    }

    #[test]
    fn formats_are_asserted_not_annotated() {
        // `Timestamp` is `{type: string, format: date-time}` — with format
        // assertion OFF (draft 2020-12's default) this export would VALIDATE.
        let mut export = minimal_characters_export();
        export["manifest"]["createdAt"] = json!("not-a-date");
        let r = validate_qtap_export(&export);
        assert!(!r.valid, "the date-time format must be asserted");
        assert!(
            r.errors
                .iter()
                .any(|e| e.starts_with("/manifest/createdAt: ")),
            "{:?}",
            r.errors
        );
    }

    #[test]
    fn every_error_is_reported_not_just_the_first() {
        // ajv `allErrors: true`: two broken sections must both be named.
        let mut export = minimal_characters_export();
        export["data"]["characters"] = json!([{ "id": 7 }]);
        export["data"]["memories"] = json!([{ "id": 7 }]);
        let r = validate_qtap_export(&export);
        assert!(!r.valid);
        let sections: std::collections::BTreeSet<&str> = r
            .errors
            .iter()
            .filter_map(|e| e.strip_prefix("/data/"))
            .filter_map(|rest| rest.split('/').next())
            .collect();
        assert!(
            sections.contains("characters") && sections.contains("memories"),
            "both broken sections must appear: {sections:?} / {:?}",
            r.errors
        );
    }
}
