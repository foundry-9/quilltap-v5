//! Recall replay harness (v4 `lib/memory/recall-replay.ts` — episodic recall
//! overhaul §3, P4.d13).
//!
//! Given a chat (and optionally a turn index), reconstruct the per-turn recall
//! distillation (retrospective / timeRange / entities / paraphrase) and run the
//! memory search TWICE — once with the episodic signals inert (the pre-overhaul
//! path) and once with them live — returning the full candidate table for each:
//! cosine, rawWeight, blendedBefore, every multiplier that fired, blendedAfter,
//! and whether the entry made the head. Nothing is persisted; the recall-history
//! ring buffer is read but never written (the one side effect is the search
//! path's `lastAccessedAt` bumps, which are harmless — the differential
//! normalizes them).
//!
//! Consumed by the `chatRecallReplay` dispatch verb (v4
//! `POST /api/v1/chats/[id]?action=recall-replay`), which the
//! `quilltap recall-replay` CLI wraps.

use serde_json::{json, Value};

use crate::chat_tasks::strip_tool_artifacts;
use crate::cheap_llm::CheapLlmSelection;
use crate::context_summary::{partition_messages_into_turns, PartitionInputMessage};
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_messages_read, chats_read};
use crate::memory_injector::{DYNAMIC_HEAD_DEFAULT_SIZE, RETRO_HEAD_SIZE};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::recall_history::recently_whispered_id_set;
use crate::services::cheap_llm_exec::{CheapLlmTaskExecutor, CheapLlmTaskOptions};
use crate::services::memory_recap::distill::{
    distill_memory_search, DistillMessage, DistilledSearch, ExtractionClock,
};
use crate::services::memory_service::{
    search_memories_semantic, RecallContextInput, SemanticSearchOptions, SemanticSearchResult,
};

/// v4 `RunRecallReplayInput`.
#[derive(Clone, Debug)]
pub struct RunRecallReplayInput {
    pub chat_id: String,
    pub user_id: String,
    pub cheap_llm: CheapLlmSelection,
    /// 1-based interchange index to replay AT (context = messages through that
    /// turn). Defaults to the last turn. Pre-validated by the route (integer ≥ 1).
    pub turn_index: Option<f64>,
    /// Character whose memories are searched. Defaults to the first present
    /// LLM character.
    pub character_id: Option<String>,
    /// Candidate table size per path (route-clamped to 1..=100).
    pub limit: Option<f64>,
    /// The `Date.now()` seam the search decay reads (v4 uses the wall clock;
    /// the differential pins it).
    pub now_ms: f64,
}

/// Build one candidate row (v4 `toRows`): every field present, `?? null` /
/// `?? []` semantics on the optional memory columns and the adjustment record.
fn to_rows(results: &[SemanticSearchResult], head_size: usize) -> Vec<Value> {
    results
        .iter()
        .enumerate()
        .map(|(index, r)| {
            let m = &r.memory;
            let nul = |k: &str| m.get(k).cloned().filter(|v| !v.is_null());
            json!({
                "memoryId": m.get("id").cloned().unwrap_or(Value::Null),
                "summary": m.get("summary").cloned().unwrap_or(Value::Null),
                "kind": nul("kind").unwrap_or_else(|| Value::String("semantic".into())),
                "occurredAt": nul("occurredAt").unwrap_or(Value::Null),
                "narrativeTime": nul("narrativeTime").unwrap_or(Value::Null),
                "createdAt": m.get("createdAt").cloned().unwrap_or(Value::Null),
                "keywords": nul("keywords").unwrap_or_else(|| json!([])),
                "cosine": crate::db::js_number_to_json(r.score),
                "rawWeight": crate::db::js_number_to_json(r.raw_weight),
                "blendedBefore": r
                    .recall_adjustment
                    .as_ref()
                    .map(|a| crate::db::js_number_to_json(a.blended_before))
                    .unwrap_or(Value::Null),
                "multiplier": r
                    .recall_adjustment
                    .as_ref()
                    .map(|a| crate::db::js_number_to_json(a.multiplier))
                    .unwrap_or(Value::Null),
                "fired": r
                    .recall_adjustment
                    .as_ref()
                    .map(|a| json!(a.fired))
                    .unwrap_or_else(|| json!([])),
                "blendedAfter": r
                    .recall_adjustment
                    .as_ref()
                    .map(|a| crate::db::js_number_to_json(a.blended_after))
                    .unwrap_or(Value::Null),
                "selected": index < head_size,
            })
        })
        .collect()
}

/// Run the replay (v4 `runRecallReplay`). Read-only against the chat and memory
/// corpus. Errors carry v4's exact `Error` messages (the route maps them to 400).
///
/// `server_tz` is the SERVER-LOCAL IANA zone (the v5 seam for v4's ambient
/// process zone), which the distill's TODAY line and day-reference scan resolve
/// their calendar in. It is a parameter rather than a field on
/// [`RunRecallReplayInput`] because the dispatch layer that builds that bag has
/// no zone to give: the host driver supplies it (`None` behaves like a UTC
/// server, which is what the differential pins).
#[allow(clippy::too_many_arguments)]
pub async fn run_recall_replay<C: CompletionProvider, E: EmbeddingProvider>(
    db: &Db,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    embedding: &E,
    input: &RunRecallReplayInput,
    server_tz: Option<&str>,
) -> Result<Value, String> {
    let limit = input.limit.unwrap_or(25.0) as usize;

    let chat = {
        let chat_id = input.chat_id.clone();
        db.read_main(move |c| chats_read::find_by_id(c, &chat_id))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Chat not found".to_string())?
    };

    // Resolve the responding character (v4: an explicit characterId must match a
    // participant; else the first LLM-controlled, non-removed participant with a
    // characterId).
    let empty = Vec::new();
    let participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let participant = match &input.character_id {
        Some(cid) => participants
            .iter()
            .find(|p| p.get("characterId").and_then(Value::as_str) == Some(cid.as_str())),
        None => participants.iter().find(|p| {
            p.get("controlledBy").and_then(Value::as_str) != Some("user")
                && p.get("status").and_then(Value::as_str) != Some("removed")
                && p.get("characterId")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty())
        }),
    };
    let participant =
        participant.ok_or("No LLM-controlled character participant found on this chat")?;
    let participant_character_id = participant
        .get("characterId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let character = {
        let cid = participant_character_id.clone();
        db.read_main(move |c| characters_read::find_by_id_raw(c, &cid))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Character record not found".to_string())?
    };
    let character_id = character
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let character_name = character
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // Slice history through the requested turn.
    let all_messages = {
        let chat_id = input.chat_id.clone();
        db.read_main(move |c| chats_messages_read::get_messages(c, &chat_id))
            .map_err(|e| e.to_string())?
    };
    let partition_input: Vec<PartitionInputMessage> = all_messages
        .iter()
        .map(|m| PartitionInputMessage {
            id: m
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            role: m.get("role").and_then(Value::as_str).map(str::to_string),
            message_type: m.get("type").and_then(Value::as_str).map(str::to_string),
            system_sender: m
                .get("systemSender")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .collect();
    let chat_type = chat.get("chatType").and_then(Value::as_str);
    let turns = partition_messages_into_turns(&partition_input, chat_type);
    if turns.is_empty() {
        return Err("Chat has no turns to replay".to_string());
    }
    // v4 `Math.min(Math.max(input.turnIndex ?? turns.length, 1), turns.length)`.
    let turn_index = (input.turn_index.unwrap_or(turns.len() as f64))
        .max(1.0)
        .min(turns.len() as f64) as usize;
    let last_turn_message_id = turns[turn_index - 1]
        .ids
        .last()
        .cloned()
        .unwrap_or_default();
    let cutoff = all_messages
        .iter()
        .position(|m| m.get("id").and_then(Value::as_str) == Some(last_turn_message_id.as_str()));
    let sliced: &[Value] = match cutoff {
        Some(i) => &all_messages[..=i],
        None => &all_messages[..],
    };
    let window: Vec<&Value> = sliced
        .iter()
        .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
        .filter(|m| {
            // `!m.systemSender` — truthiness (absent, null, or empty all pass).
            let staff = m
                .get("systemSender")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty());
            let role = m.get("role").and_then(Value::as_str);
            !staff && (role == Some("USER") || role == Some("ASSISTANT"))
        })
        .collect();

    // Historical clock: the replayed turn resolves "last week" against ITS OWN
    // date, not today's — that is the whole point of replaying old turns.
    let clock_iso = window
        .iter()
        .rev()
        .find_map(|m| {
            m.get("createdAt")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
        })
        .map(str::to_string)
        .unwrap_or_else(crate::clock::now_iso);

    // Distill the turn signals (same call, same inputs the live path uses).
    let start = window.len().saturating_sub(12);
    let recent_for_distill: Vec<DistillMessage> = window[start..]
        .iter()
        .map(|m| {
            let role = m
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase();
            let content = m.get("content").and_then(Value::as_str).unwrap_or("");
            // v4 `(role === 'ASSISTANT' ? stripToolArtifacts(content || '') :
            // content) || ''`.
            let content = if role == "assistant" {
                strip_tool_artifacts(content).unwrap_or_default()
            } else {
                content.to_string()
            };
            DistillMessage { role, content }
        })
        .filter(|m| crate::jsstr::utf16_len(&m.content) > 0)
        .collect();

    let clock = ExtractionClock {
        now_iso: clock_iso.clone(),
        timeline_mode: chat
            .get("timelineMode")
            .and_then(Value::as_str)
            .unwrap_or("realtime")
            .to_string(),
        local_tz: server_tz.map(str::to_string),
    };
    let signals: Option<DistilledSearch> = distill_memory_search(
        executor,
        completion,
        &recent_for_distill,
        &character_name,
        &input.cheap_llm,
        &character_id,
        Some(&clock),
        // The diagnostic replay is not a turn — nobody is watching a composer
        // while it runs, so it keeps v4's `background` default (v4 `02d4efa1b`,
        // bug 115).
        CheapLlmTaskOptions::default(),
    )
    .await;

    // `signals?.paraphrase || (keywords.length ? join(' ') : '') || lastContent || ''`.
    let query = signals
        .as_ref()
        .and_then(|s| s.paraphrase.clone())
        .filter(|p| !p.is_empty())
        .or_else(|| {
            signals
                .as_ref()
                .filter(|s| !s.keywords.is_empty())
                .map(|s| s.keywords.join(" "))
                .filter(|q| !q.is_empty())
        })
        .or_else(|| {
            window
                .last()
                .and_then(|m| m.get("content").and_then(Value::as_str))
                .filter(|c| !c.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_default();
    if crate::jsstr::js_trim(&query).is_empty() {
        return Err("Could not derive a recall query for this turn".to_string());
    }

    let recall_settings = crate::services::build_context::read_memory_recall_settings(db);
    // v4: EVERY non-removed participant with a characterId (user-controlled
    // included — unlike buildContext's responder+others set).
    let present_about_character_ids: Vec<String> = participants
        .iter()
        .filter(|p| {
            p.get("status").and_then(Value::as_str) != Some("removed")
                && p.get("characterId")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty())
        })
        .filter_map(|p| {
            p.get("characterId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();

    let base_ctx = RecallContextInput {
        current_project_id: chat
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_string),
        scope_policy: if recall_settings.scope_policy == "exclude" {
            crate::recall_tags::ScopePolicy::Exclude
        } else {
            crate::recall_tags::ScopePolicy::DownWeight
        },
        present_about_character_ids,
        turn_context: signals.as_ref().and_then(|s| s.context),
        turn_temporal: signals.as_ref().and_then(|s| s.temporal),
        turn_retrospective: false,
        expand_related: recall_settings.expand_related,
        recently_whispered_ids: Some(recently_whispered_id_set(
            chat.get("commonplaceRecallHistory").unwrap_or(&Value::Null),
        )),
        // Fresh-event boost against the REPLAYED TURN's clock, not wall-clock
        // now — replaying an old turn must reproduce what recall would have done
        // then. (v4 `Date.parse(clockIso)`: an unparsable clock lands as NaN and
        // the multiplier's finite guard disables the boost, exactly as `None`
        // does here. ⚠ NOT `input.now_ms`, which stays the decay seam on both
        // search calls below.)
        current_chat_id: Some(input.chat_id.clone()),
        now_ms: crate::episodic::event_time_ms(Some(&clock_iso)),
    };

    // OLD path — episodic signals inert (byte-identical to pre-overhaul recall).
    let old_results = search_memories_semantic(
        db,
        embedding,
        &character_id,
        &query,
        &SemanticSearchOptions {
            user_id: input.user_id.clone(),
            limit: Some(limit),
            min_importance: Some(0.3),
            recall_context: Some(base_ctx.clone()),
            now_ms: input.now_ms,
            ..Default::default()
        },
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    // NEW path — retrospective flip + window + entity anchors + multi-probe.
    let retrospective = signals.as_ref().is_some_and(|s| s.retrospective);
    let mut extra_probes: Vec<String> = Vec::new();
    if retrospective {
        if let Some(s) = &signals {
            let entity_probe = crate::jsstr::js_trim(&s.entities.join(" ")).to_string();
            if !entity_probe.is_empty() {
                extra_probes.push(entity_probe);
            }
            if let (Some(p), Some(tr)) = (&s.paraphrase, &s.time_range) {
                extra_probes.push(format!(
                    "{p} (around {} to {})",
                    crate::jsstr::utf16_truncate(&tr.from, 10),
                    crate::jsstr::utf16_truncate(&tr.to, 10)
                ));
            }
        }
    }
    let new_results = search_memories_semantic(
        db,
        embedding,
        &character_id,
        &query,
        &SemanticSearchOptions {
            user_id: input.user_id.clone(),
            limit: Some(limit),
            min_importance: Some(0.3),
            recall_context: Some(RecallContextInput {
                turn_retrospective: retrospective,
                ..base_ctx.clone()
            }),
            entity_anchors: signals
                .as_ref()
                .map(|s| s.entities.clone())
                .unwrap_or_default(),
            // Ungated from the retrospective flag exactly as the two live
            // consumers are (v4 `505dcb1f`) — the replay is only useful while it
            // mirrors production.
            occurred_within: signals.as_ref().and_then(|s| s.time_range.clone()),
            extra_probes,
            now_ms: input.now_ms,
            ..Default::default()
        },
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(json!({
        "chatId": input.chat_id,
        "characterId": character_id,
        "characterName": character_name,
        "turnIndex": turn_index,
        "totalTurns": turns.len(),
        "signals": signals
            .as_ref()
            .map(|s| s.signals_json())
            .unwrap_or(Value::Null),
        "query": query,
        "clockIso": clock_iso,
        "oldPath": to_rows(&old_results, DYNAMIC_HEAD_DEFAULT_SIZE),
        "newPath": to_rows(
            &new_results,
            if retrospective {
                RETRO_HEAD_SIZE
            } else {
                DYNAMIC_HEAD_DEFAULT_SIZE
            }
        ),
    }))
}
