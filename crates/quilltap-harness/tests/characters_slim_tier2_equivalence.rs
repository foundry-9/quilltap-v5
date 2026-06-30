//! Tier-2 differential test: the `characters` SLIM ROW (Phase-2, main DB, the
//! store-backed capstone sub-unit 2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-characters-slim-fixture.ts), run the SAME create
//! / update / delete op sequence from the committed spec, dump the `characters`
//! table canonically, and assert the post-op state is identical. Ids and
//! timestamps are pinned on both sides, so the dumps must match with zero
//! normalization.
//!
//! WHY THE SLIM ROW: v4's `CharactersRepository.create` provisions the character
//! vault and `.update` routes managed fields to it; both strip MANAGED_FIELDS
//! before the SQL write. The oracle drives v4's REAL protected `_create`/`_update`/
//! `_delete` via a thin subclass; the Rust port mirrors that slim-row marshaling.
//! The managed columns exist in the fixture table but both sides omit them from
//! every write, so they sit at their DDL defaults identically.
//!
//! Banks seven nullable boolean columns, two boolean-default columns, a typed
//! JSON-object column (`defaultTimestampConfig`), an open JSON column
//! (`sillyTavernData`), two typed-struct array columns (`partnerLinks` /
//! `avatarOverrides`), a string-array column (`tags`), an enum TEXT column
//! (`controlledBy`), and many nullable UUID columns.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-characters-slim-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-slim-fixture.ts
//!   QT_FIXTURE_CHARACTERS_SLIM=/tmp/qt-characters-slim-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-slim.ts \
//!     > /tmp/oracle-characters-slim.ndjson
//! Run:
//!   QT_ORACLE_CHARACTERS_SLIM=/tmp/oracle-characters-slim.ndjson \
//!   QT_FIXTURE_CHARACTERS_SLIM=/tmp/qt-characters-slim-fixture.db \
//!     cargo test -p quilltap-harness --test characters_slim_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::characters::{
    AvatarOverride, CharacterCreate, CharacterUpdate, CreateOptions, PartnerLink, TimestampConfig,
};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

/// The committed fixture spec — the single source driving both ports.
#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "create")]
    Create {
        data: CreateData,
        options: CreateOpts,
    },
    #[serde(rename = "update")]
    Update { id: String, data: UpdateData },
    #[serde(rename = "delete")]
    Delete { id: String },
}

/// The create payload — every slim column present (the corpus sets them all so no
/// Zod default has to be replicated). The `defaultTimestampConfig` / `partnerLinks`
/// / `avatarOverrides` typed shapes deserialize straight into the core structs
/// (their serde renames match the JSON keys).
#[derive(Deserialize)]
struct CreateData {
    #[serde(rename = "userId")]
    user_id: String,
    name: String,
    #[serde(rename = "defaultImageId")]
    default_image_id: Option<String>,
    #[serde(rename = "defaultConnectionProfileId")]
    default_connection_profile_id: Option<String>,
    #[serde(rename = "defaultPartnerId")]
    default_partner_id: Option<String>,
    #[serde(rename = "defaultRoleplayTemplateId")]
    default_roleplay_template_id: Option<String>,
    #[serde(rename = "defaultImageProfileId")]
    default_image_profile_id: Option<String>,
    #[serde(rename = "sillyTavernData")]
    silly_tavern_data: Option<Value>,
    #[serde(rename = "isFavorite")]
    is_favorite: bool,
    npc: bool,
    #[serde(rename = "controlledBy")]
    controlled_by: String,
    #[serde(rename = "defaultAgentModeEnabled")]
    default_agent_mode_enabled: Option<bool>,
    #[serde(rename = "defaultHelpToolsEnabled")]
    default_help_tools_enabled: Option<bool>,
    #[serde(rename = "defaultTimestampConfig")]
    default_timestamp_config: Option<TimestampConfig>,
    #[serde(rename = "defaultScenarioId")]
    default_scenario_id: Option<String>,
    #[serde(rename = "defaultSystemPromptId")]
    default_system_prompt_id: Option<String>,
    #[serde(rename = "characterDocumentMountPointId")]
    character_document_mount_point_id: Option<String>,
    #[serde(rename = "canDressThemselves")]
    can_dress_themselves: Option<bool>,
    #[serde(rename = "canCreateOutfits")]
    can_create_outfits: Option<bool>,
    #[serde(rename = "systemTransparency")]
    system_transparency: Option<bool>,
    #[serde(rename = "coreWhisperEnabled")]
    core_whisper_enabled: Option<bool>,
    #[serde(rename = "canBeCarina")]
    can_be_carina: Option<bool>,
    #[serde(rename = "partnerLinks")]
    partner_links: Vec<PartnerLink>,
    tags: Vec<String>,
    #[serde(rename = "avatarOverrides")]
    avatar_overrides: Vec<AvatarOverride>,
}

#[derive(Deserialize)]
struct CreateOpts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

/// The update patch — every slim column optional (absent -> leave untouched). The
/// corpus only sets values (clearing a nullable to NULL is deferred).
#[derive(Deserialize)]
struct UpdateData {
    #[serde(default, rename = "userId")]
    user_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "defaultImageId")]
    default_image_id: Option<String>,
    #[serde(default, rename = "defaultConnectionProfileId")]
    default_connection_profile_id: Option<String>,
    #[serde(default, rename = "defaultPartnerId")]
    default_partner_id: Option<String>,
    #[serde(default, rename = "defaultRoleplayTemplateId")]
    default_roleplay_template_id: Option<String>,
    #[serde(default, rename = "defaultImageProfileId")]
    default_image_profile_id: Option<String>,
    #[serde(default, rename = "sillyTavernData")]
    silly_tavern_data: Option<Value>,
    #[serde(default, rename = "isFavorite")]
    is_favorite: Option<bool>,
    #[serde(default)]
    npc: Option<bool>,
    #[serde(default, rename = "controlledBy")]
    controlled_by: Option<String>,
    #[serde(default, rename = "defaultAgentModeEnabled")]
    default_agent_mode_enabled: Option<bool>,
    #[serde(default, rename = "defaultHelpToolsEnabled")]
    default_help_tools_enabled: Option<bool>,
    #[serde(default, rename = "defaultTimestampConfig")]
    default_timestamp_config: Option<TimestampConfig>,
    #[serde(default, rename = "defaultScenarioId")]
    default_scenario_id: Option<String>,
    #[serde(default, rename = "defaultSystemPromptId")]
    default_system_prompt_id: Option<String>,
    #[serde(default, rename = "characterDocumentMountPointId")]
    character_document_mount_point_id: Option<String>,
    #[serde(default, rename = "canDressThemselves")]
    can_dress_themselves: Option<bool>,
    #[serde(default, rename = "canCreateOutfits")]
    can_create_outfits: Option<bool>,
    #[serde(default, rename = "systemTransparency")]
    system_transparency: Option<bool>,
    #[serde(default, rename = "coreWhisperEnabled")]
    core_whisper_enabled: Option<bool>,
    #[serde(default, rename = "canBeCarina")]
    can_be_carina: Option<bool>,
    #[serde(default, rename = "partnerLinks")]
    partner_links: Option<Vec<PartnerLink>>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default, rename = "avatarOverrides")]
    avatar_overrides: Option<Vec<AvatarOverride>>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/characters-slim-tier2.json")
}

#[test]
fn characters_slim_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHARACTERS_SLIM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_CHARACTERS_SLIM to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHARACTERS_SLIM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARACTERS_SLIM to the seed fixture .db (see header).");
            return;
        }
    };

    // Parse the committed spec (pepper + op sequence) — same file the oracle used.
    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    // Parse the oracle's expected post-op dump (one NDJSON object).
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    // Work on a fresh copy of the seed fixture so the shared file stays pristine.
    let work =
        std::env::temp_dir().join(format!("qt-characters-slim-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.characters();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &CharacterCreate {
                            user_id: data.user_id.clone(),
                            name: data.name.clone(),
                            default_image_id: data.default_image_id.clone(),
                            default_connection_profile_id: data
                                .default_connection_profile_id
                                .clone(),
                            default_partner_id: data.default_partner_id.clone(),
                            default_roleplay_template_id: data.default_roleplay_template_id.clone(),
                            default_image_profile_id: data.default_image_profile_id.clone(),
                            silly_tavern_data: data.silly_tavern_data.clone(),
                            is_favorite: data.is_favorite,
                            npc: data.npc,
                            controlled_by: data.controlled_by.clone(),
                            default_agent_mode_enabled: data.default_agent_mode_enabled,
                            default_help_tools_enabled: data.default_help_tools_enabled,
                            default_timestamp_config: data.default_timestamp_config.clone(),
                            default_scenario_id: data.default_scenario_id.clone(),
                            default_system_prompt_id: data.default_system_prompt_id.clone(),
                            character_document_mount_point_id: data
                                .character_document_mount_point_id
                                .clone(),
                            can_dress_themselves: data.can_dress_themselves,
                            can_create_outfits: data.can_create_outfits,
                            system_transparency: data.system_transparency,
                            core_whisper_enabled: data.core_whisper_enabled,
                            can_be_carina: data.can_be_carina,
                            partner_links: data.partner_links.clone(),
                            tags: data.tags.clone(),
                            avatar_overrides: data.avatar_overrides.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("characters.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &CharacterUpdate {
                                user_id: data.user_id.clone(),
                                name: data.name.clone(),
                                default_image_id: data.default_image_id.clone(),
                                default_connection_profile_id: data
                                    .default_connection_profile_id
                                    .clone(),
                                default_partner_id: data.default_partner_id.clone(),
                                default_roleplay_template_id: data
                                    .default_roleplay_template_id
                                    .clone(),
                                default_image_profile_id: data.default_image_profile_id.clone(),
                                silly_tavern_data: data.silly_tavern_data.clone(),
                                is_favorite: data.is_favorite,
                                npc: data.npc,
                                controlled_by: data.controlled_by.clone(),
                                default_agent_mode_enabled: data.default_agent_mode_enabled,
                                default_help_tools_enabled: data.default_help_tools_enabled,
                                default_timestamp_config: data.default_timestamp_config.clone(),
                                default_scenario_id: data.default_scenario_id.clone(),
                                default_system_prompt_id: data.default_system_prompt_id.clone(),
                                character_document_mount_point_id: data
                                    .character_document_mount_point_id
                                    .clone(),
                                can_dress_themselves: data.can_dress_themselves,
                                can_create_outfits: data.can_create_outfits,
                                system_transparency: data.system_transparency,
                                core_whisper_enabled: data.core_whisper_enabled,
                                can_be_carina: data.can_be_carina,
                                partner_links: data.partner_links.clone(),
                                tags: data.tags.clone(),
                                avatar_overrides: data.avatar_overrides.clone(),
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("characters.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("characters.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("characters", "id")
        .expect("dump characters");

    let _ = std::fs::remove_file(&work);

    // Structural diff: table + columns + rows must match (ignore the oracle's
    // "case" label). assert_eq on serde_json::Value is order-independent for
    // object keys and exact for the row arrays (both sides sorted by id).
    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");
    eprintln!("OK: characters slim tier-2 matched oracle ({n} rows).");
}
