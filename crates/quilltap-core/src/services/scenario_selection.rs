//! Scenario selection — the one place a chosen scenario becomes text.
//!
//! A differential port of v4 `lib/chat/scenario-selection.ts` (v4 `44a8137e`).
//!
//! Both the New Chat dialog and the in-chat scenario picker offer the same four
//! tiers of preset, plus free-text notes. This module owns the precedence chain
//! and the resolution of each tier's pointer into a body:
//!
//!   character `scenarioId` > project path > group path > general path
//!
//! The free text is NOT part of that chain — whatever the chain resolves is the
//! preset, and the notes are layered beneath it by
//! [`combine_scenario_text`](crate::scenario_text::combine_scenario_text). When
//! nothing resolves and no notes were typed, the result is `None`, which callers
//! persist as a `null` `chat.scenarioText`.
//!
//! Every tier fails soft: an unresolvable pointer logs a warning and falls
//! through to the next tier rather than refusing the whole operation. A chat is
//! still worth having when its scenario file has been renamed out from under it.
//!
//! ## The two callers
//!
//! [`chat_create`](crate::services::chat_create) (which held this chain inline
//! until v4 extracted it) and
//! [`chat_scenario`](crate::services::chat_scenario)'s `?action=scenario` verb.
//! Both pass `log_tag = "[Chats v1]"`, exactly as v4's two call sites do.
//!
//! ## JS truthiness, reproduced
//!
//! v4 guards every tier with `if (!presetBody && fields.<pointer>)` — an EMPTY
//! STRING is falsy in JS, so `scenarioId: ''` (or an empty path, or an empty
//! `projectId`) is treated as absent and the chain falls through. The same is
//! true of a resolved body: a character scenario whose `content` is `''` leaves
//! `presetBody` falsy, so the NEXT tier still gets its turn. `preset_body` is
//! therefore a plain `String` here whose emptiness IS the falsy test — not an
//! `Option` — and the pointer reads are `.filter(|s| !s.is_empty())`.
//! (v5's inline create-path chain treated `Some("")` as present; the extraction
//! closes that latent divergence, pinned by the corpus's empty-pointer arms.)
//!
//! Pinned by `chat_scenario_routes_equivalence` (its `resolver_*` cases drive
//! v4's REAL `resolveScenarioSelection`) and, for the create path's neutrality,
//! by `chat_create_capstone_equivalence`.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::{groups, projects, scenarios, DbError};
use crate::scenario_text::combine_scenario_text;

/// v4's default `logTag`. Both real call sites override it with `"[Chats v1]"`,
/// but the default is v4's and is reachable through [`resolve_scenario_selection`]'s
/// `None`.
pub const DEFAULT_LOG_TAG: &str = "[Scenario]";

/// The scenario fields as they arrive from a client — the New Chat dialog's
/// create payload and the in-chat picker's `?action=scenario` body use the same
/// names, so the same resolver serves both (v4 `ScenarioSelectionFields`).
#[derive(Debug, Default, Clone, Copy)]
pub struct ScenarioSelectionFields<'a> {
    /// Free-text notes. Appended beneath a resolved preset, or used alone.
    pub scenario: Option<&'a str>,
    /// A character scenario's UUID, looked up on `character.scenarios`.
    pub scenario_id: Option<&'a str>,
    /// `Scenarios/<file>.md` inside the project's official store. Needs `project_id`.
    pub project_scenario_path: Option<&'a str>,
    /// `Scenarios/<file>.md` inside a group's official store. Needs `group_scenario_group_id`.
    pub group_scenario_path: Option<&'a str>,
    pub group_scenario_group_id: Option<&'a str>,
    /// `Scenarios/<file>.md` inside the instance-wide "Quilltap General" mount.
    pub general_scenario_path: Option<&'a str>,
}

/// v4 `ResolveScenarioSelectionOptions`. `main`/`mount` stand in for v4's
/// `repos` (the raw project/group reads live in main; the scenario documents in
/// the mount index).
pub struct ResolveScenarioSelectionOptions<'a> {
    /// Required for `project_scenario_path` to resolve.
    pub project_id: Option<&'a str>,
    /// Required for `scenario_id` to resolve — the hydrated character row whose
    /// `scenarios` array backs the lookup.
    pub character: Option<&'a Value>,
    /// Log prefix, so warnings read as coming from the calling route.
    /// `None` → [`DEFAULT_LOG_TAG`].
    pub log_tag: Option<&'a str>,
}

/// Resolve a scenario selection into the text that lands on `chat.scenarioText`.
/// Returns `None` when nothing was chosen and nothing was typed.
///
/// v4 `resolveScenarioSelection`.
pub fn resolve_scenario_selection(
    main: &Connection,
    mount: &Connection,
    fields: &ScenarioSelectionFields<'_>,
    options: &ResolveScenarioSelectionOptions<'_>,
) -> Result<Option<String>, DbError> {
    let log_tag = options.log_tag.unwrap_or(DEFAULT_LOG_TAG);

    // The JS `presetBody` — see the module header: EMPTY means falsy, which is
    // what lets a later tier take its turn.
    let mut preset_body = String::new();

    if preset_body.is_empty() {
        if let Some(scenario_id) = truthy(fields.scenario_id) {
            match find_scenario_content(options.character, scenario_id) {
                Some(content) => preset_body = content,
                None => tracing::warn!(
                    character_id = options
                        .character
                        .and_then(|c| c.get("id"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("null"),
                    scenario_id,
                    "{log_tag} scenarioId not found on character"
                ),
            }
        }
    }

    if preset_body.is_empty() {
        if let Some(path) = truthy(fields.project_scenario_path) {
            match truthy(options.project_id) {
                None => tracing::warn!(
                    project_scenario_path = path,
                    "{log_tag} projectScenarioPath provided without projectId; ignoring"
                ),
                Some(project_id) => {
                    // Only the store pointer is needed here, which lives on the
                    // raw row — use the raw read so scenario resolution doesn't
                    // throw on a degraded store.
                    match projects::find_official_mount_point_id_raw(main, project_id)?
                        .flatten()
                        .filter(|m| !m.is_empty())
                    {
                        None => tracing::warn!(
                            project_id,
                            project_scenario_path = path,
                            "{log_tag} projectScenarioPath provided but project has no officialMountPointId"
                        ),
                        Some(mount_point_id) => {
                            match scenarios::resolve_project_scenario_body(
                                mount,
                                &mount_point_id,
                                path,
                            ) {
                                Some(body) => preset_body = body,
                                None => tracing::warn!(
                                    project_id,
                                    project_scenario_path = path,
                                    "{log_tag} projectScenarioPath did not resolve to a body"
                                ),
                            }
                        }
                    }
                }
            }
        }
    }

    if preset_body.is_empty() {
        if let Some(path) = truthy(fields.group_scenario_path) {
            match truthy(fields.group_scenario_group_id) {
                None => tracing::warn!(
                    group_scenario_path = path,
                    "{log_tag} groupScenarioPath provided without groupScenarioGroupId; ignoring"
                ),
                Some(group_id) => {
                    // Only the store pointer is needed — read the slim row so
                    // resolution doesn't throw on a degraded store.
                    match groups::find_official_mount_point_id_raw(main, group_id)?
                        .flatten()
                        .filter(|m| !m.is_empty())
                    {
                        None => tracing::warn!(
                            group_scenario_group_id = group_id,
                            group_scenario_path = path,
                            "{log_tag} groupScenarioPath provided but group has no officialMountPointId"
                        ),
                        Some(mount_point_id) => {
                            match scenarios::resolve_group_scenario_body(
                                mount,
                                &mount_point_id,
                                path,
                            ) {
                                Some(body) => preset_body = body,
                                None => tracing::warn!(
                                    group_scenario_group_id = group_id,
                                    group_scenario_path = path,
                                    "{log_tag} groupScenarioPath did not resolve to a body"
                                ),
                            }
                        }
                    }
                }
            }
        }
    }

    if preset_body.is_empty() {
        if let Some(path) = truthy(fields.general_scenario_path) {
            match scenarios::resolve_general_scenario_body(main, mount, path)? {
                Some(body) => preset_body = body,
                None => tracing::warn!(
                    general_scenario_path = path,
                    "{log_tag} generalScenarioPath did not resolve to a body"
                ),
            }
        }
    }

    // Append the user's free-text scenario notes. When a preset resolved above,
    // the notes are layered beneath it; when none did, the notes ARE the
    // scenario. (An empty `preset_body` is filtered out by `combine`, exactly as
    // JS's `undefined`/`''` both are.)
    Ok(combine_scenario_text(Some(&preset_body), fields.scenario))
}

/// JS truthiness for a string field: `undefined`, `null` and `''` are all
/// "absent" (the module header's note).
fn truthy(v: Option<&str>) -> Option<&str> {
    v.filter(|s| !s.is_empty())
}

/// v4 `character?.scenarios?.find((s) => s.id === scenarioId)?.content`.
/// A match whose `content` is missing or not a string yields `''` — falsy in v4
/// too, so the chain falls through.
fn find_scenario_content(character: Option<&Value>, scenario_id: &str) -> Option<String> {
    character?
        .get("scenarios")?
        .as_array()?
        .iter()
        .find(|s| s.get("id").and_then(Value::as_str) == Some(scenario_id))
        .map(|s| {
            s.get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A connection with NO tables. Any tier that actually QUERIES fails on it,
    /// which is what makes the JS-truthiness guards measurable: a falsy pointer
    /// must be skipped BEFORE the read, so the resolver still answers `Ok`.
    fn bare() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    /// The module header's JS-truthiness contract, pinned where the differential
    /// corpus cannot see it: every empty-string pointer produces the SAME output
    /// as an absent one, so only the WARNING differs — and a state diff is blind
    /// to a log (`differential-blind-to-a-log-only-fix`). These arms make the
    /// difference observable instead: without the `truthy()` filter each tier
    /// reads the DB, and a table-less connection turns that into an `Err`.
    #[test]
    fn empty_string_pointers_are_falsy_and_never_reach_the_db() {
        let (main, mount) = (bare(), bare());
        for fields in [
            ScenarioSelectionFields {
                project_scenario_path: Some("Scenarios/p.md"),
                ..Default::default()
            },
            ScenarioSelectionFields {
                group_scenario_path: Some("Scenarios/g.md"),
                group_scenario_group_id: Some(""),
                ..Default::default()
            },
            ScenarioSelectionFields {
                general_scenario_path: Some(""),
                ..Default::default()
            },
            ScenarioSelectionFields {
                project_scenario_path: Some(""),
                group_scenario_path: Some(""),
                general_scenario_path: Some(""),
                ..Default::default()
            },
        ] {
            let got = resolve_scenario_selection(
                &main,
                &mount,
                &fields,
                &ResolveScenarioSelectionOptions {
                    // An EMPTY `projectId` is falsy too — v4's `if (!projectId)`.
                    project_id: Some(""),
                    character: None,
                    log_tag: None,
                },
            );
            assert_eq!(
                got.expect("a falsy pointer must be skipped before any DB read"),
                None,
                "fields {fields:?} resolved something"
            );
        }
    }

    /// The character tier's falsy guard, and the falsy-`content` fall-through —
    /// both v4 arms that its own `CharacterScenario` schema (`id` a uuid,
    /// `content` `min(1)`) makes unreachable through v4's write path, so they
    /// are pinned here the way P4.D112 pinned its unreachable boundary escapes.
    #[test]
    fn empty_scenario_id_and_empty_content_are_both_falsy() {
        let (main, mount) = (bare(), bare());
        let character = json!({
            "id": "char-1",
            "scenarios": [
                { "id": "", "content": "an empty id must never be matched" },
                { "id": "s-blank", "content": "" },
            ],
        });
        fn opts(c: &serde_json::Value) -> ResolveScenarioSelectionOptions<'_> {
            ResolveScenarioSelectionOptions {
                project_id: None,
                character: Some(c),
                log_tag: None,
            }
        }
        // An empty `scenarioId` is falsy: the tier is skipped, not matched.
        assert_eq!(
            resolve_scenario_selection(
                &main,
                &mount,
                &ScenarioSelectionFields {
                    scenario_id: Some(""),
                    ..Default::default()
                },
                &opts(&character),
            )
            .unwrap(),
            None
        );
        // A match whose `content` is empty leaves `presetBody` FALSY, so the
        // free text still lands on its own rather than beneath a blank preset…
        assert_eq!(
            resolve_scenario_selection(
                &main,
                &mount,
                &ScenarioSelectionFields {
                    scenario_id: Some("s-blank"),
                    scenario: Some("Notes survive."),
                    ..Default::default()
                },
                &opts(&character),
            )
            .unwrap()
            .as_deref(),
            Some("Notes survive.")
        );
        // …and, the part an `Option<String>` model would get WRONG: a falsy
        // preset means the NEXT tier still gets its turn. Over a table-less
        // connection the project tier's read is an `Err`, and that error IS the
        // evidence — a model treating the empty match as "found" would skip the
        // read and answer `Ok`. (The general tier is no use here: its
        // `instance_settings` read tolerates a missing table by design.)
        assert!(
            resolve_scenario_selection(
                &main,
                &mount,
                &ScenarioSelectionFields {
                    scenario_id: Some("s-blank"),
                    project_scenario_path: Some("Scenarios/p.md"),
                    ..Default::default()
                },
                &ResolveScenarioSelectionOptions {
                    project_id: Some("proj-1"),
                    character: Some(&character),
                    log_tag: None,
                },
            )
            .is_err(),
            "an empty-`content` match must NOT stop the chain"
        );
    }

    /// v4's default `logTag` exists and is `'[Scenario]'`; both real call sites
    /// override it with `'[Chats v1]'`.
    #[test]
    fn default_log_tag_is_v4s() {
        assert_eq!(DEFAULT_LOG_TAG, "[Scenario]");
    }
}
