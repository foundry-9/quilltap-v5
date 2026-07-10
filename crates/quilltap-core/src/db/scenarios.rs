//! Preset-scenario body resolution for chat creation — v4's
//! `lib/mount-index/{scenarios-common,project-scenarios,group-scenarios,
//! general-scenarios}.ts`, the `resolveScenarioBody` slice only.
//!
//! A preset scenario is a plain markdown file at `Scenarios/<filename>.md`
//! inside a document store (a project's official store, a group's official
//! store, or the instance-wide "Quilltap General" store). Chat creation resolves
//! the chosen scenario's *body* (content after any YAML frontmatter, trimmed) and
//! bakes it into `chat.scenarioText`.
//!
//! This ports ONLY the read path chat creation needs — `resolveScenarioBody` and
//! the three scoped wrappers. The list / read-by-path / set-default write
//! surface (`listScenariosInFolder`, `setScenarioDefaultInFolder`, …) belongs to
//! the P4.6 New-Chat dialog verticals and is a tracked deferral.
//!
//! Every function is a free function over a borrowed [`rusqlite::Connection`] —
//! the **mount-index** DB connection (where document bytes live), composing the
//! already-ported `read_database_document` and `parse_frontmatter`. The general
//! resolver additionally reads the "Quilltap General" store pointer from the
//! main-DB `instance_settings`, so it takes both connections.

use rusqlite::Connection;

use super::database_store::read_database_document;
use super::instance_settings::get_general_mount_point_id;
use super::DbError;
use crate::jsstr::js_trim;
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
}
