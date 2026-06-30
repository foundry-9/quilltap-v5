//! The character vault **managed-fields write projection** — ports v4's
//! `writeCharacterVaultManagedFields` (`vault-overlay/managed-fields.ts`), the
//! full-character projection that lays every vault-managed content field out to
//! its file. The counterpart of the read overlay's `readCharacterVaultManagedFields`
//! and the symmetric sibling of [`super::vault_wardrobe_write::project_vault_wardrobe`].
//!
//! It writes (in v4's exact order):
//!   1. `properties.json` — `{ pronouns, aliases, title, firstMessage, talkativeness }`
//!      as `JSON.stringify(.,null,2)`.
//!   2. `identity.md` / `description.md` / `manifesto.md` / `personality.md` /
//!      `example-dialogues.md` — the raw markdown field (or `""`).
//!   3. iff a `physicalDescription` is present: `physical-description.md`
//!      (`fullDescription` or `""`) + `physical-prompts.json`
//!      (`renderPhysicalPromptsJson`).
//!   4. the `Prompts/` and `Scenarios/` folder projections (one `.md` per array
//!      entry; files not produced this pass are swept).
//!
//! Wardrobe is intentionally NOT projected here — it lives vault-first and is
//! written through [`super::vault_wardrobe_write::project_vault_wardrobe`].
//!
//! Composes the already-ported pure leaves
//! ([`crate::vault_overlay::build_system_prompt_file`] /
//! [`crate::vault_overlay::build_scenario_file`] /
//! [`crate::vault_overlay::sanitize_file_name`]) and the folder projector
//! ([`super::vault_wardrobe_write::project_array_into_vault_folder`]) over the
//! document-store write primitive
//! ([`super::doc_mount_file_links::DocMountFileLinksRepository::write_database_document`]).
//!
//! Out of scope (matches the storage primitive's existing boundary): v4's
//! post-write `reindexSingleFile` chunk pass — the differential drives v4 with the
//! reindex running and pins the link `chunkCount` / excludes `doc_mount_chunks`,
//! exactly as the groups/projects/wardrobe store-backed tests do.

use serde::{Deserialize, Serialize, Serializer};

use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::DbError;
use crate::vault_overlay::{build_scenario_file, build_system_prompt_file, sanitize_file_name};

use super::vault_wardrobe_write::project_array_into_vault_folder;

const PROPERTIES_JSON_PATH: &str = "properties.json";
const IDENTITY_MD_PATH: &str = "identity.md";
const DESCRIPTION_MD_PATH: &str = "description.md";
const MANIFESTO_MD_PATH: &str = "manifesto.md";
const PERSONALITY_MD_PATH: &str = "personality.md";
const EXAMPLE_DIALOGUES_MD_PATH: &str = "example-dialogues.md";
const PHYSICAL_DESCRIPTION_MD_PATH: &str = "physical-description.md";
const PHYSICAL_PROMPTS_JSON_PATH: &str = "physical-prompts.json";
const PROMPTS_FOLDER: &str = "Prompts";
const SCENARIOS_FOLDER: &str = "Scenarios";

/// `{ subject, object, possessive }` — v4 `PronounsSchema`. Serialized into
/// `properties.json` in this field order (matches `JSON.stringify`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pronouns {
    pub subject: String,
    pub object: String,
    pub possessive: String,
}

/// The physical-description fields the write path consumes: `fullDescription`
/// (→ `physical-description.md`) and the five prompt tiers (→ `physical-prompts.json`).
/// Other `PhysicalDescriptionSchema` fields (id/name/timestamps) are irrelevant to
/// the projected bytes, so they are not modeled here.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalDescriptionWrite {
    #[serde(default)]
    pub full_description: Option<String>,
    #[serde(default)]
    pub head_and_shoulders_prompt: Option<String>,
    #[serde(default)]
    pub short_prompt: Option<String>,
    #[serde(default)]
    pub medium_prompt: Option<String>,
    #[serde(default)]
    pub long_prompt: Option<String>,
    #[serde(default)]
    pub complete_prompt: Option<String>,
}

/// One `Prompts/*.md` source — v4 `CharacterSystemPrompt` (the bytes-relevant subset).
#[derive(Debug, Clone, Deserialize)]
pub struct SystemPromptWrite {
    pub name: String,
    pub content: String,
    #[serde(default, rename = "isDefault")]
    pub is_default: bool,
}

/// One `Scenarios/*.md` source — v4 `CharacterScenario` (the bytes-relevant subset).
#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioWrite {
    pub title: String,
    pub content: String,
}

/// The managed-field inputs `writeCharacterVaultManagedFields` reads off the raw
/// (non-overlaid) character. Optionality mirrors v4's `?? <default>` coalescing
/// exactly (`None` markdown → `""`, `None` talkativeness → `0.5`, etc.).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterVaultWriteInput {
    #[serde(default)]
    pub pronouns: Option<Pronouns>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub first_message: Option<String>,
    /// `z.number().min(0.1).max(1.0).default(0.5)` — `None` coalesces to `0.5`.
    #[serde(default)]
    pub talkativeness: Option<f64>,
    #[serde(default)]
    pub identity: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub manifesto: Option<String>,
    #[serde(default)]
    pub personality: Option<String>,
    #[serde(default)]
    pub example_dialogues: Option<String>,
    #[serde(default)]
    pub physical_description: Option<PhysicalDescriptionWrite>,
    #[serde(default)]
    pub system_prompts: Vec<SystemPromptWrite>,
    #[serde(default)]
    pub scenarios: Vec<ScenarioWrite>,
}

/// Outcome of a full-character projection (v4 `VaultManagedFieldsWriteResult`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteResult {
    pub single_file_write_count: usize,
    pub system_prompts_written: usize,
    pub scenarios_written: usize,
    pub physical_skipped_no_primary: bool,
}

/// Serialize an `f64` the way `JSON.stringify` renders a JS number: an
/// integer-valued float collapses to a bare integer (`1.0` → `1`), matching
/// [`super::js_number_to_json`]. `properties.json` feeds the content-dedup sha, so
/// this must be byte-exact.
fn serialize_js_number<S: Serializer>(value: &f64, s: S) -> Result<S::Ok, S::Error> {
    if value.is_finite()
        && value.fract() == 0.0
        && *value >= i64::MIN as f64
        && *value <= i64::MAX as f64
    {
        s.serialize_i64(*value as i64)
    } else {
        s.serialize_f64(*value)
    }
}

/// `properties.json` shape — keys in v4's literal order; every key is always
/// emitted (`null` when absent), never skipped.
#[derive(Serialize)]
struct PropertiesJson<'a> {
    pronouns: Option<&'a Pronouns>,
    aliases: &'a [String],
    title: Option<&'a str>,
    #[serde(rename = "firstMessage")]
    first_message: Option<&'a str>,
    #[serde(serialize_with = "serialize_js_number")]
    talkativeness: f64,
}

/// `physical-prompts.json` shape — v4 `renderPhysicalPromptsJson`. Five keys,
/// always emitted (`null` when the tier is unset).
#[derive(Serialize)]
struct PhysicalPromptsJson<'a> {
    #[serde(rename = "headAndShoulders")]
    head_and_shoulders: Option<&'a str>,
    short: Option<&'a str>,
    medium: Option<&'a str>,
    long: Option<&'a str>,
    complete: Option<&'a str>,
}

/// Render `physical-prompts.json` from a physical description (v4
/// `renderPhysicalPromptsJson`). A `None` description renders all-null.
pub fn render_physical_prompts_json(physical: Option<&PhysicalDescriptionWrite>) -> String {
    let prompts = PhysicalPromptsJson {
        head_and_shoulders: physical.and_then(|p| p.head_and_shoulders_prompt.as_deref()),
        short: physical.and_then(|p| p.short_prompt.as_deref()),
        medium: physical.and_then(|p| p.medium_prompt.as_deref()),
        long: physical.and_then(|p| p.long_prompt.as_deref()),
        complete: physical.and_then(|p| p.complete_prompt.as_deref()),
    };
    serde_json::to_string_pretty(&prompts)
        .expect("physical-prompts.json serialization is infallible")
}

/// Project every vault-managed content field of a character out to its file (v4
/// `writeCharacterVaultManagedFields`). Writes each single file then reprojects
/// the `Prompts/` and `Scenarios/` folders. Wardrobe is NOT handled here.
pub fn write_character_vault_managed_fields(
    links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    character: &CharacterVaultWriteInput,
) -> Result<WriteResult, DbError> {
    let mut result = WriteResult::default();

    // 1. properties.json (pretty JSON; all five keys always present).
    let props = PropertiesJson {
        pronouns: character.pronouns.as_ref(),
        aliases: &character.aliases,
        title: character.title.as_deref(),
        first_message: character.first_message.as_deref(),
        talkativeness: character.talkativeness.unwrap_or(0.5),
    };
    let props_json =
        serde_json::to_string_pretty(&props).expect("properties.json serialization is infallible");
    links.write_database_document(mount_point_id, PROPERTIES_JSON_PATH, &props_json)?;
    result.single_file_write_count += 1;

    // 2. The five markdown fields (None → "").
    for (path, value) in [
        (IDENTITY_MD_PATH, character.identity.as_deref()),
        (DESCRIPTION_MD_PATH, character.description.as_deref()),
        (MANIFESTO_MD_PATH, character.manifesto.as_deref()),
        (PERSONALITY_MD_PATH, character.personality.as_deref()),
        (
            EXAMPLE_DIALOGUES_MD_PATH,
            character.example_dialogues.as_deref(),
        ),
    ] {
        links.write_database_document(mount_point_id, path, value.unwrap_or(""))?;
        result.single_file_write_count += 1;
    }

    // 3. physical-* files, only when a primary physical description exists.
    match character.physical_description.as_ref() {
        Some(physical) => {
            links.write_database_document(
                mount_point_id,
                PHYSICAL_DESCRIPTION_MD_PATH,
                physical.full_description.as_deref().unwrap_or(""),
            )?;
            result.single_file_write_count += 1;
            let prompts_json = render_physical_prompts_json(Some(physical));
            links.write_database_document(
                mount_point_id,
                PHYSICAL_PROMPTS_JSON_PATH,
                &prompts_json,
            )?;
            result.single_file_write_count += 1;
        }
        None => {
            result.physical_skipped_no_primary = true;
        }
    }

    // 4. Prompts/ and Scenarios/ folder projections.
    project_array_into_vault_folder(
        links,
        docs,
        mount_point_id,
        PROMPTS_FOLDER,
        &character.system_prompts,
        |p| {
            (
                format!("{}.md", sanitize_file_name(&p.name)),
                build_system_prompt_file(&p.name, p.is_default, &p.content),
            )
        },
    )?;
    result.system_prompts_written = character.system_prompts.len();

    project_array_into_vault_folder(
        links,
        docs,
        mount_point_id,
        SCENARIOS_FOLDER,
        &character.scenarios,
        |s| {
            (
                format!("{}.md", sanitize_file_name(&s.title)),
                build_scenario_file(&s.title, &s.content),
            )
        },
    )?;
    result.scenarios_written = character.scenarios.len();

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_json_emits_all_keys_in_order() {
        let input = CharacterVaultWriteInput {
            pronouns: Some(Pronouns {
                subject: "she".into(),
                object: "her".into(),
                possessive: "hers".into(),
            }),
            aliases: vec!["Vi".into()],
            title: Some("The Inventor".into()),
            first_message: None,
            talkativeness: Some(0.5),
            ..Default::default()
        };
        let props = PropertiesJson {
            pronouns: input.pronouns.as_ref(),
            aliases: &input.aliases,
            title: input.title.as_deref(),
            first_message: input.first_message.as_deref(),
            talkativeness: input.talkativeness.unwrap_or(0.5),
        };
        let json = serde_json::to_string_pretty(&props).unwrap();
        assert_eq!(
            json,
            "{\n  \"pronouns\": {\n    \"subject\": \"she\",\n    \"object\": \"her\",\n    \"possessive\": \"hers\"\n  },\n  \"aliases\": [\n    \"Vi\"\n  ],\n  \"title\": \"The Inventor\",\n  \"firstMessage\": null,\n  \"talkativeness\": 0.5\n}"
        );
    }

    #[test]
    fn talkativeness_one_renders_as_bare_integer() {
        // JS `JSON.stringify(1.0)` is `1`, not `1.0` — must match (feeds the sha).
        let props = PropertiesJson {
            pronouns: None,
            aliases: &[],
            title: None,
            first_message: None,
            talkativeness: 1.0,
        };
        let json = serde_json::to_string_pretty(&props).unwrap();
        assert!(json.contains("\"talkativeness\": 1\n"), "{json}");
        // Empty aliases array renders inline as `[]` (matches JSON.stringify).
        assert!(json.contains("\"aliases\": []"), "{json}");
    }

    #[test]
    fn physical_prompts_json_all_null_when_absent() {
        let json = render_physical_prompts_json(None);
        assert_eq!(
            json,
            "{\n  \"headAndShoulders\": null,\n  \"short\": null,\n  \"medium\": null,\n  \"long\": null,\n  \"complete\": null\n}"
        );
    }

    #[test]
    fn physical_prompts_json_renders_present_tiers() {
        let physical = PhysicalDescriptionWrite {
            head_and_shoulders_prompt: Some("face".into()),
            short_prompt: Some("short".into()),
            medium_prompt: None,
            long_prompt: None,
            complete_prompt: Some("complete".into()),
            full_description: Some("ignored here".into()),
        };
        let json = render_physical_prompts_json(Some(&physical));
        assert_eq!(
            json,
            "{\n  \"headAndShoulders\": \"face\",\n  \"short\": \"short\",\n  \"medium\": null,\n  \"long\": null,\n  \"complete\": \"complete\"\n}"
        );
    }
}
