//! Recall replay harness (v4 `lib/memory/recall-replay.ts` — episodic recall
//! overhaul §3, P4.d13).
//!
//! Given a chat (and optionally a turn index), reconstruct the per-turn recall
//! distillation (retrospective / timeRange / entities / paraphrase) and run the
//! memory search TWICE — once with the episodic signals inert (the pre-overhaul
//! path) and once with them live — returning the full candidate table for each:
//! cosine, rawWeight, blendedBefore, every multiplier that fired, blendedAfter,
//! and whether the entry made the head. Nothing is persisted; the recall-history
//! ring buffer is read but never written, and since v4 `3f7320138` the search
//! does not stamp `lastAccessedAt` either — only consumers that DELIVER
//! memories do.
//!
//! Three harness knobs (R7 of the recall multiplier retuning, v4 `f7d8064be`
//! at the `01a83539d` pin) make runs comparable: `tuning` applies retuning
//! constants to the NEW path only; `signals` reuses a saved distillation
//! instead of calling the cheap LLM again (the distillation is not
//! deterministic, so two runs of one turn otherwise embed different queries);
//! `as_of` drops memories created at or after the replayed turn's opening
//! message, so an old turn is searched against the corpus as it stood then.
//! `memory_budget` sizes the new path's head as the live responder's budget
//! would (`size_memory_pools`).
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
use crate::memory_pools::size_memory_pools;
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::recall_history::recently_whispered_id_set;
use crate::recall_tuning::{
    describe_recall_tuning, resolve_recall_tuning, RecallReplaySignals, RecallTuningInput,
};
use crate::services::cheap_llm_exec::{CheapLlmTaskExecutor, CheapLlmTaskOptions};
use crate::services::memory_recap::distill::{
    distill_memory_search, DistillMessage, DistilledSearch, ExtractionClock,
};
use crate::services::memory_service::{
    search_memories_semantic, EmbeddingMemo, RecallContextInput, SemanticSearchOptions,
    SemanticSearchResult,
};

/// The RecallReplay service logger (v4 `createServiceLogger('RecallReplay')`,
/// whose `{service, module: 'service'}` context this target stands for — R-M).
pub const LOG_TARGET: &str = "quilltap::recall_replay";

/// Embeddings the replay has already paid for, by text, for the life of the
/// process (v4 `replayEmbeddingMemo`). The provider's vector for one text
/// drifts slightly from call to call (≈1e-3 in cosine was enough to swap a
/// gold memory out of a probe-set head), so a sweep that compares tunings must
/// embed each text once. Bounded: the oldest entries go first, trimmed at the
/// START of each run only (a run may overgrow it — v4 `boundedMemo()`). Keyed
/// by text and explicit profile only, so it empties on a restart — restart
/// after switching the embedding profile.
pub const REPLAY_EMBEDDING_MEMO_MAX: usize = 500;
static REPLAY_EMBEDDING_MEMO: std::sync::LazyLock<EmbeddingMemo> =
    std::sync::LazyLock::new(Default::default);

fn bounded_memo() -> EmbeddingMemo {
    REPLAY_EMBEDDING_MEMO
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .trim_to(REPLAY_EMBEDDING_MEMO_MAX);
    REPLAY_EMBEDDING_MEMO.clone()
}

/// A pinned `signals` body field as the distillation the search reads (R-J,
/// lane-local — `services/memory_recap/` is another order's): v4's consumers
/// read `retrospective === true`, an absent `entities` as `[]`, and a pinned
/// `timeRange: null` as no window.
fn pinned_as_distilled(signals: &RecallReplaySignals) -> DistilledSearch {
    DistilledSearch {
        keywords: signals.keywords.clone(),
        temporal: signals.temporal,
        context: signals.context,
        paraphrase: signals.paraphrase.clone(),
        retrospective: signals.retrospective == Some(true),
        time_range: signals.time_range.clone().flatten(),
        entities: signals.entities.clone().unwrap_or_default(),
        parse_failed: false,
    }
}

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
    /// Candidate table size per path (route-clamped to 1..=100). Absent →
    /// `max(25, new_head_size + 10)` — wide enough to show where the larger
    /// head ends.
    pub limit: Option<f64>,
    /// The `Date.now()` seam the search decay reads (v4 uses the wall clock;
    /// the differential pins it).
    pub now_ms: f64,
    /// The responding model's memory budget in tokens (route-floored). When
    /// `> 0`, the new path's `selected` rows follow the budget-sized head
    /// (`size_memory_pools`) — the old path keeps the historical fixed head —
    /// so the replay shows what the larger head adds. Otherwise both paths use
    /// the fixed historical sizes.
    pub memory_budget: Option<f64>,
    /// Retuning constants for the NEW path only (R7). Absent → the retuned
    /// defaults. A present `{}` IS a tuning (v4's truthy test): it keeps the
    /// R1 gate on under TF-IDF while describing as `defaults`.
    pub tuning: Option<RecallTuningInput>,
    /// A saved distillation to reuse instead of calling the cheap LLM: the
    /// validated signals and v4's parsed echo (the result's `signals`).
    pub signals: Option<(RecallReplaySignals, Value)>,
    /// Search only memories created before the replayed turn's opening
    /// message, weighted at the turn's clock.
    pub as_of: bool,
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
    let (signals, signals_echo): (Option<DistilledSearch>, Value) = match &input.signals {
        Some((pinned, echo)) => {
            tracing::debug!(
                target: LOG_TARGET,
                chatId = %input.chat_id,
                turnIndex = turn_index as u64,
                "Recall replay using pinned signals"
            );
            (Some(pinned_as_distilled(pinned)), echo.clone())
        }
        None => {
            let distilled = distill_memory_search(
                executor,
                completion,
                &recent_for_distill,
                &character_name,
                &input.cheap_llm,
                &character_id,
                Some(&clock),
                // The diagnostic replay is not a turn — nobody is watching a
                // composer while it runs, so it keeps v4's `background` default
                // (v4 `02d4efa1b`, bug 115).
                CheapLlmTaskOptions::default(),
            )
            .await;
            let echo = distilled
                .as_ref()
                .map(|s| s.signals_json())
                .unwrap_or(Value::Null);
            (distilled, echo)
        }
    };

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
        // P4.D265 (mechanical): honoured from P4.D267.
        present_participant_names: Vec::new(),
    };

    let embedding_memo = bounded_memo();
    let tuning = resolve_recall_tuning(input.tuning.as_ref());
    let tuning_summary = describe_recall_tuning(&tuning);
    // R4 drops entities that only name someone in the room; it needs their
    // names (resolved whenever `specific_anchors` — TRUE by default — and handed
    // to the NEW path only).
    let present_participant_names: Option<Vec<String>> = if tuning.specific_anchors {
        let mut names = Vec::new();
        for id in &base_ctx.present_about_character_ids {
            let cid = id.clone();
            let raw = db
                .read_main(move |c| characters_read::find_by_id_raw(c, &cid))
                .map_err(|e| e.to_string())?;
            if let Some(name) = raw
                .as_ref()
                .and_then(|c| c.get("name"))
                .and_then(Value::as_str)
                .filter(|n| !n.is_empty())
            {
                names.push(name.to_string());
            }
        }
        Some(names)
    } else {
        None
    };

    // asOf — the corpus as it stood when the turn opened: leave out every
    // memory created at or after the turn's opening (user) message, and decay
    // weights against the turn's clock rather than today's. (Reinforcement a
    // memory received after the turn can't be undone; its importance is read
    // as it is now.) The cutoff is the turn's OPENING message; the weight clock
    // is its LAST eligible message (`clock_iso`) — two different instants.
    let replayed_ids = &turns[turn_index - 1].ids;
    let message_by_id = |id: &str| {
        all_messages
            .iter()
            .find(|m| m.get("id").and_then(Value::as_str) == Some(id))
    };
    let opening_message = replayed_ids
        .iter()
        .filter_map(|id| message_by_id(id))
        .find(|m| m.get("role").and_then(Value::as_str) == Some("USER"))
        .or_else(|| replayed_ids.first().and_then(|id| message_by_id(id)));
    let as_of_iso: Option<String> = if input.as_of {
        opening_message
            .and_then(|m| m.get("createdAt"))
            .and_then(Value::as_str)
            .map(str::to_string)
    } else {
        None
    };
    let weight_clock_ms = as_of_iso
        .as_ref()
        .and_then(|_| crate::episodic::js_date_parse_ms(&clock_iso));
    let mut exclude_memory_ids: Option<Vec<String>> = None;
    if let Some(iso) = &as_of_iso {
        let (cid, since) = (character_id.clone(), iso.clone());
        let newer = db
            .read_main(move |c| crate::db::memories_read::count_created_since(c, &cid, &since))
            .map_err(|e| e.to_string())?;
        let recent = if newer > 0 {
            let cid = character_id.clone();
            db.read_main(move |c| crate::db::memories_read::find_recent(c, &cid, newer))
                .map_err(|e| e.to_string())?
        } else {
            Vec::new()
        };
        let excluded: Vec<String> = recent
            .iter()
            .filter(|m| {
                m.get("createdAt")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c >= iso.as_str())
            })
            .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
            .collect();
        tracing::debug!(
            target: LOG_TARGET,
            chatId = %input.chat_id,
            asOfIso = %iso,
            excluded = excluded.len() as u64,
            "Recall replay corpus cutoff"
        );
        exclude_memory_ids = Some(excluded);
    }

    let retrospective = signals.as_ref().is_some_and(|s| s.retrospective);
    let new_head_size = match input.memory_budget.filter(|b| *b > 0.0) {
        Some(budget) => size_memory_pools(budget, retrospective).head_entries,
        None if retrospective => RETRO_HEAD_SIZE,
        None => DYNAMIC_HEAD_DEFAULT_SIZE,
    };
    let limit = match input.limit {
        Some(l) => l as usize,
        None => 25.max(new_head_size + 10),
    };

    // OLD path — episodic signals inert. NOT byte-identical to pre-overhaul
    // recall (v4's comment says so, but it runs under the retuned defaults —
    // the cap, and the gate whenever the profile is neural): tuning never
    // reaches it, but the asOf cutoff does.
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
            exclude_memory_ids: exclude_memory_ids.clone().unwrap_or_default(),
            weight_clock_ms,
            embedding_memo: Some(embedding_memo.clone()),
            ..Default::default()
        },
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    // NEW path — retrospective flip + window + entity anchors + multi-probe,
    // under the requested tuning.
    let mut extra_probes: Vec<String> = Vec::new();
    if retrospective {
        if let Some(s) = &signals {
            let entity_probe = crate::jsstr::js_trim(&s.entities.join(" ")).to_string();
            if !entity_probe.is_empty() {
                extra_probes.push(entity_probe);
            }
            // v4 `signals.paraphrase && signals.timeRange` — an empty (pinned)
            // paraphrase is FALSY.
            if let (Some(p), Some(tr)) = (
                s.paraphrase.as_ref().filter(|p| !p.is_empty()),
                &s.time_range,
            ) {
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
                present_participant_names: present_participant_names.clone().unwrap_or_default(),
                ..base_ctx.clone()
            }),
            // R-E: `...(input.tuning ? {tuning} : {})` — present iff the input
            // was given (`{}` included).
            recall_tuning: input.tuning.as_ref().map(|_| tuning.clone()),
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
            exclude_memory_ids: exclude_memory_ids.clone().unwrap_or_default(),
            head_size: Some(new_head_size),
            weight_clock_ms,
            embedding_memo: Some(embedding_memo),
            ..Default::default()
        },
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    tracing::info!(
        target: LOG_TARGET,
        chatId = %input.chat_id,
        characterId = %character_id,
        turnIndex = turn_index as u64,
        retrospective = retrospective,
        oldCandidates = old_results.len() as u64,
        newCandidates = new_results.len() as u64,
        newHeadSize = new_head_size as u64,
        tuning = %tuning_summary,
        signalsPinned = input.signals.is_some(),
        // A present `null` when the cutoff is off — the house `"null"` rendering.
        asOfIso = as_of_iso.as_deref().unwrap_or("null"),
        "Recall replay complete"
    );

    Ok(json!({
        "chatId": input.chat_id,
        "characterId": character_id,
        "characterName": character_name,
        "turnIndex": turn_index,
        "totalTurns": turns.len(),
        "signals": signals_echo,
        "query": query,
        "clockIso": clock_iso,
        "oldPath": to_rows(&old_results, DYNAMIC_HEAD_DEFAULT_SIZE),
        "newPath": to_rows(&new_results, new_head_size),
        "oldHeadSize": DYNAMIC_HEAD_DEFAULT_SIZE,
        "newHeadSize": new_head_size,
        "tuning": tuning_summary,
        "signalsPinned": input.signals.is_some(),
        "asOf": as_of_iso,
        "excludedAfterAsOf": exclude_memory_ids.as_ref().map_or(0, Vec::len),
    }))
}
