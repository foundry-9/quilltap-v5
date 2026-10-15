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
//! parallelism has no DB-observable effect (each reads independent state; the
//! proactive task writes nothing — v4 `3f7320138` F6 — buildContext stamps what
//! it formats).
//!
//! The proactive task's outcome is a [`PreRecall`] (v4 `584d28164`): the
//! turn's ONE recall query — distilled and embedded once here — and what that
//! query found. It rides `BuildContextInput::pre_recall`; when it found
//! something, the context build reuses both (no second distillation, no second
//! embedding); an empty or failed search hands the build nothing it uses, and
//! the build distils again interactively (R-L8 — v4 `assemble.ts:276-333`).
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
};
use crate::db::runtime::Db;
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::recall_tags::ScopePolicy;
use crate::services::build_context::read_memory_recall_settings;
use crate::services::cheap_llm_exec::{CheapLlmTaskExecutor, CheapLlmTaskOptions};
use crate::services::dangerous_content::chat_override::should_use_uncensored_route;
use crate::services::dangerous_content::resolver::ResolvedConciergePolicy;
use crate::services::memory_recap::distill::{
    distill_memory_search, DistillMessage, ExtractionClock,
};
use crate::services::recall::assemble::PreRecall;
use crate::services::recall::context::{
    build_turn_recall_context, TurnRecallContextInput, TurnRecallSettings,
};
use crate::services::recall::query::{build_recall_query, BuildRecallQueryInput};
use crate::services::recall::search::{search_recall, RecallSearchOptions};
use crate::services::recall::types::RecallTrace;

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
    /// v4 `presentParticipantNames` (`f7d8064be`) — display names of those same
    /// characters; R4's entity anchors skip a name that merely names someone in
    /// the room (honoured by P4.D267).
    pub present_participant_names: &'a [String],
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
    pub concierge_policy: &'a ResolvedConciergePolicy,
    /// v4 `allProfiles` — the reroute's candidate pool.
    pub available_profiles: &'a [CheapLlmProfile],
    /// v4 `userId`.
    pub user_id: &'a str,
    /// v4 `chatId`.
    pub chat_id: &'a str,
    /// The wall-clock base (the injected `now_ms` seam standing in for v4's
    /// `new Date()`): drives the distill's TODAY line and the search's time decay.
    pub now_ms: i64,
    /// The SERVER-LOCAL IANA zone (the v5 seam for v4's ambient process zone):
    /// the distill's TODAY line and its deterministic day-reference scan resolve
    /// their calendar in it. ⚠ NOT the story/timestamp timezone
    /// (`ProcessMessageInput::timezone`, v4 `resolveTimezone`) — that one can be
    /// a per-chat setting; this one is where the server stands.
    pub server_tz: Option<&'a str>,
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
) -> Option<PreRecall>
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
    let recall_selection = if should_use_uncensored_route(Some(input.chat)) {
        resolve_uncensored_cheap_llm_selection(
            selection.clone(),
            true,
            Some(input.concierge_policy),
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
                Some(DistillMessage {
                    role,
                    content: cleaned,
                })
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
        // Bug 212: the memory calendar's zone (v4 `clock?.timeZone ?? await
        // refreshMemoryTimeZone()` inside the distiller).
        time_zone: Some(crate::services::memory_time_zone::refresh_memory_time_zone(
            db,
            input.server_tz.unwrap_or("UTC"),
            input.now_ms as f64,
        )),
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
        // v4's `background` default, and correct here: this pass runs after
        // delivery with nobody waiting on it, so it keeps the generous ceiling
        // and the one free timeout retry (v4 `02d4efa1b`, bug 115 — the fix
        // deliberately left the proactive pass alone).
        CheapLlmTaskOptions::default(),
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

    // The turn's one query: the paraphrase (else the keyword join), embedded
    // once here and reused by every recall pool in the context build.
    let mut trace: RecallTrace = Vec::new();
    let keywords_text = signals.keywords.join(" ");
    let query = build_recall_query(
        embedding,
        BuildRecallQueryInput {
            signals: Some(signals.clone()),
            ..BuildRecallQueryInput::plain(input.user_id, None, &keywords_text)
        },
        &mut trace,
    )
    .await;

    // The same per-turn recall context the fallback head builds: scope gating,
    // temporal weighting, context steering, participant boost, anti-repetition
    // and the fresh-event boost. (v4 passes `Date.now()`; the injected now_ms
    // seam stands in for the wall clock.)
    let recall_settings = read_memory_recall_settings(db);
    let recall_context = build_turn_recall_context(TurnRecallContextInput {
        chat_id: input.chat_id,
        chat_project_id: input.chat.get("projectId").and_then(Value::as_str),
        commonplace_recall_history: input
            .chat
            .get("commonplaceRecallHistory")
            .unwrap_or(&Value::Null),
        recall_settings: TurnRecallSettings {
            scope_policy: if recall_settings.scope_policy == "exclude" {
                ScopePolicy::Exclude
            } else {
                ScopePolicy::DownWeight
            },
            expand_related: recall_settings.expand_related,
        },
        turn_context: signals.context,
        turn_temporal: signals.temporal,
        turn_retrospective: query.retrospective,
        present_about_character_ids: input.present_about_character_ids.to_vec(),
        present_participant_names: input.present_participant_names.to_vec(),
        now_ms: input.now_ms as f64,
        tuning: None,
    });

    match search_recall(
        db,
        input.character_id,
        &query,
        &RecallSearchOptions {
            // The context builder drops every candidate already in the frozen
            // archive, then fills the head from what is left — and does not
            // search again. Pull enough that a full archive overlap still
            // leaves the largest head it can ask for (a retrospective turn:
            // 2 × max).
            limit: Some(crate::memory_pools::PROACTIVE_RECALL_POOL_SIZE),
            min_importance: Some(crate::memory_scoring::RECALL_MIN_IMPORTANCE),
            recall_context: Some(recall_context),
            now_ms: input.now_ms as f64,
            ..Default::default()
        },
        &mut trace,
        Some("head"),
    )
    .await
    {
        Ok(result) => Some(PreRecall {
            query,
            candidates: result.candidates,
            ranked: Some(result.ranked),
            trace,
        }),
        Err(error) => {
            tracing::warn!(
                target: "quilltap::pre_compute",
                chatId = %input.chat_id,
                characterId = %input.character_id,
                error = %crate::db::fallback::error_text(&error),
                "Proactive memory recall: memory search failed; the turn recalls with the query alone"
            );
            Some(PreRecall {
                query,
                candidates: Vec::new(),
                ranked: None,
                trace,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit specs porting v4's `proactiveRecallTask` jest suite
    //! (`pre-compute.service.test.ts:138-264`). The guard/windowing cases run
    //! against the pure [`messages_since_last_spoke`] helper; the distill/search legs (jest
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
            window.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>(),
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
        let window = messages_since_last_spoke(&events, "p-char", false, "").expect("window");
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
        let window = messages_since_last_spoke(&events, "p-char", false, "typed").expect("window");
        assert_eq!(
            window.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>(),
            vec!["typed"],
        );
    }

    /// The autonomous-room shape that DOES bail, transcribed from a v4
    /// instrumentation run (P4.20). An autonomous turn arrives with
    /// `isContinueMode: true` and empty `content`, so nothing is appended to the
    /// window; in a room whose only seeded line belongs to the OTHER participant
    /// seat, `characterMessages` is empty and the task never reaches the cheap
    /// LLM. Sixteen of the enclave-step fixture's seventeen turn rooms are this
    /// shape, and instrumenting v4's `proactiveRecallTask` showed every one of
    /// them bailing here — which is why the Fold room below is the only room in
    /// that fixture that can expose a windowing divergence at all.
    #[test]
    fn autonomous_opener_from_other_seat_bails() {
        let events = vec![msg("ASSISTANT", "Opening remark 1.", "p-other")];
        assert!(messages_since_last_spoke(&events, "p-char", true, "").is_none());
    }

    /// The Fold-room shape, transcribed from the same instrumentation run: eleven
    /// alternating ASSISTANT seeds, the responder holding the even seat and so
    /// last speaking at line 10. v4 computes `lastCharacterMessageIndex = 9` and
    /// distills the SINGLE trailing line — not the last-12 window buildContext
    /// would use, which is the whole eleven. This is the one place the two windows
    /// differ, and it is pinned end-to-end against v4's real code by the
    /// `fold-room-window-is-tail-only` case in the `precompute` differential.
    #[test]
    fn autonomous_fold_room_windows_only_the_trailing_line() {
        let events: Vec<Value> = (0..11)
            .map(|k| {
                msg(
                    "ASSISTANT",
                    &format!("Fold seed line {}.", k + 1),
                    if k % 2 == 0 { "p-other" } else { "p-char" },
                )
            })
            .collect();
        let window = messages_since_last_spoke(&events, "p-char", true, "").expect("window");
        assert_eq!(
            window,
            vec![("ASSISTANT".to_string(), "Fold seed line 11.".to_string())],
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
        let window = messages_since_last_spoke(&events, "p-char", false, "").expect("window");
        assert_eq!(
            window.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>(),
            vec!["real"],
        );
    }
}
