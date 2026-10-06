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
use super::serde_tristate::double_option;
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
/// the eleven others are `.nullable().optional()` and are THREE-state here
/// (`Option<Option<T>>` over [`double_option`]): `None` = the key is absent and
/// stays absent (`skip_serializing_if`), `Some(None)` = an explicit `null`,
/// written back as `null`, `Some(Some(v))` = a value. v4's Zod parse keeps an
/// explicit `null` on such a key, so a v4-written bag (the cutover wrote whole
/// former rows, NULL columns as `null`; create writes `color: null, icon: null`)
/// must come back through a v5 read-modify-write with its nulls intact — a
/// plain `Option<T>` folded `null` into absent and rewrote every such file on
/// the first v5 edit (dogfood #136). This matches `JSON.stringify(parse(x),
/// null, 2)` byte-for-byte (the dedup sha depends on it).
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectProperties {
    #[serde(default, rename = "allowAnyCharacter")]
    pub allow_any_character: bool,
    #[serde(default, rename = "characterRoster")]
    pub character_roster: Vec<String>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub color: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub icon: Option<Option<String>>,
    #[serde(default, rename = "defaultDisabledTools")]
    pub default_disabled_tools: Vec<String>,
    #[serde(default, rename = "defaultDisabledToolGroups")]
    pub default_disabled_tool_groups: Vec<String>,
    #[serde(
        default,
        rename = "defaultAgentModeEnabled",
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_agent_mode_enabled: Option<Option<bool>>,
    #[serde(
        default,
        rename = "defaultAvatarGenerationEnabled",
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_avatar_generation_enabled: Option<Option<bool>>,
    #[serde(
        default,
        rename = "defaultImageProfileId",
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_image_profile_id: Option<Option<String>>,
    #[serde(
        default,
        rename = "defaultRoleplayTemplateId",
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_roleplay_template_id: Option<Option<String>>,
    #[serde(
        default,
        rename = "defaultAlertCharactersOfLanternImages",
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_alert_characters_of_lantern_images: Option<Option<bool>>,
    /// Per-project answer-confirmation override, `z.enum(['ON','OFF']).nullable()
    /// .optional()` (v4 `add-answer-confirmation-columns-v2`). Schema-order: between
    /// `defaultAlertCharactersOfLanternImages` and `storyBackgroundsEnabled`.
    #[serde(
        default,
        rename = "answerConfirmationOverride",
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub answer_confirmation_override: Option<Option<String>>,
    #[serde(
        default,
        rename = "storyBackgroundsEnabled",
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub story_backgrounds_enabled: Option<Option<bool>>,
    #[serde(
        default,
        rename = "staticBackgroundImageId",
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub static_background_image_id: Option<Option<String>>,
    #[serde(
        default,
        rename = "storyBackgroundImageId",
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub story_background_image_id: Option<Option<String>>,
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
    ///
    /// [P4.148] v4's rules run FIRST (`zod_project_properties_issues` — the
    /// hex colour, the 50 code-point icon, the uuid roster and four ids, the
    /// ON/OFF override, every type), and a refusal is the `ZodError.message`
    /// bytes, at every site — read included (v4 refuses a stored invalid bag
    /// with `properties.json unparseable: …` → 503, ruling R-B).
    fn parse_properties(value: &Value) -> Result<ProjectProperties, String> {
        let issues = crate::api::zod_issues::zod_project_properties_issues(value);
        if !issues.is_empty() {
            return Err(crate::api::zod_issues::zod_error_message(&issues));
        }
        // [70505745a] `z.preprocess(normalizeBackgroundDisplayMode, z.enum([…]))`.
        // Rewritten only when the key is PRESENT and non-null: an absent key is
        // left to the serde default (v4's `.default('theme')`, which
        // short-circuits ahead of the preprocess) and an explicit `null` is left
        // to fail, as it fails v4's enum.
        let value = match value.get("backgroundDisplayMode") {
            Some(v) if !v.is_null() => {
                // Every present, non-null value normalizes to Some — the
                // fallback is unreachable, kept so no live path can panic.
                let normalized = normalize_background_display_mode(Some(v)).unwrap_or("theme");
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
/// state). [`ProjectsRepository::create`] runs v4's `prepareCreateData` seed over
/// it FIRST ([`seed_create_properties`]: `allowAnyCharacter ?? true`,
/// `characterRoster ?? []`), then [`StoreEntity::parse_properties`] materializes
/// the remaining schema defaults.
///
/// The seed is NOT redundant with the schema defaults, and has not been since v4
/// `9753d0eb2`: the create seed says `true` while the READ default
/// (`ProjectProperties::allow_any_character`'s `#[serde(default)]`, v4's
/// `ProjectPropertiesSchema.default(false)`) still says `false`. So a project
/// CREATED without the flag is open, while a stored bag MISSING the flag still
/// reads closed. The API path (`api/projects.rs`) inserts the flag itself (v4's
/// `createProjectSchema` prefault) and never reaches the seed; the `.qtap`
/// importer, the backup restore and the fixture builders do.
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
    ///
    /// v4 `AbstractStoreBackedRepository.create` calls the subclass's
    /// `prepareCreateData` BEFORE the entity is validated
    /// (`store-backed.repository.ts:138`), and `ProjectsRepository`'s seeds the
    /// two roster defaults — see [`seed_create_properties`].
    pub fn create(
        &self,
        input: &ProjectCreateInput,
        opts: &ProjectCreateOptions,
    ) -> Result<Value, OverlayError> {
        self.inner.create(
            &input.name,
            &ManagedFields {
                properties: seed_create_properties(&input.properties),
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

    /// The roster policy (v4 `canCharacterParticipate`, its doc rewritten at
    /// `9753d0eb2`): may this character use their tools on the project's files
    /// and shared wardrobe? (`allowAnyCharacter`, or on the roster.) It does not
    /// govern who may chat in the project. v4's call sites go through
    /// `projectRosterAdmits` in `lib/projects/roster-access.ts` (no project or
    /// no character → admit; else this predicate, fail-closed on a miss).
    /// Missing project → `false`. The `allowAnyCharacter` read here is the
    /// stored bag's value under the READ default (`false` when the key is
    /// missing) — never the create seed's `true`.
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

/// v4 `ProjectsRepository.prepareCreateData` (`projects.repository.ts:55-63` at
/// `9753d0eb2`): the roster defaults a fresh project needs before its row is
/// written — `allowAnyCharacter: data.allowAnyCharacter ?? true` and
/// `characterRoster: data.characterRoster ?? []`. JS `??` treats `null` as
/// absent, so an explicit `null` on either key is seeded too (serde would
/// otherwise refuse a `null` where the struct wants a `bool`/array). An explicit
/// `false` is kept. A non-object bag passes through unchanged — the inner
/// create's `parse_properties` refuses it, as before.
///
/// Why this exists beside the schema defaults: since `9753d0eb2` the CREATE seed
/// (`true`) and the READ default (`false`, `ProjectPropertiesSchema.default`)
/// disagree, so the seed can no longer be "reproduced for free" by the parse —
/// a flag-less create must land `true` on disk while a flag-less stored bag
/// must still read `false`.
fn seed_create_properties(properties: &Value) -> Value {
    let Value::Object(bag) = properties else {
        return properties.clone();
    };
    let mut bag = bag.clone();
    if bag.get("allowAnyCharacter").is_none_or(Value::is_null) {
        bag.insert("allowAnyCharacter".into(), Value::Bool(true));
    }
    if bag.get("characterRoster").is_none_or(Value::is_null) {
        bag.insert("characterRoster".into(), Value::Array(Vec::new()));
    }
    Value::Object(bag)
}

/// v4's CREATE-time validation, as `prepareCreateData` → `_create`'s
/// `validate` runs it (`projects.repository.ts:55-63`, `store-backed.
/// repository.ts:142-144`): the two roster defaults are seeded FIRST
/// (`allowAnyCharacter ?? true`, `characterRoster ?? []` — JS `??` takes
/// `null` too), THEN the whole bag is parsed. A caller that must refuse a bag
/// BEFORE writing anything (the `.qtap` import, the restore) validates
/// through THIS, never through `parse_properties` on the raw bag — the
/// `07b8f0209` follow-ups unification caught P4.148's import refusing a
/// project carrying `allowAnyCharacter: null` that v4 (and [`ProjectsRepository
/// ::create`], which seeds the same way) imports OPEN. `Err` is v4's
/// `ZodError.message` bytes.
pub fn parse_create_properties(properties: &Value) -> Result<ProjectProperties, String> {
    <ProjectEntity as crate::db::document_store_overlay::StoreEntity>::parse_properties(
        &seed_create_properties(properties),
    )
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
mod create_seed_tests {
    //! P4.D246 (v4 `9753d0eb2`): the CREATE seed and the READ default, pinned
    //! side by side so a future "simplification" that folds one into the other
    //! reddens here by name. The differential twins: `projects_tier2_equivalence`
    //! (Alpha's flag-less create → `true`; Eta's `null` → `true`; Theta's
    //! planted key-less bag → `false` on the RMW) and `projects_routes_equivalence`.
    use super::*;
    use serde_json::json;

    fn seeded(bag: Value) -> Map<String, Value> {
        seed_create_properties(&bag)
            .as_object()
            .cloned()
            .expect("an object bag seeds to an object")
    }

    #[test]
    fn an_absent_flag_seeds_true() {
        let out = seeded(json!({ "color": "#abcdef" }));
        assert_eq!(out["allowAnyCharacter"], Value::Bool(true));
        assert_eq!(out["color"], Value::from("#abcdef"), "other keys untouched");
    }

    /// JS `??` treats `null` as absent — the arm only a repository caller
    /// (import / restore / a fixture builder) can reach, since the API refuses
    /// a `null` at the schema.
    #[test]
    fn a_null_flag_seeds_true() {
        let out = seeded(json!({ "allowAnyCharacter": null }));
        assert_eq!(out["allowAnyCharacter"], Value::Bool(true));
    }

    #[test]
    fn an_explicit_false_is_kept() {
        let out = seeded(json!({ "allowAnyCharacter": false }));
        assert_eq!(out["allowAnyCharacter"], Value::Bool(false));
    }

    #[test]
    fn an_absent_or_null_roster_seeds_empty() {
        assert_eq!(seeded(json!({}))["characterRoster"], json!([]));
        assert_eq!(
            seeded(json!({ "characterRoster": null }))["characterRoster"],
            json!([])
        );
        assert_eq!(
            seeded(json!({ "characterRoster": ["a1000000-0000-4000-8000-000000000001"] }))
                ["characterRoster"],
            json!(["a1000000-0000-4000-8000-000000000001"]),
            "a given roster is kept"
        );
    }

    #[test]
    fn a_non_object_bag_passes_through_for_the_parse_to_refuse() {
        assert_eq!(seed_create_properties(&json!("nope")), json!("nope"));
        assert!(ProjectEntity::parse_properties(&json!("nope")).is_err());
    }

    /// The READ default did NOT move at `9753d0eb2` (v4's
    /// `ProjectPropertiesSchema.default(false)`): a stored bag missing the key
    /// hydrates CLOSED, and the seeded create bag parses OPEN — the two defaults
    /// disagree by design, which is the whole reason the seed exists.
    #[test]
    fn a_stored_bag_missing_the_flag_still_reads_false() {
        let stored = ProjectEntity::parse_properties(&json!({ "color": "#abcdef" })).unwrap();
        assert!(!stored.allow_any_character, "the READ default stays false");
        let created = ProjectEntity::parse_properties(&seed_create_properties(&json!({}))).unwrap();
        assert!(created.allow_any_character, "the CREATE seed says true");
        assert!(created.character_roster.is_empty());
    }
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
                 VALUES (?1 || '-d', ?1 || '-f', '{\"color\":\"#ff0000\"}')",
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
        assert_eq!(rows[0]["color"], Value::from("#ff0000"));
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

/// P4.146 (dogfood #136): the bag's three states through
/// `JSON.stringify(ProjectPropertiesSchema.parse(x), null, 2)`'s twin — the
/// parse + 2-space serialize every write takes.
#[cfg(test)]
mod three_state_tests {
    use super::*;
    use serde_json::json;

    fn round_trip(bag: &Value) -> String {
        serde_json::to_string_pretty(&ProjectEntity::parse_properties(bag).unwrap()).unwrap()
    }

    const NULLABLE: [&str; 11] = [
        "color",
        "icon",
        "defaultAgentModeEnabled",
        "defaultAvatarGenerationEnabled",
        "defaultImageProfileId",
        "defaultRoleplayTemplateId",
        "defaultAlertCharactersOfLanternImages",
        "answerConfirmationOverride",
        "storyBackgroundsEnabled",
        "staticBackgroundImageId",
        "storyBackgroundImageId",
    ];

    /// A null on every nullable key comes back byte-identical (schema order,
    /// 2-space, no trailing newline).
    #[test]
    fn every_nullable_null_round_trips_byte_for_byte() {
        let text = "{\n  \"allowAnyCharacter\": false,\n  \"characterRoster\": [],\n  \"color\": null,\n  \"icon\": null,\n  \"defaultDisabledTools\": [],\n  \"defaultDisabledToolGroups\": [],\n  \"defaultAgentModeEnabled\": null,\n  \"defaultAvatarGenerationEnabled\": null,\n  \"defaultImageProfileId\": null,\n  \"defaultRoleplayTemplateId\": null,\n  \"defaultAlertCharactersOfLanternImages\": null,\n  \"answerConfirmationOverride\": null,\n  \"storyBackgroundsEnabled\": null,\n  \"staticBackgroundImageId\": null,\n  \"storyBackgroundImageId\": null,\n  \"backgroundDisplayMode\": \"theme\"\n}";
        let bag: Value = serde_json::from_str(text).unwrap();
        assert_eq!(round_trip(&bag), text);
        let props = ProjectEntity::parse_properties(&bag).unwrap();
        assert_eq!(props.color, Some(None));
        assert_eq!(props.story_backgrounds_enabled, Some(None));
    }

    #[test]
    fn an_absent_key_stays_absent_and_a_value_stays_a_value() {
        let out: Value = serde_json::from_str(&round_trip(&json!({}))).unwrap();
        for key in NULLABLE {
            assert!(out.get(key).is_none(), "{key} absent");
        }
        let out: Value = serde_json::from_str(&round_trip(&json!({
            "color": "#abcdef",
            "defaultAgentModeEnabled": true,
            "answerConfirmationOverride": "ON",
        })))
        .unwrap();
        assert_eq!(out["color"], "#abcdef");
        assert_eq!(out["defaultAgentModeEnabled"], true);
        assert_eq!(out["answerConfirmationOverride"], "ON");
    }

    /// A mixed bag: null, value and absent side by side, each kept as given.
    #[test]
    fn a_mixed_bag_keeps_each_state() {
        let out: Value = serde_json::from_str(&round_trip(&json!({
            "color": null,
            "icon": "gear",
            "storyBackgroundsEnabled": false,
        })))
        .unwrap();
        assert_eq!(out.get("color"), Some(&Value::Null));
        assert_eq!(out["icon"], "gear");
        assert_eq!(out["storyBackgroundsEnabled"], false);
        assert!(out.get("staticBackgroundImageId").is_none());
    }

    /// The five defaulted keys did not move: absent materializes the default,
    /// and `backgroundDisplayMode: null` is still refused (v4's enum fails on
    /// it, and `parse_properties` leaves it to fail).
    #[test]
    fn the_defaulted_keys_are_unchanged() {
        let out: Value = serde_json::from_str(&round_trip(&json!({}))).unwrap();
        assert_eq!(out["allowAnyCharacter"], false);
        assert_eq!(out["characterRoster"], json!([]));
        assert_eq!(out["defaultDisabledTools"], json!([]));
        assert_eq!(out["defaultDisabledToolGroups"], json!([]));
        assert_eq!(out["backgroundDisplayMode"], "theme");
        assert!(
            ProjectEntity::parse_properties(&json!({ "backgroundDisplayMode": null })).is_err()
        );
        assert!(ProjectEntity::parse_properties(&json!({ "allowAnyCharacter": null })).is_err());
        for (given, want) in [("latest_chat", "latest_chat"), ("bogus", "theme")] {
            let out: Value =
                serde_json::from_str(&round_trip(&json!({ "backgroundDisplayMode": given })))
                    .unwrap();
            assert_eq!(out["backgroundDisplayMode"], want, "{given}");
        }
    }
}
