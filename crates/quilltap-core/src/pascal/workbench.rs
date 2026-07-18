//! Pascal's Workbench — server-side helpers behind `/api/v1/custom-tools`
//! (v4 `lib/pascal/workbench.ts`).
//!
//! The Workbench is the authoring surface for custom tools: a library of every
//! definition in every enabled store (no shadowing, no per-invoker perspective —
//! the whole table, face up), and a destination list for saving a definition
//! anywhere a tool can live. The chat-facing roster logic stays in
//! [`super::roster`]; this module only adds the authoring-time views over the
//! same loaders.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use super::custom_tool_types::{display_title, Roll, Visibility};
use super::roster::list_all_custom_tools;
use crate::db::doc_mount_points::{DmpRow, DocMountPointsRepository};
use crate::db::document_store_overlay::OverlayError;
use crate::db::group_doc_mount_links::GroupDocMountLinksRepository;
use crate::db::groups::GroupsRepository;
use crate::db::instance_settings::get_general_mount_point_id;
use crate::db::project_doc_mount_links::ProjectDocMountLinksRepository;
use crate::db::projects::ProjectsRepository;
use crate::db::{characters_read, DbError};

/// What a store is attached to. One store can carry several of these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MountAttachment {
    pub kind: AttachmentKind,
    /// The attached entity's id — absent for `general` and `unattached`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Human-readable badge text: the project/group/character name, etc.
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    General,
    Project,
    Group,
    Character,
    Unattached,
}

/// A valid definition, as the library lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomToolLibraryEntry {
    /// Always `true` — the discriminant the SPA branches on.
    pub valid: bool,
    pub name: String,
    pub title: String,
    pub description: String,
    pub disabled: bool,
    pub default_visibility: Visibility,
    pub roll_form: RollForm,
    pub parameter_count: usize,
    pub outcome_count: usize,
    pub mount_point_id: String,
    pub mount_name: String,
    pub definition_path: String,
    pub attachments: Vec<MountAttachment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RollForm {
    Range,
    Dice,
}

/// A broken definition file, listed rather than hidden.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomToolLibraryError {
    /// Always `false`.
    pub valid: bool,
    pub definition_path: String,
    pub mount_point_id: String,
    pub mount_name: String,
    pub reason: String,
    pub attachments: Vec<MountAttachment>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CustomToolLibraryResponse {
    pub tools: Vec<CustomToolLibraryEntry>,
    pub errors: Vec<CustomToolLibraryError>,
}

/// A save target, with the names already on that store's table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationStore {
    pub mount_point_id: String,
    pub mount_name: String,
    /// Names already defined in this store — a duplicate would be a load-time
    /// rejection, so the picker can warn before the write rather than after.
    pub existing_tool_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupDestinationStore {
    #[serde(flatten)]
    pub store: DestinationStore,
    pub official: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDestinations {
    pub project_id: String,
    pub project_name: String,
    pub stores: Vec<DestinationStore>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupDestinations {
    pub group_id: String,
    pub group_name: String,
    pub stores: Vec<GroupDestinationStore>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterDestination {
    pub character_id: String,
    pub character_name: String,
    #[serde(flatten)]
    pub store: DestinationStore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomToolDestinations {
    /// The General store, or `None` when unprovisioned.
    pub general: Option<DestinationStore>,
    pub projects: Vec<ProjectDestinations>,
    pub groups: Vec<GroupDestinations>,
    pub characters: Vec<CharacterDestination>,
    /// Enabled stores attached to nothing — inert until linked.
    pub other: Vec<DestinationStore>,
}

/// The raw attachment survey the library and the destination list both read:
/// who claims which mount, resolved once per request, never cached.
///
/// The three collections are `Vec`s rather than maps because v4 iterates JS
/// `Map`s, whose iteration order is INSERTION order — and that order reaches the
/// wire in `attachmentsForMount`'s badge sequence.
struct AttachmentSurvey {
    general_id: Option<String>,
    /// Every project WITH links, in repo order.
    projects: Vec<SurveyedProject>,
    /// Every group with links or an official store, in repo order.
    groups: Vec<SurveyedGroup>,
    /// mountPointId → owning character, for characters WITH a vault.
    character_by_mount: Vec<(String, SurveyedCharacter)>,
}

struct SurveyedProject {
    id: String,
    name: String,
    mount_point_ids: Vec<String>,
}

struct SurveyedGroup {
    id: String,
    name: String,
    official_mount_point_id: Option<String>,
    mount_point_ids: Vec<String>,
}

#[derive(Clone)]
struct SurveyedCharacter {
    character_id: String,
    character_name: String,
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn opt_str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// The store-backed repos speak `OverlayError`; the Workbench speaks `DbError`.
/// Flattened exactly as the groups/projects API surfaces already do.
fn flatten_overlay(e: OverlayError) -> DbError {
    match e {
        OverlayError::Db(d) => d,
        OverlayError::Unavailable { detail, .. } => DbError::Key(detail),
    }
}

fn survey_attachments(main: &Connection, mount: &Connection) -> Result<AttachmentSurvey, DbError> {
    let general_id = get_general_mount_point_id(main)?;

    let project_links = ProjectDocMountLinksRepository::new(mount);
    let mut projects = Vec::new();
    for project in ProjectsRepository::new(main, mount)
        .find_all()
        .map_err(flatten_overlay)?
    {
        let id = str_field(&project, "id");
        let mount_point_ids = project_links.find_by_project_id(&id)?;
        if mount_point_ids.is_empty() {
            continue;
        }
        projects.push(SurveyedProject {
            name: str_field(&project, "name"),
            id,
            mount_point_ids,
        });
    }

    let group_links = GroupDocMountLinksRepository::new(mount);
    let mut groups = Vec::new();
    for group in GroupsRepository::new(main, mount)
        .find_all()
        .map_err(flatten_overlay)?
    {
        let id = str_field(&group, "id");
        let mount_point_ids = group_links.find_by_group_id(&id)?;
        let official_mount_point_id = opt_str_field(&group, "officialMountPointId");
        if mount_point_ids.is_empty() && official_mount_point_id.is_none() {
            continue;
        }
        groups.push(SurveyedGroup {
            name: str_field(&group, "name"),
            id,
            official_mount_point_id,
            mount_point_ids,
        });
    }

    // Raw read: only `characterDocumentMountPointId` and `name` are needed, and
    // the hydrating overlay would drop characters whose vault is briefly broken —
    // exactly the stores an author may be trying to repair.
    let mut character_by_mount: Vec<(String, SurveyedCharacter)> = Vec::new();
    for character in characters_read::find_all_raw(main)? {
        let Some(mount_id) = opt_str_field(&character, "characterDocumentMountPointId") else {
            continue;
        };
        let entry = SurveyedCharacter {
            character_id: str_field(&character, "id"),
            character_name: str_field(&character, "name"),
        };
        // v4 keys a Map by mount id: a later character with the same vault
        // REPLACES the earlier one in place.
        match character_by_mount.iter_mut().find(|(m, _)| *m == mount_id) {
            Some(slot) => slot.1 = entry,
            None => character_by_mount.push((mount_id, entry)),
        }
    }

    Ok(AttachmentSurvey {
        general_id,
        projects,
        groups,
        character_by_mount,
    })
}

/// Every attachment one mount carries, in a stable display order.
fn attachments_for_mount(mount_point_id: &str, survey: &AttachmentSurvey) -> Vec<MountAttachment> {
    let mut attachments = Vec::new();

    if survey.general_id.as_deref() == Some(mount_point_id) {
        attachments.push(MountAttachment {
            kind: AttachmentKind::General,
            id: None,
            label: "General".to_string(),
        });
    }

    if let Some((_, character)) = survey
        .character_by_mount
        .iter()
        .find(|(m, _)| m == mount_point_id)
    {
        attachments.push(MountAttachment {
            kind: AttachmentKind::Character,
            id: Some(character.character_id.clone()),
            label: character.character_name.clone(),
        });
    }

    for group in &survey.groups {
        if group.official_mount_point_id.as_deref() == Some(mount_point_id)
            || group.mount_point_ids.iter().any(|m| m == mount_point_id)
        {
            attachments.push(MountAttachment {
                kind: AttachmentKind::Group,
                id: Some(group.id.clone()),
                label: group.name.clone(),
            });
        }
    }

    for project in &survey.projects {
        if project.mount_point_ids.iter().any(|m| m == mount_point_id) {
            attachments.push(MountAttachment {
                kind: AttachmentKind::Project,
                id: Some(project.id.clone()),
                label: project.name.clone(),
            });
        }
    }

    if attachments.is_empty() {
        attachments.push(MountAttachment {
            kind: AttachmentKind::Unattached,
            id: None,
            label: "Unattached".to_string(),
        });
    }

    attachments
}

/// The library: every definition in every enabled store, valid or broken, with
/// attachment badges resolved per mount.
pub fn build_custom_tool_library(
    main: &Connection,
    mount: &Connection,
) -> Result<CustomToolLibraryResponse, DbError> {
    let library = list_all_custom_tools(mount);
    let survey = survey_attachments(main, mount)?;

    let mut cache: Vec<(String, Vec<MountAttachment>)> = Vec::new();
    let mut attachments_of = |mount_point_id: &str| -> Vec<MountAttachment> {
        if let Some((_, cached)) = cache.iter().find(|(m, _)| m == mount_point_id) {
            return cached.clone();
        }
        let resolved = attachments_for_mount(mount_point_id, &survey);
        cache.push((mount_point_id.to_string(), resolved.clone()));
        resolved
    };

    let tools: Vec<CustomToolLibraryEntry> = library
        .entries
        .iter()
        .map(|entry| CustomToolLibraryEntry {
            valid: true,
            name: entry.definition.name.clone(),
            title: display_title(&entry.definition.name, entry.definition.title.as_deref()),
            description: entry.definition.description.clone(),
            disabled: entry.definition.disabled.unwrap_or(false),
            default_visibility: entry
                .definition
                .default_visibility
                .unwrap_or(Visibility::Public),
            roll_form: match entry.definition.roll {
                Some(Roll::Dice(_)) => RollForm::Dice,
                _ => RollForm::Range,
            },
            parameter_count: entry
                .definition
                .parameters
                .as_ref()
                .map(|p| p.len())
                .unwrap_or(0),
            outcome_count: entry.definition.outcomes.len(),
            mount_point_id: entry.mount_point_id.clone(),
            mount_name: entry.mount_name.clone(),
            definition_path: entry.definition_path.clone(),
            attachments: attachments_of(&entry.mount_point_id),
        })
        .collect();

    let errors: Vec<CustomToolLibraryError> = library
        .errors
        .iter()
        .map(|error| CustomToolLibraryError {
            valid: false,
            definition_path: error.definition_path.clone(),
            mount_point_id: error.mount_point_id.clone(),
            mount_name: error.mount_name.clone(),
            reason: error.reason.clone(),
            attachments: attachments_of(&error.mount_point_id),
        })
        .collect();

    Ok(CustomToolLibraryResponse { tools, errors })
}

/// The save-target list: every enabled store a definition can be written to,
/// grouped by what the store is attached to, with the tool names each already
/// carries (a same-store duplicate `name` is a load-time rejection, so the
/// picker warns before the write rather than after).
pub fn list_custom_tool_destinations(
    main: &Connection,
    mount: &Connection,
) -> Result<CustomToolDestinations, DbError> {
    let mounts: Vec<DmpRow> = DocMountPointsRepository::new(mount).find_enabled_for_docedit()?;
    let survey = survey_attachments(main, mount)?;
    let library = list_all_custom_tools(mount);

    let mut names_by_mount: Vec<(String, Vec<String>)> = Vec::new();
    for entry in &library.entries {
        match names_by_mount
            .iter_mut()
            .find(|(m, _)| *m == entry.mount_point_id)
        {
            Some((_, names)) => names.push(entry.definition.name.clone()),
            None => names_by_mount.push((
                entry.mount_point_id.clone(),
                vec![entry.definition.name.clone()],
            )),
        }
    }

    let find_mount = |id: &str| mounts.iter().find(|m| m.id == id);
    let store = |row: &DmpRow| DestinationStore {
        mount_point_id: row.id.clone(),
        mount_name: row.name.clone(),
        existing_tool_names: names_by_mount
            .iter()
            .find(|(m, _)| *m == row.id)
            .map(|(_, names)| names.clone())
            .unwrap_or_default(),
    };

    let mut claimed: Vec<String> = Vec::new();

    let mut general = None;
    if let Some(general_id) = &survey.general_id {
        if let Some(row) = find_mount(general_id) {
            general = Some(store(row));
            claimed.push(row.id.clone());
        }
    }

    let mut characters: Vec<CharacterDestination> = Vec::new();
    for (mount_point_id, owner) in &survey.character_by_mount {
        let Some(row) = find_mount(mount_point_id) else {
            continue;
        };
        characters.push(CharacterDestination {
            character_id: owner.character_id.clone(),
            character_name: owner.character_name.clone(),
            store: store(row),
        });
        claimed.push(mount_point_id.clone());
    }
    // v4 sorts with `localeCompare`; the code-unit order matches for the ASCII/BMP
    // names the corpus carries (the established `[[vault-yaml-icu-decisions]]`
    // code-unit comparator).
    characters.sort_by(|a, b| a.character_name.cmp(&b.character_name));

    let mut groups: Vec<GroupDestinations> = Vec::new();
    for group in &survey.groups {
        let mut stores: Vec<GroupDestinationStore> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        let candidates = group
            .official_mount_point_id
            .iter()
            .chain(group.mount_point_ids.iter());
        for mount_point_id in candidates {
            if seen.iter().any(|s| s == mount_point_id) {
                continue;
            }
            seen.push(mount_point_id.clone());
            let Some(row) = find_mount(mount_point_id) else {
                continue;
            };
            stores.push(GroupDestinationStore {
                store: store(row),
                official: group.official_mount_point_id.as_deref() == Some(mount_point_id.as_str()),
            });
            claimed.push(mount_point_id.clone());
        }
        if !stores.is_empty() {
            groups.push(GroupDestinations {
                group_id: group.id.clone(),
                group_name: group.name.clone(),
                stores,
            });
        }
    }
    groups.sort_by(|a, b| a.group_name.cmp(&b.group_name));

    let mut projects: Vec<ProjectDestinations> = Vec::new();
    for project in &survey.projects {
        let mut stores: Vec<DestinationStore> = Vec::new();
        for mount_point_id in &project.mount_point_ids {
            let Some(row) = find_mount(mount_point_id) else {
                continue;
            };
            stores.push(store(row));
            claimed.push(mount_point_id.clone());
        }
        if !stores.is_empty() {
            projects.push(ProjectDestinations {
                project_id: project.id.clone(),
                project_name: project.name.clone(),
                stores,
            });
        }
    }
    projects.sort_by(|a, b| a.project_name.cmp(&b.project_name));

    let mut other: Vec<DestinationStore> = mounts
        .iter()
        .filter(|row| !claimed.contains(&row.id))
        .map(&store)
        .collect();
    other.sort_by(|a, b| a.mount_name.cmp(&b.mount_name));

    Ok(CustomToolDestinations {
        general,
        projects,
        groups,
        characters,
        other,
    })
}
