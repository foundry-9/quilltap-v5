//! Scenario selection — the one place a chosen scenario becomes the chat's
//! recipe.
//!
//! A differential port of v4 `lib/chat/scenario-selection.ts` (`545e299bb`,
//! pinned at `83334b9a5`).
//!
//! Both the New Chat dialog and the in-chat scenario picker offer presets from
//! every kind of owner, plus free-text notes. This module owns the precedence
//! chain and turns each kind of pointer into a scenario read through the
//! scenario location ([`crate::scenarios::location`]):
//!
//!   `scenarioRef` (a browser pick from anywhere)
//!     > character `scenarioId` > project path > group path > general path
//!
//! The free text is NOT part of that chain — whatever the chain resolves is the
//! preset, and the notes are layered beneath it by
//! [`combine_scenario_text`](crate::scenario_text::combine_scenario_text).
//!
//! The result is a RECIPE ([`ResolvedScenario`]): `scenarioText` (the one thing
//! the prompt reads), `scenarioSource` (which file the preset came from and the
//! hash of its body) and `scenarioNotes`. Callers write all three together and
//! nothing else writes them. With no preset the text is the notes alone and
//! there is no source — a custom scene is wholly its text, with nothing to
//! reload (and its notes are NOT stored as `scenarioNotes`).
//!
//! Every legacy kind fails soft: an unresolvable pointer logs a warning and
//! falls through to the next. A `scenarioRef` that is malformed or names a
//! mount that is not a scenario owner is REFUSED outright
//! ([`ScenarioSelectionError::Refused`], a 400 `Invalid scenarioRef: …` at both
//! callers); one whose file has gone missing fails soft like the rest. A path
//! with a malformed `%` escape THROWS (V8's `URIError` through the location's
//! decode — R-Z), which its callers answer as v4's route does.
//!
//! The character arm reads the vault THROUGH THE LOCATION (`list({
//! includeArchived: true })` + `idForPath`), so an ARCHIVED or another user's
//! character's scenario no longer resolves (Δ21 — pre-span v4 read the
//! hydrated `character.scenarios`).
//!
//! ## JS truthiness, reproduced
//!
//! v4 guards every kind with `if (!preset && fields.<pointer>)` and its owner
//! checks with `!ownerId` — an EMPTY STRING is falsy, so `scenarioId: ''` (or an
//! empty path, or an empty `projectId`) is treated as absent and the chain falls
//! through, before any read.
//!
//! ## The two callers
//!
//! [`chat_create`](crate::services::chat_create) and
//! [`chat_scenario`](crate::services::chat_scenario)'s `?action=scenario` verb,
//! both with `log_tag = "[Chats v1]"`, exactly as v4's two call sites.
//!
//! Pinned by `chat_scenario_routes_equivalence` (its `resolver_*` rows drive
//! v4's REAL `resolveScenarioSelection`), `scenario_resolvers_equivalence` (the
//! per-kind reads) and `chat_create_capstone_equivalence`.

use rusqlite::Connection;

use crate::db::chats::{ScenarioRef, ScenarioSource};
use crate::db::scenarios::{ParsedScenario, ScenarioPathResolution};
use crate::jsstr::js_trim;
use crate::scenario_text::combine_scenario_text;
use crate::scenarios::location::{
    lookup_scenario_ref, normalize_scenario_path, resolve_scenario_location, ScenarioLocation,
    ScenarioLocationError, ScenarioRefLookup, ScenarioScope,
};

/// v4 `ResolvedScenario` (`scenario-selection.ts:77-81`): the RECIPE that lands
/// on the chat — `scenarioText` (the one thing the prompt reads),
/// `scenarioSource` (which file the preset came from and the hash of its body)
/// and `scenarioNotes`. Callers write all three together and nothing else
/// writes them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedScenario {
    pub scenario_text: Option<String>,
    pub scenario_source: Option<ScenarioSource>,
    pub scenario_notes: Option<String>,
}

/// v4 `hashScenarioBody(body)` (`scenario-selection.ts:92-94`): lowercase hex
/// SHA-256 of the body's UTF-8 bytes — the `bodySha256` of a source. The body
/// is the PARSED one (trimmed; frontmatter and, for a character file, the
/// `# heading` title line removed), as the location's `read` returns it (L-11).
pub fn hash_scenario_body(body: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(body.as_bytes()))
}

/// v4 `sameResolvedScenario(a, b)` (`scenario-selection.ts:97-104`): true when
/// two recipes would leave a chat byte-identical — text and notes `===`, and
/// the sources both absent or equal on `mountPointId`, `path` and `bodySha256`.
pub fn same_resolved_scenario(a: &ResolvedScenario, b: &ResolvedScenario) -> bool {
    let same_source = match (&a.scenario_source, &b.scenario_source) {
        (None, None) => true,
        (Some(sa), Some(sb)) => {
            sa.mount_point_id == sb.mount_point_id
                && sa.path == sb.path
                && sa.body_sha256 == sb.body_sha256
        }
        _ => false,
    };
    a.scenario_text == b.scenario_text && a.scenario_notes == b.scenario_notes && same_source
}

/// v4's default `logTag`. Both real call sites override it with `"[Chats v1]"`,
/// but the default is v4's and is reachable through
/// [`ResolveScenarioSelectionOptions::log_tag`]'s `None`.
pub const DEFAULT_LOG_TAG: &str = "[Scenario]";

/// v4 `ScenarioSelectionFields` — the scenario fields as they arrive from a
/// client. The New Chat dialog's create payload and the in-chat picker's
/// `?action=scenario` body use the same names, so the same resolver serves
/// both.
#[derive(Debug, Default, Clone, Copy)]
pub struct ScenarioSelectionFields<'a> {
    /// Free-text notes. Appended beneath a resolved preset, or used alone.
    pub scenario: Option<&'a str>,
    /// A scenario anywhere — the browser's pick. Most specific, so first.
    pub scenario_ref: Option<&'a ScenarioRef>,
    /// A character scenario's id (path-derived), looked up in the character's vault.
    pub scenario_id: Option<&'a str>,
    /// `Scenarios/<file>.md` inside the project's official store. Needs `project_id`.
    pub project_scenario_path: Option<&'a str>,
    /// `Scenarios/<file>.md` inside a group's official store. Needs `group_scenario_group_id`.
    pub group_scenario_path: Option<&'a str>,
    pub group_scenario_group_id: Option<&'a str>,
    /// `Scenarios/<file>.md` inside the instance-wide "Quilltap General" mount.
    pub general_scenario_path: Option<&'a str>,
}

/// v4 `ResolveScenarioSelectionOptions` (`scenario-selection.ts:66-75`). The
/// pre-span `character` (a hydrated row) became `characterId`, and `userId` is
/// NEW and required (the character location is the caller's characters only).
#[derive(Debug, Clone, Copy)]
pub struct ResolveScenarioSelectionOptions<'a> {
    pub user_id: &'a str,
    /// Required for `project_scenario_path` to resolve.
    pub project_id: Option<&'a str>,
    /// The character whose vault backs a `scenario_id` lookup.
    pub character_id: Option<&'a str>,
    /// Log prefix, so warnings read as coming from the calling route.
    /// `None` → [`DEFAULT_LOG_TAG`].
    pub log_tag: Option<&'a str>,
}

/// The resolver's two failures.
#[derive(Debug)]
pub enum ScenarioSelectionError {
    /// v4 `ScenarioRefRefusedError(reason)` (`scenario-selection.ts:84-89`) — a
    /// `scenarioRef` that can never resolve. `Display` = `Invalid scenarioRef:
    /// <reason>`, the 400 both callers answer.
    Refused(String),
    /// Anything the location throws (a malformed `%` escape's `URIError`, a
    /// failed read) — v4 lets it propagate to the route.
    Location(ScenarioLocationError),
}

impl std::fmt::Display for ScenarioSelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScenarioSelectionError::Refused(reason) => write!(f, "Invalid scenarioRef: {reason}"),
            ScenarioSelectionError::Location(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ScenarioSelectionError {}

impl From<ScenarioLocationError> for ScenarioSelectionError {
    fn from(e: ScenarioLocationError) -> Self {
        ScenarioSelectionError::Location(e)
    }
}

/// A resolved preset: where it lives and what it reads.
struct Preset {
    location: ScenarioLocation,
    scenario: ParsedScenario,
}

/// JS truthiness for a string field: `undefined`, `null` and `''` are all
/// "absent" (the module header's note).
fn truthy(v: Option<&str>) -> Option<&str> {
    v.filter(|s| !s.is_empty())
}

/// The path field a `readFromScope` line names (v4 keys its metadata by the
/// FIELD — `{ [field]: rawPath }`, a tracing field NAME must be static).
#[derive(Clone, Copy)]
enum PathField {
    Project,
    Group,
    General,
}

impl PathField {
    fn name(self) -> &'static str {
        match self {
            PathField::Project => "projectScenarioPath",
            PathField::Group => "groupScenarioPath",
            PathField::General => "generalScenarioPath",
        }
    }
}

/// One `readFromScope` line: `{log_tag} {field} <what>` with the field keyed by
/// its own name, the other fields as given (v4's metadata key order).
macro_rules! field_line {
    ($field:expr, $raw:expr; $($pre:ident = $pv:expr),* ; $($post:ident = $qv:expr),* ; $($msg:tt)+) => {{
        let raw: &str = $raw;
        match $field {
            PathField::Project => tracing::warn!($($pre = $pv,)* projectScenarioPath = raw, $($post = $qv,)* $($msg)+),
            PathField::Group => tracing::warn!($($pre = $pv,)* groupScenarioPath = raw, $($post = $qv,)* $($msg)+),
            PathField::General => tracing::warn!($($pre = $pv,)* generalScenarioPath = raw, $($post = $qv,)* $($msg)+),
        }
    }};
}

/// v4 `readFromScope(scope, ownerId, rawPath, options, field)`
/// (`scenario-selection.ts:115-146`): read one owner's scenario by path. Every
/// miss logs a warning and returns `None` so the chain falls through.
fn read_from_scope(
    main: &Connection,
    mount: &Connection,
    scope: ScenarioScope,
    owner_id: Option<&str>,
    raw_path: &str,
    options: &ResolveScenarioSelectionOptions<'_>,
    field: PathField,
) -> Result<Option<Preset>, ScenarioSelectionError> {
    let log_tag = options.log_tag.unwrap_or(DEFAULT_LOG_TAG);
    if scope != ScenarioScope::General && truthy(owner_id).is_none() {
        field_line!(field, raw_path; ; ; "{} {} provided without its owner; ignoring", log_tag, field.name());
        return Ok(None);
    }
    let normalized = match normalize_scenario_path(raw_path).map_err(ScenarioLocationError::from)? {
        ScenarioPathResolution::Ok(p) => p,
        ScenarioPathResolution::Err(reason) => {
            field_line!(field, raw_path; ; reason = reason.as_str();
                "{} {} is not a scenario path; ignoring", log_tag, field.name());
            return Ok(None);
        }
    };
    let Some(location) =
        resolve_scenario_location(main, mount, scope, owner_id, options.user_id, false)?
    else {
        field_line!(field, raw_path; ownerId = owner_id.unwrap_or("null"); ;
            "{} {} provided but its {} has no scenario store", log_tag, field.name(), scope.as_str());
        return Ok(None);
    };
    let Some(scenario) = location.read(mount, &normalized)? else {
        field_line!(field, raw_path; ownerId = owner_id.unwrap_or("null"); ;
            "{} {} did not resolve to a body", log_tag, field.name());
        return Ok(None);
    };
    Ok(Some(Preset { location, scenario }))
}

/// v4 `readCharacterScenarioById(scenarioId, options)`
/// (`scenario-selection.ts:149-167`): the character's location (the caller's,
/// live, vaulted), its list WITH archived, the scenario whose path-derived id
/// matches; a miss (no character, no location, no match) logs `scenarioId not
/// found on character`.
fn read_character_scenario_by_id(
    main: &Connection,
    mount: &Connection,
    scenario_id: &str,
    options: &ResolveScenarioSelectionOptions<'_>,
) -> Result<Option<Preset>, ScenarioSelectionError> {
    let log_tag = options.log_tag.unwrap_or(DEFAULT_LOG_TAG);
    let location = match truthy(options.character_id) {
        Some(character_id) => resolve_scenario_location(
            main,
            mount,
            ScenarioScope::Character,
            Some(character_id),
            options.user_id,
            false,
        )?,
        None => None,
    };
    if let Some(location) = location {
        let listed = location.list(mount, true)?;
        if let Some(scenario) = listed
            .scenarios
            .into_iter()
            .find(|s| location.id_for_path(&s.path).as_deref() == Some(scenario_id))
        {
            return Ok(Some(Preset { location, scenario }));
        }
    }
    tracing::warn!(
        characterId = options.character_id.unwrap_or("null"),
        scenarioId = scenario_id,
        "{log_tag} scenarioId not found on character"
    );
    Ok(None)
}

/// v4 `resolveScenarioSelection(fields, options)`
/// (`scenario-selection.ts:173-232`): resolve a selection into the recipe that
/// lands on the chat. Refuses a `scenarioRef` that can never resolve.
pub fn resolve_scenario_selection(
    main: &Connection,
    mount: &Connection,
    fields: &ScenarioSelectionFields<'_>,
    options: &ResolveScenarioSelectionOptions<'_>,
) -> Result<ResolvedScenario, ScenarioSelectionError> {
    let log_tag = options.log_tag.unwrap_or(DEFAULT_LOG_TAG);
    let mut preset: Option<Preset> = None;

    if let Some(scenario_ref) = fields.scenario_ref {
        match lookup_scenario_ref(main, mount, scenario_ref, options.user_id)? {
            ScenarioRefLookup::Refused(reason) => {
                return Err(ScenarioSelectionError::Refused(reason))
            }
            ScenarioRefLookup::Ok { location, scenario } => {
                preset = Some(Preset { location, scenario })
            }
            // `{ ...fields.scenarioRef }` — the parsed ref's own key order.
            ScenarioRefLookup::Missing { .. } => tracing::warn!(
                mountPointId = %scenario_ref.mount_point_id,
                path = %scenario_ref.path,
                "{log_tag} scenarioRef points at a missing file"
            ),
        }
    }

    if preset.is_none() {
        if let Some(scenario_id) = truthy(fields.scenario_id) {
            preset = read_character_scenario_by_id(main, mount, scenario_id, options)?;
        }
    }
    if preset.is_none() {
        if let Some(path) = truthy(fields.project_scenario_path) {
            preset = read_from_scope(
                main,
                mount,
                ScenarioScope::Project,
                options.project_id,
                path,
                options,
                PathField::Project,
            )?;
        }
    }
    if preset.is_none() {
        if let Some(path) = truthy(fields.group_scenario_path) {
            preset = read_from_scope(
                main,
                mount,
                ScenarioScope::Group,
                fields.group_scenario_group_id,
                path,
                options,
                PathField::Group,
            )?;
        }
    }
    if preset.is_none() {
        if let Some(path) = truthy(fields.general_scenario_path) {
            preset = read_from_scope(
                main,
                mount,
                ScenarioScope::General,
                None,
                path,
                options,
                PathField::General,
            )?;
        }
    }

    // v4 `fields.scenario?.trim() ? fields.scenario.trim() : null` — JS trim.
    let notes = fields
        .scenario
        .map(js_trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string);

    let Some(preset) = preset else {
        // No preset: the notes ARE the scenario, and there is nothing to reload.
        return Ok(ResolvedScenario {
            scenario_text: notes,
            scenario_source: None,
            scenario_notes: None,
        });
    };

    let body = &preset.scenario.body;
    let result = ResolvedScenario {
        scenario_text: combine_scenario_text(Some(body), notes.as_deref()),
        scenario_source: Some(ScenarioSource {
            mount_point_id: preset.location.mount_point_id().to_string(),
            path: preset.scenario.path.clone(),
            body_sha256: hash_scenario_body(body),
        }),
        scenario_notes: notes.clone(),
    };
    tracing::debug!(
        scope = preset.location.scope().as_str(),
        ownerId = preset.location.id().unwrap_or("null"),
        mountPointId = preset.location.mount_point_id(),
        path = %preset.scenario.path,
        hasNotes = notes.is_some(),
        "{log_tag} Resolved scenario selection"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A connection with NO tables. Any kind that actually QUERIES fails on it,
    /// which is what makes the JS-truthiness guards measurable: a falsy pointer
    /// must be skipped BEFORE the read, so the resolver still answers `Ok`.
    fn bare() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    /// The module header's JS-truthiness contract, pinned where the differential
    /// corpus cannot see it: every empty-string pointer — and an empty
    /// `projectId` / `groupScenarioGroupId` — produces the SAME output as an
    /// absent one, before any read.
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
                scenario_id: Some(""),
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
                    user_id: "u",
                    // An EMPTY `projectId` is falsy too — v4's `if (!ownerId)`.
                    project_id: Some(""),
                    character_id: None,
                    log_tag: None,
                },
            );
            assert_eq!(
                got.expect("a falsy pointer must be skipped before any DB read"),
                ResolvedScenario::default(),
                "fields {fields:?} resolved something"
            );
        }
    }

    /// Notes under no preset are the WHOLE scene, never `scenarioNotes` (Δ24);
    /// whitespace-only notes are nothing.
    #[test]
    fn a_custom_scene_is_its_text_alone() {
        let (main, mount) = (bare(), bare());
        let opts = ResolveScenarioSelectionOptions {
            user_id: "u",
            project_id: None,
            character_id: None,
            log_tag: None,
        };
        let got = resolve_scenario_selection(
            &main,
            &mount,
            &ScenarioSelectionFields {
                scenario: Some("  A rooftop.  "),
                ..Default::default()
            },
            &opts,
        )
        .unwrap();
        assert_eq!(
            got,
            ResolvedScenario {
                scenario_text: Some("A rooftop.".into()),
                scenario_source: None,
                scenario_notes: None,
            }
        );
        let blank = resolve_scenario_selection(
            &main,
            &mount,
            &ScenarioSelectionFields {
                scenario: Some("\u{FEFF} "),
                ..Default::default()
            },
            &opts,
        )
        .unwrap();
        assert_eq!(blank, ResolvedScenario::default());
    }
}
