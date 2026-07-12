//! Preset-scenario service surface — v4's
//! `lib/mount-index/{scenarios-common,project-scenarios,group-scenarios,
//! general-scenarios}.ts`.
//!
//! A preset scenario is a plain markdown file at `Scenarios/<filename>.md`
//! inside a document store (a project's official store, a group's official
//! store, or the instance-wide "Quilltap General" store). Chat creation resolves
//! the chosen scenario's *body* (content after any YAML frontmatter, trimmed) and
//! bakes it into `chat.scenarioText`.
//!
//! The `resolveScenarioBody` read slice and (P4.6n) the full list / read-by-path /
//! create / rename / delete / set-default service surface v4's
//! `scenarios-common.ts` offers — `parseScenarioDoc`, `listScenariosInFolder`,
//! `readScenarioByPath`, `setScenarioDefaultInFolder`, `buildScenarioFileContent`,
//! and `resolveScenarioPath`. The three scope wrappers (group / project / general)
//! reuse the same primitives, differing only in the folder constant and (general)
//! the `instance_settings` pointer + unprovisioned degradation.
//!
//! Every function is a free function over a borrowed [`rusqlite::Connection`] —
//! the **mount-index** DB connection (where document bytes live), composing the
//! already-ported `read_database_document` / `write_database_document`,
//! `parse_frontmatter` / `serialize_frontmatter` / `update_frontmatter_in_content`,
//! and the `doc_mount_documents` folder/path reads. The general resolver
//! additionally reads the "Quilltap General" store pointer from the main-DB
//! `instance_settings`, so it takes both connections.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Map, Value};

use super::database_store::{read_database_document, write_database_document, StoreError};
use super::doc_mount_documents::{DocMountDocumentsRepository, VaultFolderDoc};
use super::instance_settings::get_general_mount_point_id;
use super::DbError;
use crate::collation::locale_compare;
use crate::doc_edit::markdown_parser::{serialize_frontmatter, update_frontmatter_in_content};
use crate::jsstr::{js_trim, utf16_truncate};
use crate::markdown::{body_after, parse_frontmatter};

/// The folder every scope keeps its scenarios in (all three constants are
/// `'Scenarios'` in v4).
pub const SCENARIOS_FOLDER: &str = "Scenarios";

/// v4 `resolveScenarioBody` (`scenarios-common.ts:195`): resolve a scenario's
/// body by `<folderName>/<filename>.md` path. Accepts a bare filename or a full
/// relative path. Returns `None` when the file is missing or has no usable body.
///
/// `conn` is the mount-index DB connection (where the store's documents live).
/// Any read error (missing document, degraded store) is swallowed to `None` —
/// v4 logs a warning and returns null.
pub fn resolve_scenario_body(
    conn: &Connection,
    mount_point_id: &str,
    scenario_path: &str,
    folder_name: &str,
) -> Option<String> {
    // v4: normalised = scenarioPath.trim(); prefix the folder; append `.md`.
    let trimmed = scenario_path.trim();
    let prefix = format!("{folder_name}/");
    let mut normalised = if trimmed.starts_with(&prefix) {
        trimmed.to_string()
    } else {
        // `replace(/^\/+/, '')` — strip leading slashes before joining.
        format!("{}{}", prefix, trimmed.trim_start_matches('/'))
    };
    // `/\.md$/i` — case-insensitive `.md` suffix.
    if !ends_with_md_ci(&normalised) {
        normalised.push_str(".md");
    }

    // v4 wraps the read in try/catch → null. `read_database_document` returns a
    // store error on a missing document, which we swallow.
    let doc = read_database_document(conn, mount_point_id, &normalised).ok()?;
    let fm = parse_frontmatter(&doc.content);
    let body = js_trim(body_after(&doc.content, &fm));
    if body.is_empty() {
        None
    } else {
        Some(body.to_string())
    }
}

/// v4 `resolveProjectScenarioBody` — resolve from a project's official store.
pub fn resolve_project_scenario_body(
    mount_index: &Connection,
    mount_point_id: &str,
    scenario_path: &str,
) -> Option<String> {
    resolve_scenario_body(mount_index, mount_point_id, scenario_path, SCENARIOS_FOLDER)
}

/// v4 `resolveGroupScenarioBody` — resolve from a group's official store.
pub fn resolve_group_scenario_body(
    mount_index: &Connection,
    mount_point_id: &str,
    scenario_path: &str,
) -> Option<String> {
    resolve_scenario_body(mount_index, mount_point_id, scenario_path, SCENARIOS_FOLDER)
}

/// v4 `resolveGeneralScenarioBody` — resolve from the instance-wide "Quilltap
/// General" store. Reads the store pointer from `instance_settings` (main DB);
/// returns `None` when the singleton mount has not been provisioned. The
/// document read then goes to the mount-index DB.
pub fn resolve_general_scenario_body(
    main: &Connection,
    mount_index: &Connection,
    scenario_path: &str,
) -> Result<Option<String>, DbError> {
    let Some(mount_point_id) = get_general_mount_point_id(main)? else {
        return Ok(None);
    };
    Ok(resolve_scenario_body(
        mount_index,
        &mount_point_id,
        scenario_path,
        SCENARIOS_FOLDER,
    ))
}

// ===========================================================================
// The list / read / write service surface (v4 `scenarios-common.ts`, P4.6n)
// ===========================================================================

/// v4 `ParsedScenario` — a parsed `Scenarios/*.md` file. Serializes to the API
/// DTO (`description` omitted when absent, matching v4's conditional spread).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedScenario {
    /// Relative path inside the mount point, e.g. `Scenarios/welcome.md`.
    pub path: String,
    /// Filename inside the folder, without the `.md` extension.
    pub filename: String,
    /// Display title — frontmatter `name` if present, else filename.
    pub name: String,
    /// Optional one-line description from frontmatter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// True when this scenario is the effective default after conflict resolution.
    pub is_default: bool,
    /// True when the file's own frontmatter set `isDefault: true`.
    pub raw_is_default: bool,
    /// Scenario body — content after frontmatter, trimmed.
    pub body: String,
    pub last_modified: String,
    pub created_at: String,
    pub updated_at: String,
}

/// v4 `ListScenariosResult` — the parsed scenarios plus any default-conflict
/// warnings.
#[derive(Debug, Clone, PartialEq)]
pub struct ListScenariosResult {
    pub scenarios: Vec<ParsedScenario>,
    pub warnings: Vec<String>,
}

/// Error from the scenarios write surface — a store/DB failure, or the
/// "chosen default not found" case v4's `setScenarioDefaultInFolder` throws.
#[derive(Debug)]
pub enum ScenarioWriteError {
    /// A `write_database_document` / repo-read failure.
    Store(StoreError),
    /// v4 `throw new Error('Cannot set default — scenario not found: <path>')`.
    /// Unreachable on the route path (set-default runs after the write lands the
    /// file), but ported faithfully.
    DefaultNotFound(String),
}

impl From<DbError> for ScenarioWriteError {
    fn from(e: DbError) -> Self {
        ScenarioWriteError::Store(StoreError::Db(e))
    }
}
impl From<StoreError> for ScenarioWriteError {
    fn from(e: StoreError) -> Self {
        ScenarioWriteError::Store(e)
    }
}
impl std::fmt::Display for ScenarioWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScenarioWriteError::Store(e) => write!(f, "{e}"),
            ScenarioWriteError::DefaultNotFound(p) => {
                write!(f, "Cannot set default — scenario not found: {p}")
            }
        }
    }
}

/// v4 `parseScenarioDoc` (`scenarios-common.ts:71`): parse one `Scenarios/*.md`
/// file. Returns `None` when the file has no usable body (empty bodies are
/// dropped). `rawIsDefault` reflects the file's own frontmatter; `isDefault` is
/// set to `rawIsDefault` here and resolved against siblings by the caller.
pub fn parse_scenario_doc(doc: &VaultFolderDoc) -> Option<ParsedScenario> {
    let parsed = parse_frontmatter(&doc.content);

    let mut frontmatter_name: Option<String> = None;
    let mut frontmatter_description: Option<String> = None;
    let mut raw_is_default = false;
    if let Some(Value::Object(data)) = &parsed.data {
        if let Some(Value::String(n)) = data.get("name") {
            let t = js_trim(n);
            if !t.is_empty() {
                frontmatter_name = Some(t.to_string());
            }
        }
        if let Some(Value::String(d)) = data.get("description") {
            let t = js_trim(d);
            if !t.is_empty() {
                // v4 `.trim().slice(0, 500)` — UTF-16 code units.
                frontmatter_description = Some(utf16_truncate(t, 500));
            }
        }
        raw_is_default = data.get("isDefault") == Some(&Value::Bool(true));
    }

    // v4 `content.slice(parsed.bodyStartOffset).trim()`.
    let body = js_trim(body_after(&doc.content, &parsed)).to_string();
    if body.is_empty() {
        // v4 logs a warning and returns null; the log is a no-op seam here.
        return None;
    }

    let file_name_no_ext = strip_md_suffix(&doc.file_name);
    // v4 `(frontmatterName ?? fileNameNoExt).slice(0, 200)`.
    let name = utf16_truncate(
        frontmatter_name.as_deref().unwrap_or(&file_name_no_ext),
        200,
    );

    Some(ParsedScenario {
        path: doc.relative_path.clone(),
        filename: file_name_no_ext,
        name,
        description: frontmatter_description,
        is_default: raw_is_default,
        raw_is_default,
        body,
        last_modified: doc.last_modified.clone(),
        created_at: doc.created_at.clone(),
        updated_at: doc.updated_at.clone(),
    })
}

/// v4 `listScenariosInFolder` (`scenarios-common.ts:123`): list every
/// `<folder>/*.md`, parsed and sorted by `relativePath.localeCompare` (the ICU4X
/// en-US port), then default-conflict resolution — the alphabetically-first
/// `rawIsDefault` wins; later ones are demoted to `isDefault:false` in the
/// RESPONSE ONLY (on-disk frontmatter is NOT rewritten) with a warning naming the
/// winner + ignored files.
pub fn list_scenarios_in_folder(
    mount: &Connection,
    mount_point_id: &str,
    folder_name: &str,
) -> Result<ListScenariosResult, DbError> {
    let repo = DocMountDocumentsRepository::new(mount);
    let mut docs = repo.find_many_by_mount_points_in_folder(
        &[mount_point_id.to_string()],
        folder_name,
        ".md",
    )?;

    // v4 `docs.sort((a, b) => a.relativePath.localeCompare(b.relativePath))`.
    docs.sort_by(|a, b| locale_compare(&a.relative_path, &b.relative_path));

    let mut scenarios: Vec<ParsedScenario> = Vec::new();
    for doc in &docs {
        if let Some(s) = parse_scenario_doc(doc) {
            scenarios.push(s);
        }
    }

    let mut warnings: Vec<String> = Vec::new();
    let mut default_claimed = false;
    let mut offenders: Vec<String> = Vec::new();
    for s in &mut scenarios {
        if s.raw_is_default {
            if !default_claimed {
                default_claimed = true;
                s.is_default = true;
            } else {
                s.is_default = false;
                offenders.push(s.filename.clone());
            }
        }
    }
    if !offenders.is_empty() {
        let winner = scenarios
            .iter()
            .find(|s| s.is_default)
            .map(|s| s.filename.clone())
            .unwrap_or_else(|| "<none>".to_string());
        let ignored = offenders
            .iter()
            .map(|o| format!("\"{o}\""))
            .collect::<Vec<_>>()
            .join(", ");
        warnings.push(format!(
            "Multiple scenarios mark themselves as default; using \"{winner}\". \
             These are also marked default but are being ignored: {ignored}."
        ));
    }

    Ok(ListScenariosResult {
        scenarios,
        warnings,
    })
}

/// v4 `readScenarioByPath` (`scenarios-common.ts:179`): read one scenario by
/// relative path. `isDefault` reflects ONLY the file's own frontmatter (no
/// sibling cross-reference). `None` when the file is missing or its body is empty.
pub fn read_scenario_by_path(
    mount: &Connection,
    mount_point_id: &str,
    relative_path: &str,
) -> Result<Option<ParsedScenario>, DbError> {
    let repo = DocMountDocumentsRepository::new(mount);
    let Some(doc) = repo.find_with_link_by_mount_point_and_path(mount_point_id, relative_path)?
    else {
        return Ok(None);
    };
    Ok(parse_scenario_doc(&doc))
}

/// v4 `setScenarioDefaultInFolder` (`scenarios-common.ts:232`): a **sequenced
/// multi-file write with NO transaction** — (1) set `isDefault:true` on the chosen
/// file (throw when absent), (2) demote every other `isDefault:true` file, each
/// individually try/caught. The deliberate no-transaction means a partial failure
/// falls back to the alphabetical tiebreaker (a soft warning surfaces on the next
/// list). Writes only when `updateFrontmatterInContent` changed the content.
pub fn set_scenario_default_in_folder(
    mount: &Connection,
    mount_point_id: &str,
    default_path: &str,
    folder_name: &str,
) -> Result<(), ScenarioWriteError> {
    let repo = DocMountDocumentsRepository::new(mount);
    let docs = repo.find_many_by_mount_points_in_folder(
        &[mount_point_id.to_string()],
        folder_name,
        ".md",
    )?;

    // Step 1: set the chosen file's isDefault:true.
    let mut chosen_found = false;
    for doc in &docs {
        if doc.relative_path == default_path {
            let updated =
                update_frontmatter_in_content(&doc.content, &one_bool("isDefault", true), false);
            if updated != doc.content {
                write_database_document(mount, mount_point_id, &doc.relative_path, &updated)?;
            }
            chosen_found = true;
            break;
        }
    }
    if !chosen_found {
        return Err(ScenarioWriteError::DefaultNotFound(
            default_path.to_string(),
        ));
    }

    // Step 2: demote any other file that had isDefault:true (each try/caught).
    for doc in &docs {
        if doc.relative_path == default_path {
            continue;
        }
        let parsed = parse_frontmatter(&doc.content);
        let has_default = matches!(
            parsed.data.as_ref().and_then(|d| d.get("isDefault")),
            Some(Value::Bool(true))
        );
        if has_default {
            let updated =
                update_frontmatter_in_content(&doc.content, &one_bool("isDefault", false), false);
            if updated != doc.content {
                // v4 swallows the demote failure (the alphabetical tiebreaker
                // compensates; a soft warning surfaces on the next list).
                let _ =
                    write_database_document(mount, mount_point_id, &doc.relative_path, &updated);
            }
        }
    }

    Ok(())
}

/// v4 `buildScenarioFileContent` (`scenarios-common.ts:286`): frontmatter for the
/// NON-EMPTY name/description and truthy isDefault only; with no keys the result
/// is the bare body (no `---` block).
pub fn build_scenario_file_content(
    name: Option<&str>,
    description: Option<&str>,
    is_default: bool,
    body: &str,
) -> String {
    let mut frontmatter: Map<String, Value> = Map::new();
    if let Some(n) = name {
        let t = js_trim(n);
        if !t.is_empty() {
            frontmatter.insert("name".into(), Value::String(t.to_string()));
        }
    }
    if let Some(d) = description {
        let t = js_trim(d);
        if !t.is_empty() {
            frontmatter.insert("description".into(), Value::String(t.to_string()));
        }
    }
    if is_default {
        frontmatter.insert("isDefault".into(), Value::Bool(true));
    }

    if frontmatter.is_empty() {
        // v4: `${''}${'' ? '\n' : ''}${body}` → just the body.
        body.to_string()
    } else {
        // v4: `${fmBlock}\n${body}` (serializeFrontmatter carries its own trailing
        // newline; the extra `\n` separates it from the body).
        format!("{}\n{}", serialize_frontmatter(&frontmatter), body)
    }
}

/// A resolved scenario path, or v4's rejection message.
pub enum ScenarioPathResolution {
    Ok(String),
    Err(String),
}

/// v4 `resolveScenarioPath` (`scenarios-common.ts:312`): normalise a scenarioPath
/// into a clean `<folder>/<file>.md` relative path; reject empty / `..` / `//` /
/// nested paths. **Boundary seam:** v4 `decodeURIComponent`s the raw Next.js route
/// segment first; the v5 dispatch transport delivers an already-decoded JSON
/// string, so (like [`resolve_scenario_body`]) no decode is applied here.
pub fn resolve_scenario_path(scenario_path: &str, folder_name: &str) -> ScenarioPathResolution {
    let candidate = js_trim(scenario_path);
    if candidate.is_empty() {
        return ScenarioPathResolution::Err("scenarioPath cannot be empty".to_string());
    }
    if candidate.contains("..") || candidate.contains("//") {
        return ScenarioPathResolution::Err("Invalid scenarioPath".to_string());
    }
    let prefix = format!("{folder_name}/");
    let mut candidate = if candidate.starts_with(&prefix) {
        candidate.to_string()
    } else {
        format!("{}{}", prefix, candidate.trim_start_matches('/'))
    };
    if !ends_with_md_ci(&candidate) {
        candidate.push_str(".md");
    }
    // Reject nested paths under the folder.
    let rest = &candidate[folder_name.len() + 1..];
    if rest.contains('/') {
        return ScenarioPathResolution::Err("Scenarios cannot live in nested folders".to_string());
    }
    ScenarioPathResolution::Ok(candidate)
}

/// Build a single-key frontmatter update map (`{ <key>: <bool> }`).
fn one_bool(key: &str, value: bool) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(key.to_string(), Value::Bool(value));
    m
}

/// v4 `doc.fileName.replace(/\.md$/i, '')` — strip ONE trailing case-insensitive
/// `.md` suffix.
fn strip_md_suffix(name: &str) -> String {
    if ends_with_md_ci(name) {
        name[..name.len() - 3].to_string()
    } else {
        name.to_string()
    }
}

/// `/\.md$/i` — a case-insensitive `.md` suffix test.
fn ends_with_md_ci(s: &str) -> bool {
    let bytes = s.as_bytes();
    let n = bytes.len();
    n >= 3
        && bytes[n - 3] == b'.'
        && bytes[n - 2].eq_ignore_ascii_case(&b'm')
        && bytes[n - 1].eq_ignore_ascii_case(&b'd')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md_suffix_test_is_case_insensitive() {
        assert!(ends_with_md_ci("a.md"));
        assert!(ends_with_md_ci("a.MD"));
        assert!(ends_with_md_ci("a.Md"));
        assert!(!ends_with_md_ci("a.mdx"));
        assert!(!ends_with_md_ci("amd"));
        assert!(ends_with_md_ci(".md")); // exactly ".md" — a valid suffix
        assert!(!ends_with_md_ci("md"));
    }

    fn ok(res: ScenarioPathResolution) -> String {
        match res {
            ScenarioPathResolution::Ok(p) => p,
            ScenarioPathResolution::Err(e) => panic!("expected Ok, got Err({e})"),
        }
    }
    fn err(res: ScenarioPathResolution) -> String {
        match res {
            ScenarioPathResolution::Err(e) => e,
            ScenarioPathResolution::Ok(p) => panic!("expected Err, got Ok({p})"),
        }
    }

    #[test]
    fn resolve_scenario_path_shapes() {
        assert_eq!(
            ok(resolve_scenario_path("prologue", "Scenarios")),
            "Scenarios/prologue.md"
        );
        assert_eq!(
            ok(resolve_scenario_path("prologue.md", "Scenarios")),
            "Scenarios/prologue.md"
        );
        assert_eq!(
            ok(resolve_scenario_path("Scenarios/prologue.md", "Scenarios")),
            "Scenarios/prologue.md"
        );
        // Leading slashes stripped before joining.
        assert_eq!(
            ok(resolve_scenario_path("/prologue", "Scenarios")),
            "Scenarios/prologue.md"
        );
        // Case-insensitive .md is preserved (no double extension).
        assert_eq!(
            ok(resolve_scenario_path("Prologue.MD", "Scenarios")),
            "Scenarios/Prologue.MD"
        );
    }

    #[test]
    fn resolve_scenario_path_rejections() {
        assert_eq!(
            err(resolve_scenario_path("   ", "Scenarios")),
            "scenarioPath cannot be empty"
        );
        assert_eq!(
            err(resolve_scenario_path("../secret.md", "Scenarios")),
            "Invalid scenarioPath"
        );
        assert_eq!(
            err(resolve_scenario_path("a//b.md", "Scenarios")),
            "Invalid scenarioPath"
        );
        assert_eq!(
            err(resolve_scenario_path("sub/deep.md", "Scenarios")),
            "Scenarios cannot live in nested folders"
        );
    }

    #[test]
    fn build_content_bare_body_when_no_frontmatter() {
        // Empty name/description + isDefault false → bare body, no `---`.
        assert_eq!(
            build_scenario_file_content(None, None, false, "The tale begins."),
            "The tale begins."
        );
        // Whitespace-only name is treated as empty (js_trim).
        assert_eq!(
            build_scenario_file_content(Some("   "), None, false, "body"),
            "body"
        );
    }

    #[test]
    fn build_content_with_frontmatter_block() {
        let out = build_scenario_file_content(
            Some("Prologue"),
            Some("Opening"),
            true,
            "The tale begins.",
        );
        assert_eq!(
            out,
            "---\nname: Prologue\ndescription: Opening\nisDefault: true\n---\n\nThe tale begins."
        );
    }

    #[test]
    fn parse_scenario_doc_drops_empty_body() {
        let doc = VaultFolderDoc {
            content: "---\nname: Empty\n---\n   \n".to_string(),
            mount_point_id: "mp".into(),
            relative_path: "Scenarios/empty.md".into(),
            file_name: "empty.md".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            updated_at: "2026-01-02T00:00:00.000Z".into(),
            last_modified: "2026-01-03T00:00:00.000Z".into(),
        };
        assert!(parse_scenario_doc(&doc).is_none());
    }

    #[test]
    fn parse_scenario_doc_fields() {
        let doc = VaultFolderDoc {
            content: "---\nname: Prologue\ndescription: Opening scene\nisDefault: true\n---\n\nThe tale begins.".to_string(),
            mount_point_id: "mp".into(),
            relative_path: "Scenarios/prologue.md".into(),
            file_name: "prologue.md".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            updated_at: "2026-01-02T00:00:00.000Z".into(),
            last_modified: "2026-01-03T00:00:00.000Z".into(),
        };
        let s = parse_scenario_doc(&doc).expect("parsed");
        assert_eq!(s.path, "Scenarios/prologue.md");
        assert_eq!(s.filename, "prologue");
        assert_eq!(s.name, "Prologue");
        assert_eq!(s.description.as_deref(), Some("Opening scene"));
        assert!(s.is_default);
        assert!(s.raw_is_default);
        assert_eq!(s.body, "The tale begins.");
        assert_eq!(s.last_modified, "2026-01-03T00:00:00.000Z");

        // No frontmatter name → filename fallback; no description → omitted.
        let doc2 = VaultFolderDoc {
            content: "Just a body.".to_string(),
            file_name: "interlude.md".into(),
            relative_path: "Scenarios/interlude.md".into(),
            ..doc
        };
        let s2 = parse_scenario_doc(&doc2).expect("parsed");
        assert_eq!(s2.name, "interlude");
        assert_eq!(s2.description, None);
        assert!(!s2.raw_is_default);
    }
}
