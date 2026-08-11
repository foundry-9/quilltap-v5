//! v4 `import-entities.ts` — conflict-strategy importers for tags, roleplay
//! templates, projects, groups, and chats (with messages). (Memories are the
//! sibling `memories.rs`; characters are `characters.rs`.)
//!
//! Failure discipline per v4: tags and roleplay templates log per-item failures
//! WITHOUT a warnings entry; projects, groups and chats push a
//! `Failed to import <kind> "<name>": <error>` warning. Loop preambles have no
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
use serde_json::{json, Map, Value};

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
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let repo = tags::TagsRepository::new(main);

    for raw in items {
        let source_id = super::id_of(raw);
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
                        let Ok(t) = serde_json::from_value::<ImportedTag>(raw.clone()) else {
                            return Ok(());
                        };
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
            let Ok(t) = serde_json::from_value::<ImportedTag>(raw.clone()) else {
                return Ok(());
            };
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
            // v4: logged, dropped, NO warnings entry.
            tracing::warn!(tag_id = %source_id, error = %e, "Failed to import tag");
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
) -> Result<Counts, DbError> {
    let mut imported = 0u32;
    let mut skipped = 0u32;
    let repo = RoleplayTemplatesRepository::new(main);

    for raw in items {
        let source_id = super::id_of(raw);
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
                        let Ok(t) = serde_json::from_value::<ImportedTemplate>(migrated.clone())
                        else {
                            return Ok(());
                        };
                        let name = format!("{} (imported)", t.name);
                        create_template(&repo, user_id, t, name, options, DUPLICATE_MINTS)?;
                        imported += 1;
                        return Ok(());
                    }
                }
            }
            let Ok(t) = serde_json::from_value::<ImportedTemplate>(migrated) else {
                return Ok(());
            };
            let name = t.name.clone();
            let new_id = create_template(&repo, user_id, t, name, options, &source_id)?;
            id_map.set(source_id.clone(), new_id);
            imported += 1;
            Ok(())
        })();
        if let Err(e) = out {
            tracing::warn!(template_id = %source_id, error = %e, "Failed to import roleplay template");
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

/// Fold a hydrated flat project/group payload into the store-backed create
/// shape: `description`/`instructions`/`state` become their own fields and the
/// listed property keys become the `properties` bag (absent keys get their
/// schema defaults from `parse_properties`, mirroring Zod).
fn fold_properties(raw: &Value, keys: &[&str]) -> Value {
    let mut bag = Map::new();
    if let Some(obj) = raw.as_object() {
        for k in keys {
            if let Some(v) = obj.get(*k) {
                bag.insert((*k).to_string(), v.clone());
            }
        }
    }
    Value::Object(bag)
}

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
            // v4's findById swallows overlay failures to null.
            let existing = repo.find_by_id(&source_id).ok().flatten();
            if existing.is_some() {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => {
                        skipped += 1;
                        id_map.set(source_id.clone(), source_id.clone());
                        return Ok(());
                    }
                    ConflictStrategy::Overwrite => {
                        repo.delete(&source_id).map_err(|e| e.to_string())?;
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
        if let Err(e) = out {
            warnings.push(format!("Failed to import project \"{name}\": {e}"));
            tracing::warn!(project_id = %source_id, error = %e, "Failed to import project");
        }
    }
    Ok(Counts {
        imported,
        skipped,
        messages: 0,
    })
}

fn create_project(
    repo: &projects::ProjectsRepository,
    raw: &Value,
    name_override: Option<String>,
    options: &ImportOptions,
    source_id: &str,
) -> Result<String, String> {
    let input = projects::ProjectCreateInput {
        name: name_override.unwrap_or_else(|| display_name(raw)),
        description: opt_str_field(raw, "description"),
        instructions: opt_str_field(raw, "instructions"),
        state: raw.get("state").cloned().unwrap_or_else(|| json!({})),
        properties: fold_properties(
            raw,
            <projects::ProjectEntity as crate::db::document_store_overlay::StoreEntity>::property_keys(),
        ),
    };
    let created = repo
        .create(&input, &store_create_options(options, source_id))
        .map_err(|e| e.to_string())?;
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
            let existing = repo.find_by_id(&source_id).ok().flatten();
            if existing.is_some() {
                match options.conflict_strategy {
                    ConflictStrategy::Skip => {
                        skipped += 1;
                        id_map.set(source_id.clone(), source_id.clone());
                        return Ok(());
                    }
                    ConflictStrategy::Overwrite => {
                        repo.delete(&source_id).map_err(|e| e.to_string())?;
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
        if let Err(e) = out {
            warnings.push(format!("Failed to import group \"{name}\": {e}"));
            tracing::warn!(group_id = %source_id, error = %e, "Failed to import group");
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
    let input = groups::GroupCreateInput {
        name: name_override.unwrap_or_else(|| display_name(raw)),
        description: opt_str_field(raw, "description"),
        instructions: opt_str_field(raw, "instructions"),
        state: raw.get("state").cloned().unwrap_or_else(|| json!({})),
        color: opt_str_field(raw, "color"),
        icon: opt_str_field(raw, "icon"),
    };
    let created = repo
        .create(&input, &store_create_options(options, source_id))
        .map_err(|e| e.to_string())?;
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
            let existing = chats_read::find_by_id(main, &source_id).map_err(|e| e.to_string())?;
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
                        repo.delete(&source_id).map_err(|e| e.to_string())?;
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
                            Err(e) => warnings
                                .push(format!("Failed to import message in chat \"{title}\": {e}")),
                        },
                        Err(e) => warnings
                            .push(format!("Failed to import message in chat \"{title}\": {e}")),
                    }
                }
            }
            imported += 1;
            Ok(())
        })();
        if let Err(e) = out {
            warnings.push(format!("Failed to import chat \"{title}\": {e}"));
            tracing::warn!(chat_id = %source_id, error = %e, "Failed to import chat");
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
    let create: ChatCreate =
        serde_json::from_value(Value::Object(obj)).map_err(|e| e.to_string())?;
    let (id, now) = mint_or_preserve(options, source_id);
    repo.create(
        &create,
        &crate::db::chats::CreateOptions {
            id: id.clone(),
            created_at: now.clone(),
            updated_at: now,
        },
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}
