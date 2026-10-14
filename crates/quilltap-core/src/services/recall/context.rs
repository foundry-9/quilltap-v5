//! The turn's recall context in OWNED form, and the two builders v4's recall
//! takes from `lib/memory/recall/tags.ts` — `buildTurnRecallContext` and
//! `buildRetrospectiveProbes`.
//!
//! [`crate::recall_tags::RecallContext`] is the borrowed view
//! `combineRecallMultipliers` reads; a search carries a context across awaits
//! and stages, so it owns one of these and lends the view per row
//! ([`RecallContextInput::view`]). Unlike the pre-Phase-E form (which lived in
//! `memory_service.rs` and carried neither), this one holds v4's `tuning` and
//! `occurredWithin` too: v4's stages copy the context with `{ ...ctx, tuning }`
//! (R1 off for text) and `{ ...ctx, occurredWithin }` (the soft window), and
//! the owned form is what those copies are made of.

use std::collections::HashSet;

use serde_json::Value;

use crate::recall_tags::{
    ContextTag, MemoryTagView, RecallContext, ScopePolicy, TemporalTag, TimeWindow,
};
use crate::recall_tuning::{ResolvedRecallTuning, DEFAULT_RECALL_TUNING};
use crate::services::memory_recap::distill::DistilledSearch;

/// v4 `RecallContext`, owned.
#[derive(Debug, Clone, Default)]
pub struct RecallContextInput {
    pub current_project_id: Option<String>,
    pub scope_policy: ScopePolicy,
    pub present_about_character_ids: Vec<String>,
    pub turn_context: Option<ContextTag>,
    /// Carried for debug parity with v4 (the retrospective flag, not this
    /// guess, flips the temporal multipliers).
    pub turn_temporal: Option<TemporalTag>,
    pub turn_retrospective: bool,
    /// The soft window — set only by the score stage on its window-starved arm
    /// (v4 `{ ...context, occurredWithin: opts.window }`).
    pub occurred_within: Option<TimeWindow>,
    /// Item 5: one-hop related-memory expansion after the top hits are ranked.
    pub expand_related: bool,
    pub recently_whispered_ids: Option<HashSet<String>>,
    /// The fresh-event boost's echo guard (v4 `currentChatId`).
    pub current_chat_id: Option<String>,
    /// The fresh-event boost's reference clock, epoch ms (v4 `nowMs`).
    pub now_ms: Option<f64>,
    /// v4 `tuning` — `None` → [`DEFAULT_RECALL_TUNING`] (v4 `recallTuningOf`).
    pub tuning: Option<ResolvedRecallTuning>,
    /// The names of the characters present this turn (R4 skips them as anchors).
    pub present_participant_names: Vec<String>,
}

impl RecallContextInput {
    /// The borrowed view `combine_recall_multipliers` reads.
    pub fn view(&self) -> RecallContext<'_> {
        RecallContext {
            current_project_id: self.current_project_id.as_deref(),
            scope_policy: self.scope_policy,
            present_about_character_ids: &self.present_about_character_ids,
            turn_context: self.turn_context,
            turn_temporal: self.turn_temporal,
            turn_retrospective: self.turn_retrospective,
            occurred_within: self.occurred_within.as_ref(),
            expand_related: self.expand_related,
            recently_whispered_ids: self.recently_whispered_ids.as_ref(),
            current_chat_id: self.current_chat_id.as_deref(),
            now_ms: self.now_ms,
            tuning: self.tuning.as_ref(),
            present_participant_names: &self.present_participant_names,
        }
    }

    /// v4 `recallTuningOf(ctx)` — the context's own tuning, else the shipped
    /// defaults.
    pub fn tuning_of(&self) -> &ResolvedRecallTuning {
        self.tuning.as_ref().unwrap_or(&DEFAULT_RECALL_TUNING)
    }
}

/// The recall settings a turn context reads (v4
/// `Pick<MemoryRecallSettings, 'scopePolicy' | 'expandRelated'>`).
#[derive(Debug, Clone, Copy, Default)]
pub struct TurnRecallSettings {
    pub scope_policy: ScopePolicy,
    pub expand_related: bool,
}

/// v4 `TurnRecallContextInput`. `turn_retrospective` / `tuning` are optional
/// there (assigned only when defined); here `turn_retrospective` is a plain
/// bool (v4's absent reads as `false` everywhere it is consumed) and `tuning`
/// stays optional.
pub struct TurnRecallContextInput<'a> {
    pub chat_id: &'a str,
    pub chat_project_id: Option<&'a str>,
    /// The chat's raw `commonplaceRecallHistory` cell (the ring).
    pub commonplace_recall_history: &'a Value,
    pub recall_settings: TurnRecallSettings,
    pub turn_context: Option<ContextTag>,
    pub turn_temporal: Option<TemporalTag>,
    pub turn_retrospective: bool,
    pub present_about_character_ids: Vec<String>,
    pub present_participant_names: Vec<String>,
    pub now_ms: f64,
    pub tuning: Option<ResolvedRecallTuning>,
}

/// v4 `buildTurnRecallContext` — the one place the per-turn recall context is
/// assembled, so the proactive pass and the turn's own search read the
/// targeting tags back identically.
pub fn build_turn_recall_context(input: TurnRecallContextInput<'_>) -> RecallContextInput {
    RecallContextInput {
        current_project_id: input.chat_project_id.map(str::to_string),
        scope_policy: input.recall_settings.scope_policy,
        present_about_character_ids: input.present_about_character_ids,
        turn_context: input.turn_context,
        turn_temporal: input.turn_temporal,
        turn_retrospective: input.turn_retrospective,
        occurred_within: None,
        expand_related: input.recall_settings.expand_related,
        recently_whispered_ids: Some(crate::recall_history::recently_whispered_id_set(
            input.commonplace_recall_history,
        )),
        current_chat_id: Some(input.chat_id.to_string()),
        now_ms: Some(input.now_ms),
        tuning: input.tuning,
        present_participant_names: input.present_participant_names,
    }
}

/// v4 `buildRetrospectiveProbes(signals, retrospective)`: on a retrospective
/// turn, the entity string (joined, trimmed) and — when there is both a
/// paraphrase and a window — the paraphrase pinned to the window's dates.
/// `None` (v4 `undefined`) when neither applies.
pub fn build_retrospective_probes(
    signals: Option<&DistilledSearch>,
    retrospective: bool,
) -> Option<Vec<String>> {
    let signals = signals.filter(|_| retrospective)?;
    let mut probes = Vec::new();
    let entity_probe = crate::jsstr::js_trim(&signals.entities.join(" ")).to_string();
    if !entity_probe.is_empty() {
        probes.push(entity_probe);
    }
    if let (Some(paraphrase), Some(range)) = (&signals.paraphrase, &signals.time_range) {
        // `paraphrase` is never `Some("")` (the parse null-empties it), so
        // v4's truthiness test is `is_some`.
        probes.push(format!(
            "{paraphrase} (around {} to {})",
            crate::jsstr::utf16_truncate(&range.from, 10),
            crate::jsstr::utf16_truncate(&range.to, 10)
        ));
    }
    if probes.is_empty() {
        None
    } else {
        Some(probes)
    }
}

/// A memory row's `keywords` (strings only).
pub fn row_keywords(memory: &Value) -> Vec<String> {
    memory
        .get("keywords")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The [`MemoryTagView`] of a net-JSON memory row (`keywords` borrowed from the
/// caller's [`row_keywords`]).
pub fn tag_view<'a>(memory: &'a Value, keywords: &'a [String]) -> MemoryTagView<'a> {
    let s = |k: &str| memory.get(k).and_then(Value::as_str);
    MemoryTagView {
        id: s("id"),
        project_id: s("projectId"),
        keywords,
        about_character_id: s("aboutCharacterId"),
        occurred_at: s("occurredAt"),
        created_at: s("createdAt"),
        chat_id: s("chatId"),
    }
}
