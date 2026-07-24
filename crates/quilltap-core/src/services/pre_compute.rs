//! Pre-context pre-compute — the proactive memory-recall task (v4
//! `lib/services/chat-message/pre-compute.service.ts`, `proactiveRecallTask`).
//!
//! v4's pre-compute service runs TWO tasks in a `Promise.all` before
//! `buildMessageContext`:
//!
//!   1. **Compression cache check** — see whether the cheap LLM already produced a
//!      compressed history for this chat at the current message count.
//!   2. **Proactive memory recall** — extract keywords from the messages since the
//!      character last spoke, then semantic-search this character's memories.
//!
//! **Structural divergence (recorded, deliberate):** v4 holds both tasks in one
//! `pre-compute.service.ts`. v5's compression half is ALREADY PORTED inline at its
//! landed site (`orchestrator.rs`, the async pre-compression cache check,
//! W4.4a4 — differential-pinned there); this module holds ONLY the sibling
//! proactive task. v4 runs the two in parallel purely as a latency optimization
//! (`Promise.all`); v5 runs them sequentially at the same spine site — the
//! parallelism has no DB-observable effect (each reads independent state and the
//! proactive task's only writes are through `search_memories_semantic`'s
//! access-time bump, which is order-independent of the compression cache read).
//!
//! The proactive task's outcome rides `BuildContextInput`
//! (`pre_searched_memories` / `recall_signals`); a non-empty `pre_searched_memories`
//! suppresses build_context's own fallback distill exactly as v4
//! `context-manager.ts:1141-1145`, and `recall_signals` seeds the retrospective
//! cadence on both paths.
//!
//! **The keep-alive is deliberately NOT ported here (recorded).** v4's
//! `runPreContextPreCompute` arms a 15 s `setInterval` keep-alive during a
//! compression-cache miss (`pre-compute.service.ts:98-124`) so an SSE proxy does
//! not drop the connection during a long buildContext. v5 keep-alives at the
//! TRANSPORT layer instead — `quilltap-web`'s one global SSE stream sends
//! `: keep-alive\n\n` every 15 s for the WHOLE connection
//! (`quilltap-web::events::KEEP_ALIVE_INTERVAL`), which strictly subsumes v4's
//! per-turn interval (always on, not just during compression) and keeps the
//! boundary clean (a heartbeat is a transport concern, not spine business logic).
//! Porting v4's per-turn interval into the spine would double the keep-alive and
//! violate the transport-agnostic boundary — so this module owns only the
//! proactive task, and `RunPreContextPreComputeResult.stopKeepAlive` has no v5
//! analog.

use serde_json::Value;

use crate::cheap_llm::{
    resolve_uncensored_cheap_llm_selection, CheapLlmProfile, CheapLlmSelection,
    DangerousContentSettings,
};
use crate::db::runtime::Db;
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::recall_tags::ScopePolicy;
use crate::services::build_context::read_memory_recall_settings;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::dangerous_content::chat_override::is_chat_active_dangerous;
use crate::services::memory_recap::distill::{
    distill_memory_search, DistillMessage, DistilledSearch, ExtractionClock,
};
use crate::services::memory_service::{
    search_memories_semantic, RecallContextInput, SemanticSearchOptions, SemanticSearchResult,
};

/// The resolved orchestrator state the proactive task reads (v4
/// `RunPreContextPreComputeOptions`, the subset `proactiveRecallTask` consumes).
pub struct ProactiveRecallInput<'a> {
    /// The chat net-JSON (v4 `chat`) — `projectId`, `timelineMode`,
    /// `commonplaceRecallHistory`, and the `isChatActiveDangerous` gate.
    pub chat: &'a Value,
    /// v4 `character.id` — empty guards the task off (as `!character.id` does).
    pub character_id: &'a str,
    /// v4 `character.name` — the keyword-extraction subject + the
    /// `recalling_memories` status message.
    pub character_name: &'a str,
    /// v4 `characterParticipant.id` — the window keys on the PARTICIPANT id, not
    /// `character.id` (a character can hold multiple participant seats).
    pub character_participant_id: &'a str,
    /// v4 `presentAboutCharacterIds` — the responding character + every other
    /// character participant, for the participant-aware recall boost (item 4).
    pub present_about_character_ids: &'a [String],
    /// v4 `isContinueMode` — a continue turn has no just-saved user message to push.
    pub is_continue_mode: bool,
    /// v4 `content` — the verbatim user-message text for this turn (empty in
    /// continue mode).
    pub content: &'a str,
    /// v4 `existingMessages` — the ChatEvent snapshot (raw net JSON).
    pub existing_messages: &'a [Value],
    /// v4 `cheapLLMSelection` — `None` guards the task off (as `!cheapLLMSelection`
    /// does).
    pub cheap_llm_selection: Option<&'a CheapLlmSelection>,
    /// v4 `dangerSettings` — the uncensored reroute for actively-dangerous chats.
    pub danger_settings: &'a DangerousContentSettings,
    /// v4 `allProfiles` — the reroute's candidate pool.
    pub available_profiles: &'a [CheapLlmProfile],
    /// v4 `userId`.
    pub user_id: &'a str,
    /// v4 `chatId`.
    pub chat_id: &'a str,
    /// The wall-clock base (the injected `now_ms` seam standing in for v4's
    /// `new Date()`): drives the distill's TODAY line and the search's time decay.
    pub now_ms: i64,
}

/// v4 `ProactiveRecallOutcome` (`{ memories, signals }`). Both fields are `None`
/// (v4 `undefined`) when the task didn't run to a result; a successful extraction
/// always carries `signals`, and `memories` is `None` only when the search came
/// back empty or threw (the signals STILL flow so the retrospective cadence
/// fires). The whole task returns `None` (v4's `return undefined`) for the
/// guard / never-spoke / empty-window / extraction-fail short-circuits.
pub struct ProactiveRecallOutcome {
    pub memories: Option<Vec<SemanticSearchResult>>,
    pub signals: Option<DistilledSearch>,
}

/// v4 `proactiveRecallTask`'s windowing (`pre-compute.service.ts:185-214`): the
/// USER/ASSISTANT messages since this character last spoke, as `(role, content)`
/// pairs. Returns `None` (v4's `return undefined`) when the character never spoke
/// or the resulting window is empty. Pure — unit-tested directly; the downstream
/// distill/search legs are proven end-to-end by the tier-3 differential.
pub(crate) fn messages_since_last_spoke(
    existing_messages: &[Value],
    character_participant_id: &str,
    is_continue_mode: bool,
    content: &str,
) -> Option<Vec<(String, String)>> {
    // Filter to actual message events with proper type narrowing (v4:
    // `type === 'message' && 'role' in m && 'content' in m`).
    let message_events: Vec<&Value> = existing_messages
        .iter()
        .filter(|m| {
            m.get("type").and_then(Value::as_str) == Some("message")
                && m.get("role").is_some()
                && m.get("content").is_some()
        })
        .collect();

    // Find messages since this character last spoke. v4 keys on the PARTICIPANT id:
    // `m.role === 'ASSISTANT' && m.participantId === characterParticipant.id`.
    let is_char_message = |m: &Value| {
        m.get("role").and_then(Value::as_str) == Some("ASSISTANT")
            && m.get("participantId").and_then(Value::as_str) == Some(character_participant_id)
    };

    // v4 `characterMessages.length === 0` (never spoke) → undefined. Then
    // `lastIndexOf(lastCharacterMessage)` on `messageEvents`; since each event is a
    // distinct object, that is the LAST index in `messageEvents` matching the
    // predicate.
    let last_char_index = message_events.iter().rposition(|m| is_char_message(m))?;

    // v4 `messageEvents.slice(lastCharacterMessageIndex + 1).filter(USER || ASSISTANT)`.
    let mut window: Vec<(String, String)> = message_events[last_char_index + 1..]
        .iter()
        .filter_map(|m| {
            let role = m.get("role").and_then(Value::as_str)?;
            if role != "USER" && role != "ASSISTANT" {
                return None;
            }
            let content = m
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Some((role.to_string(), content))
        })
        .collect();

    // Include the new user message that was just saved but isn't in the
    // existingMessages snapshot (v4: `if (!isContinueMode && content)` — a truthy,
    // non-empty string only).
    if !is_continue_mode && !content.is_empty() {
        window.push(("USER".to_string(), content.to_string()));
    }

    if window.is_empty() {
        return None;
    }

    Some(window)
}

/// v4 `proactiveRecallTask` (`pre-compute.service.ts:172-340`). Windows the
/// messages since this character last spoke, distills them into recall signals via
/// the cheap LLM, then semantic-searches the character's memories. Emits the
/// `recalling_keywords` / `recalling_memories` status frames through `emit_status`
/// (stage, message) at v4's exact points — the caller wraps the frame with the
/// character name/id.
#[allow(clippy::too_many_arguments)]
pub async fn proactive_recall_task<EMB, CMP>(
    db: &Db,
    embedding: &EMB,
    completion: &CMP,
    executor: &CheapLlmTaskExecutor,
    input: &ProactiveRecallInput<'_>,
    emit_status: &mut dyn FnMut(&str, String),
) -> Option<ProactiveRecallOutcome>
where
    EMB: EmbeddingProvider,
    CMP: CompletionProvider,
{
    // v4 `if (!cheapLLMSelection || !character.id) return undefined`.
    let selection = input.cheap_llm_selection?;
    if input.character_id.is_empty() {
        return None;
    }

    // v4 `messagesSinceLastSpoke` (pre-compute.service.ts:185-214) — the window +
    // never-spoke / empty-window short-circuits. `None` here → the whole task
    // returns `undefined`.
    let messages_since_last_spoke = messages_since_last_spoke(
        input.existing_messages,
        input.character_participant_id,
        input.is_continue_mode,
        input.content,
    )?;

    emit_status(
        "recalling_keywords",
        "Analyzing recent conversation...".to_string(),
    );

    // For dangerous chats, use the uncensored provider for keyword extraction.
    // Gate on the canonical accessor so an Off-duty chat never reroutes here,
    // independent of how `dangerSettings` was resolved upstream (v4:225-234).
    let recall_selection = if is_chat_active_dangerous(Some(input.chat)) {
        resolve_uncensored_cheap_llm_selection(
            selection.clone(),
            true,
            Some(input.danger_settings),
            input.available_profiles,
        )
    } else {
        selection.clone()
    };

    // Extract keywords via the cheap LLM, stripping tool artifacts from assistant
    // messages so the keyword extractor sees only narrative content (v4:238-260, the
    // `reduce` that lowercases the role and drops emptied assistant slices).
    let distill_messages: Vec<DistillMessage> = messages_since_last_spoke
        .iter()
        .filter_map(|(role, content)| {
            let role = role.to_lowercase();
            if role == "assistant" {
                let cleaned = crate::chat_tasks::strip_tool_artifacts(content)?;
                Some(DistillMessage { role, content: cleaned })
            } else {
                Some(DistillMessage {
                    role,
                    content: content.clone(),
                })
            }
        })
        .collect();

    // Episodic recall: the distillation resolves "last week" into an absolute
    // timeRange against this clock (v4 passes `{ nowIso: new Date().toISOString(),
    // timelineMode: chat.timelineMode ?? 'realtime' }`; the injected now_ms seam
    // stands in for the wall clock).
    let clock = ExtractionClock {
        now_iso: crate::clock::iso_from_unix_ms(input.now_ms),
        timeline_mode: input
            .chat
            .get("timelineMode")
            .and_then(Value::as_str)
            .unwrap_or("realtime")
            .to_string(),
    };

    // v4 `if (!keywordResult.success || !keywordResult.result ||
    // keywordResult.result.keywords.length === 0) return undefined`. Our
    // `distill_memory_search` returns `None` on `!success`; the parse-failed catch
    // arm returns `Some` with empty keywords, which the length guard rejects.
    let signals = match distill_memory_search(
        executor,
        completion,
        &distill_messages,
        input.character_name,
        &recall_selection,
        input.character_id,
        Some(&clock),
    )
    .await
    {
        Some(d) if !d.keywords.is_empty() => d,
        _ => return None,
    };

    emit_status(
        "recalling_memories",
        format!("Searching {}'s memories...", input.character_name),
    );

    // Prefer the natural-language paraphrase as the embedding query (item 3); a
    // keyword bag throws away the sentence structure the embedding model is trained
    // on. Fall back to the keyword join when the model omits a paraphrase (v4's
    // `||`, where an empty paraphrase is falsy — the parse already null-empties it).
    let search_query = signals
        .paraphrase
        .clone()
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| signals.keywords.join(" "));

    // Same per-turn recall context the dynamic head uses, so the proactive path
    // gets identical scope gating, temporal down-weighting, context steering,
    // participant boost, and anti-repetition (see recall_tags). chat.projectId is
    // the rename-proof comparand; the turn's temporal/context guess and
    // present-character set drive items 3-4.
    let recall_settings = read_memory_recall_settings(db);
    let retrospective = signals.retrospective;
    let recall_context = RecallContextInput {
        current_project_id: input
            .chat
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_string),
        scope_policy: if recall_settings.scope_policy == "exclude" {
            ScopePolicy::Exclude
        } else {
            ScopePolicy::DownWeight
        },
        turn_context: signals.context,
        turn_temporal: signals.temporal,
        turn_retrospective: retrospective,
        present_about_character_ids: input.present_about_character_ids.to_vec(),
        expand_related: recall_settings.expand_related,
        recently_whispered_ids: Some(crate::recall_history::recently_whispered_id_set(
            input
                .chat
                .get("commonplaceRecallHistory")
                .unwrap_or(&Value::Null),
        )),
    };

    // Retrospective turns: multi-probe (entity string; paraphrase + resolved date
    // phrase) so a "remember Lighthouse Point last week?" turn probes the vector
    // space from every angle the reference offers (v4:296-308).
    let mut extra_probes: Vec<String> = Vec::new();
    if retrospective {
        let entity_probe = crate::jsstr::js_trim(&signals.entities.join(" ")).to_string();
        if !entity_probe.is_empty() {
            extra_probes.push(entity_probe);
        }
        if let (Some(p), Some(tr)) = (&signals.paraphrase, &signals.time_range) {
            extra_probes.push(format!(
                "{p} (around {} to {})",
                crate::jsstr::utf16_truncate(&tr.from, 10),
                crate::jsstr::utf16_truncate(&tr.to, 10)
            ));
        }
    }

    // v4 wraps only the search in try/catch: on empty results OR a throw, it returns
    // `{ memories: undefined, signals }` (the signals STILL flow). v5's
    // `search_memories_semantic` returns `Result`; every non-(Ok non-empty) arm
    // converges on `memories: None` (v4 logs a warning on the throw arm — outside
    // the differential contract, so not reproduced).
    let search = search_memories_semantic(
        db,
        embedding,
        input.character_id,
        &search_query,
        &SemanticSearchOptions {
            user_id: input.user_id.to_string(),
            limit: Some(20),
            min_importance: Some(0.3),
            recall_context: Some(recall_context),
            // Episodic recall: entity anchoring always (a verbatim place name cannot
            // be sliced off by the cosine floor); window + probes only on
            // retrospective turns.
            entity_anchors: signals.entities.clone(),
            occurred_within: if retrospective {
                signals.time_range.clone()
            } else {
                None
            },
            extra_probes,
            now_ms: input.now_ms as f64,
            ..Default::default()
        },
    )
    .await;

    // v4: on non-empty results → `{ memories: results.slice(0, 10), signals }`; on
    // empty OR a throw → `{ memories: undefined, signals }` (signals STILL flow).
    Some(finish_outcome(search.unwrap_or_default(), signals))
}

/// v4's search-outcome shaping (`pre-compute.service.ts:328-339`): non-empty
/// results become the memories capped at 10 (`.slice(0, 10)`); an empty list
/// yields `memories: None` — but the signals STILL flow so the retrospective
/// cadence fires. (v5 collapses the empty-and-throw arms here: `search.
/// unwrap_or_default()` maps a search error to the empty list, which v4's `catch`
/// returns identically.)
fn finish_outcome(results: Vec<SemanticSearchResult>, signals: DistilledSearch) -> ProactiveRecallOutcome {
    let memories = if results.is_empty() {
        None
    } else {
        Some(results.into_iter().take(10).collect())
    };
    ProactiveRecallOutcome {
        memories,
        signals: Some(signals),
    }
}

#[cfg(test)]
mod tests {
    //! Unit specs porting v4's `proactiveRecallTask` jest suite
    //! (`pre-compute.service.test.ts:138-264`). The guard/windowing cases run
    //! against the pure [`messages_since_last_spoke`] helper and the 10-cap
    //! against [`finish_outcome`]; the distill/search legs (jest
    //! "extracts-then-searches" / "extraction fails" / "search throws" /
    //! search-empty-signals-flow / multi-character) are proven end-to-end against
    //! v4's REAL code by the tier-3 `precompute` differential
    //! (`QT_ORACLE_PRECOMPUTE`), which drives the whole `proactive_recall_task`
    //! over a fixture DB — a stronger check than a mocked unit here could be. The
    //! uncensored reroute (jest "routes through the uncensored cheap-LLM
    //! selection") is the pure `resolve_uncensored_cheap_llm_selection`, itself
    //! tier-1 differentialed in `cheap_llm`.
    use super::*;
    use serde_json::json;

    fn fake_result(id: &str) -> SemanticSearchResult {
        SemanticSearchResult {
            memory: json!({ "id": id }),
            score: 0.5,
            used_embedding: true,
            effective_weight: 0.5,
            raw_weight: 0.5,
            recall_adjustment: None,
        }
    }

    /// jest "caps the returned memory list at 10".
    #[test]
    fn caps_memories_at_ten() {
        let many: Vec<SemanticSearchResult> =
            (0..15).map(|i| fake_result(&format!("m{i}"))).collect();
        let out = finish_outcome(many, DistilledSearch::default());
        assert_eq!(out.memories.expect("memories").len(), 10);
    }

    /// An empty result set → `memories: None`, but the signals STILL flow (v4's
    /// search-empty arm; the retrospective cadence still fires downstream).
    #[test]
    fn empty_results_drop_memories_keep_signals() {
        let out = finish_outcome(Vec::new(), DistilledSearch::default());
        assert!(out.memories.is_none());
        assert!(out.signals.is_some());
    }

    fn msg(role: &str, content: &str, participant: &str) -> Value {
        json!({
            "type": "message",
            "role": role,
            "content": content,
            "participantId": participant,
        })
    }

    /// jest "returns undefined when the character has never spoken".
    #[test]
    fn never_spoke_yields_none() {
        let events = vec![msg("USER", "hi", "p-user")];
        assert!(messages_since_last_spoke(&events, "p-char", false, "").is_none());
    }

    /// jest "extracts keywords from messages since the character last spoke" — the
    /// window content + order (the just-saved user line is appended last).
    #[test]
    fn window_content_and_order() {
        let events = vec![
            msg("ASSISTANT", "past line", "p-char"),
            msg("USER", "a new question", "p-user"),
        ];
        let window = messages_since_last_spoke(&events, "p-char", false, "a fresh user line")
            .expect("window");
        assert_eq!(
            window
                .iter()
                .map(|(_, c)| c.as_str())
                .collect::<Vec<_>>(),
            vec!["a new question", "a fresh user line"],
        );
    }

    /// The window keys on the PARTICIPANT id, not `character.id`: a message from
    /// the SAME character on a DIFFERENT participant seat does not count as "spoke".
    #[test]
    fn window_is_participant_scoped() {
        let events = vec![
            // Same character, different participant seat → not "this" participant.
            msg("ASSISTANT", "other seat", "p-other"),
            msg("USER", "u1", "p-user"),
            // This participant last spoke here.
            msg("ASSISTANT", "my line", "p-char"),
            msg("USER", "u2", "p-user"),
        ];
        let window =
            messages_since_last_spoke(&events, "p-char", false, "").expect("window");
        assert_eq!(
            window.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>(),
            vec!["u2"],
        );
    }

    /// Continue mode does NOT push the (empty) content line; an empty resulting
    /// window short-circuits to `None`.
    #[test]
    fn continue_mode_no_push_empty_window_none() {
        let events = vec![msg("ASSISTANT", "my line", "p-char")];
        // Continue mode, no trailing user/assistant messages → empty window → None.
        assert!(messages_since_last_spoke(&events, "p-char", true, "ignored").is_none());
        // Non-continue with content → the pushed user line makes it non-empty.
        let window =
            messages_since_last_spoke(&events, "p-char", false, "typed").expect("window");
        assert_eq!(
            window.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>(),
            vec!["typed"],
        );
    }

    /// Non-message events and events missing role/content are filtered out before
    /// windowing (v4's `type === 'message' && 'role' in m && 'content' in m`).
    #[test]
    fn non_message_events_filtered() {
        let events = vec![
            msg("ASSISTANT", "my line", "p-char"),
            json!({ "type": "status", "role": "USER", "content": "not a message" }),
            msg("USER", "real", "p-user"),
        ];
        let window =
            messages_since_last_spoke(&events, "p-char", false, "").expect("window");
        assert_eq!(
            window.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>(),
            vec!["real"],
        );
    }
}
