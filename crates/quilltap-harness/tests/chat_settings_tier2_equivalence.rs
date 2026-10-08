//! Tier-2 differential test: the `chat_settings` repo (Phase-2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-chat-settings-fixture.ts), run the SAME create /
//! update / delete op sequence from the committed spec, dump the `chat_settings`
//! table canonically, and assert the post-op state is identical. Ids and
//! timestamps are pinned on both sides, so the dumps must match with zero
//! normalization.
//!
//! This banks the WIDEST JSON-object surface yet: two UUID TEXT, one enum TEXT +
//! one plain-string TEXT, a record/map JSON column (tagStyles, kept {}), ~15
//! nested typed-struct JSON columns reproduced in SCHEMA field order, five
//! nullable UUID/string TEXT columns, six boolean columns, and the FIRST
//! INTEGER-affinity number column (sidebarWidth: .min(256).max(512) — both bounds
//! integer → INTEGER, unlike the prior min-only/bare REAL number columns).
//!
//! The nested objects in the spec deserialize directly into the port's typed
//! structs (which derive Deserialize), so the serde-struct field order both
//! deserializes the input and — on re-serialize in `create`/`update` — emits the
//! schema key order v4's Zod `.parse` produces.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-chat-settings-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chat-settings-fixture.ts
//!   QT_FIXTURE_CHAT_SETTINGS=/tmp/qt-chat-settings-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-settings-tier2.ts \
//!     > /tmp/oracle-chat-settings.ndjson
//! Run:
//!   QT_ORACLE_CHAT_SETTINGS=/tmp/oracle-chat-settings.ndjson \
//!   QT_FIXTURE_CHAT_SETTINGS=/tmp/qt-chat-settings-fixture.db \
//!     cargo test -p quilltap-harness --test chat_settings_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::chat_settings::{
    AgentModeSettings, AnswerConfirmationSettings, AutoHousekeepingSettings, AutoLockSettings,
    AutonomousRoomSettings, ChatSettingsCreate, ChatSettingsUpdate, CheapLlmSettings,
    ContextCompressionSettings, CoreWhisperSettings, CreateOptions, ImpersonationVoiceMode,
    LlmLoggingSettings, MemoryCascadePreferences, MemoryExtractionLimits, SmartTypographySettings,
    StoryBackgroundsSettings, ThemePreference, ThinkingDisplaySettings, TimestampConfig,
    TokenDisplaySettings, WardrobeImageSettings,
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
// The `create` payload mirrors chat_settings' ~33-column surface, so its variant
// is naturally far larger than `update`/`delete`. This is a one-shot test fixture
// parsed once per case — the size asymmetry is immaterial here.
#[allow(clippy::large_enum_variant)]
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
    /// P4.D255: a raw plant, identical on both sides.
    #[serde(rename = "sql")]
    Sql { sql: String, params: Vec<Value> },
    /// P4.D255: hydrate one user's row through the port's POSITIONAL read.
    #[serde(rename = "read")]
    Read {
        #[serde(rename = "userId")]
        user_id: String,
    },
}

/// The full create input — every persisted column, nested objects deserialized
/// straight into the port's typed structs (schema field order).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateData {
    user_id: String,
    avatar_display_mode: String,
    avatar_display_style: String,
    tag_styles: Value,
    // v4's field is `cheapLLMSettings` (uppercase acronym) — camelCase rename
    // would yield `cheapLlmSettings`, so pin the exact key.
    #[serde(rename = "cheapLLMSettings")]
    cheap_llm_settings: CheapLlmSettings,
    #[serde(default)]
    image_description_profile_id: Option<String>,
    // (`uncensoredImageDescriptionProfileId` left the schema at v4 `3b463d6b1`;
    // a spec row still carrying it is a strip probe, ignored here as v4's Zod
    // drops it.)
    #[serde(default)]
    default_roleplay_template_id: Option<String>,
    theme_preference: ThemePreference,
    sidebar_width: i64,
    default_timestamp_config: TimestampConfig,
    memory_cascade_preferences: MemoryCascadePreferences,
    auto_housekeeping_settings: AutoHousekeepingSettings,
    memory_extraction_limits: MemoryExtractionLimits,
    autonomous_room_settings: AutonomousRoomSettings,
    token_display_settings: TokenDisplaySettings,
    context_compression_settings: ContextCompressionSettings,
    llm_logging_settings: LlmLoggingSettings,
    auto_detect_rng: bool,
    custom_tools: bool,
    composition_mode_default: bool,
    composer_spellcheck: bool,
    composer_emoji: bool,
    composer_unicode: bool,
    /// P4.D251 (v4 `07b8f0209`): the voice MODE that replaced P4.D179's
    /// boolean. The spec's `#[serde(default)]`-free shape means every create
    /// op in `chat-settings-tier2.json` must name it.
    impersonation_voice_mode: ImpersonationVoiceMode,
    text_replacements_enabled: bool,
    auto_scroll_on_response_complete: bool,
    agent_mode_settings: AgentModeSettings,
    core_whisper: CoreWhisperSettings,
    thinking_display: ThinkingDisplaySettings,
    answer_confirmation_settings: AnswerConfirmationSettings,
    smart_typography_settings: SmartTypographySettings,
    story_backgrounds_settings: StoryBackgroundsSettings,
    /// P4.D255: absent → NULL (v4's `.optional()` create).
    #[serde(default)]
    wardrobe_image_settings: Option<WardrobeImageSettings>,
    /// v4 `3b463d6b1` (#76): the raw object (absent → the `.default()`
    /// literal), Zod-parsed by the repository write.
    #[serde(default)]
    concierge_settings: Option<Value>,
    auto_lock_settings: AutoLockSettings,
    #[serde(default)]
    timezone: Option<String>,
}

#[derive(Deserialize)]
struct CreateOpts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

/// The update patch — the representative subset the port exposes.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateData {
    #[serde(default)]
    avatar_display_mode: Option<String>,
    #[serde(default)]
    avatar_display_style: Option<String>,
    #[serde(default)]
    tag_styles: Option<Value>,
    #[serde(default, rename = "cheapLLMSettings")]
    cheap_llm_settings: Option<CheapLlmSettings>,
    #[serde(default)]
    image_description_profile_id: Option<String>,
    #[serde(default)]
    default_roleplay_template_id: Option<String>,
    #[serde(default)]
    theme_preference: Option<ThemePreference>,
    #[serde(default)]
    sidebar_width: Option<i64>,
    #[serde(default)]
    concierge_settings: Option<Value>,
    #[serde(default)]
    auto_lock_settings: Option<AutoLockSettings>,
    #[serde(default)]
    auto_detect_rng: Option<bool>,
    custom_tools: Option<bool>,
    #[serde(default)]
    composition_mode_default: Option<bool>,
    #[serde(default)]
    composer_spellcheck: Option<bool>,
    #[serde(default)]
    composer_emoji: Option<bool>,
    #[serde(default)]
    composer_unicode: Option<bool>,
    #[serde(default)]
    impersonation_voice_mode: Option<ImpersonationVoiceMode>,
    #[serde(default)]
    text_replacements_enabled: Option<bool>,
    #[serde(default)]
    auto_scroll_on_response_complete: Option<bool>,
    #[serde(default)]
    answer_confirmation_settings: Option<AnswerConfirmationSettings>,
    #[serde(default)]
    smart_typography_settings: Option<SmartTypographySettings>,
    #[serde(default)]
    wardrobe_image_settings: Option<WardrobeImageSettings>,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-settings-tier2.json")
}

#[test]
fn chat_settings_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHAT_SETTINGS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHAT_SETTINGS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHAT_SETTINGS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_CHAT_SETTINGS to the seed fixture .db (see test header)."
            );
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
    let scratch_dir = tempfile::Builder::new()
        .prefix("qt-chat-settings-rust-")
        .tempdir()
        .expect("tempdir");
    let work = scratch_dir.path().join("qt-chat-settings-rust.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    let mut reads: Vec<Option<String>> = Vec::new();
    {
        let repo = writer.chat_settings();
        for op in spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &ChatSettingsCreate {
                            user_id: data.user_id,
                            avatar_display_mode: data.avatar_display_mode,
                            avatar_display_style: data.avatar_display_style,
                            tag_styles: data.tag_styles,
                            cheap_llm_settings: data.cheap_llm_settings,
                            image_description_profile_id: data.image_description_profile_id,
                            default_roleplay_template_id: data.default_roleplay_template_id,
                            theme_preference: data.theme_preference,
                            sidebar_width: data.sidebar_width,
                            default_timestamp_config: data.default_timestamp_config,
                            memory_cascade_preferences: data.memory_cascade_preferences,
                            auto_housekeeping_settings: data.auto_housekeeping_settings,
                            memory_extraction_limits: data.memory_extraction_limits,
                            autonomous_room_settings: data.autonomous_room_settings,
                            token_display_settings: data.token_display_settings,
                            context_compression_settings: data.context_compression_settings,
                            llm_logging_settings: data.llm_logging_settings,
                            auto_detect_rng: data.auto_detect_rng,
                            custom_tools: data.custom_tools,
                            composition_mode_default: data.composition_mode_default,
                            composer_spellcheck: data.composer_spellcheck,
                            composer_emoji: data.composer_emoji,
                            composer_unicode: data.composer_unicode,
                            impersonation_voice_mode: data.impersonation_voice_mode,
                            text_replacements_enabled: data.text_replacements_enabled,
                            auto_scroll_on_response_complete: data.auto_scroll_on_response_complete,
                            agent_mode_settings: data.agent_mode_settings,
                            core_whisper: data.core_whisper,
                            thinking_display: data.thinking_display,
                            answer_confirmation_settings: data.answer_confirmation_settings,
                            smart_typography_settings: data.smart_typography_settings,
                            story_backgrounds_settings: data.story_backgrounds_settings,
                            wardrobe_image_settings: data.wardrobe_image_settings,
                            concierge_settings: data.concierge_settings,
                            auto_lock_settings: data.auto_lock_settings,
                            timezone: data.timezone,
                        },
                        &CreateOptions {
                            id: options.id,
                            created_at: options.created_at,
                            updated_at: options.updated_at,
                        },
                    )
                    .expect("chat_settings.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            &id,
                            &ChatSettingsUpdate {
                                avatar_display_mode: data.avatar_display_mode,
                                avatar_display_style: data.avatar_display_style,
                                tag_styles: data.tag_styles,
                                cheap_llm_settings: data.cheap_llm_settings,
                                image_description_profile_id: data.image_description_profile_id,
                                default_roleplay_template_id: data.default_roleplay_template_id,
                                theme_preference: data.theme_preference,
                                sidebar_width: data.sidebar_width,
                                concierge_settings: data.concierge_settings,
                                auto_lock_settings: data.auto_lock_settings,
                                auto_detect_rng: data.auto_detect_rng,
                                custom_tools: data.custom_tools,
                                composition_mode_default: data.composition_mode_default,
                                composer_spellcheck: data.composer_spellcheck,
                                composer_emoji: data.composer_emoji,
                                composer_unicode: data.composer_unicode,
                                impersonation_voice_mode: data.impersonation_voice_mode,
                                text_replacements_enabled: data.text_replacements_enabled,
                                auto_scroll_on_response_complete: data
                                    .auto_scroll_on_response_complete,
                                answer_confirmation_settings: data.answer_confirmation_settings,
                                smart_typography_settings: data.smart_typography_settings,
                                wardrobe_image_settings: data.wardrobe_image_settings,
                                timezone: data.timezone,
                                updated_at: data.updated_at,
                            },
                        )
                        .expect("chat_settings.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(&id).expect("chat_settings.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
                Op::Sql { sql, params } => {
                    let binds: Vec<rusqlite::types::Value> = params
                        .iter()
                        .map(|v| match v {
                            Value::Null => rusqlite::types::Value::Null,
                            Value::String(s) => rusqlite::types::Value::Text(s.clone()),
                            other => panic!("unsupported sql param {other}"),
                        })
                        .collect();
                    writer
                        .connection()
                        .execute(&sql, rusqlite::params_from_iter(binds))
                        .expect("sql plant");
                }
                Op::Read { user_id } => {
                    let row = quilltap_core::db::chat_settings::find_by_user_id(
                        writer.connection(),
                        &user_id,
                    )
                    .expect("find_by_user_id");
                    reads.push(row.map(|r| serde_json::to_string(&r).unwrap()));
                }
            }
        }
    }

    let got = writer
        .dump_table_json("chat_settings", "id")
        .expect("dump chat_settings");

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

    // P4.D255: the READ leg — each hydrated row's JSON text, byte for byte
    // (key order included: the port's read is positional).
    let oracle_reads: Vec<Option<String>> = oracle["reads"]
        .as_array()
        .expect("oracle reads (regenerate: the P4.D255 case emits them)")
        .iter()
        .map(|v| v.as_str().map(str::to_string))
        .collect();
    assert!(!oracle_reads.is_empty(), "the oracle's read leg is EMPTY");
    assert_eq!(reads.len(), oracle_reads.len(), "read op count");
    for (i, (mine, theirs)) in reads.iter().zip(&oracle_reads).enumerate() {
        assert_eq!(mine, theirs, "read #{i} diverged");
    }

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");
    eprintln!("OK: chat_settings tier-2 matched oracle ({n} rows).");
}
