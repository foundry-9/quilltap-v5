//! The **seed subset** of v4's quilltap-import service (P4.4u4) — exactly the
//! code paths the `lorian-and-riya.qtap` sample-content seed and the
//! `reset-builtins` route exercise: the legacy monolithic-JSON parse + the
//! subset [`execute_import`] (characters with the legacy `scenario` → `scenarios`
//! migration + per-character wardrobe, then memories, then the character
//! reconcile loop), `conflictStrategy: 'skip'` (id-then-name existence check),
//! the shared id-map, and the counts/warnings result shape.
//!
//! v4 sources: `lib/import/quilltap-import/{validation,execute,import-characters,
//! import-entities,reconcile}.ts` + the barrel `quilltap-import-service.ts`.
//!
//! ## The deliberate divergence — refuse loudly, never import a fraction
//!
//! Everything OUTSIDE the subset is a **hard typed refusal** ([`ImportError`]),
//! not a silent skip. v4 would import tags / profiles / templates / projects /
//! groups / chats / annotations / chatDocuments / document stores; this port
//! refuses them because importing only the characters+memories slice of a richer
//! payload would silently drop the rest. The seed file (data keys exactly
//! `['characters','memories']`) never trips it. The NDJSON serialization
//! (`format: 'qtap-ndjson'`) and the `overwrite`/`duplicate` conflict strategies
//! are dead for both seed consumers and refuse on sniff.
//!
//! ## The two serializations, one of which is dead
//!
//! `.qtap` is NOT an archive. Legacy monolithic JSON (`manifest.format ===
//! 'quilltap-export'`, `version === '1.0'`, hard-pinned in v4
//! `validation.ts:48,55`) is the seed format and the only one ported. The
//! streaming NDJSON reassembler is unported (refusal on sniff).
//!
//! ## Composition — no re-port
//!
//! [`execute_import`] operates directly over the MAIN + MOUNT-INDEX connections
//! (run it inside a single [`crate::db::runtime::Db::write`] closure — the
//! sync-handlers-over-both-connections idiom). It composes the already-ported
//! primitives: [`create_character`] (slim row + vault provision + managed-field
//! projection, which mints a fresh id — so imported ids are minted, matching v4's
//! `repos.characters.create`), [`WardrobeRepository::create`],
//! [`MemoriesRepository::create`], and the character reads.

mod characters;
mod memories;
mod reconcile;
pub mod reset;
pub mod seed;
pub mod seed_assets;

use serde::Deserialize;
use serde_json::Value;

use crate::db::DbError;

/// v4 `ConflictStrategy` (`lib/export/types.ts`). Only `Skip` is ported — both
/// seed consumers pass it; `Overwrite`/`Duplicate` are a loud refusal (the
/// interactive import route is a future vertical).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictStrategy {
    Skip,
    Overwrite,
    Duplicate,
}

/// v4 `ImportOptions` (the subset the seed path passes). `include_related_entities`
/// is **read by nothing** in the v4 pipeline (grep-confirmed) — carried for
/// fidelity with `seedFromImports`' call site.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub conflict_strategy: ConflictStrategy,
    pub include_memories: bool,
    /// Dead option — no code reads it. Kept to mirror v4's call site verbatim.
    pub include_related_entities: bool,
}

impl ImportOptions {
    /// The exact options both seed consumers pass (`seedFromImports` :212–216 and
    /// `handleResetBuiltins` :263–267): skip, memories on, related off.
    pub fn seed_defaults() -> Self {
        Self {
            conflict_strategy: ConflictStrategy::Skip,
            include_memories: true,
            include_related_entities: false,
        }
    }
}

/// The per-entity counts v4 threads through (`QuilltapExportCounts`, the subset
/// this port touches). Absent kinds stay 0.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportCounts {
    pub characters: u32,
    pub memories: u32,
}

/// v4 `ImportResult` (the subset). `imported_character_ids` is the values of the
/// character id-map — for `skip` these are the existing/created destination ids.
#[derive(Debug, Clone)]
pub struct ImportResult {
    pub success: bool,
    pub imported: ImportCounts,
    pub skipped: ImportCounts,
    pub warnings: Vec<String>,
    pub imported_character_ids: Vec<String>,
}

/// A parsed + validated legacy-JSON export (v4 `QuilltapExport`). `manifest` and
/// `data` stay dynamic [`Value`]s — v4's `getExportData` reads them the same way.
#[derive(Debug, Clone)]
pub struct QuilltapExport {
    pub manifest: Value,
    pub data: Value,
}

/// A hard, typed refusal — the deliberate divergence from v4. Distinct from the
/// per-item `warnings` inside [`ImportResult`]: these mean "this payload is
/// outside the ported subset" and are returned as `Err` before any writes.
#[derive(Debug)]
pub enum ImportError {
    /// `JSON.parse` failed, or the top level is not an object (v4
    /// `parseExportFile` / `validateExportFormat`'s object guards).
    ParseJson(String),
    /// `manifest` missing or not an object.
    MissingManifest,
    /// `manifest.format` is neither the ported `quilltap-export` legacy format.
    InvalidFormat { got: String },
    /// The streaming NDJSON serialization (`format: 'qtap-ndjson'`) — unported.
    Ndjson,
    /// `manifest.version` is not the pinned `'1.0'`.
    UnsupportedVersion { got: String },
    /// `data` missing or not an object.
    MissingData,
    /// The payload carries entity kinds outside the ported subset
    /// (characters + memories). The unsupported keys are enumerated.
    UnsupportedEntities(Vec<String>),
    /// A conflict strategy other than `skip` — dead for both seed consumers.
    UnsupportedConflictStrategy(ConflictStrategy),
    /// A per-character non-empty `pluginData` payload — outside the subset.
    UnsupportedPluginData { character_name: String },
    /// A store/DB write failed at a non-per-item chokepoint.
    Db(DbError),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::ParseJson(m) => write!(f, "Invalid export file: {m}"),
            ImportError::MissingManifest => write!(f, "Missing or invalid manifest"),
            ImportError::InvalidFormat { got } => {
                write!(f, "Invalid format: expected 'quilltap-export', got '{got}'")
            }
            ImportError::Ndjson => write!(
                f,
                "The streaming NDJSON export format ('qtap-ndjson') is not supported by the \
                 sample-content importer (only the legacy monolithic 'quilltap-export' JSON is)."
            ),
            ImportError::UnsupportedVersion { got } => {
                write!(f, "Unsupported version: {got}. Only 1.0 is supported.")
            }
            ImportError::MissingData => write!(f, "Missing or invalid data section"),
            ImportError::UnsupportedEntities(keys) => write!(
                f,
                "The sample-content importer supports only 'characters' and 'memories'; \
                 refusing to import a fraction of a payload that also carries: {}",
                keys.join(", ")
            ),
            ImportError::UnsupportedConflictStrategy(s) => write!(
                f,
                "Only the 'skip' conflict strategy is supported by the sample-content \
                 importer; got {s:?}."
            ),
            ImportError::UnsupportedPluginData { character_name } => write!(
                f,
                "Character {character_name:?} carries pluginData, which the sample-content \
                 importer does not support."
            ),
            ImportError::Db(e) => write!(f, "Import failed: {e}"),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<DbError> for ImportError {
    fn from(e: DbError) -> Self {
        ImportError::Db(e)
    }
}

/// The entity kinds the ported subset handles. Any OTHER top-level `data` key is
/// a loud refusal.
const SUPPORTED_ENTITY_KEYS: &[&str] = &["characters", "memories"];

/// An insertion-ordered id-map (source id → destination id) — the ported subset's
/// analog of v4's `idMaps.characters` (`Map`, insertion-ordered). Only the
/// characters map is non-empty for the seed; the other nine v4 maps stay empty.
/// Insertion order matters: `imported_character_ids` is `Array.from(map.values())`.
#[derive(Default)]
pub(crate) struct IdMap(Vec<(String, String)>);

impl IdMap {
    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn set(&mut self, key: String, value: String) {
        if let Some(entry) = self.0.iter_mut().find(|(k, _)| *k == key) {
            entry.1 = value;
        } else {
            self.0.push((key, value));
        }
    }

    fn values(&self) -> Vec<String> {
        self.0.iter().map(|(_, v)| v.clone()).collect()
    }
}

/// v4 `parseExportFile` + `validateExportFormat` (`validation.ts`) — parse the
/// legacy monolithic JSON and hard-pin `format`/`version`. Diverges only to sniff
/// and loudly refuse the NDJSON serialization.
pub fn parse_export_file(json_string: &str) -> Result<QuilltapExport, ImportError> {
    let data: Value = serde_json::from_str(json_string).map_err(|e| {
        // A `qtap-ndjson` stream is line-delimited, not one JSON object, so it
        // fails to parse here — sniff for it to refuse with a clear message.
        if json_string.contains("qtap-ndjson") {
            ImportError::Ndjson
        } else {
            ImportError::ParseJson(e.to_string())
        }
    })?;
    validate_export_format(&data)?;
    let obj = data.as_object().expect("validated as object");
    Ok(QuilltapExport {
        manifest: obj.get("manifest").cloned().unwrap_or(Value::Null),
        data: obj.get("data").cloned().unwrap_or(Value::Null),
    })
}

/// v4 `validateExportFormat` — the `format`/`version` hard pins (+ the NDJSON
/// sniff, this port's divergence).
fn validate_export_format(data: &Value) -> Result<(), ImportError> {
    let obj = data.as_object().ok_or(ImportError::ParseJson(
        "Export data must be a JSON object".to_string(),
    ))?;

    let manifest = obj
        .get("manifest")
        .and_then(Value::as_object)
        .ok_or(ImportError::MissingManifest)?;

    let format = manifest.get("format").and_then(Value::as_str).unwrap_or("");
    if format == "qtap-ndjson" {
        return Err(ImportError::Ndjson);
    }
    if format != "quilltap-export" {
        return Err(ImportError::InvalidFormat {
            got: format.to_string(),
        });
    }

    // v4 pins `version === '1.0'` (a string). Match the JSON text form.
    let version = manifest.get("version");
    let version_ok = version.and_then(Value::as_str) == Some("1.0");
    if !version_ok {
        let got = match version {
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => "undefined".to_string(),
        };
        return Err(ImportError::UnsupportedVersion { got });
    }

    if !obj.get("data").map(Value::is_object).unwrap_or(false) {
        return Err(ImportError::MissingData);
    }

    Ok(())
}

/// v4 `executeImport` (`execute.ts:41`) restricted to the ported subset:
/// characters (with scenario-migration + wardrobe) → memories (gated) → the
/// character reconcile loop. Operates over the MAIN + MOUNT-INDEX connections
/// (run inside one `Db::write` closure).
///
/// Returns `Err(ImportError)` for a payload OUTSIDE the subset (the deliberate
/// divergence — enumerated unsupported keys / conflict strategy). Otherwise
/// returns the v4-shaped [`ImportResult`]; a DB write failure at a top-level
/// chokepoint yields `success: false` with the error in `warnings` (v4's
/// one-big-try catch), while per-item failures land in `warnings` and continue.
pub fn execute_import(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    user_id: &str,
    export: &QuilltapExport,
    options: &ImportOptions,
) -> Result<ImportResult, ImportError> {
    // Subset guard — refuse loudly BEFORE any write.
    if options.conflict_strategy != ConflictStrategy::Skip {
        return Err(ImportError::UnsupportedConflictStrategy(
            options.conflict_strategy,
        ));
    }
    let data = export.data.as_object().ok_or(ImportError::MissingData)?;
    let unsupported: Vec<String> = data
        .iter()
        // v4's importers no-op on empty arrays; only a NON-EMPTY unsupported kind
        // is a fraction we'd silently drop.
        .filter(|(k, v)| {
            !SUPPORTED_ENTITY_KEYS.contains(&k.as_str())
                && v.as_array().map(|a| !a.is_empty()).unwrap_or(!v.is_null())
        })
        .map(|(k, _)| k.clone())
        .collect();
    if !unsupported.is_empty() {
        let mut keys = unsupported;
        keys.sort();
        return Err(ImportError::UnsupportedEntities(keys));
    }

    let mut warnings: Vec<String> = Vec::new();
    let mut imported = ImportCounts::default();
    let mut skipped = ImportCounts::default();
    // The character id-map (source id → destination id). The other nine v4 id-maps
    // are empty for the subset (no tags/profiles/templates/projects/groups/chats/
    // mountPoints in a supported payload).
    let mut character_id_map = IdMap::default();

    // The one-big-try body: a DB error at a chokepoint → success:false (v4 catch).
    let outcome = import_body(
        main,
        mount,
        user_id,
        data,
        options,
        &mut imported,
        &mut skipped,
        &mut warnings,
        &mut character_id_map,
    );

    match outcome {
        Ok(()) => Ok(ImportResult {
            success: true,
            imported,
            skipped,
            warnings,
            imported_character_ids: character_id_map.values(),
        }),
        Err(ImportError::Db(e)) => {
            // v4's catch: return success:false with the error appended to warnings.
            warnings.push(format!("Import failed: {e}"));
            Ok(ImportResult {
                success: false,
                imported,
                skipped,
                warnings,
                imported_character_ids: character_id_map.values(),
            })
        }
        // A subset violation surfaced mid-run (e.g. pluginData) stays a hard Err.
        Err(other) => Err(other),
    }
}

/// The dependency-ordered import steps (v4 `executeImport`'s try body, subset):
/// characters → memories → reconcile. Extracted so the mutable borrows of the
/// counts / warnings / id-map end before [`execute_import`] builds the result.
#[allow(clippy::too_many_arguments)]
fn import_body(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    user_id: &str,
    data: &serde_json::Map<String, Value>,
    options: &ImportOptions,
    imported: &mut ImportCounts,
    skipped: &mut ImportCounts,
    warnings: &mut Vec<String>,
    character_id_map: &mut IdMap,
) -> Result<(), ImportError> {
    // 6. Characters (the seed's projects/groups/etc. steps don't fire).
    if let Some(chars) = data.get("characters").and_then(Value::as_array) {
        if !chars.is_empty() {
            let counts = characters::import_characters(
                main,
                mount,
                user_id,
                chars,
                options,
                character_id_map,
                warnings,
            )?;
            imported.characters = counts.imported;
            skipped.characters = counts.skipped;
        }
    }

    // 8. Memories (gated on include_memories).
    if options.include_memories {
        if let Some(mems) = data.get("memories").and_then(Value::as_array) {
            if !mems.is_empty() {
                let counts = memories::import_memories(main, mems, character_id_map, warnings)?;
                imported.memories = counts.imported;
                skipped.memories = counts.skipped;
            }
        }
    }

    // Post-import reconciliation (the character loop; every branch no-ops for the
    // seed but is ported faithfully — it is cheap and the differential covers it).
    reconcile::reconcile_characters(main, mount, character_id_map, warnings)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use serde_json::json;

    fn refuses_parse(s: &str) -> ImportError {
        parse_export_file(s).expect_err("should refuse")
    }

    #[test]
    fn parse_pins_format_and_version() {
        // Good manifest, empty data → parses.
        let ok = json!({"manifest": {"format": "quilltap-export", "version": "1.0"}, "data": {}});
        assert!(parse_export_file(&ok.to_string()).is_ok());

        // Wrong format.
        let bad_fmt =
            json!({"manifest": {"format": "something-else", "version": "1.0"}, "data": {}});
        assert!(matches!(
            refuses_parse(&bad_fmt.to_string()),
            ImportError::InvalidFormat { .. }
        ));

        // Wrong version.
        let bad_ver =
            json!({"manifest": {"format": "quilltap-export", "version": "2.0"}, "data": {}});
        assert!(matches!(
            refuses_parse(&bad_ver.to_string()),
            ImportError::UnsupportedVersion { .. }
        ));

        // Missing manifest.
        let no_manifest = json!({"data": {}});
        assert!(matches!(
            refuses_parse(&no_manifest.to_string()),
            ImportError::MissingManifest
        ));

        // Missing data.
        let no_data = json!({"manifest": {"format": "quilltap-export", "version": "1.0"}});
        assert!(matches!(
            refuses_parse(&no_data.to_string()),
            ImportError::MissingData
        ));
    }

    #[test]
    fn ndjson_serialization_refuses_on_sniff() {
        // An NDJSON stream is line-delimited (not one JSON object) — the sniff fires
        // on the parse failure.
        let ndjson = "{\"format\":\"qtap-ndjson\",\"version\":1}\n{\"kind\":\"character\"}\n";
        assert!(matches!(refuses_parse(ndjson), ImportError::Ndjson));

        // Even a single-object payload claiming the NDJSON format refuses.
        let claim = json!({"manifest": {"format": "qtap-ndjson", "version": 1}, "data": {}});
        assert!(matches!(
            refuses_parse(&claim.to_string()),
            ImportError::Ndjson
        ));
    }

    #[test]
    fn refuses_unsupported_entities_before_writing() {
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();
        let export = QuilltapExport {
            manifest: json!({"format": "quilltap-export", "version": "1.0"}),
            // characters (supported) alongside a NON-EMPTY unsupported kind.
            data: json!({
                "characters": [],
                "projects": [{"id": "p1"}],
                "tags": [{"id": "t1"}],
            }),
        };
        let err = execute_import(
            &main,
            &mount,
            "user",
            &export,
            &ImportOptions::seed_defaults(),
        )
        .expect_err("unsupported entities refuse");
        match err {
            ImportError::UnsupportedEntities(keys) => {
                // Enumerated + sorted; empty supported arrays don't trip it.
                assert_eq!(keys, vec!["projects".to_string(), "tags".to_string()]);
            }
            other => panic!("expected UnsupportedEntities, got {other:?}"),
        }
    }

    #[test]
    fn refuses_non_skip_conflict_strategy() {
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();
        let export = QuilltapExport {
            manifest: json!({"format": "quilltap-export", "version": "1.0"}),
            data: json!({"characters": []}),
        };
        for strat in [ConflictStrategy::Overwrite, ConflictStrategy::Duplicate] {
            let opts = ImportOptions {
                conflict_strategy: strat,
                include_memories: true,
                include_related_entities: false,
            };
            let err = execute_import(&main, &mount, "user", &export, &opts)
                .expect_err("non-skip refuses");
            assert!(matches!(err, ImportError::UnsupportedConflictStrategy(_)));
        }
    }

    #[test]
    fn empty_supported_payload_is_a_clean_no_op() {
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();
        let export = QuilltapExport {
            manifest: json!({"format": "quilltap-export", "version": "1.0"}),
            data: json!({"characters": [], "memories": []}),
        };
        let result = execute_import(
            &main,
            &mount,
            "user",
            &export,
            &ImportOptions::seed_defaults(),
        )
        .expect("empty payload imports cleanly");
        assert!(result.success);
        assert_eq!(result.imported, ImportCounts::default());
        assert!(result.imported_character_ids.is_empty());
    }
}
