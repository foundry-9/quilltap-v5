//! v4 `import-entities.ts` — conflict-strategy importers for tags, roleplay
//! templates, projects, groups, and chats (with messages). (Memories are the
//! sibling `memories.rs`; characters are `characters.rs`.)
//!
//! Failure discipline per v4: EVERY per-item catch here pushes a
//! `Failed to import <kind> "<name>": <error>` warning and logs. Tags and
//! roleplay templates only logged until v4 `275cd7bc` (bug 79) — the import had
//! just stopped swallowing destination read errors, and strictness alone would
//! have traded a silently wrong branch for a silent skip. Loop preambles have no
//! reads here, so nothing in this module reaches `executeImport`'s outer catch
//! except through the repositories' own errors at non-per-item points.
//!
//! The `duplicate` arm for roleplay templates, projects, groups and chats
//! reproduces v4's phantom-id quirk (`import-entities.ts:138-139` and friends):
//! a `randomUUID()` goes INTO the id map and the row is created under a
//! DIFFERENT freshly-minted id. Tags are the exception — their map records the
//! REAL created id (`:58`).

use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{ConflictStrategy, IdMap, ImportOptions};
use crate::db::chats::{ChatCreate, ChatsRepository};
use crate::db::chats_messages::{ChatEventInput, ChatMessagesRepository};
use crate::db::roleplay_templates::{
    self, DialogueDetection, RenderingPattern, RoleplayTemplatesRepository, RtCreate, StringOrPair,
    TemplateDelimiter,
};
use crate::db::{chats_read, groups, projects, tags, DbError};

pub(super) struct Counts {
    pub imported: u32,
    pub skipped: u32,
    pub messages: u32,
}

/// The plain mint (v4's `create(data)` with no options) — used by the arms
/// that never claim a source id (v4's `duplicate` rename branches).
fn mint() -> (String, String) {
    (uuid::Uuid::new_v4().to_string(), crate::clock::now_iso())
}

/// v4's `01e481f6` create fork: `options.preserveIds ? { id: x.id } : undefined`.
fn mint_or_preserve(options: &ImportOptions, source_id: &str) -> (String, String) {
    super::mint_or_preserve(options, source_id)
}

/// The source id a `duplicate`-strategy create passes: none. v4's `01e481f6`
/// diff forks exactly ONE create per importer — the plain arm — so a
/// conflict-strategy rename still mints, even under `preserveIds`. (Profiles
/// are the exception and fork both arms; see `profiles.rs`.)
const DUPLICATE_MINTS: &str = "";

/// The same fork for the store-backed kinds (projects / groups), whose create
/// options are all-optional: v4 passes `undefined` on the ordinary path, which
/// is `Default::default()` here, and `{ id }` under `preserveIds`.
fn store_create_options(
    options: &ImportOptions,
    source_id: &str,
) -> crate::db::store_backed::StoreCreateOptions {
    if options.preserve_ids && !source_id.is_empty() {
        crate::db::store_backed::StoreCreateOptions {
            id: Some(source_id.to_string()),
            created_at: None,
            updated_at: None,
        }
    } else {
        Default::default()
    }
}

// ===========================================================================
// Tags
// ===========================================================================

/// The tag payload (v4 `TagSchema` minus id/userId/timestamps). `nameLower` is
/// optional here because `tags.create` re-derives it (v4 `(nameLower || name)
/// .toLowerCase()`); `visualStyle` stays raw JSON so a malformed style fails the
/// item exactly where v4's Zod parse would.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportedTag {
    name: String,
    #[serde(default)]
    name_lower: Option<String>,
    #[serde(default)]
    quick_hide: Option<bool>,
    #[serde(default)]
    visual_style: Option<tags::TagVisualStyle>,
}

/// v4 `importTags` (`import-entities.ts:26`).
pub(super) fn import_tags(
    main: &Connection,
    user_id: &str,
    items: &[Value],
    options: &ImportOptions,
    id_map: &mut IdMap,
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let repo = tags::TagsRepository::new(main);

    for raw in items {
        let source_id = super::id_of(raw);
        let name = super::warning_display_name(raw);
        let out: Result<(), DbError> = (|| {
            let existing = tags::find_full_by_id(main, &source_id)?;
            if existing.is_some() {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => {
                        skipped += 1;
                        id_map.set(source_id.clone(), source_id.clone());
                        return Ok(());
                    }
                    ConflictStrategy::Overwrite => {
                        repo.delete(&source_id)?;
                    }
                    ConflictStrategy::Duplicate => {
                        // v4's create Zod-validates inside the per-item `try`,
                        // so a malformed tag lands in the same catch the write
                        // failures do — which `275cd7bc` gave a named warning.
                        // P4.143 item 9: the refusal goes to the ONE per-item
                        // catch below (v4's shape) — the same warning, plus v4's
                        // WARN `Failed to import tag {tagId, error}`.
                        let t = serde_json::from_value::<ImportedTag>(raw.clone())
                            .map_err(|e| DbError::Internal(super::serde_error_text(&e)))?;
                        // v4: name `${name} (imported)`, nameLower
                        // `${nameLower || name.toLowerCase()} (imported)` — the
                        // create's own `(nameLower || name).toLowerCase()` then
                        // lowercases the whole thing.
                        let name_lower = format!(
                            "{} (imported)",
                            t.name_lower
                                .clone()
                                .filter(|s| !s.is_empty())
                                .unwrap_or_else(|| t.name.to_lowercase())
                        );
                        let (id, now) = mint();
                        repo.create(
                            &tags::TagCreate {
                                user_id: user_id.to_string(),
                                name: format!("{} (imported)", t.name),
                                name_lower: Some(name_lower),
                                quick_hide: t.quick_hide,
                                visual_style: t.visual_style,
                            },
                            &tags::CreateOptions {
                                id: id.clone(),
                                created_at: now.clone(),
                                updated_at: now,
                            },
                        )?;
                        // Tags map onto the REAL created id (unlike the phantom
                        // arm the other kinds carry).
                        id_map.set(source_id.clone(), id);
                        imported += 1;
                        return Ok(());
                    }
                }
            }
            // P4.143 item 9: refused → the per-item catch (warning + WARN).
            let t = serde_json::from_value::<ImportedTag>(raw.clone())
                .map_err(|e| DbError::Internal(super::serde_error_text(&e)))?;
            let (id, now) = mint_or_preserve(options, &source_id);
            repo.create(
                &tags::TagCreate {
                    user_id: user_id.to_string(),
                    name: t.name,
                    name_lower: t.name_lower,
                    quick_hide: t.quick_hide,
                    visual_style: t.visual_style,
                },
                &tags::CreateOptions {
                    id: id.clone(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            )?;
            id_map.set(source_id.clone(), id);
            imported += 1;
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!(
                "Failed to import tag \"{name}\": {}",
                super::item_error_text(&e)
            ));
            // v4 `import-entities.ts:73-82`: `{ tagId, error }` (camelCase).
            tracing::warn!(tagId = super::id_field(raw), error = %super::item_error_text(&e), "Failed to import tag");
        }
    }
    Ok(Counts {
        imported,
        skipped,
        messages: 0,
    })
}

// ===========================================================================
// Roleplay templates
// ===========================================================================

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportedTemplate {
    name: String,
    #[serde(default)]
    description: Option<String>,
    system_prompt: String,
    #[serde(default)]
    is_built_in: bool,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    delimiters: Vec<TemplateDelimiter>,
    #[serde(default)]
    rendering_patterns: Vec<RenderingPattern>,
    #[serde(default)]
    dialogue_detection: Option<DialogueDetection>,
    #[serde(default = "d_star")]
    narration_delimiters: StringOrPair,
}

fn d_star() -> StringOrPair {
    StringOrPair::Single("*".to_string())
}

/// v4's `annotationButtons` → `delimiters` back-compat conversion
/// (`import-entities.ts:91-119`), applied to the raw JSON BEFORE
/// deserialization, plus the legacy `pluginName` strip (`:122`). Mutates and
/// returns a copy, like v4 mutates the payload object in place.
fn migrate_template_legacy_fields(raw: &Value) -> Value {
    let mut out = raw.clone();
    let Some(obj) = out.as_object_mut() else {
        return out;
    };
    let has_delimiters = obj
        .get("delimiters")
        .and_then(Value::as_array)
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    let buttons_truthy = obj
        .get("annotationButtons")
        .map(|v| !v.is_null())
        .unwrap_or(false);
    if buttons_truthy && !has_delimiters {
        let buttons = obj
            .get("annotationButtons")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let style_of = |key: &str| -> Option<&'static str> {
            match key {
                "Narration" | "Nar" => Some("qt-chat-narration"),
                "Internal Monologue" | "Int" => Some("qt-chat-inner-monologue"),
                "Out of Character" | "OOC" => Some("qt-chat-ooc"),
                _ => None,
            }
        };
        let delimiters: Vec<Value> = buttons
            .iter()
            .map(|btn| {
                let s = |k: &str| btn.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                let prefix = s("prefix");
                let suffix = s("suffix");
                let label = s("label");
                let abbrev = s("abbrev");
                let name = if !label.is_empty() {
                    label.clone()
                } else if !abbrev.is_empty() {
                    abbrev.clone()
                } else {
                    "Unknown".to_string()
                };
                let button_name = if !abbrev.is_empty() {
                    abbrev.clone()
                } else if !label.is_empty() {
                    label.clone()
                } else {
                    "?".to_string()
                };
                let style = style_of(&label)
                    .or_else(|| style_of(&abbrev))
                    .unwrap_or("qt-chat-narration");
                // A prefix with no suffix is a line-start marker (e.g. "// " OOC);
                // everything else is a wrap delimiter. Mirrors the kinds migration.
                if !prefix.is_empty() && suffix.is_empty() {
                    json!({"kind": "linePrefix", "name": name, "buttonName": button_name,
                           "marker": prefix, "style": style})
                } else {
                    let delims = if prefix == suffix {
                        Value::String(prefix.clone())
                    } else {
                        json!([prefix, suffix])
                    };
                    json!({"kind": "wrap", "name": name, "buttonName": button_name,
                           "delimiters": delims, "style": style})
                }
            })
            .collect();
        obj.insert("delimiters".to_string(), Value::Array(delimiters));
        obj.remove("annotationButtons");
    }
    obj.remove("pluginName");
    out
}

/// v4 `importRoleplayTemplates` (`import-entities.ts:79`). Global repo: built-in
/// and user templates share the table; the import stamps the importing user's id
/// over whatever the payload carried (v4 `{...templateData, userId}`).
pub(super) fn import_roleplay_templates(
    main: &Connection,
    user_id: &str,
    items: &[Value],
    options: &ImportOptions,
    id_map: &mut IdMap,
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let repo = RoleplayTemplatesRepository::new(main);

    for raw in items {
        let source_id = super::id_of(raw);
        let name = super::warning_display_name(raw);
        let out: Result<(), DbError> = (|| {
            let migrated = migrate_template_legacy_fields(raw);
            let existing = roleplay_templates::find_full_json_by_id(main, &source_id)?;
            if existing.is_some() {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => {
                        skipped += 1;
                        id_map.set(source_id.clone(), source_id.clone());
                        return Ok(());
                    }
                    ConflictStrategy::Overwrite => {
                        repo.delete(&source_id)?;
                    }
                    ConflictStrategy::Duplicate => {
                        // Phantom-map quirk (see the module header).
                        let phantom = uuid::Uuid::new_v4().to_string();
                        id_map.set(source_id.clone(), phantom);
                        // P4.143 item 9: refused → the per-item catch (warning + WARN).
                        let t = serde_json::from_value::<ImportedTemplate>(migrated.clone())
                            .map_err(|e| DbError::Internal(super::serde_error_text(&e)))?;
                        let name = format!("{} (imported)", t.name);
                        create_template(&repo, user_id, t, name, options, DUPLICATE_MINTS)?;
                        imported += 1;
                        return Ok(());
                    }
                }
            }
            // P4.143 item 9: refused → the per-item catch (warning + WARN).
            let t = serde_json::from_value::<ImportedTemplate>(migrated)
                .map_err(|e| DbError::Internal(super::serde_error_text(&e)))?;
            let name = t.name.clone();
            let new_id = create_template(&repo, user_id, t, name, options, &source_id)?;
            id_map.set(source_id.clone(), new_id);
            imported += 1;
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!(
                "Failed to import roleplay template \"{name}\": {}",
                super::item_error_text(&e)
            ));
            // v4 `import-entities.ts:171-180`: `{ templateId, error }`.
            tracing::warn!(templateId = super::id_field(raw), error = %super::item_error_text(&e), "Failed to import roleplay template");
        }
    }
    Ok(Counts {
        imported,
        skipped,
        messages: 0,
    })
}

fn create_template(
    repo: &RoleplayTemplatesRepository,
    user_id: &str,
    t: ImportedTemplate,
    name: String,
    options: &ImportOptions,
    source_id: &str,
) -> Result<String, DbError> {
    let create = RtCreate {
        user_id: Some(user_id.to_string()),
        name,
        description: t.description,
        system_prompt: t.system_prompt,
        is_built_in: t.is_built_in,
        tags: t.tags,
        delimiters: t.delimiters,
        rendering_patterns: t.rendering_patterns,
        dialogue_detection: t.dialogue_detection,
        narration_delimiters: t.narration_delimiters,
    };
    let (id, now) = mint_or_preserve(options, source_id);
    repo.create(
        &create,
        &roleplay_templates::CreateOptions {
            id: id.clone(),
            created_at: now.clone(),
            updated_at: now,
        },
    )?;
    Ok(id)
}

// ===========================================================================
// Projects and groups (store-backed; create provisions a fresh official store)
// ===========================================================================

fn opt_str_field(raw: &Value, key: &str) -> Option<String> {
    raw.get(key).and_then(Value::as_str).map(|s| s.to_string())
}

fn display_name(raw: &Value) -> String {
    raw.get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// v4 `importProjects` (`import-entities.ts:169`). `officialMountPointId` is
/// excluded so `create()` provisions a fresh store; per-item failure pushes a
/// warning (unlike tags/templates/profiles).
pub(super) fn import_projects(
    main: &Connection,
    mount: &Connection,
    items: &[Value],
    options: &ImportOptions,
    id_map: &mut IdMap,
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let repo = projects::ProjectsRepository::new(main, mount);

    for raw in items {
        let source_id = super::id_of(raw);
        let name = display_name(raw);
        let out: Result<(), String> = (|| {
            // [P4.48] v4's `repos.projects.findById` swallows a DB read error to
            // null (`_findById` = `safeQuery(…, null)`) but does NOT swallow an
            // overlay failure — `applyOverlayOne` throws, and this loop's
            // per-item `catch` turns it into the `Failed to import project "…"`
            // warning below. Propagating both legs matches v4 on the overlay
            // one and is the ruled divergence on the read one.
            let existing = repo
                .find_by_id(&source_id)
                .map_err(|e| super::overlay_error_text(&e))?;
            if existing.is_some() {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => {
                        skipped += 1;
                        id_map.set(source_id.clone(), source_id.clone());
                        return Ok(());
                    }
                    ConflictStrategy::Overwrite => {
                        repo.delete(&source_id)
                            .map_err(|e| super::item_error_text(&e))?;
                    }
                    ConflictStrategy::Duplicate => {
                        let phantom = uuid::Uuid::new_v4().to_string();
                        id_map.set(source_id.clone(), phantom);
                        create_project(
                            &repo,
                            raw,
                            Some(format!("{name} (imported)")),
                            options,
                            DUPLICATE_MINTS,
                        )?;
                        imported += 1;
                        return Ok(());
                    }
                }
            }
            let created_id = create_project(&repo, raw, None, options, &source_id)?;
            id_map.set(source_id.clone(), created_id);
            imported += 1;
            Ok(())
        })();
        if let Err(text) = out {
            warnings.push(format!("Failed to import project \"{name}\": {text}"));
            tracing::warn!(projectId = super::id_field(raw), error = %text, "Failed to import project");
        }
    }
    Ok(Counts {
        imported,
        skipped,
        messages: 0,
    })
}

/// The create payload v4's importers hand the store-backed `create`: the
/// bundle item minus `id` / `createdAt` / `updatedAt` / `officialMountPointId`
/// (`import-entities.ts:221,231,280,290` — the destructure), with the
/// `duplicate` arm's rename applied. What `_create` validates WHOLE.
fn store_create_payload(raw: &Value, name_override: Option<&str>) -> Value {
    let mut entity = raw.as_object().cloned().unwrap_or_default();
    for k in ["id", "createdAt", "updatedAt", "officialMountPointId"] {
        entity.remove(k);
    }
    if let Some(name) = name_override {
        entity.insert("name".into(), Value::String(name.to_string()));
    }
    Value::Object(entity)
}

/// The id a store-backed import create claims — the source id under
/// `preserveIds` ([`store_create_options`]'s fork), else none (it mints).
fn claimed_store_id<'a>(options: &ImportOptions, source_id: &'a str) -> Option<&'a str> {
    (options.preserve_ids && !source_id.is_empty()).then_some(source_id)
}

fn create_project(
    repo: &projects::ProjectsRepository,
    raw: &Value,
    name_override: Option<String>,
    options: &ImportOptions,
    source_id: &str,
) -> Result<String, String> {
    // [P4.148 → the TRAP] Validate BEFORE anything is written, as v4's
    // `_create` validates first (`store-backed.repository.ts:143-144`).
    // `repo.create` used to INSERT the slim row and provision the store before
    // `write_managed_fields` parsed the bag, so a refused project half-wrote a
    // row and a store. (`StoreBackedRepository::create`'s own order is
    // P4.158's file.) [P4.155, R-B] The WHOLE entity, as `_create` validates
    // it — the row keys (`name` 1–100, `description` ≤ 2000, `instructions` ≤
    // 10000, `state` a record) and the bag seeded by `prepareCreateData`
    // (`allowAnyCharacter: null` / `characterRoster: null` import OPEN) — so
    // an over-long name no longer imports. [P4.155, R-A] A refusal logs v4's
    // three repository ERRORs before the per-item WARN.
    let entity = store_create_payload(raw, name_override.as_deref());
    if let Err(zod) = projects::parse_create_entity(&entity, claimed_store_id(options, source_id)) {
        crate::db::document_store_overlay::log_refused_store_create(
            crate::db::document_store_overlay::StoreKind::Project,
            entity.get("name"),
            &zod,
        );
        return Err(zod);
    }
    let properties = crate::db::document_store_overlay::fold_properties(
        &entity,
        <projects::ProjectEntity as crate::db::document_store_overlay::StoreEntity>::property_keys(
        ),
    );
    let input = projects::ProjectCreateInput {
        name: display_name(&entity),
        description: opt_str_field(&entity, "description"),
        instructions: opt_str_field(&entity, "instructions"),
        state: entity.get("state").cloned().unwrap_or_else(|| json!({})),
        properties,
    };
    let created = repo
        .create(&input, &store_create_options(options, source_id))
        .map_err(|e| super::overlay_error_text(&e))?;
    Ok(created
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}

/// v4 `importGroups` (`import-entities.ts:234`).
pub(super) fn import_groups(
    main: &Connection,
    mount: &Connection,
    items: &[Value],
    options: &ImportOptions,
    id_map: &mut IdMap,
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let repo = groups::GroupsRepository::new(main, mount);

    for raw in items {
        let source_id = super::id_of(raw);
        let name = display_name(raw);
        let out: Result<(), String> = (|| {
            // [P4.48] See `import_projects` — same two legs, same disposition.
            let existing = repo
                .find_by_id(&source_id)
                .map_err(|e| super::overlay_error_text(&e))?;
            if existing.is_some() {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => {
                        skipped += 1;
                        id_map.set(source_id.clone(), source_id.clone());
                        return Ok(());
                    }
                    ConflictStrategy::Overwrite => {
                        repo.delete(&source_id)
                            .map_err(|e| super::item_error_text(&e))?;
                    }
                    ConflictStrategy::Duplicate => {
                        let phantom = uuid::Uuid::new_v4().to_string();
                        id_map.set(source_id.clone(), phantom);
                        create_group(
                            &repo,
                            raw,
                            Some(format!("{name} (imported)")),
                            options,
                            DUPLICATE_MINTS,
                        )?;
                        imported += 1;
                        return Ok(());
                    }
                }
            }
            let created_id = create_group(&repo, raw, None, options, &source_id)?;
            id_map.set(source_id.clone(), created_id);
            imported += 1;
            Ok(())
        })();
        if let Err(text) = out {
            warnings.push(format!("Failed to import group \"{name}\": {text}"));
            tracing::warn!(groupId = super::id_field(raw), error = %text, "Failed to import group");
        }
    }
    Ok(Counts {
        imported,
        skipped,
        messages: 0,
    })
}

fn create_group(
    repo: &groups::GroupsRepository,
    raw: &Value,
    name_override: Option<String>,
    options: &ImportOptions,
    source_id: &str,
) -> Result<String, String> {
    // v4 spreads the bundle entity into `repos.groups.create` →
    // `_create` validates it against `GroupSchema` (which spreads
    // `GroupPropertiesSchema.shape`) → `writeManagedFields(parseProperties(entity))`
    // (`import-entities.ts:286,300`, `base.repository.ts`), so an explicit
    // `null` colour/icon is KEPT and an absent one stays ABSENT — measured at
    // `52d6e7ecd`: unlike the create route, the import injects no `|| null`
    // (dogfood #136). A non-string, non-null value FAILS the validate, and the
    // per-item catch in `import_groups` records `Failed to import group` and
    // skips the group — the same fold-then-parse the project import runs
    // (P4.146's lane had dropped the value and imported the group; the
    // `52d6e7ecd` unification's review measured v4 and corrected it).
    // [P4.155, R-B / R-A] The WHOLE `GroupSchema` entity is validated, as for
    // projects, and a refusal logs v4's three repository ERRORs.
    let entity = store_create_payload(raw, name_override.as_deref());
    let properties =
        match groups::parse_create_entity(&entity, claimed_store_id(options, source_id)) {
            Ok(properties) => properties,
            Err(zod) => {
                crate::db::document_store_overlay::log_refused_store_create(
                    crate::db::document_store_overlay::StoreKind::Group,
                    entity.get("name"),
                    &zod,
                );
                return Err(zod);
            }
        };
    let input = groups::GroupCreateInput {
        name: display_name(&entity),
        description: opt_str_field(&entity, "description"),
        instructions: opt_str_field(&entity, "instructions"),
        state: entity.get("state").cloned().unwrap_or_else(|| json!({})),
    };
    let created = repo
        .create_with_properties(
            &input,
            &properties,
            &store_create_options(options, source_id),
        )
        .map_err(|e| super::overlay_error_text(&e))?;
    Ok(created
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}

// ===========================================================================
// Chats (with messages)
// ===========================================================================

/// v4 `importChats` (`import-entities.ts:297`). The exported chat is the full
/// hydrated `ChatMetadata` (minus the two ephemeral caches the export drops)
/// plus a `messages` array; `create` re-materializes the Zod defaults via
/// [`ChatCreate`]'s serde defaults. Message ids/timestamps come from the
/// payload (`addMessage` preserves them), which is what keeps
/// `conversationAnnotations.sourceMessageId` valid without a message id map.
pub(super) fn import_chats(
    main: &Connection,
    user_id: &str,
    items: &[Value],
    options: &ImportOptions,
    id_map: &mut IdMap,
    warnings: &mut Vec<String>,
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let mut messages = 0u32;
    let repo = ChatsRepository::new(main);
    let messages_repo = ChatMessagesRepository::new(main);

    for raw in items {
        let source_id = super::id_of(raw);
        let title = raw
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let out: Result<(), String> = (|| {
            let existing =
                chats_read::find_by_id(main, &source_id).map_err(|e| super::item_error_text(&e))?;
            let mut title_override: Option<String> = None;
            if existing.is_some() {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => {
                        skipped += 1;
                        id_map.set(source_id.clone(), source_id.clone());
                        return Ok(());
                    }
                    ConflictStrategy::Overwrite => {
                        // v4 passes { syncVaults: false } — the plain row+messages
                        // delete, no vault-summary sweep.
                        repo.delete(&source_id)
                            .map_err(|e| super::item_error_text(&e))?;
                    }
                    ConflictStrategy::Duplicate => {
                        // Phantom-map quirk: the map records this minted id, the
                        // row is created under another (see the module header).
                        let phantom = uuid::Uuid::new_v4().to_string();
                        id_map.set(source_id.clone(), phantom);
                        title_override = Some(format!("{title} (imported)"));
                    }
                }
            }

            // v4 forks only the non-duplicate create: the `duplicate` arm
            // renames and mints, and its map entry is a phantom anyway.
            let create_source_id = if title_override.is_some() {
                DUPLICATE_MINTS
            } else {
                source_id.as_str()
            };
            let new_chat_id = create_chat(
                &repo,
                user_id,
                raw,
                title_override.clone(),
                options,
                create_source_id,
            )?;
            // The non-duplicate paths record the REAL created id.
            if title_override.is_none() {
                id_map.set(source_id.clone(), new_chat_id.clone());
            }

            // Add messages (per-message try → warning + continue).
            if let Some(events) = raw.get("messages").and_then(Value::as_array) {
                for message in events {
                    match serde_json::from_value::<ChatEventInput>(message.clone()) {
                        Ok(event) => match messages_repo.add_message(&new_chat_id, &event) {
                            Ok(()) => messages += 1,
                            Err(e) => warnings.push(format!(
                                "Failed to import message in chat \"{title}\": {}",
                                super::item_error_text(&e)
                            )),
                        },
                        Err(e) => warnings.push(format!(
                            "Failed to import message in chat \"{title}\": {}",
                            super::serde_error_text(&e)
                        )),
                    }
                }
            }
            imported += 1;
            Ok(())
        })();
        if let Err(text) = out {
            warnings.push(format!("Failed to import chat \"{title}\": {text}"));
            // v4's `{ chatId, error }` (camelCase, P4.143 item 9).
            tracing::warn!(chatId = super::id_field(raw), error = %text, "Failed to import chat");
        }
    }
    Ok(Counts {
        imported,
        skipped,
        messages,
    })
}

fn create_chat(
    repo: &ChatsRepository,
    user_id: &str,
    raw: &Value,
    title_override: Option<String>,
    options: &ImportOptions,
    source_id: &str,
) -> Result<String, String> {
    // v4 strips id/userId/messages/createdAt/updatedAt and re-injects the
    // importing user's id (the user-scoped repo). serde ignores the stripped
    // keys that aren't ChatCreate fields; userId must be overridden explicitly.
    let mut obj = raw.as_object().cloned().unwrap_or_default();
    obj.insert("userId".to_string(), Value::String(user_id.to_string()));
    if let Some(t) = title_override {
        obj.insert("title".to_string(), Value::String(t));
    }
    obj.remove("id");
    obj.remove("messages");
    obj.remove("createdAt");
    obj.remove("updatedAt");
    // v4 `4d370a90f` (#75): a bundle from before the three Concierge states
    // carries only the legacy pair; derive the state so the chat keeps its
    // behaviour (`withConciergeModeFromLegacy(stripScenarioSeededSummary(…))` at
    // both of v4's `importChats` create sites — the one fork here covers both,
    // as for the strip below). The derive reads only the Concierge keys and the
    // strip only the summary pair, so applying it to the bundle's JSON before
    // the typed decode is v4's composition exactly.
    let obj = crate::services::dangerous_content::chat_override::with_concierge_mode_from_legacy(
        Value::Object(obj),
    );
    // v4's `repos.chats.create` validates the three Concierge enums (P4.124):
    // an out-of-enum value fails the chat with the ZodError message.
    // A refused create is v4's `validate` throw inside `_create`: the three
    // repository ERRORs precede the caller's `Failed to import chat` WARN.
    if let Some(zod) =
        crate::services::dangerous_content::chat_override::concierge_columns_zod_error(&obj)
    {
        crate::services::dangerous_content::chat_override::log_chat_create_validation_failure(&zod);
        return Err(zod);
    }
    let mut create: ChatCreate = serde_json::from_value(obj).map_err(|e| {
        let e = super::serde_error_text(&e);
        crate::services::dangerous_content::chat_override::log_chat_create_validation_failure(&e);
        e
    })?;
    // v4 bug 158 (`da9c4f34f`): a pre-fix export carries the chat's scenario in
    // `contextSummary` as well as `scenarioText`; the boot heal that cleared
    // those rows has long since run in this instance, so the import is the only
    // thing standing between a stale bundle and the bug coming back.
    //
    // v4 strips at BOTH of its `repos.chats.create` sites in `importChats` (the
    // duplicate-rename create and the preserve-ids create). v5 forked those two
    // into ONE `create_chat` under the `DUPLICATE_MINTS` sentinel, so this one
    // call covers both.
    crate::services::scenario_seeded_summary::strip_scenario_seeded_summary(&mut create);
    let (id, now) = mint_or_preserve(options, source_id);
    repo.create(
        &create,
        &crate::db::chats::CreateOptions {
            id: id.clone(),
            created_at: now.clone(),
            updated_at: now,
        },
    )
    .map_err(|e| {
        // v4's two repository ERRORs (`_create`'s rethrowing `safeQuery`, then
        // `chats.repository.ts:280`'s own wrap) — each `{collection: chats,
        // error: <bare>, strictFailures: true}` inside the import's strict
        // scope, which the homes read themselves — precede the caller's
        // `Failed to import chat` WARN. NO `Data validation failed` on this
        // arm. Measured at `07b8f0209` through the oracle's `refuse-chat-inserts`
        // plant (`execute_chat_create_db_failure`; Shared contract C2).
        crate::db::fallback::log_create_failure("chats", &e);
        crate::db::fallback::log_chat_create_wrap_failure(&e);
        super::item_error_text(&e)
    })?;
    Ok(id)
}

#[cfg(test)]
mod rendered_markdown_strip_tests {
    use super::*;

    /// P4.D235 (v4 `f7f3d7bf0`): `renderedMarkdown` left the chat schema, so
    /// an old `.qtap` carrying a stored transcript has it STRIPPED on import
    /// (v4's `ChatMetadataSchema` no longer declares it; v5's `ChatCreate` no
    /// longer has the field and serde ignores the key). Pinned on the MIGRATED
    /// shape — a table that still has the column — where a surviving binder
    /// would write the bundle's bytes: the created row's column stays NULL.
    #[test]
    fn a_bundle_chat_carrying_rendered_markdown_imports_without_it() {
        let schema: Value =
            serde_json::from_str(include_str!("../provisioning/fresh_schema.json")).unwrap();
        let ddl = schema["main"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|s| s.starts_with("CREATE TABLE \"chats\" ("))
            .unwrap();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(ddl).unwrap();
        conn.execute_batch("ALTER TABLE \"chats\" ADD COLUMN \"renderedMarkdown\" TEXT")
            .unwrap();
        let raw = json!({
            "id": "src-chat",
            "userId": "someone-else",
            "title": "An old bundle",
            "participants": [],
            "renderedMarkdown": "# A transcript v4 no longer stores",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-01-01T00:00:00.000Z",
        });
        let repo = ChatsRepository::new(&conn);
        let id = create_chat(
            &repo,
            "u1",
            &raw,
            None,
            &ImportOptions::seed_defaults(),
            "src-chat",
        )
        .expect("the chat imports");
        let stored: Option<String> = conn
            .query_row(
                "SELECT \"renderedMarkdown\" FROM \"chats\" WHERE id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, None, "the bundle's transcript was stripped");
        let read = chats_read::find_by_id(&conn, &id).unwrap().unwrap();
        assert!(read.get("renderedMarkdown").is_none());
        assert_eq!(read["title"], "An old bundle");
    }

    /// P4.124 item 14 + its unification follow-up: an out-of-enum
    /// `conciergeMode` is refused with v4's ZodError message, and the three
    /// repository ERRORs v4's `validate` / `_create` / `chats.create` log
    /// precede the caller's catch (the third since P4.143); a valid chat logs
    /// none.
    #[test]
    fn a_bundle_chat_with_an_unknown_concierge_mode_logs_v4s_repository_errors() {
        let schema: Value =
            serde_json::from_str(include_str!("../provisioning/fresh_schema.json")).unwrap();
        let ddl = schema["main"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|s| s.starts_with("CREATE TABLE \"chats\" ("))
            .unwrap();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(ddl).unwrap();
        let repo = ChatsRepository::new(&conn);
        let raw = json!({
            "id": "src-chat",
            "userId": "someone-else",
            "title": "A bogus mode",
            "participants": [],
            "conciergeMode": "bogus",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-01-01T00:00:00.000Z",
        });
        let (got, lines) = crate::test_support::captured_with(|| {
            create_chat(
                &repo,
                "u1",
                &raw,
                None,
                &ImportOptions::seed_defaults(),
                "src-chat",
            )
        });
        let err = got.expect_err("the chat is refused");
        assert!(err.contains("invalid_value"), "{err}");
        let errors: Vec<&String> = lines.iter().filter(|l| l.starts_with("ERROR ")).collect();
        // THREE since P4.143 item 3 (`chats.repository.ts`'s own `safeQuery`).
        assert_eq!(errors.len(), 3, "{lines:?}");
        assert!(
            errors[0]
                .starts_with("ERROR quilltap::db Data validation failed collection=chats error="),
            "{}",
            errors[0]
        );
        assert!(
            errors[1]
                .starts_with("ERROR quilltap::db Error creating entity collection=chats error="),
            "{}",
            errors[1]
        );
        assert!(
            errors[2]
                .starts_with("ERROR quilltap::db Failed to create chat collection=chats error="),
            "{}",
            errors[2]
        );
        // Silence leg.
        let mut ok = raw.clone();
        ok["conciergeMode"] = json!("moderated");
        let (got, lines) = crate::test_support::captured_with(|| {
            create_chat(
                &repo,
                "u1",
                &ok,
                None,
                &ImportOptions::seed_defaults(),
                "src-chat",
            )
        });
        got.expect("a valid mode imports");
        assert!(!lines.iter().any(|l| l.starts_with("ERROR ")), "{lines:?}");
    }

    /// P4.143 item 9: a refused tag / roleplay template logs v4's ONE per-item
    /// WARN — `Failed to import tag {tagId, error}` (`import-entities.ts:73-82`)
    /// and `Failed to import roleplay template {templateId, error}`
    /// (`:171-180`), camelCase, the raw item's id, before `error` — beside the
    /// named warning; a clean item logs none. Before P4.143 both serde arms
    /// logged NO WARN (only the outer write-failure arm did, `tag_id` /
    /// `template_id`).
    #[test]
    fn a_refused_tag_or_template_logs_v4s_one_warn() {
        const T: &str = "quilltap_core::services::quilltap_import::entities";
        let schema: Value =
            serde_json::from_str(include_str!("../provisioning/fresh_schema.json")).unwrap();
        let conn = Connection::open_in_memory().unwrap();
        for t in ["tags", "roleplay_templates"] {
            let head = format!("CREATE TABLE \"{t}\" (");
            let ddl = schema["main"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .find(|s| s.starts_with(&head))
                .unwrap();
            conn.execute_batch(ddl).unwrap();
        }
        let opts = ImportOptions::seed_defaults();
        let warns = |lines: Vec<String>| -> Vec<String> {
            lines
                .into_iter()
                .filter(|l| l.starts_with("WARN "))
                .collect()
        };
        type Importer = fn(
            &Connection,
            &str,
            &[Value],
            &ImportOptions,
            &mut IdMap,
            &mut Vec<String>,
        ) -> Result<Counts, DbError>;
        let families: [(&str, Importer, Value, Value, &str); 2] = [
            (
                "tag",
                import_tags,
                json!({"id": "tag-bad", "name": "Bad", "visualStyle": "not-an-object"}),
                json!({"id": "a4000000-0000-4000-8000-000000000001", "name": "Good"}),
                "tagId=tag-bad error=invalid type: string \"not-an-object\", expected struct \
                 TagVisualStyle",
            ),
            (
                "roleplay template",
                import_roleplay_templates,
                json!({"id": "rt-bad", "name": "Bad", "systemPrompt": 42}),
                json!({"id": "a4000000-0000-4000-8000-000000000002", "name": "Good",
                       "systemPrompt": "p"}),
                "templateId=rt-bad error=invalid type: integer `42`, expected a string",
            ),
        ];
        for (kind, import, bad, good, fields) in families {
            let mut warnings = Vec::new();
            let (counts, lines) = crate::test_support::captured_with(|| {
                import(
                    &conn,
                    "u",
                    std::slice::from_ref(&bad),
                    &opts,
                    &mut IdMap::default(),
                    &mut warnings,
                )
            });
            assert_eq!(counts.unwrap().imported, 0, "{kind}");
            assert_eq!(warnings.len(), 1, "{kind}: {warnings:?}");
            assert_eq!(
                warns(lines),
                vec![format!("WARN {T} Failed to import {kind} {fields}")],
                "{kind}"
            );
            let mut warnings = Vec::new();
            let (counts, lines) = crate::test_support::captured_with(|| {
                import(
                    &conn,
                    "u",
                    std::slice::from_ref(&good),
                    &opts,
                    &mut IdMap::default(),
                    &mut warnings,
                )
            });
            assert_eq!(counts.unwrap().imported, 1, "{kind}: {warnings:?}");
            assert!(warns(lines).is_empty(), "{kind}");
        }
    }
}

/// P4.146 (dogfood #136): a bundle group's `color`/`icon` land in
/// `properties.json` as v4's `repos.groups.create(entity)` writes them — an
/// explicit `null` kept, an absent key left absent (no `|| null` on the import).
#[cfg(test)]
mod group_null_import_tests {
    use super::*;

    fn conns() -> (Connection, Connection) {
        let schema: Value =
            serde_json::from_str(include_str!("../provisioning/fresh_schema.json")).unwrap();
        let open = |part: &str| {
            let conn = Connection::open_in_memory().unwrap();
            for ddl in schema[part].as_array().unwrap() {
                conn.execute_batch(ddl.as_str().unwrap()).unwrap();
            }
            conn
        };
        (open("main"), open("mountIndex"))
    }

    fn stored_properties(mount: &Connection, mount_point_id: &str) -> String {
        mount
            .query_row(
                "SELECT d.content FROM doc_mount_file_links l \
                 JOIN doc_mount_documents d ON d.fileId = l.fileId \
                 WHERE l.mountPointId = ?1 AND l.relativePath = 'properties.json'",
                [mount_point_id],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// [P4.148 item 13] The project / group per-item WARNs carry v4's
    /// camelCase id field (`import-entities.ts:244,309` — `projectId`,
    /// `groupId`), then the bare error (here a ZodError message: `color:
    /// "red"` fails `HexColorSchema`). Silence leg: a sound item warns nothing.
    #[test]
    fn project_and_group_warns_carry_v4s_camel_case_ids() {
        let (main, mount) = conns();
        let opts = ImportOptions::seed_defaults();
        let bad = |id: &str| json!({ "id": id, "name": "Red", "state": {}, "color": "red" });
        let good = |id: &str| json!({ "id": id, "name": "Fine", "state": {}, "color": "#abc" });
        const T: &str = "WARN quilltap_core::services::quilltap_import::entities";
        for (kind, field, run) in [
            (
                "project",
                "projectId",
                import_projects
                    as fn(
                        &Connection,
                        &Connection,
                        &[Value],
                        &ImportOptions,
                        &mut IdMap,
                        &mut Vec<String>,
                    ) -> Result<Counts, DbError>,
            ),
            ("group", "groupId", import_groups),
        ] {
            let mut warnings = Vec::new();
            let (out, lines) = crate::test_support::captured_with(|| {
                run(
                    &main,
                    &mount,
                    &[bad("src-bad")],
                    &opts,
                    &mut IdMap::default(),
                    &mut warnings,
                )
            });
            out.unwrap();
            let warns: Vec<&String> = lines.iter().filter(|l| l.starts_with("WARN ")).collect();
            assert_eq!(warns.len(), 1, "{kind}: {lines:?}");
            let (head, error) = warns[0].split_once(" error=").unwrap();
            assert_eq!(
                head,
                format!("{T} Failed to import {kind} {field}=src-bad"),
                "{kind}"
            );
            assert!(
                error.starts_with('[') && error.contains("\"regex\""),
                "{kind}: {error}"
            );
            let (out, quiet) = crate::test_support::captured_with(|| {
                run(
                    &main,
                    &mount,
                    &[good("src-good")],
                    &opts,
                    &mut IdMap::default(),
                    &mut Vec::new(),
                )
            });
            out.unwrap();
            assert!(
                quiet.iter().all(|l| !l.starts_with("WARN ")),
                "{kind}: {quiet:?}"
            );
        }
    }

    /// v4 seeds the two roster defaults BEFORE `_create` validates
    /// (`projects.repository.ts:55-63` `prepareCreateData`), so a bundle
    /// project carrying `allowAnyCharacter: null` / `characterRoster: null`
    /// imports OPEN with the seeded values on disk — never a refusal. Caught at
    /// the `07b8f0209` follow-ups unification: the import's validate-before-
    /// write had parsed the RAW bag and refused the project (data loss).
    #[test]
    fn a_null_roster_default_is_seeded_on_import_not_refused() {
        let (main, mount) = conns();
        let opts = ImportOptions::seed_defaults();
        let raw = json!({
            "id": "p-src-null", "name": "Nulled Roster", "state": {},
            "allowAnyCharacter": null, "characterRoster": null,
        });
        let mut warnings = Vec::new();
        let mut id_map = IdMap::default();
        let counts =
            import_projects(&main, &mount, &[raw], &opts, &mut id_map, &mut warnings).unwrap();
        assert_eq!((counts.imported, counts.skipped), (1, 0), "{warnings:?}");
        assert!(warnings.is_empty(), "{warnings:?}");
        let mp: String = main
            .query_row(
                "SELECT officialMountPointId FROM projects WHERE name = 'Nulled Roster'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let bag: Value = serde_json::from_str(&stored_properties(&mount, &mp)).unwrap();
        assert_eq!(bag["allowAnyCharacter"], json!(true), "{bag}");
        assert_eq!(bag["characterRoster"], json!([]), "{bag}");
    }

    #[test]
    fn a_null_colour_is_kept_and_an_absent_icon_stays_absent() {
        let (main, mount) = conns();
        let repo = groups::GroupsRepository::new(&main, &mount);
        let opts = ImportOptions::seed_defaults();
        for (raw, want) in [
            (
                json!({ "id": "g-src-1", "name": "Nulled", "color": null, "icon": null }),
                "{\n  \"color\": null,\n  \"icon\": null\n}",
            ),
            (
                json!({ "id": "g-src-2", "name": "Half", "color": null }),
                "{\n  \"color\": null\n}",
            ),
            (json!({ "id": "g-src-3", "name": "Bare" }), "{}"),
            (
                json!({ "id": "g-src-4", "name": "Valued", "color": "#abcdef", "icon": "gear" }),
                "{\n  \"color\": \"#abcdef\",\n  \"icon\": \"gear\"\n}",
            ),
        ] {
            let source = raw["id"].as_str().unwrap().to_string();
            let id = create_group(&repo, &raw, None, &opts, &source).unwrap();
            let group = repo.find_by_id(&id).unwrap().unwrap();
            let mp = group["officialMountPointId"].as_str().unwrap();
            assert_eq!(stored_properties(&mount, mp), want, "{raw}");
        }
    }

    /// v4 validates the spread bundle entity against `GroupSchema` before it
    /// writes, so a colour or icon that is neither a string nor `null` fails
    /// the group (the per-item catch warns and skips it). The P4.146 lane had
    /// dropped the value and imported the group anyway.
    #[test]
    fn a_non_string_colour_or_icon_fails_the_group_as_v4_does() {
        let (main, mount) = conns();
        let repo = groups::GroupsRepository::new(&main, &mount);
        let opts = ImportOptions::seed_defaults();
        for raw in [
            json!({ "id": "g-bad-1", "name": "Numbered", "color": 5 }),
            json!({ "id": "g-bad-2", "name": "Listed", "icon": ["gear"] }),
        ] {
            let source = raw["id"].as_str().unwrap().to_string();
            let err = create_group(&repo, &raw, None, &opts, &source)
                .expect_err("v4's validate refuses the bag");
            assert!(!err.is_empty(), "{raw}");
            assert!(
                repo.find_all().unwrap().is_empty(),
                "nothing is written for a refused group: {raw}"
            );
        }
    }
}
