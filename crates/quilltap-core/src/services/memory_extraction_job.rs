//! The `MEMORY_EXTRACTION` job handler (v4
//! `lib/background-jobs/handlers/memory-extraction.ts`) — per turn.
//!
//! Reads a turn-keyed payload (`chatId` + `turnOpenerMessageId`), rebuilds the
//! [`TurnTranscript`](crate::memory_tasks::TurnTranscript) from current chat
//! state, and runs the per-turn memory extraction pipeline
//! ([`process_turn_for_memory`]). This replaced v4's prior
//! per-assistant-message handler, which fired once for every character
//! response and re-extracted the same user message N times in multi-character
//! turns.
//!
//! The finalizer's `trigger_turn_memory_extraction` enqueues one of these on
//! every closed turn; before P4.6bj the jobs died on the runner's "recognized
//! but not yet available" loud fallback — the whole episodic-campaign
//! extraction pipeline was verified but dormant.
//!
//! ## Failure shape (v4's exactly)
//!
//! A missing connection profile / chat-settings row THROWS (`Err` → the job
//! FAILS and the runner's backoff retries); a missing chat and an
//! empty-slices turn are logged skips (`Ok(())` → COMPLETED). The debug-log
//! persist and the token-tracking event each swallow their own failures.
//!
//! ## Model boundary (tier-3 seams)
//!
//! [`CompletionProvider`] + [`EmbeddingProvider`] via the ported
//! [`CheapLlmTaskExecutor`]; the cost figure behind the MEMORY_EXTRACTION
//! system event rides [`MessageCostEstimator`]. `memory_extraction_limits` is
//! v4's `getMemoryExtractionLimits()` instance-settings read, resolved by the
//! caller (the host reads [`crate::db::instance_settings`]; the differential
//! injects the corpus limits — the `carina_memory_extraction` precedent).

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::{chat_settings, chats_read, connection_profiles};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::cost_estimation::MessageCostEstimator;
use crate::services::cost_events::{create_memory_extraction_event, TokenUsage};
use crate::services::dangerous_content::chat_override::should_use_uncensored_route;
use crate::services::dangerous_content::resolver::resolve_dangerous_content_settings;
use crate::services::memory_processor::{
    process_turn_for_memory, CheapLlmSettings, MemoryExtractionLimits, TurnMemoryExtractionContext,
};
use crate::services::turn_transcript::{
    build_turn_transcript, resolve_user_character_participant, BuildTurnTranscriptOptions,
};

/// The `MEMORY_EXTRACTION` job payload (v4 `MemoryExtractionPayload`).
#[derive(Clone, Debug, Default)]
pub struct MemoryExtractionPayload {
    pub chat_id: String,
    /// `None` for greeting/continue turns with no fresh user input.
    pub turn_opener_message_id: Option<String>,
    /// The autonomous-turn terminal anchor (see the transcript builder).
    pub extraction_anchor_message_id: Option<String>,
    pub connection_profile_id: String,
}

impl MemoryExtractionPayload {
    pub fn decode(payload: &Value) -> Self {
        let s = |k: &str| {
            payload
                .get(k)
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_default()
        };
        let opt = |k: &str| {
            payload
                .get(k)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        Self {
            chat_id: s("chatId"),
            turn_opener_message_id: opt("turnOpenerMessageId"),
            extraction_anchor_message_id: opt("extractionAnchorMessageId"),
            connection_profile_id: s("connectionProfileId"),
        }
    }
}

/// v4 `handleMemoryExtraction`. `Err(message)` is v4's `throw` (the runner
/// records it as `lastError` and the ported backoff decides retry-vs-dead).
#[allow(clippy::too_many_arguments)]
pub async fn handle_memory_extraction<C, E, K>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    cost: &K,
    user_id: &str,
    payload: &MemoryExtractionPayload,
    memory_extraction_limits: Option<MemoryExtractionLimits>,
) -> Result<(), String>
where
    C: CompletionProvider,
    E: EmbeddingProvider,
    K: MessageCostEstimator,
{
    // ── The two throwing reads ──
    let pid = payload.connection_profile_id.clone();
    let connection_profile = db
        .read_main(move |c| connection_profiles::find_by_id(c, &pid))
        .map_err(|e| format!("{e:?}"))?
        .ok_or_else(|| {
            format!(
                "Connection profile not found: {}",
                payload.connection_profile_id
            )
        })?;

    let uid = user_id.to_string();
    let chat_settings = db
        .read_main(move |c| chat_settings::find_by_user_id(c, &uid))
        .map_err(|e| format!("{e:?}"))?
        .ok_or_else(|| format!("Chat settings not found for user: {user_id}"))?;

    // ── Chat missing at job execution → logged skip (v4 warns + returns) ──
    let cid = payload.chat_id.clone();
    let Some(chat) = db
        .read_main(move |c| chats_read::find_by_id(c, &cid))
        .map_err(|e| format!("{e:?}"))?
    else {
        return Ok(());
    };

    let cid = payload.chat_id.clone();
    let all_raw_messages = db
        .read_main(move |c| crate::db::chats_messages_read::get_messages(c, &cid))
        .map_err(|e| format!("{e:?}"))?;
    // v4 filters to `type === 'message'` up front (the builder re-checks).
    let message_events: Vec<Value> = all_raw_messages
        .iter()
        .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
        .cloned()
        .collect();

    // ── Hydrate every CHARACTER participant's character row ──
    let participants: Vec<Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut participant_characters: std::collections::HashMap<String, Value> =
        std::collections::HashMap::new();
    for p in &participants {
        if p.get("type").and_then(Value::as_str) != Some("CHARACTER") {
            continue;
        }
        let Some(char_id) = p
            .get("characterId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let cid = char_id.to_string();
        if participant_characters.contains_key(&cid) {
            continue;
        }
        let lookup = cid.clone();
        if let Ok(Some(character)) = db.read_main(|main| {
            db.read_mount_index(|mount| {
                crate::db::characters_read::find_by_id(main, mount, &lookup)
            })
        }) {
            participant_characters.insert(cid, character);
        }
    }

    let user_character = resolve_user_character_participant(&participants, &participant_characters);

    let transcript = build_turn_transcript(
        &message_events,
        &participants,
        &participant_characters,
        &BuildTurnTranscriptOptions {
            turn_opener_message_id: payload.turn_opener_message_id.clone(),
            extraction_anchor_message_id: payload.extraction_anchor_message_id.clone(),
            user_character_id: user_character.map(|c| json_str(c, "id")),
            user_character_name: user_character.map(|c| json_str(c, "name")),
            user_character_pronouns: user_character
                .and_then(crate::services::turn_transcript::pronouns_from_character),
        },
    );

    // Turn has no character contributions → logged skip (v4 returns).
    if transcript.character_slices.is_empty() {
        return Ok(());
    }

    let uid = user_id.to_string();
    let available_profiles = db
        .read_main(move |c| connection_profiles::find_by_user_id(c, &uid))
        .unwrap_or_default();

    // v4 `resolveDangerousContentSettings(chatSettings, chat).settings`, then
    // narrowed to the slim `cheap_llm` shape the memory processor consumes.
    let global_danger = chat_settings
        .get("dangerousContentSettings")
        .and_then(|d| serde_json::from_value(d.clone()).ok());
    let resolved_danger = resolve_dangerous_content_settings(global_danger, Some(&chat)).settings;
    let danger_settings = crate::cheap_llm::DangerousContentSettings {
        mode: resolved_danger.mode,
        uncensored_text_profile_id: resolved_danger.uncensored_text_profile_id,
    };

    // Orienting context (background only, never a memory source): the
    // project's description lets the extractor judge a memory's scope; the
    // rolling chat summary frames its temporal hinge. The project lookup is
    // guarded — a broken project store must not sink the whole extraction.
    let chat_context_summary = chat
        .get("contextSummary")
        .and_then(Value::as_str)
        .map(str::to_string);
    let project_id = chat
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let mut project_description: Option<String> = None;
    if let Some(pid) = &project_id {
        let lookup = pid.clone();
        let read = db.read_main(|main| {
            db.read_mount_index(|mount| {
                let repo = crate::db::projects::ProjectsRepository::new(main, mount);
                match repo.find_by_id(&lookup) {
                    Ok(p) => Ok(p),
                    // Degrade to no project description (v4's catch → debug log).
                    Err(_) => Ok(None),
                }
            })
        });
        if let Ok(Some(project)) = read {
            project_description = project
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
    }

    // Anchor derived memories to the historical chat timestamp rather than
    // letting createdAt default to "now" — a regenerate sweep over an old chat
    // must not mint memories that look freshly minted (wrong for chronology,
    // recency, and the age-decayed housekeeping signals). Latest assistant
    // message in the turn → the user turn opener → the chat's own createdAt.
    let find_created_at = |id: &str| -> Option<String> {
        message_events
            .iter()
            .find(|m| m.get("id").and_then(Value::as_str) == Some(id))
            .and_then(|m| m.get("createdAt").and_then(Value::as_str))
            .map(str::to_string)
    };
    let mut source_message_timestamp: Option<String> = transcript
        .latest_assistant_message_id
        .as_deref()
        .and_then(find_created_at);
    if source_message_timestamp.is_none() {
        if let Some(opener) = &payload.turn_opener_message_id {
            source_message_timestamp = find_created_at(opener);
        }
    }
    if source_message_timestamp.is_none() {
        source_message_timestamp = chat
            .get("createdAt")
            .and_then(Value::as_str)
            .map(str::to_string);
    }

    let ctx = TurnMemoryExtractionContext {
        transcript,
        participant_characters,
        chat_id: payload.chat_id.clone(),
        project_id,
        project_description,
        chat_context_summary,
        user_id: user_id.to_string(),
        connection_profile: crate::services::image_job_common::cheap_llm_profile_from_value(
            &connection_profile,
        ),
        cheap_llm_settings: cheap_settings_from(&chat_settings),
        available_profiles: Some(
            available_profiles
                .iter()
                .map(crate::services::image_job_common::cheap_llm_profile_from_value)
                .collect(),
        ),
        danger_settings: Some(danger_settings),
        is_dangerous_chat: should_use_uncensored_route(Some(&chat)),
        memory_extraction_limits,
        source_message_timestamp,
        // Episodic spine: which clock the chat's story runs on (drives the
        // extraction CLOCK block and narrativeTime capture).
        timeline_mode: chat
            .get("timelineMode")
            .and_then(Value::as_str)
            .map(str::to_string),
        dry_run: false,
        // 4.6 Private Character Rooms: autonomous-source attribution — the
        // prompts get the user-absence clause and the resulting memory rows
        // carry `witnessedContext = 'autonomous_room'`.
        in_autonomous_room: chat.get("chatType").and_then(Value::as_str) == Some("autonomous"),
        registry_cheapest_for_current: None,
    };

    let result = process_turn_for_memory(db, completion, embedding, executor, &ctx).await;

    // A pass lost to a timeout is work that never happened, and nothing
    // downstream re-queues it. Fail the job rather than let it report a clean
    // finish over the hole (v4 `a1d88aa3a`, bug 107). A refusal or an
    // unparseable answer would fail identically on every retry and keeps the
    // old log-and-move-on behaviour.
    //
    // Placed BEFORE the debug-log write, as v4 places it: v4's note in
    // `BACKGROUND_JOBS_CHILD.md` is that the debug logs describing the timeout
    // never reach the message, because they are child writes discarded on the
    // throw. v5 has no child and applies as it goes, so the ORDER is what
    // reproduces that outcome here.
    //
    // ⚠ RECORDED MECHANISM DIVERGENCE (measured, P4.D136). v4's retry is
    // ATOMIC: `handleChildJobResult` only calls `applyWritesAtomically` when
    // `msg.ok`, so a turn that lost one character's pass discards the others
    // too and the re-run is duplicate-free. v5's writer applies as it goes, so
    // the successful passes SURVIVE and the re-run repeats them. The outcome is
    // the same because the repeat is idempotent — v4's own handler-audit table
    // answers "Idempotent under retry? Yes (memories upserted by content hash)"
    // — so v5 needs no buffering to be correct here; it reaches v4's guarantee
    // by a different route, and pays less for it (the passes that DID succeed
    // are not thrown away).
    if result.passes_lost_to_timeout > 0 {
        // v4 also carries `jobId`. v5's core handler takes a payload, not the
        // job — the host wrapper owns the job row — so the field has no source
        // here; the chat id and the count are what v5 can name.
        tracing::error!(
            chat_id = %payload.chat_id,
            passes_lost_to_timeout = result.passes_lost_to_timeout,
            error = result.error.as_deref().unwrap_or(""),
            "[MemoryExtraction] Extraction passes lost to a cheap-LLM timeout; failing the job for retry"
        );
        return Err(
            crate::services::cheap_llm_exec::cheap_llm_task_lost_message(
                "memory-extraction",
                result.error.as_deref(),
            ),
        );
    }
    // v4 logs the success/failure shape either way; the job itself COMPLETES.

    // Persist debug logs onto the latest assistant message of the turn so the
    // operator can pop the debug panel and see what the per-turn pass did
    // (best-effort, v4 catches).
    if !result.debug_logs.is_empty() {
        if let Some(source_id) = &result.source_message_id {
            let chat_id = payload.chat_id.clone();
            let source_id = source_id.clone();
            let updates = json!({ "debugMemoryLogs": result.debug_logs.clone() });
            let _ = db
                .write(move |writers| {
                    writers
                        .main()
                        .chat_messages()
                        .update_message(&chat_id, &source_id, &updates)
                })
                .await;
        }
    }

    // Token-tracking event mirroring the prior per-message behaviour
    // (best-effort, v4 catches; gated on nonzero usage).
    if result.usage.prompt_tokens != 0 || result.usage.completion_tokens != 0 {
        let provider = json_str(&connection_profile, "provider");
        let model = json_str(&connection_profile, "modelName");
        let estimated = cost
            .estimate(
                &provider,
                &model,
                result.usage.prompt_tokens,
                result.usage.completion_tokens,
                user_id,
            )
            .await;
        let _ = create_memory_extraction_event(
            db,
            &payload.chat_id,
            Some(TokenUsage {
                prompt_tokens: Some(result.usage.prompt_tokens as f64),
                completion_tokens: Some(result.usage.completion_tokens as f64),
                total_tokens: Some(result.usage.total_tokens as f64),
            }),
            Some(provider),
            Some(model),
            estimated,
        )
        .await;
    }

    Ok(())
}

/// The memory-processor `CheapLlmSettings` off `chatSettings.cheapLLMSettings`
/// (v4 maps exactly `strategy` / `userDefinedProfileId` / `fallbackToLocal` —
/// the `carina_memory_extraction` twin).
fn cheap_settings_from(chat_settings: &Value) -> CheapLlmSettings {
    let cheap = chat_settings.get("cheapLLMSettings");
    CheapLlmSettings {
        strategy: cheap
            .and_then(|c| c.get("strategy"))
            .and_then(Value::as_str)
            .unwrap_or("PROVIDER_CHEAPEST")
            .to_string(),
        user_defined_profile_id: cheap
            .and_then(|c| c.get("userDefinedProfileId"))
            .and_then(Value::as_str)
            .map(str::to_string),
        fallback_to_local: cheap
            .and_then(|c| c.get("fallbackToLocal"))
            .and_then(Value::as_bool)
            .unwrap_or(true),
    }
}

fn json_str(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Parse the `getMemoryExtractionLimits` instance-settings object into the
/// processor's limits shape (documented defaults on missing keys).
pub fn limits_from_value(v: &Value) -> MemoryExtractionLimits {
    MemoryExtractionLimits {
        enabled: v.get("enabled").and_then(Value::as_bool).unwrap_or(false),
        max_per_hour: v.get("maxPerHour").and_then(Value::as_f64).unwrap_or(20.0),
        soft_start_fraction: v
            .get("softStartFraction")
            .and_then(Value::as_f64)
            .unwrap_or(0.7),
        soft_floor: v.get("softFloor").and_then(Value::as_f64).unwrap_or(0.7),
    }
}
