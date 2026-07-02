//! Tier-1 differential test: system-prompt builder (chat-orchestration wave
//! 2.1). Covers buildIdentityStack, buildPublicIdentityCard, buildSystemPrompt,
//! buildOtherParticipantsInfo, buildIdentityReinforcement — pure functions,
//! exact string / structural equality against v4's real builders.
//!
//! Generate the oracle output (TZ=UTC required — the undefined-timezone offset
//! path and the pinned clock both assume it):
//!   cd ~/source/quilltap-server
//!   TZ=UTC npx tsx ~/source/quilltap-v5/harness/oracle/cases/system-prompt.ts \
//!     > /tmp/oracle-system-prompt.ndjson
//! Run:
//!   QT_ORACLE_SYSTEM_PROMPT=/tmp/oracle-system-prompt.ndjson \
//!     cargo test -p quilltap-harness --test system_prompt_equivalence

use std::collections::HashMap;

use quilltap_core::chat_timestamp::{TimestampConfig, TimestampFormat, TimestampMode};
use quilltap_core::system_prompt::{
    build_identity_reinforcement, build_identity_stack, build_other_participants_info,
    build_public_identity_card, build_system_prompt, BuildIdentityStackOptions,
    BuildSystemPromptOptions, Character, ChatParticipant, ParticipantStatus, PhysicalDescription,
    Pronouns, ScenarioEntry, SystemPromptEntry, UserCharacter,
};
use serde::Deserialize;

// ---- wire shapes ----------------------------------------------------------

#[derive(Deserialize)]
struct WirePronouns {
    subject: String,
    object: String,
    possessive: String,
}

#[derive(Deserialize)]
struct WireSystemPrompt {
    id: String,
    content: String,
    #[serde(rename = "isDefault", default)]
    is_default: bool,
}

#[derive(Deserialize)]
struct WireScenario {
    content: String,
}

#[derive(Deserialize)]
struct WirePhysical {
    name: String,
    #[serde(rename = "usageContext", default)]
    usage_context: Option<String>,
    #[serde(rename = "shortPrompt", default)]
    short_prompt: Option<String>,
    #[serde(rename = "mediumPrompt", default)]
    medium_prompt: Option<String>,
    #[serde(rename = "longPrompt", default)]
    long_prompt: Option<String>,
    #[serde(rename = "completePrompt", default)]
    complete_prompt: Option<String>,
    #[serde(rename = "fullDescription", default)]
    full_description: Option<String>,
}

#[derive(Deserialize, Default)]
struct WireCharacter {
    name: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    identity: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    manifesto: Option<String>,
    #[serde(default)]
    personality: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    pronouns: Option<WirePronouns>,
    #[serde(rename = "physicalDescription", default)]
    physical_description: Option<WirePhysical>,
    #[serde(rename = "exampleDialogues", default)]
    example_dialogues: Option<String>,
    #[serde(default)]
    scenarios: Vec<WireScenario>,
    #[serde(rename = "systemPrompts", default)]
    system_prompts: Vec<WireSystemPrompt>,
}

#[derive(Deserialize)]
struct WireUserCharacter {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
struct WireTimestampConfig {
    mode: String,
    format: String,
    #[serde(rename = "customFormat", default)]
    custom_format: Option<String>,
    #[serde(rename = "useFictionalTime", default)]
    use_fictional_time: bool,
    #[serde(rename = "fictionalBaseTimestamp", default)]
    fictional_base_timestamp: Option<String>,
    #[serde(rename = "fictionalBaseRealTime", default)]
    fictional_base_real_time: Option<String>,
    #[serde(rename = "autoPrepend", default)]
    auto_prepend: bool,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(rename = "intervalMinutes", default)]
    interval_minutes: i64,
}

#[derive(Deserialize)]
struct WireParticipant {
    id: String,
    #[serde(rename = "type")]
    type_: String,
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Deserialize)]
struct WireOtherInfo {
    name: String,
    #[serde(default)]
    aliases: Option<Vec<String>>,
    #[serde(default)]
    pronouns: Option<WirePronouns>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

// The SystemPrompt variant is a wide wire shape; boxing every variant to
// equalize sizes buys nothing for a deserialize-only test enum.
#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "identityStack")]
    IdentityStack {
        id: String,
        character: WireCharacter,
        #[serde(rename = "userCharacter", default)]
        user_character: Option<WireUserCharacter>,
        #[serde(rename = "selectedSystemPromptId", default)]
        selected_system_prompt_id: Option<String>,
        #[serde(rename = "scenarioText", default)]
        scenario_text: Option<String>,
        out: String,
    },
    #[serde(rename = "publicCard")]
    PublicCard {
        id: String,
        character: WireCharacter,
        #[serde(rename = "userName", default)]
        user_name: Option<String>,
        out: String,
    },
    #[serde(rename = "systemPrompt")]
    SystemPrompt {
        id: String,
        character: WireCharacter,
        #[serde(rename = "userCharacter", default)]
        user_character: Option<WireUserCharacter>,
        #[serde(rename = "roleplayTemplate", default)]
        roleplay_template: Option<WireRoleplay>,
        #[serde(rename = "toolInstructions", default)]
        tool_instructions: Option<String>,
        #[serde(rename = "selectedSystemPromptId", default)]
        selected_system_prompt_id: Option<String>,
        #[serde(rename = "timestampConfig", default)]
        timestamp_config: Option<WireTimestampConfig>,
        #[serde(rename = "isInitialMessage", default)]
        is_initial_message: bool,
        #[serde(default)]
        timezone: Option<String>,
        #[serde(rename = "scenarioText", default)]
        scenario_text: Option<String>,
        #[serde(rename = "precompiledIdentityStack", default)]
        precompiled_identity_stack: Option<String>,
        #[serde(rename = "nowMs")]
        now_ms: i64,
        #[serde(rename = "localOffsetMin")]
        local_offset_min: i64,
        out: String,
    },
    #[serde(rename = "otherParticipants")]
    OtherParticipants {
        id: String,
        #[serde(rename = "respondingParticipantId")]
        responding_participant_id: String,
        participants: Vec<WireParticipant>,
        characters: HashMap<String, WireCharacter>,
        out: Vec<WireOtherInfo>,
    },
    #[serde(rename = "reinforcement")]
    Reinforcement {
        id: String,
        #[serde(rename = "characterName")]
        character_name: String,
        out: String,
    },
}

#[derive(Deserialize)]
struct WireRoleplay {
    #[serde(rename = "systemPrompt")]
    system_prompt: String,
}

// ---- conversions ----------------------------------------------------------

fn to_pronouns(w: &WirePronouns) -> Pronouns {
    Pronouns {
        subject: w.subject.clone(),
        object: w.object.clone(),
        possessive: w.possessive.clone(),
    }
}

fn to_char(w: &WireCharacter) -> Character {
    Character {
        name: w.name.clone(),
        title: w.title.clone(),
        identity: w.identity.clone(),
        description: w.description.clone(),
        manifesto: w.manifesto.clone(),
        personality: w.personality.clone(),
        aliases: w.aliases.clone(),
        pronouns: w.pronouns.as_ref().map(to_pronouns),
        physical_description: w
            .physical_description
            .as_ref()
            .map(|p| PhysicalDescription {
                name: p.name.clone(),
                usage_context: p.usage_context.clone(),
                short_prompt: p.short_prompt.clone(),
                medium_prompt: p.medium_prompt.clone(),
                long_prompt: p.long_prompt.clone(),
                complete_prompt: p.complete_prompt.clone(),
                full_description: p.full_description.clone(),
            }),
        example_dialogues: w.example_dialogues.clone(),
        scenarios: w
            .scenarios
            .iter()
            .map(|s| ScenarioEntry {
                content: s.content.clone(),
            })
            .collect(),
        system_prompts: w
            .system_prompts
            .iter()
            .map(|p| SystemPromptEntry {
                id: p.id.clone(),
                content: p.content.clone(),
                is_default: p.is_default,
            })
            .collect(),
    }
}

fn to_user(w: &Option<WireUserCharacter>) -> Option<UserCharacter> {
    w.as_ref().map(|u| UserCharacter {
        name: u.name.clone(),
        description: u.description.clone().unwrap_or_default(),
    })
}

fn to_status(s: &Option<String>) -> Option<ParticipantStatus> {
    s.as_deref().map(|s| match s {
        "active" => ParticipantStatus::Active,
        "silent" => ParticipantStatus::Silent,
        "absent" => ParticipantStatus::Absent,
        "removed" => ParticipantStatus::Removed,
        other => panic!("unknown participant status: {other}"),
    })
}

fn to_ts_config(w: &WireTimestampConfig) -> TimestampConfig {
    TimestampConfig {
        mode: match w.mode.as_str() {
            "NONE" => TimestampMode::None,
            "START_ONLY" => TimestampMode::StartOnly,
            "EVERY_MESSAGE" => TimestampMode::EveryMessage,
            "EVERY_N_MINUTES" => TimestampMode::EveryNMinutes,
            other => panic!("unknown timestamp mode: {other}"),
        },
        format: match w.format.as_str() {
            "ISO8601" => TimestampFormat::Iso8601,
            "FRIENDLY" => TimestampFormat::Friendly,
            "DATE_ONLY" => TimestampFormat::DateOnly,
            "TIME_ONLY" => TimestampFormat::TimeOnly,
            "CUSTOM" => TimestampFormat::Custom,
            other => panic!("unknown timestamp format: {other}"),
        },
        custom_format: w.custom_format.clone(),
        use_fictional_time: w.use_fictional_time,
        fictional_base_timestamp: w.fictional_base_timestamp.clone(),
        fictional_base_real_time: w.fictional_base_real_time.clone(),
        auto_prepend: w.auto_prepend,
        timezone: w.timezone.clone(),
        interval_minutes: w.interval_minutes,
    }
}

/// Assert the port's `OtherParticipantInfo` list matches the oracle's, field by
/// field (aliases/pronouns/description/status all diffed).
fn assert_others(
    id: &str,
    got: &[quilltap_core::system_prompt::OtherParticipantInfo],
    want: &[WireOtherInfo],
) {
    assert_eq!(got.len(), want.len(), "otherParticipants '{id}' length");
    for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        assert_eq!(g.name, w.name, "others '{id}'[{i}] name");
        assert_eq!(g.aliases, w.aliases, "others '{id}'[{i}] aliases");
        let want_pr = w.pronouns.as_ref().map(to_pronouns);
        assert_eq!(g.pronouns, want_pr, "others '{id}'[{i}] pronouns");
        assert_eq!(
            g.description, w.description,
            "others '{id}'[{i}] description"
        );
        assert_eq!(g.status, to_status(&w.status), "others '{id}'[{i}] status");
    }
}

#[test]
fn system_prompt_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_SYSTEM_PROMPT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_SYSTEM_PROMPT to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::IdentityStack {
                id,
                character,
                user_character,
                selected_system_prompt_id,
                scenario_text,
                out,
            } => {
                let ch = to_char(&character);
                let uc = to_user(&user_character);
                let got = build_identity_stack(&BuildIdentityStackOptions {
                    character: &ch,
                    user_character: uc.as_ref(),
                    selected_system_prompt_id: selected_system_prompt_id.as_deref(),
                    scenario_text: scenario_text.as_deref(),
                });
                assert_eq!(got, out, "identityStack '{id}'");
            }
            Row::PublicCard {
                id,
                character,
                user_name,
                out,
            } => {
                let ch = to_char(&character);
                let got = build_public_identity_card(&ch, user_name.as_deref());
                assert_eq!(got, out, "publicCard '{id}'");
            }
            Row::SystemPrompt {
                id,
                character,
                user_character,
                roleplay_template,
                tool_instructions,
                selected_system_prompt_id,
                timestamp_config,
                is_initial_message,
                timezone,
                scenario_text,
                precompiled_identity_stack,
                now_ms,
                local_offset_min,
                out,
            } => {
                let ch = to_char(&character);
                let uc = to_user(&user_character);
                let ts = timestamp_config.as_ref().map(to_ts_config);
                let got = build_system_prompt(&BuildSystemPromptOptions {
                    character: &ch,
                    user_character: uc.as_ref(),
                    roleplay_template: roleplay_template.as_ref().map(|r| r.system_prompt.as_str()),
                    tool_instructions: tool_instructions.as_deref(),
                    selected_system_prompt_id: selected_system_prompt_id.as_deref(),
                    timestamp_config: ts.as_ref(),
                    is_initial_message,
                    timezone: timezone.as_deref(),
                    scenario_text: scenario_text.as_deref(),
                    precompiled_identity_stack: precompiled_identity_stack.as_deref(),
                    now_ms,
                    local_offset_minutes: local_offset_min,
                })
                .unwrap_or_else(|e| panic!("systemPrompt '{id}' errored: {e}"));
                assert_eq!(got, out, "systemPrompt '{id}'");
            }
            Row::OtherParticipants {
                id,
                responding_participant_id,
                participants,
                characters,
                out,
            } => {
                let char_map: HashMap<String, Character> = characters
                    .iter()
                    .map(|(k, v)| (k.clone(), to_char(v)))
                    .collect();
                let ports: Vec<ChatParticipant> = participants
                    .iter()
                    .map(|p| ChatParticipant {
                        id: p.id.clone(),
                        is_character: p.type_ == "CHARACTER",
                        character_id: Some(p.character_id.clone()),
                        status: to_status(&p.status),
                    })
                    .collect();
                let got =
                    build_other_participants_info(&responding_participant_id, &ports, &char_map);
                assert_others(&id, &got, &out);
            }
            Row::Reinforcement {
                id,
                character_name,
                out,
            } => {
                let got = build_identity_reinforcement(&character_name);
                assert_eq!(got, out, "reinforcement '{id}'");
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: system-prompt matched oracle ({count} rows).");
}
