//! Tier-2 differential test: v4's `CharactersRepository.create` (Phase-2, the
//! store-backed capstone sub-unit 3b — the keystone create integration).
//!
//! Both sides start from the SAME pair of EMPTY fixtures (a MAIN db with the slim
//! `characters` table + a MOUNT-INDEX db with the store tables), create the SAME
//! character, then SIX tables are structural-diffed: the MAIN slim `characters`
//! row + the MOUNT-INDEX store tables (`doc_mount_points` / `_folders` / `_files` /
//! `_documents` / `_file_links`). The Rust port drives
//! [`character_vault::create_character`] over two writers; v4 drives the real
//! `repos.characters.create` (see the oracle).
//!
//! Minted-values remap with ONE shared id-map across all six tables (characters →
//! points → folders → files → documents → links, rows in natural-key order). NOTHING
//! is pinned: the character id, the mount point id, and every file/document/link/
//! folder id are minted, so every FK verifies by RELATIONSHIP —
//! `characters.characterDocumentMountPointId` → the mount point, `link.fileId` →
//! `file.id`, `document.fileId` → `file.id`, `folder.mountPointId` → the store.
//! Timestamps → `<ts>`; the link `chunkCount` → `<cc>` (a v4-only `reindexSingleFile`
//! artifact); `doc_mount_chunks` is excluded.
//!
//! Banks: the 6-step create (slim row + provision + scaffold + project + link), the
//! 7 scaffold folders, the managed-field overwrite of the five identity markdown
//! files + `properties.json` (the scaffold defaults for `physical-*` survive — no
//! physicalDescription), the orphan-on-rewrite default-`properties.json` file/
//! document row, and the one systemPrompt + one scenario projected into `Prompts/`
//! and `Scenarios/`.
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CHARCREATE_MAIN=/tmp/qt-charcreate-main.db \
//!   QT_FIXTURE_CHARCREATE_MOUNT=/tmp/qt-charcreate-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-characters-create-fixture.ts
//!   QT_FIXTURE_CHARCREATE_MAIN=/tmp/qt-charcreate-main.db \
//!   QT_FIXTURE_CHARCREATE_MOUNT=/tmp/qt-charcreate-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-create.ts > /tmp/oracle-charcreate.ndjson
//! Run:
//!   QT_ORACLE_CHARCREATE=/tmp/oracle-charcreate.ndjson \
//!   QT_FIXTURE_CHARCREATE_MAIN=/tmp/qt-charcreate-main.db \
//!   QT_FIXTURE_CHARCREATE_MOUNT=/tmp/qt-charcreate-mount.db \
//!     cargo test -p quilltap-harness --test characters_create_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::character_vault::create_character;
use quilltap_core::db::characters::{
    AvatarOverride, CharacterCreate, PartnerLink, TimestampConfig,
};
use quilltap_core::db::vault_character_write::CharacterVaultWriteInput;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    character: Value,
}

/// The slim (non-managed) subset of the corpus character. The vault-managed keys
/// in the same object are ignored (serde drops unknown fields).
#[derive(Deserialize)]
struct SlimData {
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

impl SlimData {
    fn into_create(self) -> CharacterCreate {
        CharacterCreate {
            user_id: self.user_id,
            name: self.name,
            default_image_id: self.default_image_id,
            default_connection_profile_id: self.default_connection_profile_id,
            default_partner_id: self.default_partner_id,
            default_roleplay_template_id: self.default_roleplay_template_id,
            default_image_profile_id: self.default_image_profile_id,
            silly_tavern_data: self.silly_tavern_data,
            is_favorite: self.is_favorite,
            npc: self.npc,
            controlled_by: self.controlled_by,
            default_agent_mode_enabled: self.default_agent_mode_enabled,
            default_help_tools_enabled: self.default_help_tools_enabled,
            default_timestamp_config: self.default_timestamp_config,
            default_scenario_id: self.default_scenario_id,
            default_system_prompt_id: self.default_system_prompt_id,
            // create always provisions a fresh vault; the FK is nulled internally.
            character_document_mount_point_id: None,
            can_dress_themselves: self.can_dress_themselves,
            can_create_outfits: self.can_create_outfits,
            system_transparency: self.system_transparency,
            core_whisper_enabled: self.core_whisper_enabled,
            can_be_carina: self.can_be_carina,
            partner_links: self.partner_links,
            tags: self.tags,
            avatar_overrides: self.avatar_overrides,
        }
    }
}

/// Per-table normalization spec. `from_mount` = read from the mount-index writer
/// (else main). The slice order is the canonical walk order for the shared id-remap.
struct TableSpec {
    table: &'static str,
    oracle_key: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
    from_mount: bool,
    pin_chunk_count: bool,
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "characters",
        oracle_key: "characters",
        order_by: "name",
        id_columns: &["id", "characterDocumentMountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: false,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_points",
        oracle_key: "points",
        order_by: "name",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_folders",
        oracle_key: "folders",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_files",
        oracle_key: "files",
        order_by: "sha256",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_documents",
        oracle_key: "documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
        from_mount: true,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_file_links",
        oracle_key: "links",
        order_by: "relativePath",
        id_columns: &["id", "fileId", "folderId", "mountPointId"],
        ts_columns: &[
            "lastModified",
            "descriptionUpdatedAt",
            "createdAt",
            "updatedAt",
        ],
        from_mount: true,
        pin_chunk_count: true,
    },
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/characters-create-tier2.json")
}

fn normalize_table(dump: &mut Value, spec: &TableSpec, id_map: &mut HashMap<String, String>) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{}: dump has no rows array", spec.table));

    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{}: row is not an object", spec.table));

        for col in spec.id_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let next = format!("ID_{}", id_map.len());
                let token = id_map.entry(raw.clone()).or_insert(next).clone();
                obj.insert((*col).to_string(), Value::String(token));
            }
        }
        for col in spec.ts_columns {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
        if spec.pin_chunk_count {
            obj.insert("chunkCount".to_string(), Value::String("<cc>".to_string()));
        }
    }
}

fn normalize_all(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

#[test]
fn characters_create_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHARCREATE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHARCREATE to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_CHARCREATE_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARCREATE_MAIN to the main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_CHARCREATE_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHARCREATE_MOUNT to the mount fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    // Deserialize the corpus character into both halves: the slim row and the
    // vault-managed inputs (each ignores the other's keys).
    let slim: SlimData =
        serde_json::from_value(spec.character.clone()).expect("parse slim character data");
    let vault: CharacterVaultWriteInput =
        serde_json::from_value(spec.character.clone()).expect("parse vault character data");

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-charcreate-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-charcreate-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));

    // The op under test: create the character end-to-end.
    create_character(
        main.connection(),
        mount.connection(),
        &slim.into_create(),
        &vault,
    )
    .unwrap_or_else(|e| panic!("create_character: {e}"));

    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            let w = if s.from_mount { &mount } else { &main };
            w.dump_table_json(s.table, s.order_by)
                .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
        })
        .collect();
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            oracle
                .get(s.oracle_key)
                .cloned()
                .unwrap_or_else(|| panic!("oracle missing dump for {}", s.oracle_key))
        })
        .collect();

    normalize_all(&mut got);
    normalize_all(&mut want);

    for (i, s) in TABLES.iter().enumerate() {
        assert_eq!(got[i]["table"], want[i]["table"], "{}: table name", s.table);
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{}: column set / order",
            s.table
        );
        assert_eq!(
            got[i]["rows"], want[i]["rows"],
            "{}: remapped row state diverged\n  rust:   {}\n  oracle: {}",
            s.table, got[i]["rows"], want[i]["rows"]
        );
    }

    // Sanity: the corpus produced the expected shape.
    let rows = |key: &str| {
        let i = TABLES.iter().position(|t| t.oracle_key == key).unwrap();
        got[i]["rows"].as_array().unwrap().clone()
    };
    assert_eq!(rows("characters").len(), 1, "1 character row");
    assert_eq!(rows("points").len(), 1, "1 vault mount-point row");
    assert_eq!(rows("folders").len(), 7, "7 scaffold folders");
    assert_eq!(
        rows("links").len(),
        10,
        "10 links (6 md + 2 json + 1 prompt + 1 scenario)"
    );
    // 8 live files + 1 orphaned default-properties.json (orphan-on-rewrite).
    assert_eq!(
        rows("files").len(),
        9,
        "9 files (8 live + 1 orphaned default props)"
    );
    assert_eq!(rows("documents").len(), 9, "9 documents");

    eprintln!("OK: characters create tier-2 matched oracle (6 tables, 2 DBs).");
}
