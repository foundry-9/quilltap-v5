//! Recall replay harness (v4 `lib/memory/recall/replay.ts` — episodic recall
//! overhaul §3; memory refactor §3.8, P4.D291).
//!
//! Given a chat (and optionally a turn index), rebuild the turn's recall query
//! ([`build_recall_query`], distilling or reusing pinned signals), run the
//! search stages TWICE — once with the episodic signals inert (the
//! pre-overhaul path) and once with them live — and then the turn's
//! [`assemble_recall`] over the live path's pool, against the turn's own
//! clock, its pinned participants and the anti-repetition ring as it stood at
//! that turn (read back from the chat's per-message recall records). Returns
//! both candidate tables (cosine, salience, blend, every multiplier that
//! fired, the head cut), the stage trace (what the archive removed, what the
//! budget cut, what the inter-character section took), and what the text
//! fallback would have returned. Nothing is persisted; nothing is stamped —
//! no stage before `deliver` writes, and the replay never delivers.
//!
//! Harness knobs (R7 of the recall multiplier retuning): `tuning` applies to
//! the new path only; `signals` reuses a saved distillation (the distillation
//! is not deterministic); `as_of` drops memories created after the turn
//! opened, so an old turn searches the corpus as it stood then;
//! `memory_budget` sizes the new path's head as the live responder's budget
//! would ([`size_memory_pools`]).
//!
//! Consumed by the `chatRecallReplay` dispatch verb (v4
//! `POST /api/v1/chats/[id]?action=recall-replay`), which the
//! `quilltap recall-replay` CLI wraps.

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::chat_tasks::strip_tool_artifacts;
use crate::cheap_llm::CheapLlmSelection;
use crate::context_summary::{partition_messages_into_turns, PartitionInputMessage};
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_messages_read, chats_read};
use crate::memory_pools::{size_memory_pools, DYNAMIC_HEAD_DEFAULT_SIZE, RETRO_HEAD_SIZE};
use crate::memory_scoring::RECALL_MIN_IMPORTANCE;
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::recall_history::{parse_recall_history, RECALL_HISTORY_TURNS};
use crate::recall_tuning::{
    describe_recall_tuning, resolve_recall_tuning, RecallReplaySignals, RecallTuningInput,
};
use crate::services::cheap_llm_exec::{CheapLlmTaskExecutor, CheapLlmTaskOptions};
use crate::services::memory_recap::distill::{
    distill_memory_search, DistillMessage, DistilledSearch, ExtractionClock,
};
use crate::services::recall::assemble::{
    assemble_recall, PlanRecallSettings, PlanReplay, PreRecall, RecallChat, RecallPlan,
};
use crate::services::recall::context::{
    build_turn_recall_context, TurnRecallContextInput, TurnRecallSettings,
};
use crate::services::recall::deliver::ring_from_debug_log;
use crate::services::recall::query::{build_recall_query, BuildRecallQueryInput, EmbeddingMemo};
use crate::services::recall::search::{search_recall, RecallSearchOptions};
use crate::services::recall::{RecallCandidate, RecallQuery, RecallTrace};

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

/// The memory budget the assemble step sizes its pools by when none is given
/// (the floor — v4 `REPLAY_DEFAULT_MEMORY_BUDGET`).
const REPLAY_DEFAULT_MEMORY_BUDGET: f64 = 2000.0;

/// A pinned `signals` body field as the distillation the search reads (R-J):
/// v4's consumers read `retrospective === true`, an absent `entities` as `[]`,
/// and a pinned `timeRange: null` as no window.
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
    /// The `Date.now()` seam (v4 uses the wall clock; the differential pins
    /// it): the salience clock of both paths when `as_of` is off.
    pub now_ms: f64,
    /// The responding model's memory budget in tokens (route-floored). When
    /// `> 0`, the new path's `selected` rows follow the budget-sized head
    /// (`size_memory_pools`) — the old path keeps the historical fixed head —
    /// and the assemble step sizes its pools by it (else by the 2000 floor).
    pub memory_budget: Option<f64>,
    /// Retuning constants for the NEW path only (R7). Absent → the retuned
    /// defaults. A present `{}` IS a tuning (v4's truthy test): it keeps the
    /// R1 gate on under TF-IDF while describing as `defaults`.
    pub tuning: Option<RecallTuningInput>,
    /// A saved distillation to reuse instead of calling the cheap LLM: the
    /// validated signals and v4's parsed echo (the result's `signals`).
    pub signals: Option<(RecallReplaySignals, Value)>,
    /// Search only memories created before the replayed turn's opening
    /// message, decayed at the turn's clock.
    pub as_of: bool,
}

/// One candidate row of the replay table (v4 `RecallReplayRow`, key order
/// as v4 builds it). Numbers ride as JSON values through `js_number_to_json`
/// (an integer-valued double renders bare; a NaN serialises as `null`).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecallReplayRow {
    pub memory_id: Value,
    pub summary: Value,
    pub kind: Value,
    pub occurred_at: Value,
    pub narrative_time: Value,
    pub created_at: Value,
    pub keywords: Value,
    pub cosine: Value,
    pub salience: Value,
    pub blended_before: Value,
    pub multiplier: Value,
    pub fired: Vec<String>,
    pub blended_after: Value,
    /// True when the row would make the dynamic head at this path's head size.
    pub selected: bool,
}

/// Where the replay's ring came from (v4 `ringSource`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum RingSource {
    /// The turn's own recall records (`debugMemoryLogs`).
    #[serde(rename = "debug-log")]
    DebugLog,
    /// An older chat with no records that early: its CURRENT ring.
    #[serde(rename = "chat")]
    Chat,
}

impl RingSource {
    pub fn as_str(self) -> &'static str {
        match self {
            RingSource::DebugLog => "debug-log",
            RingSource::Chat => "chat",
        }
    }
}

/// v4 `RecallReplayResult`, key order as v4 returns it.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecallReplayResult {
    pub chat_id: String,
    pub character_id: String,
    pub character_name: String,
    pub turn_index: usize,
    pub total_turns: usize,
    /// The distilled turn signals driving the new path (v4's parsed echo).
    pub signals: Value,
    /// The query both paths embedded (once).
    pub query: String,
    /// Clock the distillation resolved against (the replayed turn's timestamp).
    pub clock_iso: String,
    /// Pre-overhaul path: episodic signals inert.
    pub old_path: Vec<RecallReplayRow>,
    /// Overhaul path: retrospective flip, window, entity anchors, multi-probe.
    pub new_path: Vec<RecallReplayRow>,
    /// What the text fallback would have returned for the new path's question.
    pub fallback_path: Vec<RecallReplayRow>,
    pub old_head_size: usize,
    pub new_head_size: usize,
    /// The new path's tuning, summarised (`defaults` when none was given).
    pub tuning: String,
    pub signals_pinned: bool,
    /// The corpus cutoff when `as_of` was set.
    pub as_of: Option<String>,
    pub excluded_after_as_of: usize,
    /// The anti-repetition ring the replay ranked under, in insertion order.
    pub ring: Vec<String>,
    pub ring_source: RingSource,
    /// What each pool of the turn's assemble rendered (ids).
    pub pools: Value,
    /// The stage trace: query, the new path's search stages, then assemble.
    pub trace: RecallTrace,
}

/// Build one candidate row (v4 `toRows`): every field present, `?? null` /
/// `?? []` semantics on the optional memory columns and the adjustment record.
fn to_rows(results: &[RecallCandidate], head_size: usize) -> Vec<RecallReplayRow> {
    use crate::db::js_number_to_json as num;
    results
        .iter()
        .enumerate()
        .map(|(index, r)| {
            let m = &r.memory;
            let nul = |k: &str| m.get(k).cloned().filter(|v| !v.is_null());
            let adj = r.recall_adjustment.as_ref();
            RecallReplayRow {
                memory_id: m.get("id").cloned().unwrap_or(Value::Null),
                summary: m.get("summary").cloned().unwrap_or(Value::Null),
                kind: nul("kind").unwrap_or_else(|| Value::String("semantic".into())),
                occurred_at: nul("occurredAt").unwrap_or(Value::Null),
                narrative_time: nul("narrativeTime").unwrap_or(Value::Null),
                created_at: m.get("createdAt").cloned().unwrap_or(Value::Null),
                keywords: nul("keywords").unwrap_or_else(|| json!([])),
                cosine: num(r.score),
                // `r.salience ?? null` — always a number on the candidate.
                salience: num(r.salience),
                blended_before: adj.map_or(Value::Null, |a| num(a.blended_before)),
                multiplier: adj.map_or(Value::Null, |a| num(a.multiplier)),
                fired: adj.map(|a| a.fired.clone()).unwrap_or_default(),
                blended_after: adj.map_or(Value::Null, |a| num(a.blended_after)),
                selected: index < head_size,
            }
        })
        .collect()
}

/// v4 `ringAsOf(messages, beforeMessageId)` — the anti-repetition ring as it
/// stood when `before_message_id` was written: the head ids of the last
/// [`RECALL_HISTORY_TURNS`] recall records before it (an EMPTY ring counts as
/// a record seen, but contributes no turn). `None` when the chat has no recall
/// records that early (it predates them). `messages` are the chat's message
/// events, in order.
pub fn ring_as_of(messages: &[&Value], before_message_id: &str) -> Option<Vec<Vec<String>>> {
    let cutoff = messages
        .iter()
        .position(|m| m.get("id").and_then(Value::as_str) == Some(before_message_id));
    let before = match cutoff {
        Some(i) => &messages[..i],
        None => messages,
    };
    let mut turns: Vec<Vec<String>> = Vec::new();
    let mut saw_record = false;
    for m in before {
        let lines: Option<Vec<String>> =
            m.get("debugMemoryLogs").and_then(Value::as_array).map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            });
        let Some(ring) = ring_from_debug_log(lines.as_deref()) else {
            continue;
        };
        saw_record = true;
        if !ring.is_empty() {
            turns.push(ring);
        }
    }
    saw_record.then(|| {
        let start = turns.len().saturating_sub(RECALL_HISTORY_TURNS);
        turns.split_off(start)
    })
}

/// The ring as an INSERTION-ORDERED id list (v4 `[...recentlyWhisperedIds]`,
/// a `Set` built turn by turn) — the `HashSet` the multiplier reads cannot
/// give the result its order.
fn ordered_ring(raw_history: &Value) -> Vec<String> {
    let mut seen = HashSet::new();
    parse_recall_history(raw_history)
        .into_iter()
        .flatten()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

/// Run the replay (v4 `runRecallReplay`). Read-only against the chat and memory
/// corpus. Errors carry v4's exact `Error` messages (the route maps them to 400).
///
/// `server_tz` is the host's calendar NAME — the memory calendar's fallback
/// when the instance names no timezone (the distill's TODAY line and day
/// references resolve on the memory calendar, bug 212). It is a parameter
/// rather than a field on [`RunRecallReplayInput`] because the dispatch layer
/// that builds that bag has no zone to give: the host driver supplies it
/// (`None` behaves like a UTC host, which is what the differential pins).
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
        // v4 passes no zone: the distiller reads `refreshMemoryTimeZone()`.
        time_zone: Some(crate::services::memory_time_zone::refresh_memory_time_zone(
            db,
            server_tz.unwrap_or("UTC"),
            input.now_ms,
        )),
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

    // v4 `window[window.length - 1]?.content || ''`.
    let fallback_text = window
        .last()
        .and_then(|m| m.get("content").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();
    let embedding_memo = bounded_memo();
    let mut trace: RecallTrace = Vec::new();
    let query = build_recall_query(
        embedding,
        BuildRecallQueryInput {
            signals: signals.clone(),
            embedding_memo: Some(embedding_memo),
            ..BuildRecallQueryInput::plain(&input.user_id, None, &fallback_text)
        },
        &mut trace,
    )
    .await;
    if crate::jsstr::js_trim(&query.text).is_empty() {
        return Err("Could not derive a recall query for this turn".to_string());
    }

    let recall_settings = crate::services::build_context::read_memory_recall_settings(db);
    let scope_policy = if recall_settings.scope_policy == "exclude" {
        crate::recall_tags::ScopePolicy::Exclude
    } else {
        crate::recall_tags::ScopePolicy::DownWeight
    };
    // v4: EVERY non-removed participant with a characterId (user-controlled
    // included — unlike buildContext's responder+others set).
    let present_about_character_ids: Vec<String> = participants
        .iter()
        .filter(|p| p.get("status").and_then(Value::as_str) != Some("removed"))
        .filter_map(|p| {
            p.get("characterId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .collect();
    // Present characters are ALWAYS read raw (v4 `findByIdRaw`), in
    // participant order; a missing one is `undefined` in v4's array.
    let mut present_characters: Vec<Option<Value>> = Vec::new();
    for id in &present_about_character_ids {
        let cid = id.clone();
        present_characters.push(
            db.read_main(move |c| characters_read::find_by_id_raw(c, &cid))
                .map_err(|e| e.to_string())?,
        );
    }
    let name_of = |c: &Value| {
        c.get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };

    let tuning = resolve_recall_tuning(input.tuning.as_ref());
    let tuning_summary = describe_recall_tuning(&tuning);
    // R4 drops entities that only name someone in the room; it needs their
    // names (handed to the NEW path only).
    let present_participant_names: Option<Vec<String>> = tuning.specific_anchors.then(|| {
        present_characters
            .iter()
            .flatten()
            .map(name_of)
            .filter(|n| !n.is_empty())
            .collect()
    });

    // asOf — the corpus as it stood when the turn opened: leave out every
    // memory created at or after the turn's opening (user) message, and decay
    // salience against the turn's clock rather than today's. The cutoff is the
    // turn's OPENING message; the salience clock is its LAST eligible message
    // (`clock_iso`) — two different instants.
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
    // v4 `nowMs = asOfIso ? Date.parse(clockIso) : undefined` (undefined → the
    // wall clock, the `now_ms` seam here).
    let clock_ms = crate::episodic::js_date_parse_ms(&clock_iso).map_or(f64::NAN, |ms| ms as f64);
    let replay_now_ms: Option<f64> = as_of_iso.as_ref().map(|_| clock_ms);
    let search_now_ms = replay_now_ms.unwrap_or(input.now_ms);
    let mut exclude_memory_ids: Option<HashSet<String>> = None;
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
        let excluded: HashSet<String> = recent
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

    // The anti-repetition ring as it stood at this turn, from the recall
    // records on the messages before it; a chat older than those records falls
    // back to its current ring.
    let message_events: Vec<&Value> = all_messages
        .iter()
        .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
        .collect();
    let reconstructed = opening_message
        .and_then(|m| m.get("id").and_then(Value::as_str))
        .and_then(|id| ring_as_of(&message_events, id));
    let ring_source = if reconstructed.is_some() {
        RingSource::DebugLog
    } else {
        RingSource::Chat
    };
    let ring_history: Value = match &reconstructed {
        Some(turns) => json!({ "turns": turns }),
        None => chat
            .get("commonplaceRecallHistory")
            .cloned()
            .unwrap_or(Value::Null),
    };

    let retrospective = query.retrospective;
    let new_head_size = match input.memory_budget.filter(|b| *b > 0.0) {
        Some(budget) => size_memory_pools(budget, retrospective).head_entries,
        None if retrospective => RETRO_HEAD_SIZE,
        None => DYNAMIC_HEAD_DEFAULT_SIZE,
    };
    let limit = match input.limit {
        Some(l) => l as usize,
        None => 25.max(new_head_size + 10),
    };

    // The same context assembly as the live paths. The OLD path leaves the
    // retrospective flag off entirely; the fresh-event boost runs against the
    // REPLAYED TURN's clock.
    let base_ctx = build_turn_recall_context(TurnRecallContextInput {
        chat_id: &input.chat_id,
        chat_project_id: chat.get("projectId").and_then(Value::as_str),
        commonplace_recall_history: &ring_history,
        recall_settings: TurnRecallSettings {
            scope_policy,
            expand_related: recall_settings.expand_related,
        },
        turn_context: signals.as_ref().and_then(|s| s.context),
        turn_temporal: signals.as_ref().and_then(|s| s.temporal),
        turn_retrospective: false,
        present_about_character_ids,
        present_participant_names: Vec::new(),
        now_ms: clock_ms,
        tuning: None,
    });

    // OLD path — episodic signals inert: no probes, anchors or window.
    let old_query = RecallQuery {
        probes: Vec::new(),
        anchors: Vec::new(),
        window: None,
        ..query.clone()
    };
    let old_results = search_recall(
        db,
        &character_id,
        &old_query,
        &RecallSearchOptions {
            limit: Some(limit),
            min_importance: Some(RECALL_MIN_IMPORTANCE),
            recall_context: Some(base_ctx.clone()),
            exclude_ids: exclude_memory_ids.clone(),
            now_ms: search_now_ms,
            ..Default::default()
        },
        &mut Vec::new(),
        Some("old"),
    )
    .await
    .map_err(|e| e.to_string())?;

    // NEW path — the live path under the requested tuning.
    let mut new_ctx = base_ctx.clone();
    new_ctx.turn_retrospective = retrospective;
    // `...(input.tuning ? { tuning } : {})` — present iff the input was given
    // (`{}` included).
    if input.tuning.is_some() {
        new_ctx.tuning = Some(tuning.clone());
    }
    if let Some(names) = &present_participant_names {
        new_ctx.present_participant_names = names.clone();
    }
    let new_search = RecallSearchOptions {
        limit: Some(limit),
        min_importance: Some(RECALL_MIN_IMPORTANCE),
        recall_context: Some(new_ctx.clone()),
        exclude_ids: exclude_memory_ids.clone(),
        head_size: Some(new_head_size),
        now_ms: search_now_ms,
        ..Default::default()
    };
    let new_results = search_recall(
        db,
        &character_id,
        &query,
        &new_search,
        &mut trace,
        Some("head"),
    )
    .await
    .map_err(|e| e.to_string())?;
    // What the text fallback would have done with the same question.
    let fallback_results = search_recall(
        db,
        &character_id,
        &query,
        &RecallSearchOptions {
            force_text: true,
            ..new_search.clone()
        },
        &mut Vec::new(),
        Some("fallback"),
    )
    .await
    .map_err(|e| e.to_string())?;

    // The turn's assemble over the live pool: what each pool took and cut.
    let others: Vec<(String, String)> = present_characters
        .iter()
        .flatten()
        .filter_map(|c| {
            let id = c.get("id").and_then(Value::as_str)?;
            (id != character_id).then(|| (id.to_string(), name_of(c)))
        })
        .collect();
    let mut present = vec![(character_id.clone(), character_name.clone())];
    present.extend(others.iter().cloned());
    let chat_id = input.chat_id.clone();
    let bundle = assemble_recall(
        db,
        embedding,
        completion,
        executor,
        RecallPlan {
            user_id: &input.user_id,
            embedding_profile_id: None,
            provider: None,
            character_id: &character_id,
            character_name: &character_name,
            character_mount_point_id: None,
            chat: RecallChat {
                id: &chat_id,
                project_id: chat.get("projectId").and_then(Value::as_str),
                compaction_generation: chat
                    .get("compactionGeneration")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
                commonplace_recall_history: &ring_history,
                timeline_mode: chat.get("timelineMode").and_then(Value::as_str),
            },
            memory_budget: input.memory_budget.unwrap_or(REPLAY_DEFAULT_MEMORY_BUDGET) as i64,
            min_importance: RECALL_MIN_IMPORTANCE,
            skip_memories: false,
            present,
            inter_character_subjects: others,
            current_state_tokens: 0,
            recall_settings: PlanRecallSettings {
                scope_policy,
                expand_related: recall_settings.expand_related,
                per_turn_conversation_summaries: recall_settings.per_turn_conversation_summaries,
            },
            recent_window: fallback_text.clone(),
            distill: None,
            pre_recall: Some(PreRecall {
                query: query.clone(),
                candidates: new_results.candidates.clone(),
                ranked: Some(new_results.ranked.clone()),
                trace: Vec::new(),
            }),
            recap: None,
            conversations: None,
            on_status_change: None,
            replay: Some(PlanReplay {
                exclude_ids: Some(exclude_memory_ids.clone().unwrap_or_default()),
                now_ms: replay_now_ms,
                tuning: None,
                ring: new_ctx.recently_whispered_ids.clone(),
            }),
            now_ms: search_now_ms,
        },
    )
    .await;
    trace.extend(bundle.trace.iter().cloned());

    tracing::info!(
        target: LOG_TARGET,
        chatId = %input.chat_id,
        characterId = %character_id,
        turnIndex = turn_index as u64,
        retrospective = retrospective,
        oldCandidates = old_results.candidates.len() as u64,
        newCandidates = new_results.candidates.len() as u64,
        newHeadSize = new_head_size as u64,
        tuning = %tuning_summary,
        signalsPinned = input.signals.is_some(),
        // A present `null` when the cutoff is off — the house `"null"` rendering.
        asOfIso = as_of_iso.as_deref().unwrap_or("null"),
        ringSource = ring_source.as_str(),
        "Recall replay complete"
    );

    let result = RecallReplayResult {
        chat_id: input.chat_id.clone(),
        character_id,
        character_name,
        turn_index,
        total_turns: turns.len(),
        signals: signals_echo,
        query: query.text,
        clock_iso,
        old_path: to_rows(&old_results.candidates, DYNAMIC_HEAD_DEFAULT_SIZE),
        new_path: to_rows(&new_results.candidates, new_head_size),
        fallback_path: to_rows(&fallback_results.candidates, new_head_size),
        old_head_size: DYNAMIC_HEAD_DEFAULT_SIZE,
        new_head_size,
        tuning: tuning_summary,
        signals_pinned: input.signals.is_some(),
        as_of: as_of_iso,
        excluded_after_as_of: exclude_memory_ids.as_ref().map_or(0, HashSet::len),
        ring: ordered_ring(&ring_history),
        ring_source,
        pools: bundle.pools.to_json(),
        trace,
    };
    serde_json::to_value(&result).map_err(|e| e.to_string())
}
