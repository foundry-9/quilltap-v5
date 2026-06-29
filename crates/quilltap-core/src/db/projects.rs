//! The `projects` repository — the second **store-backed** entity, reusing the
//! generic [`super::store_backed::StoreBackedRepository`] bound to
//! [`ProjectEntity`] (v4's `ProjectsRepository`). Structurally identical to
//! `groups`; the deltas are the **16-key `properties.json` bag** (vs 2 for
//! groups) and the **character-roster operations** layered on top.
//!
//! Like groups, a project's substantive content does NOT live in `projects`
//! columns. The slim row (id/name/officialMountPointId/timestamps) lives in the
//! MAIN db; `description`/`instructions`/`state` + the `ProjectPropertiesSchema`
//! bag live in the project's official store as the four overlay files. The
//! roster (`characterRoster` / `allowAnyCharacter`) lives in `properties.json`,
//! so the roster ops read the hydrated project and write back through `update()`
//! (which routes the change to the store) — exactly v4's design.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::document_store_overlay::{ManagedFields, OverlayError, StoreEntity};
use super::project_doc_mount_links::ProjectDocMountLinksRepository;
use super::store_backed::StoreBackedRepository;
use super::DbError;

pub use super::store_backed::StoreCreateOptions as ProjectCreateOptions;

fn default_background_display_mode() -> String {
    "theme".to_string()
}

/// The `properties.json` bag (v4 `ProjectPropertiesSchema`), serialized in
/// schema-declaration order. Five fields carry Zod `.default(...)` and are
/// therefore **always materialized** (`allowAnyCharacter`, `characterRoster`,
/// `defaultDisabledTools`, `defaultDisabledToolGroups`, `backgroundDisplayMode`);
/// the rest are `.nullable().optional()` → `skip_serializing_if` so an absent key
/// stays absent. This matches `JSON.stringify(parse(x), null, 2)` byte-for-byte
/// (the dedup sha depends on it). The null-vs-absent distinction on the optional
/// keys is the open-JSON seam (serde folds `null`→`None`); the corpus keeps them
/// present-or-absent.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectProperties {
    #[serde(default, rename = "allowAnyCharacter")]
    pub allow_any_character: bool,
    #[serde(default, rename = "characterRoster")]
    pub character_roster: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, rename = "defaultDisabledTools")]
    pub default_disabled_tools: Vec<String>,
    #[serde(default, rename = "defaultDisabledToolGroups")]
    pub default_disabled_tool_groups: Vec<String>,
    #[serde(
        default,
        rename = "defaultAgentModeEnabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_agent_mode_enabled: Option<bool>,
    #[serde(
        default,
        rename = "defaultAvatarGenerationEnabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_avatar_generation_enabled: Option<bool>,
    #[serde(
        default,
        rename = "defaultImageProfileId",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_image_profile_id: Option<String>,
    #[serde(
        default,
        rename = "defaultRoleplayTemplateId",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_roleplay_template_id: Option<String>,
    #[serde(
        default,
        rename = "defaultAlertCharactersOfLanternImages",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_alert_characters_of_lantern_images: Option<bool>,
    #[serde(
        default,
        rename = "storyBackgroundsEnabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub story_backgrounds_enabled: Option<bool>,
    #[serde(
        default,
        rename = "staticBackgroundImageId",
        skip_serializing_if = "Option::is_none"
    )]
    pub static_background_image_id: Option<String>,
    #[serde(
        default,
        rename = "storyBackgroundImageId",
        skip_serializing_if = "Option::is_none"
    )]
    pub story_background_image_id: Option<String>,
    #[serde(
        default = "default_background_display_mode",
        rename = "backgroundDisplayMode"
    )]
    pub background_display_mode: String,
}

/// The project's [`StoreEntity`] binding for the generic engine + base repository.
pub struct ProjectEntity;

impl StoreEntity for ProjectEntity {
    type Properties = ProjectProperties;

    fn entity_label() -> &'static str {
        "project"
    }

    fn property_keys() -> &'static [&'static str] {
        &[
            "allowAnyCharacter",
            "characterRoster",
            "color",
            "icon",
            "defaultDisabledTools",
            "defaultDisabledToolGroups",
            "defaultAgentModeEnabled",
            "defaultAvatarGenerationEnabled",
            "defaultImageProfileId",
            "defaultRoleplayTemplateId",
            "defaultAlertCharactersOfLanternImages",
            "storyBackgroundsEnabled",
            "staticBackgroundImageId",
            "storyBackgroundImageId",
            "backgroundDisplayMode",
        ]
    }

    fn parse_properties(value: &Value) -> Result<ProjectProperties, String> {
        if !value.is_object() {
            return Err(format!("expected a JSON object, got: {value}"));
        }
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())
    }

    fn slim_table() -> &'static str {
        "projects"
    }

    fn store_name_prefix() -> &'static str {
        "Project Files: "
    }

    fn find_store_links(mount: &Connection, entity_id: &str) -> Result<Vec<String>, DbError> {
        ProjectDocMountLinksRepository::new(mount).find_by_project_id(entity_id)
    }

    fn link_store(
        mount: &Connection,
        entity_id: &str,
        mount_point_id: &str,
    ) -> Result<(), DbError> {
        ProjectDocMountLinksRepository::new(mount).link(entity_id, mount_point_id)
    }
}

/// Create payload for a project. `properties` is the property-bag subset as a
/// JSON object (the caller's hydrated fields minus name/description/instructions/
/// state); [`StoreEntity::parse_properties`] materializes the schema defaults
/// (mirrors v4's `prepareCreateData` seeding `allowAnyCharacter`/`characterRoster`
/// — the schema defaults make the seeding redundant, reproduced here for free).
pub struct ProjectCreateInput {
    pub name: String,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub state: Value,
    pub properties: Value,
}

/// The projects repository — the generic store-backed base + roster operations.
pub struct ProjectsRepository<'c> {
    inner: StoreBackedRepository<'c, ProjectEntity>,
}

impl<'c> ProjectsRepository<'c> {
    pub fn new(main: &'c Connection, mount: &'c Connection) -> Self {
        Self {
            inner: StoreBackedRepository::new(main, mount),
        }
    }

    /// Create a project, provision its store, and return the overlaid entity.
    pub fn create(
        &self,
        input: &ProjectCreateInput,
        opts: &ProjectCreateOptions,
    ) -> Result<Value, OverlayError> {
        self.inner.create(
            &input.name,
            &ManagedFields {
                properties: input.properties.clone(),
                description: input.description.clone(),
                instructions: input.instructions.clone(),
                state: input.state.clone(),
            },
            opts,
        )
    }

    /// Update a project (store-resident fields routed to the store; the DB-only
    /// remainder written to the slim row). `patch` is the partial entity as a map.
    pub fn update(
        &self,
        id: &str,
        patch: &Map<String, Value>,
    ) -> Result<Option<Value>, OverlayError> {
        self.inner.update(id, patch)
    }

    /// Find by id, hydrated (throws `Unavailable` if the store is missing).
    pub fn find_by_id(&self, id: &str) -> Result<Option<Value>, OverlayError> {
        self.inner.find_by_id(id)
    }

    /// Find all, each hydrated (drops a row whose store is unavailable).
    pub fn find_all(&self) -> Result<Vec<Value>, OverlayError> {
        self.inner.find_all()
    }

    /// Delete the slim row (the official store is orphaned).
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        self.inner.delete(id)
    }

    // ── character-roster operations (v4 `ProjectsRepository`) ─────────────────

    /// Add a character to the roster (v4 `addToRoster`): read the hydrated
    /// project, push if absent, write `characterRoster` back through `update`.
    /// Returns the updated (or unchanged) project, or `None` if not found.
    pub fn add_to_roster(
        &self,
        project_id: &str,
        character_id: &str,
    ) -> Result<Option<Value>, OverlayError> {
        let Some(project) = self.find_by_id(project_id)? else {
            return Ok(None);
        };
        let mut roster = roster_of(&project);
        if !roster.iter().any(|c| c == character_id) {
            roster.push(character_id.to_string());
            return self.update(project_id, &roster_patch(roster));
        }
        Ok(Some(project))
    }

    /// Remove a character from the roster (v4 `removeFromRoster`).
    pub fn remove_from_roster(
        &self,
        project_id: &str,
        character_id: &str,
    ) -> Result<Option<Value>, OverlayError> {
        let Some(project) = self.find_by_id(project_id)? else {
            return Ok(None);
        };
        let roster = roster_of(&project);
        let filtered: Vec<String> = roster
            .iter()
            .filter(|c| c.as_str() != character_id)
            .cloned()
            .collect();
        if filtered.len() != roster.len() {
            return self.update(project_id, &roster_patch(filtered));
        }
        Ok(Some(project))
    }

    /// Set the `allowAnyCharacter` flag (v4 `setAllowAnyCharacter`).
    pub fn set_allow_any_character(
        &self,
        project_id: &str,
        allow: bool,
    ) -> Result<Option<Value>, OverlayError> {
        let mut patch = Map::new();
        patch.insert("allowAnyCharacter".into(), Value::Bool(allow));
        self.update(project_id, &patch)
    }

    /// Whether a character may participate (v4 `canCharacterParticipate`):
    /// `allowAnyCharacter` OR the roster contains it. Missing project → `false`.
    pub fn can_character_participate(
        &self,
        project_id: &str,
        character_id: &str,
    ) -> Result<bool, OverlayError> {
        let Some(project) = self.find_by_id(project_id)? else {
            return Ok(false);
        };
        if project
            .get("allowAnyCharacter")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Ok(true);
        }
        Ok(roster_of(&project).iter().any(|c| c == character_id))
    }

    /// Find every project whose roster contains `character_id` (v4
    /// `findByCharacterId` — `characterRoster` is in the store now, so it lists
    /// all hydrated projects and filters in memory).
    pub fn find_by_character_id(&self, character_id: &str) -> Result<Vec<Value>, OverlayError> {
        Ok(self
            .find_all()?
            .into_iter()
            .filter(|p| roster_of(p).iter().any(|c| c == character_id))
            .collect())
    }
}

/// Read `characterRoster` off a hydrated project (absent/non-array → empty).
fn roster_of(project: &Value) -> Vec<String> {
    project
        .get("characterRoster")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A `{ characterRoster: [...] }` update patch.
fn roster_patch(roster: Vec<String>) -> Map<String, Value> {
    let mut patch = Map::new();
    patch.insert(
        "characterRoster".into(),
        Value::Array(roster.into_iter().map(Value::String).collect()),
    );
    patch
}
