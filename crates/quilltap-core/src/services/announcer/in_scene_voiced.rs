//! In-scene voiced line rewriter — the rehearsal for an impersonated seat. v4
//! `lib/services/announcer/in-scene-voiced.ts` (`generateInSceneVoicedLine`,
//! NEW at `686954937`).
//!
//! The operator has taken a character's seat with the Salon's Impersonate button
//! (`chat.impersonatingParticipantIds` — the Bug 44 overlay, which leaves the
//! seat's `controlledBy` and its profile/prompt selections intact) and typed a
//! line. This hands that line back to the character whose seat it is and asks
//! them to say it in their own voice, given where the scene has actually got to.
//!
//! Where the off-scene rehearsal ([`super::character_voiced`]) tells the
//! character they stand outside the conversation, this one puts them in the
//! room: they get their per-turn system prompt (identity stack, roleplay
//! template, Taboo, standing instructions — deliberately NO tool instructions),
//! the tail of the transcript shaped exactly as they would see it on a real
//! turn, a Commonplace recall against the draft, and then the draft itself with
//! the instruction to restate it.
//!
//! Pure and side-effect-free apart from the provider call and its LLM log row:
//! nothing about the rehearsal is persisted, and it never throws — a failure
//! comes back as `{ success: false, error }` so the dialog can keep offering
//! "Send as written".
//!
//! ## The one deliberate read v4 does that the turn path does not
//!
//! The rehearsal reads the STORED compiled identity stack itself
//! ([`crate::services::system_prompt_compiler::get_compiled_identity_stack`]),
//! exactly as v4's `:228` does. That does NOT lift the standing P4.D103/P4.D163
//! deferral on the ORCHESTRATOR's turn-time reader (`orchestrator.rs` still
//! passes `precompiled_identity_stack: None`): the rehearsal is a new surface
//! with no deferral debt, and v4's `subprompts: precompiledIdentityStack ? null
//! : subprompts` arm is only measurable with a real stack present.
//!
//! Pinned by `in_scene_voiced_tier3_equivalence`.

use std::collections::HashMap;

use serde_json::Value;

use crate::db::runtime::Db;
use crate::jsstr::{js_trim, utf16_len};
use crate::message_attribution::{
    attribute_messages_for_character, compute_presence_windows_for_participant,
    filter_messages_by_history_access, filter_messages_by_presence_windows,
    filter_whisper_messages, AttributionMessage, AttributionParticipant, HistoryMessage, HostEvent,
};
use crate::message_formatter::{
    format_messages_for_provider, MultiCharacterMessage as FormatterMessage, WireRole,
};
use crate::model::completion::{CompletionMessage, CompletionProvider, CompletionRole};
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::subprompts::SubpromptForPrompt;
use crate::system_prompt::{
    build_system_prompt, BuildSystemPromptOptions, Character, PhysicalDescription, Pronouns,
    ScenarioEntry, SystemPromptEntry, UserCharacter,
};

use super::voice_rewrite_core::{
    build_selection, execute_voice_rewrite, recall_for_seed, ExecuteVoiceRewriteParams,
    VoiceRewriteResult,
};

/// v4 `IN_SCENE_REWRITE_WINDOW` — how many played messages of transcript the
/// character is shown. Enough for the line to answer the moment it lands in;
/// deliberately not the whole chat, which would make a rehearsal cost as much as
/// a turn. Not a setting — a rehearsal is a fixed-shape call, and the operator
/// has the proposal in front of them either way.
pub const IN_SCENE_REWRITE_WINDOW: usize = 12;

/// v4 `TASK_TYPE`. P4.D179 maps it (with `announcement-rewrite`) to the
/// `VOICE_REWRITE` log type; this module only spells the string.
pub const TASK_TYPE: &str = "impersonation-voice-rewrite";

const LOG_CONTEXT: &str = "[InSceneVoicedLine]";

/// v4 `MIN_MAX_TOKENS` / `MAX_MAX_TOKENS` — floor and ceiling on the rewrite's
/// output budget.
///
/// ⚠ The FLOOR never reaches a provider on either side: the cheap-LLM executor
/// applies v4's own `effectiveMaxTokens` floor of 2048
/// (`core-execution.ts:316`, `cheap_llm_exec.rs:442`), so anything under 2048
/// is raised before the wire. It is carried faithfully anyway because it is what
/// the service hands the executor, and that is the value the differential
/// compares.
const MIN_MAX_TOKENS: usize = 1024;
const MAX_MAX_TOKENS: usize = 4096;

/// v4 `maxTokensForSeed` — output budget for the rewrite. A proclamation fits in
/// the announcement's flat 2048, but a long dramatic paragraph does not, so the
/// ceiling follows the draft's own length (and a terse draft still gets room to
/// breathe).
///
/// `seedMarkdown.length` is JS's **UTF-16 unit** count, not scalars: an astral
/// character costs two. The caller passes the TRIMMED seed (v4 `:315`).
pub fn max_tokens_for_seed(seed_markdown: &str) -> usize {
    let half = utf16_len(seed_markdown) / 2;
    MAX_MAX_TOKENS.min(MIN_MAX_TOKENS.max(half))
}

/// v4 `REWRITE_INSTRUCTION` — the instruction that turns a draft into a turn.
///
/// Kept verbatim-preserving on purpose: dice notation is read by Pascal's
/// auto-detect, `@Name` addresses by Carina, and a Markdown link by the renderer
/// — none of them survive being paraphrased. (Carina addresses bypass the gate
/// outright on the client; this sentence is the belt to that braces.)
///
/// v4 writes it as eight lines `.join(' ')`; carried here as the ONE joined
/// string those eight lines produce, em dashes and backticks included, with
/// `scene's` a plain apostrophe (v4 escapes it only because the literal is
/// single-quoted).
const REWRITE_INSTRUCTION: &str = "It is your turn to speak in the conversation above. Below is your own rough draft of what you want to say next — the meaning and substance of it. Rewrite it in your own voice, the way you would actually say it given your personality, manner of speech, and everything that has just happened. Keep the meaning, the addressees, every specific fact, and any dice notation or `@Name` address exactly as written. Match the scene's conventions for narration and dialogue. Say only this — do not continue past it, do not answer it, and do not speak for anyone else.";

/// v4 `InSceneVoicedLineParams`. `chat`, `participant`, `character` and
/// `profile` arrive as the route's already-resolved rows.
pub struct InSceneVoicedLineParams<'a> {
    pub chat: &'a Value,
    /// The seat the operator is impersonating.
    pub participant: &'a Value,
    pub character: &'a Value,
    pub profile: &'a Value,
    pub seed_markdown: &'a str,
    /// Resolved by the caller: override → participant selection → character
    /// default → `isDefault` → first.
    pub system_prompt_id: Option<&'a str>,
    /// Resolved by the caller from `participant.selectedSubpromptIds`.
    pub subprompts: Option<&'a [SubpromptForPrompt]>,
    pub user_id: &'a str,
    /// The injected `Date.now()` (ms) the Commonplace recall's time-decay and
    /// relative-age labels read.
    pub now_ms: f64,
}

/// v4 `InSceneVoicedLineResult`.
pub type InSceneVoicedLineResult = VoiceRewriteResult;

// ---------------------------------------------------------------------------
// Small JSON readers (the `character_voiced` convention — each service keeps its
// own projection of the rows it is handed)
// ---------------------------------------------------------------------------

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}
fn b(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(Value::as_bool)
}
fn str_array(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Project the vault-overlaid character row into the prompt builder's subset.
/// (The same projection `character_voiced::to_sys_character` makes; kept
/// per-service because each reads a different slice of the row and v4's two
/// services likewise each construct their own `buildSystemPrompt` argument.)
fn to_sys_character(c: &Value) -> Character {
    let pronouns = c.get("pronouns").and_then(|p| {
        Some(Pronouns {
            subject: s(p, "subject")?,
            object: s(p, "object")?,
            possessive: s(p, "possessive")?,
        })
    });
    let physical_description = c.get("physicalDescription").and_then(|p| {
        Some(PhysicalDescription {
            name: s(p, "name")?,
            usage_context: s(p, "usageContext"),
            short_prompt: s(p, "shortPrompt"),
            medium_prompt: s(p, "mediumPrompt"),
            long_prompt: s(p, "longPrompt"),
            complete_prompt: s(p, "completePrompt"),
            full_description: s(p, "fullDescription"),
        })
    });
    Character {
        name: s(c, "name").unwrap_or_default(),
        title: s(c, "title"),
        identity: s(c, "identity"),
        description: s(c, "description"),
        manifesto: s(c, "manifesto"),
        personality: s(c, "personality"),
        aliases: str_array(c, "aliases"),
        pronouns,
        physical_description,
        example_dialogues: s(c, "exampleDialogues"),
        scenarios: c
            .get("scenarios")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|sc| ScenarioEntry {
                        content: s(sc, "content").unwrap_or_default(),
                        archived: sc.get("archived") == Some(&Value::Bool(true)),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        system_prompts: c
            .get("systemPrompts")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|sp| SystemPromptEntry {
                        id: s(sp, "id").unwrap_or_default(),
                        content: s(sp, "content").unwrap_or_default(),
                        is_default: b(sp, "isDefault").unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn participants_of(chat: &Value) -> Vec<Value> {
    chat.get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// One `type: 'message'` event → the attribution shape. v4 maps the SAME six
/// keys twice (once for `all` with `hostEvent`, once for `played` without), so
/// `with_host_event` is which of the two it is building.
fn to_attribution_message(m: &Value, with_host_event: bool) -> AttributionMessage {
    AttributionMessage {
        id: s(m, "id"),
        role: s(m, "role").unwrap_or_default(),
        content: s(m, "content").unwrap_or_default(),
        participant_id: s(m, "participantId"),
        // v4's two maps carry no `thoughtSignature`; the shape is v5's and is
        // only read by the formatter, which the rehearsal does not feed one.
        thought_signature: None,
        created_at: s(m, "createdAt"),
        target_participant_ids: m
            .get("targetParticipantIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            }),
        host_event: if with_host_event {
            m.get("hostEvent")
                .filter(|v| !v.is_null())
                .map(|he| HostEvent {
                    participant_id: s(he, "participantId"),
                    to_status: he
                        .get("toStatus")
                        .and_then(Value::as_str)
                        .and_then(|st| match st {
                            "active" => Some(crate::chat_predicates::ParticipantStatus::Active),
                            "silent" => Some(crate::chat_predicates::ParticipantStatus::Silent),
                            "absent" => Some(crate::chat_predicates::ParticipantStatus::Absent),
                            "removed" => Some(crate::chat_predicates::ParticipantStatus::Removed),
                            _ => None,
                        }),
                })
        } else {
            None
        },
    }
}

/// v4 `buildTranscriptForSeat` — shape the transcript tail the way a real turn
/// would for this seat.
///
/// Presence windows are computed from the FULL event list (the Host status
/// announcements that define them are themselves whispers, and would be gone by
/// the time the played subset is taken), then applied to the played messages.
/// Staff whispers are excluded outright — a rehearsal is about what was said in
/// the room, so there is nothing left for whisper-role normalisation to flip.
///
/// ⚠ v4 filters in the order whisper → history-access → presence; `build_context`
/// does history → whisper. The orders are NOT interchangeable in general (each
/// filter sees a different input list), so this follows v4's in-scene order
/// rather than the turn path's.
fn build_transcript_for_seat(events: &[Value], participant: &Value) -> Vec<AttributionMessage> {
    let message_events: Vec<&Value> = events
        .iter()
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("message"))
        .collect();

    let all: Vec<AttributionMessage> = message_events
        .iter()
        .map(|m| to_attribution_message(m, true))
        .collect();

    // Played messages only: the character's and the operator's own speech.
    let played: Vec<AttributionMessage> = message_events
        .iter()
        .filter(|m| {
            let role = m.get("role").and_then(Value::as_str);
            // v4 `typeof m.content === 'string' && m.content.trim().length > 0`.
            let content_ok = m
                .get("content")
                .and_then(Value::as_str)
                .is_some_and(|c| !js_trim(c).is_empty());
            m.get("systemSender") != Some(&Value::Bool(true))
                && matches!(role, Some("USER") | Some("ASSISTANT"))
                && content_ok
        })
        .map(|m| to_attribution_message(m, false))
        .collect();

    let participant_id = s(participant, "id").unwrap_or_default();

    // Whispers this seat is not party to, then the history the seat has access
    // to, then the stretches it was actually present for.
    let mask = filter_whisper_messages(&played, &participant_id);
    let mut shaped: Vec<AttributionMessage> = played
        .into_iter()
        .zip(mask)
        .filter_map(|(m, keep)| keep.then_some(m))
        .collect();

    let has_history_access = b(participant, "hasHistoryAccess").unwrap_or(false);
    let join_ms = s(participant, "createdAt")
        .as_deref()
        .and_then(crate::clock::iso_to_ms)
        .map(|ms| ms as f64)
        .unwrap_or(0.0);
    let hist: Vec<HistoryMessage> = shaped
        .iter()
        .map(|m| HistoryMessage {
            created_at_ms: m
                .created_at
                .as_deref()
                .and_then(crate::clock::iso_to_ms)
                .map(|ms| ms as f64),
        })
        .collect();
    let mask = filter_messages_by_history_access(&hist, has_history_access, join_ms);
    shaped = shaped
        .into_iter()
        .zip(mask)
        .filter_map(|(m, keep)| keep.then_some(m))
        .collect();

    if !has_history_access {
        let windows = compute_presence_windows_for_participant(
            &all,
            &participant_id,
            s(participant, "createdAt").unwrap_or_default().as_str(),
        );
        let mask = filter_messages_by_presence_windows(&shaped, &windows);
        shaped = shaped
            .into_iter()
            .zip(mask)
            .filter_map(|(m, keep)| keep.then_some(m))
            .collect();
    }

    // v4 `shaped.slice(-IN_SCENE_REWRITE_WINDOW)`.
    let start = shaped.len().saturating_sub(IN_SCENE_REWRITE_WINDOW);
    shaped.split_off(start)
}

/// v4 `generateInSceneVoicedLine`. Never throws — v4 wraps the whole body in
/// try/catch and returns `{success:false, proposedMarkdown:'', error}`.
pub async fn generate_in_scene_voiced_line<C, E>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    params: &InSceneVoicedLineParams<'_>,
) -> InSceneVoicedLineResult
where
    C: CompletionProvider,
    E: EmbeddingProvider,
{
    match generate_inner(db, completion, embedding, executor, params).await {
        Ok(result) => result,
        Err(error) => {
            // v4's outer catch. `chatId`/`participantId`/`characterId`/`error`
            // are v4's bag keys, in v4's order, at v4's ERROR level.
            tracing::error!(
                chatId = %s(params.chat, "id").unwrap_or_default(),
                participantId = %s(params.participant, "id").unwrap_or_default(),
                characterId = %s(params.character, "id").unwrap_or_default(),
                error = %error,
                "{LOG_CONTEXT} Unexpected failure"
            );
            InSceneVoicedLineResult {
                success: false,
                proposed_markdown: String::new(),
                error: Some(error),
            }
        }
    }
}

/// The body of v4's `try`. Every `?` here is a throw into v4's outer catch.
async fn generate_inner<C, E>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    params: &InSceneVoicedLineParams<'_>,
) -> Result<InSceneVoicedLineResult, String>
where
    C: CompletionProvider,
    E: EmbeddingProvider,
{
    let chat_id = s(params.chat, "id").unwrap_or_default();
    let participant_id = s(params.participant, "id").unwrap_or_default();
    let character_id = s(params.character, "id").unwrap_or_default();

    // The chosen profile as-is; provider params ride along so per-model settings
    // (e.g. DeepSeek thinking mode) take effect for this call too.
    let selection = build_selection(params.profile);

    let user_id = params.user_id.to_string();
    let chat_settings = db
        .read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &user_id))
        .map_err(|e| e.to_string())?;

    // `{{user}}` is the OWNER persona, not the seat being impersonated — the
    // seat's compiled stack was built that way and the overlay does not change
    // it. Passing NO active-speaker id is what makes the resolver fall back to
    // the owner seat rather than to whoever is being impersonated.
    let resolved_identity = crate::services::user_identity_resolver::resolve_user_identity(
        db,
        params.user_id,
        params.chat,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    let user_character = UserCharacter {
        name: resolved_identity.name,
        description: resolved_identity.description,
    };

    let roleplay_template = crate::services::participant_resolver::get_roleplay_template(
        db,
        params.chat,
        chat_settings
            .as_ref()
            .and_then(|s| s.get("defaultRoleplayTemplateId"))
            .and_then(Value::as_str),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Taboo and standing instructions are style guidance, not payload — the turn
    // path tolerates a failed read and so does this.
    let taboo_phrases = match db.read_main(crate::db::instance_settings::get_taboo_settings) {
        Ok(phrases) => phrases,
        Err(error) => {
            tracing::warn!(
                chatId = %chat_id,
                error = %error,
                "{LOG_CONTEXT} Failed to read Taboo settings — continuing without them"
            );
            Vec::new()
        }
    };
    let standing_instructions = crate::standing_instructions::resolve_standing_instructions_section(
        db,
        params.chat.get("projectId").and_then(Value::as_str),
        Some(&character_id),
    );

    // The line is PLAYED, so it gets the per-turn prompt the seat would get —
    // template, Taboo, standing instructions, the lot. No tool instructions: a
    // rehearsal has nothing to call.
    let precompiled_identity_stack =
        crate::services::system_prompt_compiler::get_compiled_identity_stack(
            params.chat,
            &participant_id,
        );
    let sys_character = to_sys_character(params.character);
    let system_prompt = build_system_prompt(&BuildSystemPromptOptions {
        character: &sys_character,
        user_character: Some(&user_character),
        roleplay_template: roleplay_template.as_ref().map(|t| t.system_prompt.as_str()),
        tool_instructions: None,
        selected_system_prompt_id: params.system_prompt_id,
        timestamp_config: None,
        is_initial_message: false,
        timezone: None,
        scenario_text: params.chat.get("scenarioText").and_then(Value::as_str),
        precompiled_identity_stack: precompiled_identity_stack.as_deref(),
        // v4 `subprompts: precompiledIdentityStack ? null : subprompts` — a
        // precompiled stack already has them baked in.
        subprompts: if precompiled_identity_stack.is_some() {
            None
        } else {
            params.subprompts
        },
        taboo_phrases: Some(&taboo_phrases),
        standing_instructions: standing_instructions.as_deref(),
        now_ms: 0,
        local_offset_minutes: 0,
    })
    .map_err(|e| e.to_string())?;

    // The scene as this seat sees it, with `[Name]` attribution in a
    // multi-character room and the seat's own lines as `assistant`.
    let cid = chat_id.clone();
    let events = db
        .read_main(move |c| crate::db::chats_messages_read::get_messages(c, &cid))
        .map_err(|e| e.to_string())?;
    let shaped = build_transcript_for_seat(&events, params.participant);

    let chat_participants = participants_of(params.chat);
    let mut participant_characters: HashMap<String, String> = HashMap::new();
    for p in &chat_participants {
        let Some(pcid) = s(p, "characterId").filter(|c| !c.is_empty()) else {
            continue;
        };
        if participant_characters.contains_key(&pcid) {
            continue;
        }
        // The rehearsing seat's character is the ALREADY-LOADED row — v4 does
        // not re-read it, and a re-read would cost a vault hit per rehearsal.
        if pcid == character_id {
            participant_characters.insert(pcid, s(params.character, "name").unwrap_or_default());
            continue;
        }
        let lookup = pcid.clone();
        let other = db.read_main(|main| {
            db.read_mount_index(|mount| {
                crate::db::characters_read::find_by_id(main, mount, &lookup)
            })
        });
        match other {
            Ok(Some(c)) => {
                participant_characters.insert(pcid, s(&c, "name").unwrap_or_default());
            }
            // v4 `if (other)` — a missing row is simply not added, no warn.
            Ok(None) => {}
            Err(error) => {
                // A broken vault on a bystander must not cost the rehearsal; the
                // message simply goes unattributed.
                tracing::warn!(
                    chatId = %chat_id,
                    characterId = %pcid,
                    error = %error,
                    "{LOG_CONTEXT} Could not load a participant character for attribution"
                );
            }
        }
    }

    let attr_participants: Vec<AttributionParticipant> = chat_participants
        .iter()
        .map(|p| AttributionParticipant {
            id: s(p, "id").unwrap_or_default(),
            participant_type: s(p, "type").unwrap_or_default(),
            character_id: s(p, "characterId"),
            controlled_by: s(p, "controlledBy").unwrap_or_default(),
            status: crate::chat_predicates::participant_status_from_str(
                p.get("status").and_then(Value::as_str),
            ),
        })
        .collect();
    let attributed = attribute_messages_for_character(
        &shaped,
        &participant_id,
        &participant_characters,
        &attr_participants,
    );

    let character_name = s(params.character, "name").unwrap_or_default();
    let provider = s(params.profile, "provider").unwrap_or_default();
    let formatter_input: Vec<FormatterMessage> = attributed
        .iter()
        .map(|m| FormatterMessage {
            role: match m.role {
                "assistant" => WireRole::Assistant,
                _ => WireRole::User,
            },
            content: m.content.clone(),
            id: m.id.clone(),
            name: m.name.clone(),
            participant_id: m.participant_id.clone(),
            thought_signature: m.thought_signature.clone(),
        })
        .collect();
    let transcript_messages: Vec<CompletionMessage> =
        format_messages_for_provider(&formatter_input, &provider, &character_name)
            .into_iter()
            .map(|m| CompletionMessage {
                role: match m.role {
                    WireRole::System => CompletionRole::System,
                    WireRole::Assistant => CompletionRole::Assistant,
                    WireRole::User => CompletionRole::User,
                },
                // v4 `...(m.name ? { name: m.name } : {})` — v5's
                // `CompletionMessage` has no `name` field, so the formatter's
                // name reaches the wire through its content prefix instead
                // (`format_messages_for_provider` already applied whichever of
                // the two the provider supports).
                content: m.content,
            })
            .collect();

    // Commonplace recall against the draft.
    let recall_text = recall_for_seed(
        db,
        embedding,
        &character_id,
        params.seed_markdown,
        &chat_id,
        params.now_ms,
        LOG_CONTEXT,
    )
    .await;

    let seed_trimmed = js_trim(params.seed_markdown).to_string();
    let mut user_parts: Vec<String> = Vec::new();
    if !recall_text.is_empty() {
        user_parts.push(recall_text.clone());
    }
    user_parts.push(REWRITE_INSTRUCTION.to_string());
    user_parts.push("Draft:".to_string());
    user_parts.push(seed_trimmed.clone());

    let mut messages = vec![CompletionMessage {
        role: CompletionRole::System,
        content: system_prompt.clone(),
    }];
    messages.extend(transcript_messages.iter().cloned());
    messages.push(CompletionMessage {
        role: CompletionRole::User,
        content: user_parts.join("\n\n"),
    });

    let max_tokens = max_tokens_for_seed(&seed_trimmed);

    // v4's debug bag, key for key and in v4's order.
    tracing::debug!(
        chatId = %chat_id,
        participantId = %participant_id,
        characterId = %character_id,
        profileId = %s(params.profile, "id").unwrap_or_default(),
        transcriptMessages = transcript_messages.len(),
        systemPromptLength = utf16_len(&system_prompt),
        hasRecall = !recall_text.is_empty(),
        hasTemplate = roleplay_template.is_some(),
        tabooPhrases = taboo_phrases.len(),
        usedPrecompiledStack = precompiled_identity_stack.is_some(),
        seedLength = utf16_len(&seed_trimmed),
        maxTokens = max_tokens,
        "{LOG_CONTEXT} Composed rewrite request"
    );

    Ok(execute_voice_rewrite(
        completion,
        executor,
        ExecuteVoiceRewriteParams {
            selection: &selection,
            messages,
            task_type: TASK_TYPE,
            character_id: &character_id,
            max_tokens: max_tokens as f64,
        },
    )
    .await)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4 `maxTokensForSeed` — `min(4096, max(1024, floor(len / 2)))`, where
    /// `len` is UTF-16 units.
    #[test]
    fn the_budget_follows_the_draft_between_its_two_bounds() {
        assert_eq!(max_tokens_for_seed(""), 1024);
        // Anything under 2048 units floors to MIN.
        assert_eq!(max_tokens_for_seed(&"a".repeat(2047)), 1024);
        assert_eq!(max_tokens_for_seed(&"a".repeat(2048)), 1024);
        assert_eq!(max_tokens_for_seed(&"a".repeat(2050)), 1025);
        assert_eq!(max_tokens_for_seed(&"a".repeat(6000)), 3000);
        // …and anything over 8192 units caps at MAX.
        assert_eq!(max_tokens_for_seed(&"a".repeat(8192)), 4096);
        assert_eq!(max_tokens_for_seed(&"a".repeat(100_000)), 4096);
    }

    /// **UTF-16, not scalars.** JS `String.length` counts units, so an astral
    /// character costs TWO — a draft of 3,000 emoji is 6,000 units and buys
    /// 3,000 tokens, where a `chars().count()` port would buy 1,500.
    #[test]
    fn an_astral_draft_is_measured_in_utf16_units() {
        let astral = "\u{1F4DC}".repeat(3000); // 📜 — 2 UTF-16 units each
        assert_eq!(astral.chars().count(), 3000);
        assert_eq!(utf16_len(&astral), 6000);
        assert_eq!(max_tokens_for_seed(&astral), 3000);
    }

    /// v4's eight `.join(' ')` lines, reassembled. Pinned as bytes because every
    /// clause of it is load-bearing on a real turn.
    #[test]
    fn the_rewrite_instruction_is_v4s_joined_eight_lines() {
        let v4_lines = [
            "It is your turn to speak in the conversation above. Below is your own rough",
            "draft of what you want to say next — the meaning and substance of it.",
            "Rewrite it in your own voice, the way you would actually say it given your",
            "personality, manner of speech, and everything that has just happened. Keep",
            "the meaning, the addressees, every specific fact, and any dice notation or",
            "`@Name` address exactly as written. Match the scene's conventions for",
            "narration and dialogue. Say only this — do not continue past it, do not",
            "answer it, and do not speak for anyone else.",
        ];
        assert_eq!(REWRITE_INSTRUCTION, v4_lines.join(" "));
    }
}
