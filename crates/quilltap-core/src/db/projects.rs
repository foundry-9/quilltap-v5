//! The `projects` repository — the second **store-backed** entity, reusing the
//! generic [`super::store_backed::StoreBackedRepository`] bound to
//! [`ProjectEntity`] (v4's `ProjectsRepository`). Structurally identical to
//! `groups`; the deltas are the **17-key `properties.json` bag** (vs 2 for
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

/// Background display modes retired in 4.9 (v4 `70505745a`,
/// `RETIRED_BACKGROUND_DISPLAY_MODES`). Both were offered in the UI and neither
/// ever worked: `'project'` read `storyBackgroundImageId`, which only the
/// `'latest_chat'` path ever wrote, and `'static'` read
/// `staticBackgroundImageId`, which nothing anywhere wrote — there was no upload
/// control and the field was not even accepted by the update schema. Projects
/// left in either mode are read as `'theme'`, which is the "no image" outcome
/// they were already producing.
pub const RETIRED_BACKGROUND_DISPLAY_MODES: [&str; 2] = ["project", "static"];

/// v4 `normalizeBackgroundDisplayMode`: coerce a stored background display mode,
/// retired or otherwise unrecognised, to a currently-valid one. Anything not
/// recognised becomes `'theme'`; absent/`null` returns `None` so the schema's own
/// default still applies.
///
/// v4 takes and returns `unknown` because it is a Zod `preprocess`; here the
/// input is the raw JSON value and the output the string to substitute. Both
/// `undefined` and `null` map to "leave it to the default" — but note that
/// v4's `.default('theme')` short-circuits BEFORE the preprocess, so an
/// **explicit `null`** never reaches the default and fails the enum. v5 keeps
/// that: [`ProjectEntity::parse_properties`] rewrites the key only when it is
/// present and non-null, so a JSON `null` still fails deserialization the way
/// v4's parse throws.
pub fn normalize_background_display_mode(value: Option<&Value>) -> Option<&'static str> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s == "latest_chat" => Some("latest_chat"),
        Some(Value::String(s)) if s == "theme" => Some("theme"),
        _ => Some("theme"),
    }
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
    /// Per-project answer-confirmation override, `z.enum(['ON','OFF']).nullable()
    /// .optional()` (v4 `add-answer-confirmation-columns-v2`). Schema-order: between
    /// `defaultAlertCharactersOfLanternImages` and `storyBackgroundsEnabled`.
    #[serde(
        default,
        rename = "answerConfirmationOverride",
        skip_serializing_if = "Option::is_none"
    )]
    pub answer_confirmation_override: Option<String>,
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
            "answerConfirmationOverride",
            "storyBackgroundsEnabled",
            "staticBackgroundImageId",
            "storyBackgroundImageId",
            "backgroundDisplayMode",
        ]
    }

    /// v4 `ProjectPropertiesSchema.parse`. The one chokepoint every project
    /// property bag passes through — the overlay READ, the write overlay's
    /// read-modify-write serialize, and `write_managed_fields` on create — which
    /// is why [`normalize_background_display_mode`] applied here covers v4's
    /// whole claim in one place: "Writes route through the same parse, so a
    /// pre-4.9 .qtap import or backup restore lands on a valid value."
    fn parse_properties(value: &Value) -> Result<ProjectProperties, String> {
        if !value.is_object() {
            return Err(format!("expected a JSON object, got: {value}"));
        }
        // [70505745a] `z.preprocess(normalizeBackgroundDisplayMode, z.enum([…]))`.
        // Rewritten only when the key is PRESENT and non-null: an absent key is
        // left to the serde default (v4's `.default('theme')`, which
        // short-circuits ahead of the preprocess) and an explicit `null` is left
        // to fail, as it fails v4's enum.
        let value = match value.get("backgroundDisplayMode") {
            Some(v) if !v.is_null() => {
                let normalized = normalize_background_display_mode(Some(v))
                    .expect("a present, non-null mode always normalizes to a value");
                let mut obj = value.as_object().cloned().unwrap_or_default();
                obj.insert(
                    "backgroundDisplayMode".to_string(),
                    Value::String(normalized.to_string()),
                );
                Value::Object(obj)
            }
            _ => value.clone(),
        };
        serde_json::from_value(value).map_err(|e| e.to_string())
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

    /// Find a batch by id, each hydrated — the store-backed base's `findByIds`
    /// (v4 `store-backed.repository.ts:101`). A row whose store is unavailable is
    /// **dropped**, exactly as [`Self::find_all`] drops it, so the result may be
    /// shorter than the input for either reason. This is the chat-list preload's
    /// project batch: one read for every project a listed chat belongs to, in
    /// place of a `find_by_id` per chat.
    pub fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Value>, OverlayError> {
        self.inner.find_by_ids(ids)
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

/// Read a project's `officialMountPointId` pointer WITHOUT the store overlay (v4
/// `projects.findById(...).officialMountPointId`). The doc-edit path resolver's
/// `project`-scope alias reads only this slim pointer. `Ok(None)` when the
/// project is absent; `Ok(Some(None))` when it has no official store.
pub fn find_official_mount_point_id_raw(
    main: &Connection,
    id: &str,
) -> Result<Option<Option<String>>, DbError> {
    main.query_row(
        "SELECT officialMountPointId FROM projects WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get::<_, Option<String>>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
    .map_err(DbError::from)
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

#[cfg(test)]
mod find_by_ids_tests {
    use super::*;
    use rusqlite::params;

    /// The MAIN-db slim table (`projects`), five columns.
    fn main_db(rows: &[(&str, &str, Option<&str>)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE projects (id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL, \
             officialMountPointId TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);",
        )
        .unwrap();
        for (id, name, mount_point_id) in rows {
            conn.execute(
                "INSERT INTO projects (id, name, officialMountPointId, createdAt, updatedAt) \
                 VALUES (?1, ?2, ?3, '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
                params![id, name, mount_point_id],
            )
            .unwrap();
        }
        conn
    }

    /// The MOUNT-INDEX side: the three-table join the overlay reads, seeded with
    /// one `properties.json` per named store.
    fn mount_db(stores: &[&str]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE doc_mount_files (id TEXT PRIMARY KEY NOT NULL);
             CREATE TABLE doc_mount_documents (id TEXT PRIMARY KEY NOT NULL, \
                fileId TEXT NOT NULL, content TEXT);
             CREATE TABLE doc_mount_file_links (id TEXT PRIMARY KEY NOT NULL, \
                fileId TEXT NOT NULL, mountPointId TEXT NOT NULL, relativePath TEXT NOT NULL);",
        )
        .unwrap();
        for mp in stores {
            conn.execute(
                "INSERT INTO doc_mount_files (id) VALUES (?1 || '-f')",
                params![mp],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO doc_mount_documents (id, fileId, content) \
                 VALUES (?1 || '-d', ?1 || '-f', '{\"color\":\"red\"}')",
                params![mp],
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

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    fn sorted_ids(rows: &[Value]) -> Vec<String> {
        let mut out: Vec<String> = rows
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect();
        out.sort();
        out
    }

    /// The batch comes back hydrated (the property bag overlaid), an absent id is
    /// simply missing, and a project whose store is unavailable is **dropped**
    /// rather than raised — `find_all`'s semantics, not `find_by_id`'s.
    #[test]
    fn returns_hydrated_projects_and_drops_an_unavailable_store() {
        let main = main_db(&[
            ("p1", "One", Some("mp-1")),
            ("p2", "Two", Some("mp-2")),
            ("p3", "Three", None),
        ]);
        let mount = mount_db(&["mp-1", "mp-2"]);
        let repo = ProjectsRepository::new(&main, &mount);

        let rows = repo.find_by_ids(&ids(&["p1", "p3", "ghost"])).unwrap();
        assert_eq!(sorted_ids(&rows), vec!["p1".to_string()]);
        assert_eq!(rows[0]["name"], Value::from("One"));
        // The store's own bytes win: `color` comes from `properties.json`, and the
        // schema defaults are materialized alongside it.
        assert_eq!(rows[0]["color"], Value::from("red"));
        assert_eq!(rows[0]["allowAnyCharacter"], Value::from(false));
        assert_eq!(rows[0]["backgroundDisplayMode"], Value::from("theme"));
        // …the same entity `find_by_id` hydrates one at a time.
        assert_eq!(rows[0], repo.find_by_id("p1").unwrap().unwrap());
    }

    #[test]
    fn empty_input_answers_empty() {
        let main = main_db(&[("p1", "One", Some("mp-1"))]);
        let mount = mount_db(&["mp-1"]);
        assert!(ProjectsRepository::new(&main, &mount)
            .find_by_ids(&[])
            .unwrap()
            .is_empty());
    }
}
