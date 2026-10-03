//! Project roster access — v4 `lib/projects/roster-access.ts` (`9753d0eb2`).
//!
//! The one question the project roster answers: may this character reach the
//! project's files and shared wardrobe through their own tools?
//!
//! The roster deliberately gates nothing else. A character off the roster still
//! joins project chats, still receives the project's instructions, and still
//! benefits from automatic knowledge retrieval; what they lose is the ability to
//! open, list, search or edit project-store documents with `doc_*` /
//! `search_scriptorium`, and to pick garments from the project's `Wardrobe/`
//! folders with `wardrobe_*`.
//!
//! The policy itself (`allowAnyCharacter` OR on the roster) lives in
//! [`ProjectsRepository::can_character_participate`]; this module is the
//! call-site chokepoint that handles the "no project" / "no character" cases,
//! fails CLOSED through the `db::fallback` home, and logs the outcome.
//!
//! **v4's seven sites, ported one for one:** the doc path resolver's
//! accessible-pool collector and its `project` scope
//! (`doc_edit::path_resolver`), `doc_open_document`'s new-blank scope
//! (`tools::doc_edit::document_ui`), `doc_grep`'s legacy on-disk walk and
//! `doc_list_files`' project branch (`tools::doc_edit::text`),
//! `search_scriptorium`'s standard pool (`tools::search`), and the shared
//! wardrobe's project tier (`wardrobe_tiers`, with the operator's Salon equip
//! bypassing it). **Deliberately UNGATED — v4 `9753d0eb2` touched none of
//! them:** chat membership, project instructions, automatic knowledge, the
//! Scenario Builder's pre-built pool, the collector's operator-override arm,
//! every equipped-outfit / avatar / announcement / image read, and
//! `resolve_project_mount_point_ids_for_chat` with its six callers.
//!
//! The `[ProjectRoster]` DEBUG is v4's ROOT logger (no service name); this
//! module's default tracing target stands in for it, and the differentials
//! compare its fields by name (`projectId`, `characterId`, `allowed`).

use rusqlite::Connection;

use crate::db::projects::ProjectsRepository;

/// The refusal a character sees when they reach for a project file off-roster
/// (v4 `PROJECT_ROSTER_REFUSAL`, byte-exact).
pub const PROJECT_ROSTER_REFUSAL: &str = "You are not on this project's character roster, so its files are closed to you. Ask the user to add you to the roster in the project's Characters card.";

/// JS truthiness over an optional string: `null`/`undefined`/`""` are all
/// absent (v4's `if (!projectId || !characterId)`).
fn truthy(s: Option<&str>) -> Option<&str> {
    s.filter(|s| !s.is_empty())
}

/// Whether `character_id` may use their tools on the project's files and shared
/// wardrobe (v4 `projectRosterAdmits`).
///
/// - No project → `true` (nothing to gate).
/// - No character → `true` (an operator surface, not a character's tool call).
/// - Otherwise the project's roster policy; a missing project or a failed
///   lookup is a refusal (fail closed — the two v4 lines live in
///   [`crate::db::fallback::can_character_participate_or_false`]).
///
/// Neither guard logs; the DEBUG fires only when both ids are truthy, AFTER the
/// policy has answered — so on a failed lookup the home's ERROR precedes it and
/// it reads `allowed: false`.
pub fn project_roster_admits(
    main: &Connection,
    mount: &Connection,
    project_id: Option<&str>,
    character_id: Option<&str>,
) -> bool {
    let (Some(project_id), Some(character_id)) = (truthy(project_id), truthy(character_id)) else {
        return true;
    };
    let allowed =
        crate::db::fallback::can_character_participate_or_false(project_id, character_id, || {
            ProjectsRepository::new(main, mount).can_character_participate(project_id, character_id)
        });
    tracing::debug!(
        projectId = %project_id,
        characterId = %character_id,
        allowed,
        "[ProjectRoster] Tool access check"
    );
    allowed
}

/// The project id a character's tools may use: `project_id` when the roster
/// admits them, `None` otherwise (v4 `rosterGatedProjectId`). For call sites
/// that thread a project id into a mount-pool resolution and should simply see
/// no project tier. A falsy project answers `None` with no line.
pub fn roster_gated_project_id(
    main: &Connection,
    mount: &Connection,
    project_id: Option<&str>,
    character_id: Option<&str>,
) -> Option<String> {
    let project_id = truthy(project_id)?;
    project_roster_admits(main, mount, Some(project_id), character_id)
        .then(|| project_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::captured_with;
    use rusqlite::params;

    /// The MAIN-db slim table, as `db::projects`' own tests seed it.
    fn main_db(rows: &[(&str, Option<&str>)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE projects (id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL, \
             officialMountPointId TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);",
        )
        .unwrap();
        for (id, mp) in rows {
            conn.execute(
                "INSERT INTO projects (id, name, officialMountPointId, createdAt, updatedAt) \
                 VALUES (?1, ?1, ?2, '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
                params![id, mp],
            )
            .unwrap();
        }
        conn
    }

    /// The MOUNT-INDEX three-table join the overlay reads; one `properties.json`
    /// per `(store, bytes)` pair. A store named in MAIN but absent here is the
    /// "store missing" plant (`properties.json missing`).
    fn mount_db(stores: &[(&str, &str)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE doc_mount_files (id TEXT PRIMARY KEY NOT NULL);
             CREATE TABLE doc_mount_documents (id TEXT PRIMARY KEY NOT NULL, \
                fileId TEXT NOT NULL, content TEXT);
             CREATE TABLE doc_mount_file_links (id TEXT PRIMARY KEY NOT NULL, \
                fileId TEXT NOT NULL, mountPointId TEXT NOT NULL, relativePath TEXT NOT NULL);",
        )
        .unwrap();
        for (mp, props) in stores {
            conn.execute(
                "INSERT INTO doc_mount_files (id) VALUES (?1 || '-f')",
                params![mp],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO doc_mount_documents (id, fileId, content) \
                 VALUES (?1 || '-d', ?1 || '-f', ?2)",
                params![mp, props],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO doc_mount_file_links (id, fileId, mountPointId, relativePath) \
                 VALUES (?1 || '-l', ?1 || '-f', ?1, 'properties.json')",
                params![mp],
            )
            .unwrap();
        }
        conn
    }

    const CLOSED: &str = r#"{"allowAnyCharacter":false,"characterRoster":["ada"]}"#;
    const OPEN: &str = r#"{"allowAnyCharacter":true,"characterRoster":[]}"#;

    fn world() -> (Connection, Connection) {
        (
            main_db(&[("p-closed", Some("mp-c")), ("p-open", Some("mp-o"))]),
            mount_db(&[("mp-c", CLOSED), ("mp-o", OPEN)]),
        )
    }

    fn debug_lines(lines: &[String]) -> Vec<&String> {
        lines
            .iter()
            .filter(|l| l.contains("[ProjectRoster] Tool access check"))
            .collect()
    }

    #[test]
    fn falsy_ids_admit_and_log_nothing() {
        // No tables at all: a guard that reached the repository would error.
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();
        for (p, c) in [
            (None, Some("c-1")),
            (Some("p-1"), None),
            (Some(""), Some("c-1")),
            (Some("p-1"), Some("")),
            (None, None),
        ] {
            let (admits, lines) = captured_with(|| project_roster_admits(&main, &mount, p, c));
            assert!(admits, "{p:?}/{c:?} must admit");
            assert!(lines.is_empty(), "{p:?}/{c:?} must log nothing: {lines:?}");
        }
        for p in [None, Some("")] {
            let (gated, lines) =
                captured_with(|| roster_gated_project_id(&main, &mount, p, Some("c-1")));
            assert_eq!(gated, None);
            assert!(lines.is_empty(), "{lines:?}");
        }
        // A falsy CHARACTER with a real project: the id passes through, no line.
        let (gated, lines) =
            captured_with(|| roster_gated_project_id(&main, &mount, Some("p-1"), Some("")));
        assert_eq!(gated.as_deref(), Some("p-1"));
        assert!(lines.is_empty(), "{lines:?}");
    }

    #[test]
    fn rostered_admitted_stranger_refused_allow_any_admits_everyone() {
        let (main, mount) = world();
        let (on, lines) =
            captured_with(|| project_roster_admits(&main, &mount, Some("p-closed"), Some("ada")));
        assert!(on);
        let d = debug_lines(&lines);
        assert_eq!(d.len(), 1, "{lines:?}");
        assert!(
            d[0].starts_with("DEBUG quilltap_core::project_roster_access"),
            "{}",
            d[0]
        );
        assert!(
            d[0].contains("projectId=p-closed characterId=ada allowed=true"),
            "{}",
            d[0]
        );

        let (off, lines) =
            captured_with(|| project_roster_admits(&main, &mount, Some("p-closed"), Some("bea")));
        assert!(!off);
        let d = debug_lines(&lines);
        assert_eq!(d.len(), 1, "{lines:?}");
        assert!(
            d[0].contains("projectId=p-closed characterId=bea allowed=false"),
            "{}",
            d[0]
        );
        assert!(
            !lines.iter().any(|l| l.starts_with("ERROR ")),
            "a plain refusal is not a failure: {lines:?}"
        );

        let (any, _) =
            captured_with(|| project_roster_admits(&main, &mount, Some("p-open"), Some("bea")));
        assert!(any, "Allow Any Character admits a stranger");
    }

    #[test]
    fn roster_gated_project_id_withholds_the_id_off_the_roster() {
        let (main, mount) = world();
        assert_eq!(
            roster_gated_project_id(&main, &mount, Some("p-closed"), Some("ada")).as_deref(),
            Some("p-closed")
        );
        assert_eq!(
            roster_gated_project_id(&main, &mount, Some("p-closed"), Some("bea")),
            None
        );
        assert_eq!(
            roster_gated_project_id(&main, &mount, Some("p-open"), Some("bea")).as_deref(),
            Some("p-open")
        );
    }

    #[test]
    fn a_missing_project_row_is_a_silent_refusal() {
        // v4: `findById` → null → `false`, no error line (the project is simply
        // not there); only the chokepoint's own DEBUG.
        let (main, mount) = world();
        let (admits, lines) =
            captured_with(|| project_roster_admits(&main, &mount, Some("p-ghost"), Some("ada")));
        assert!(!admits);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("allowed=false"), "{}", lines[0]);
    }

    #[test]
    fn a_missing_slim_table_fails_closed_through_the_inner_home() {
        // The slim read fails (no `projects` table): v4's `_findById` logs its own
        // fallback line and answers null → `false` with NO outer line; the DEBUG
        // follows, reading `allowed: false`.
        let main = Connection::open_in_memory().unwrap();
        let (_, mount) = world();
        let (admits, lines) =
            captured_with(|| project_roster_admits(&main, &mount, Some("p-1"), Some("c-1")));
        assert!(!admits);
        let err = lines
            .iter()
            .position(|l| l.contains("Error finding entity by ID"))
            .unwrap_or_else(|| panic!("no inner home line: {lines:?}"));
        assert_eq!(
            lines[err],
            "ERROR quilltap::db Error finding entity by ID collection=projects id=p-1 \
             error=no such table: projects"
        );
        let dbg = lines
            .iter()
            .position(|l| l.contains("[ProjectRoster] Tool access check"))
            .unwrap_or_else(|| panic!("no DEBUG: {lines:?}"));
        assert!(err < dbg, "v4 logs AFTER the policy answers: {lines:?}");
        assert!(lines[dbg].contains("allowed=false"), "{}", lines[dbg]);
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Error checking character participation")),
            "the inner failure never reaches v4's outer catch: {lines:?}"
        );
    }

    #[test]
    fn a_broken_store_fails_closed_through_the_outer_home() {
        // The slim row exists but its store has no `properties.json`: v4's
        // `applyOverlayOne` throws past the inner read into the OUTER `safeQuery`
        // → `Error checking character participation` → `false`.
        let main = main_db(&[("p-1", Some("mp-gone"))]);
        let mount = mount_db(&[]);
        let (admits, lines) =
            captured_with(|| project_roster_admits(&main, &mount, Some("p-1"), Some("c-1")));
        assert!(!admits);
        let err = lines
            .iter()
            .position(|l| l.contains("Error checking character participation"))
            .unwrap_or_else(|| panic!("no outer home line: {lines:?}"));
        assert_eq!(
            lines[err],
            "ERROR quilltap::db Error checking character participation collection=projects \
             projectId=p-1 characterId=c-1 error=Project p-1 has no usable document store \
             (officialMountPointId=mp-gone): properties.json missing"
        );
        let dbg = lines
            .iter()
            .position(|l| l.contains("[ProjectRoster] Tool access check"))
            .unwrap_or_else(|| panic!("no DEBUG: {lines:?}"));
        assert!(err < dbg, "{lines:?}");
        assert!(lines[dbg].contains("allowed=false"), "{}", lines[dbg]);
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Error finding entity by ID")),
            "an overlay failure is NOT the inner read's line: {lines:?}"
        );
    }
}
