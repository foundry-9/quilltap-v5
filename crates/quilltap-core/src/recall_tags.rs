//! Port of lib/memory/recall-tags.ts — recall-side targeting-tag multipliers.
//!
//! The memory extractor materializes three controlled targeting tags into every
//! memory's `keywords` array (temporal | scope | context). This module reads
//! them back at recall time and turns them into bounded, clamped multipliers on
//! the already-computed blended recall score. It is the single source of truth
//! for the closed vocabularies.
//!
//! Pure + I/O-free — no logging, no DB, no LLM (carried from the TS header) —
//! so it is a clean tier-1 differential target: deterministic, exactly checkable
//! against the oracle. Every constant, branch order, and float multiplication
//! order mirrors the source so results are byte-equal.
//!
//! The episodic recall overhaul (v4 `8bf3cb5f`, P4.d13) made the loop
//! turn-aware: `turn_retrospective` flips the temporal multipliers and suspends
//! anti-repetition, and `occurred_within` adds the soft time-window boost. The
//! formerly-deferred `RELATED_EXPANSION` caps and `expand_related` /
//! `turn_temporal` context fields land here too — `search_memories_semantic`'s
//! recallContext path (their consumer) is ported in the same round.

use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Closed vocabularies (single source of truth) as Rust enums.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TemporalTag {
    Past,
    Moment,
    Present,
    Future,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScopeTag {
    Narrow,
    Wide,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContextTag {
    Philosophy,
    Relationships,
    History,
    Banter,
    Mannerisms,
    Trivia,
    Information,
}

impl TemporalTag {
    /// Parse a normalized (trimmed + lowercased) bare word, or None if not in
    /// the closed vocabulary.
    pub fn from_kw(kw: &str) -> Option<Self> {
        match kw {
            "past" => Some(Self::Past),
            "moment" => Some(Self::Moment),
            "present" => Some(Self::Present),
            "future" => Some(Self::Future),
            _ => None,
        }
    }
    /// Canonical lowercase form (matches the TS string values).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Past => "past",
            Self::Moment => "moment",
            Self::Present => "present",
            Self::Future => "future",
        }
    }
}

impl ScopeTag {
    pub fn from_kw(kw: &str) -> Option<Self> {
        match kw {
            "narrow" => Some(Self::Narrow),
            "wide" => Some(Self::Wide),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Narrow => "narrow",
            Self::Wide => "wide",
        }
    }
}

impl ContextTag {
    pub fn from_kw(kw: &str) -> Option<Self> {
        match kw {
            "philosophy" => Some(Self::Philosophy),
            "relationships" => Some(Self::Relationships),
            "history" => Some(Self::History),
            "banter" => Some(Self::Banter),
            "mannerisms" => Some(Self::Mannerisms),
            "trivia" => Some(Self::Trivia),
            "information" => Some(Self::Information),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Philosophy => "philosophy",
            Self::Relationships => "relationships",
            Self::History => "history",
            Self::Banter => "banter",
            Self::Mannerisms => "mannerisms",
            Self::Trivia => "trivia",
            Self::Information => "information",
        }
    }
}

/// Defaults MUST match the extraction-side defaults in `applyTargetingTags`. A
/// legacy/untagged memory therefore reads as present / wide / information and is
/// never penalized for missing data.
pub const DEFAULT_TEMPORAL: TemporalTag = TemporalTag::Present;
pub const DEFAULT_SCOPE: ScopeTag = ScopeTag::Wide;
pub const DEFAULT_CONTEXT: ContextTag = ContextTag::Information;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TargetingTags {
    pub temporal: TemporalTag,
    pub scope: ScopeTag,
    pub context: ContextTag,
}

/// Policy for what to do with a cross-project `scope: narrow` memory at recall.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ScopePolicy {
    #[default]
    DownWeight,
    Exclude,
}

// ---------------------------------------------------------------------------
// Tunable multiplier constants (mirror RECALL_MULTIPLIERS).
// ---------------------------------------------------------------------------

/// `scope: narrow` memory whose project matches the current chat.
pub const SCOPE_NARROW_SAME_PROJECT: f64 = 1.15;
/// Cross-project `scope: narrow` under the `down-weight` policy.
pub const SCOPE_NARROW_CROSS_PROJECT_DOWN_WEIGHT: f64 = 0.15;
/// `temporal: past` — history still matters, but rarely should outrank a live fact.
pub const TEMPORAL_PAST: f64 = 0.85;
/// `temporal: moment` — true only at one instant.
pub const TEMPORAL_MOMENT: f64 = 0.7;
/// Item 3 — the memory's `context` tag matches the turn's guessed dominant context.
pub const CONTEXT_MATCH: f64 = 1.1;
/// Item 4 — the memory is *about* a character present in the room this turn.
pub const PARTICIPANT_PRESENT: f64 = 1.2;
/// Anti-repetition — the memory was whispered in one of the last few turns.
/// SUSPENDED on retrospective turns — a fumbled recall the user re-asks about
/// must not bury the very memory they are trying to pin down.
pub const RECENTLY_WHISPERED: f64 = 0.6;
/// Retrospective turn — `temporal: past` flips from a penalty to a boost:
/// the exact class of memory a "remember last week?" turn needs.
pub const TEMPORAL_PAST_RETROSPECTIVE: f64 = 1.15;
/// Retrospective turn — `moment` memories stop being penalized.
pub const TEMPORAL_MOMENT_RETROSPECTIVE: f64 = 1.0;
/// Event time falls inside the turn's resolved time window (soft fallback
/// when the window-filtered pool was too small — see `searchMemoriesSemantic`).
pub const OCCURRED_WITHIN_WINDOW: f64 = 1.3;
/// Fresh-event boost — the memory's event time (occurredAt ?? createdAt) is
/// within the last 24h / 48h. The blend's recency term (0.25 weight, 30-day
/// half-life) distinguishes yesterday from twelve days ago by ~0.05 — far less
/// than one targeting-tag multiplier — so without this, "what just happened"
/// holds no ground against evergreen present-tagged memories. Unconditional
/// (not gated on the retrospective flag) by design: it is the safety net for
/// every turn the retrospective classifier misses.
pub const FRESH_EVENT_24H: f64 = 1.6;
pub const FRESH_EVENT_48H: f64 = 1.35;

/// Milliseconds in the two fresh-event bands.
const HOUR_MS: f64 = 60.0 * 60.0 * 1000.0;
const FRESH_24H_MS: f64 = 24.0 * HOUR_MS;
const FRESH_48H_MS: f64 = 48.0 * HOUR_MS;

/// Clamp on the *combined* multiplier so no single memory can explode the
/// ranking.
pub const MULTIPLIER_CLAMP_MIN: f64 = 0.0;
pub const MULTIPLIER_CLAMP_MAX: f64 = 4.0;

/// Item 5 — caps on one-hop related-memory expansion so a corpus-heavy
/// character can't balloon the candidate set. `MAX_PER_HIT` bounds neighbors
/// pulled from any single top hit; `MAX_TOTAL` bounds the whole expansion
/// across all hits. (v4 `RELATED_EXPANSION`.)
pub const RELATED_EXPANSION_MAX_PER_HIT: usize = 3;
pub const RELATED_EXPANSION_MAX_TOTAL: usize = 10;

/// An absolute `{from, to}` ISO time window (v4's structural
/// `{ from: string; to: string }` — the distill's `timeRange` and the recall
/// context's `occurredWithin` share this shape).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeWindow {
    pub from: String,
    pub to: String,
}

/// Result of a single adjustment: its multiplier plus short debug labels.
#[derive(Clone, Debug, PartialEq)]
pub struct RecallMultiplier {
    pub multiplier: f64,
    /// Short labels (e.g. `narrow✓`, `past↓`) for the per-turn debug log/whisper.
    pub fired: Vec<&'static str>,
    /// True only for the cross-project narrow + `exclude` policy case.
    pub exclude: bool,
}

impl RecallMultiplier {
    fn pass() -> Self {
        RecallMultiplier {
            multiplier: 1.0,
            fired: vec![],
            exclude: false,
        }
    }
}

/// Combined recall adjustment for one memory, clamped and ready to apply.
#[derive(Clone, Debug, PartialEq)]
pub struct CombinedRecallAdjustment {
    pub multiplier: f64,
    pub fired: Vec<&'static str>,
    pub exclude: bool,
}

/// Minimal structural view of a memory this module needs (keeps it
/// Memory-import-free, mirroring the TS `MemoryTagView`).
#[derive(Clone, Default)]
pub struct MemoryTagView<'a> {
    pub id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub keywords: &'a [String],
    pub about_character_id: Option<&'a str>,
    /// ISO event time (episodic spine); write clock stands in when absent.
    /// `Some("")` is NOT nullish in v4 (`??`), so it does NOT fall through to
    /// `created_at` — it just parses NaN and passes through.
    pub occurred_at: Option<&'a str>,
    pub created_at: Option<&'a str>,
    /// The chat the memory was extracted from — the fresh-event echo guard
    /// reads it (v4 `MemoryTagView.chatId`).
    pub chat_id: Option<&'a str>,
}

/// Per-turn recall context (the subset `combineRecallMultipliers` reads).
#[derive(Clone, Default)]
pub struct RecallContext<'a> {
    /// The current chat's project, or None when project-less.
    pub current_project_id: Option<&'a str>,
    /// What to do with a cross-project `scope: narrow` memory.
    pub scope_policy: ScopePolicy,
    /// IDs of characters present in the room this turn (incl. the responder).
    pub present_about_character_ids: &'a [String],
    /// The turn's dominant `context` axis, or None.
    pub turn_context: Option<ContextTag>,
    /// The turn's dominant `temporal` axis (same cheap-LLM guess). Carried for
    /// debug parity with v4; the retrospective flag below (not this guess) is
    /// what flips the temporal multipliers.
    pub turn_temporal: Option<TemporalTag>,
    /// True when the per-turn extraction judged this turn RETROSPECTIVE — the
    /// user (or a character) is referencing past shared events. Flips the
    /// temporal multipliers (past 0.85 → 1.15, moment 0.70 → 1.0) and suspends
    /// the anti-repetition penalty (the user is deliberately re-asking).
    pub turn_retrospective: bool,
    /// Resolved absolute time window the turn references. Memories whose event
    /// time (occurredAt ?? createdAt) falls inside get the bounded
    /// [`OCCURRED_WITHIN_WINDOW`] boost. The hard-filter stage lives in
    /// `searchMemoriesSemantic`; this multiplier is the soft fallback when the
    /// filtered pool was too small.
    pub occurred_within: Option<&'a TimeWindow>,
    /// When true, one-hop related-memory expansion runs inside
    /// `searchMemoriesSemantic` after the top hits are ranked (item 5).
    pub expand_related: bool,
    /// Memory IDs whispered in the last few turns of this chat.
    pub recently_whispered_ids: Option<&'a HashSet<String>>,
    /// The current chat's id — the fresh-event boost skips memories extracted
    /// from this same chat (echo guard).
    pub current_chat_id: Option<&'a str>,
    /// Reference clock for the fresh-event boost, ms since epoch. Absent → the
    /// boost is disabled.
    pub now_ms: Option<f64>,
}

/// Parse the three targeting tags back out of a memory's keywords array.
///
/// Mirrors the extraction-side materialization: `temporal`/`context` are bare
/// words, `scope` is `scope: <value>`. The extractor appends the real tags at
/// the END of the keywords array, so we iterate with last-match-wins — a free
/// keyword that happens to collide with a vocabulary word is overridden by the
/// appended tag. Unknown/missing values fall back to the same defaults the
/// extractor uses.
pub fn parse_targeting_tags(keywords: &[String]) -> TargetingTags {
    let mut temporal = DEFAULT_TEMPORAL;
    let mut scope = DEFAULT_SCOPE;
    let mut context = DEFAULT_CONTEXT;

    for raw in keywords {
        let kw = raw.trim().to_lowercase();
        if let Some(rest) = kw.strip_prefix("scope:") {
            // `scope: <value>` — value is itself trimmed (matches `.slice().trim()`).
            if let Some(s) = ScopeTag::from_kw(rest.trim()) {
                scope = s;
            }
        } else if let Some(t) = TemporalTag::from_kw(&kw) {
            temporal = t;
        } else if let Some(c) = ContextTag::from_kw(&kw) {
            context = c;
        }
    }

    TargetingTags {
        temporal,
        scope,
        context,
    }
}

/// Item 1 — scope + project gating.
///
/// - `scope: wide` → pass through (regardless of project).
/// - `scope: narrow`, memory has no projectId → pass through (never penalize on
///   missing data).
/// - `scope: narrow`, memory's project === current chat's project → boost.
/// - `scope: narrow`, memory's project differs from (or exists where the chat
///   has none) → cross-project: exclude or strong down-weight per policy.
pub fn scope_project_multiplier(
    tags: TargetingTags,
    memory_project_id: Option<&str>,
    current_project_id: Option<&str>,
    policy: ScopePolicy,
) -> RecallMultiplier {
    // `!memoryProjectId` in TS is also true for an empty string; mirror that by
    // treating Some("") as "no project".
    let mem_proj = memory_project_id.filter(|s| !s.is_empty());
    if tags.scope != ScopeTag::Narrow || mem_proj.is_none() {
        return RecallMultiplier::pass();
    }
    let mem_proj = mem_proj.unwrap();
    let cur_proj = current_project_id.filter(|s| !s.is_empty());
    if let Some(cur) = cur_proj {
        if mem_proj == cur {
            return RecallMultiplier {
                multiplier: SCOPE_NARROW_SAME_PROJECT,
                fired: vec!["narrow✓"],
                exclude: false,
            };
        }
    }
    if policy == ScopePolicy::Exclude {
        return RecallMultiplier {
            multiplier: 0.0,
            fired: vec!["narrow✗-exclude"],
            exclude: true,
        };
    }
    RecallMultiplier {
        multiplier: SCOPE_NARROW_CROSS_PROJECT_DOWN_WEIGHT,
        fired: vec!["narrow✗"],
        exclude: false,
    }
}

/// Item 2 — temporal weighting, now turn-aware (`turnTemporal` made real).
///
/// Default turns: `past` facts rarely should outrank live ones; `moment` facts
/// are true only at a single instant. Recall always runs BEFORE the current
/// turn's extraction, so any recalled `moment` memory was produced on a prior
/// turn — the penalty applies unconditionally. `present`/`future` pass through.
///
/// Retrospective turns invert the frame: the user is deliberately invoking the
/// past, so `past` becomes a boost and `moment` stops being penalized —
/// without this, the exact class of memory a "remember last week?" turn needs
/// is systematically demoted at the moment it is asked for.
pub fn temporal_multiplier(tags: TargetingTags, retrospective: bool) -> RecallMultiplier {
    match tags.temporal {
        TemporalTag::Past => {
            if retrospective {
                RecallMultiplier {
                    multiplier: TEMPORAL_PAST_RETROSPECTIVE,
                    fired: vec!["past↑retro"],
                    exclude: false,
                }
            } else {
                RecallMultiplier {
                    multiplier: TEMPORAL_PAST,
                    fired: vec!["past↓"],
                    exclude: false,
                }
            }
        }
        TemporalTag::Moment => {
            if retrospective {
                RecallMultiplier {
                    multiplier: TEMPORAL_MOMENT_RETROSPECTIVE,
                    fired: vec!["moment·retro"],
                    exclude: false,
                }
            } else {
                RecallMultiplier {
                    multiplier: TEMPORAL_MOMENT,
                    fired: vec!["moment↓"],
                    exclude: false,
                }
            }
        }
        _ => RecallMultiplier::pass(),
    }
}

/// Time-window boost — the memory's event time (occurredAt ?? createdAt) falls
/// inside the turn's resolved retrospective window. Soft fallback companion to
/// the hard filter in `searchMemoriesSemantic`. No window, or no parsable
/// event time → pass through.
pub fn occurred_within_multiplier(
    memory: &MemoryTagView,
    window: Option<&TimeWindow>,
) -> RecallMultiplier {
    let Some(window) = window else {
        return RecallMultiplier::pass();
    };
    // v4 `memory.occurredAt ?? memory.createdAt` — nullish coalescing, so a
    // present-but-empty occurredAt is used (and parses NaN → pass through).
    let Some(event_iso) = memory.occurred_at.or(memory.created_at) else {
        return RecallMultiplier::pass();
    };
    // v4 gates `if (!eventIso)` — an empty string is falsy → pass through
    // (event_time_ms also returns None for empty, same outcome).
    let (Some(t), Some(from), Some(to)) = (
        crate::episodic::event_time_ms(Some(event_iso)),
        crate::episodic::event_time_ms(Some(&window.from)),
        crate::episodic::event_time_ms(Some(&window.to)),
    ) else {
        return RecallMultiplier::pass();
    };
    if t >= from && t <= to {
        return RecallMultiplier {
            multiplier: OCCURRED_WITHIN_WINDOW,
            fired: vec!["window↑"],
            exclude: false,
        };
    }
    RecallMultiplier::pass()
}

/// Fresh-event boost — the memory's event time is within the last 24h/48h.
///
/// Unconditional, unlike [`occurred_within_multiplier`]: it fires whether or not
/// the turn was judged retrospective, because it exists precisely for the turns
/// where that judgement fails. The ranking blend's recency term is too weak to
/// keep yesterday's events in front of well-tagged evergreen memories, so a
/// coarse freshness band does the work the blend cannot.
///
/// Echo guard: memories extracted from the CURRENT chat are skipped. They are
/// already in the transcript the model is reading, and boosting them floods the
/// handful of whisper slots with restatements of the last few turns. v4 gates it
/// on JS TRUTHINESS (`memory.chatId && currentChatId && …`), so an empty string
/// on either side disables the guard — mirrored by the `is_empty` filters.
///
/// No clock, no parsable event time, or an event time in the future → pass
/// through (never penalize on missing data — house rule).
pub fn fresh_event_multiplier(
    memory: &MemoryTagView,
    now_ms: Option<f64>,
    current_chat_id: Option<&str>,
) -> RecallMultiplier {
    // v4 `nowMs === null || nowMs === undefined || !Number.isFinite(nowMs)`.
    let Some(now) = now_ms.filter(|n| n.is_finite()) else {
        return RecallMultiplier::pass();
    };
    let mem_chat = memory.chat_id.filter(|s| !s.is_empty());
    let cur_chat = current_chat_id.filter(|s| !s.is_empty());
    if let (Some(m), Some(c)) = (mem_chat, cur_chat) {
        if m == c {
            return RecallMultiplier::pass();
        }
    }
    // v4 `memory.occurredAt ?? memory.createdAt` — nullish coalescing, so a
    // present-but-empty occurredAt is used (and then falsy → pass through);
    // `event_time_ms` returns None for both the empty and unparsable arms,
    // which is the same outcome v4's `!eventIso` / `!Number.isFinite(t)` reach.
    let Some(t) = memory
        .occurred_at
        .or(memory.created_at)
        .and_then(|iso| crate::episodic::event_time_ms(Some(iso)))
    else {
        return RecallMultiplier::pass();
    };

    let age = now - t;
    if age < 0.0 {
        return RecallMultiplier::pass();
    }
    if age <= FRESH_24H_MS {
        return RecallMultiplier {
            multiplier: FRESH_EVENT_24H,
            fired: vec!["fresh24↑"],
            exclude: false,
        };
    }
    if age <= FRESH_48H_MS {
        return RecallMultiplier {
            multiplier: FRESH_EVENT_48H,
            fired: vec!["fresh48↑"],
            exclude: false,
        };
    }
    RecallMultiplier::pass()
}

/// Item 3 — context-axis steering. Boost a memory whose own `context` tag
/// matches the turn's guessed dominant context. No turn guess → pass through.
pub fn context_multiplier(
    tags: TargetingTags,
    turn_context: Option<ContextTag>,
) -> RecallMultiplier {
    if let Some(turn) = turn_context {
        if tags.context == turn {
            return RecallMultiplier {
                multiplier: CONTEXT_MATCH,
                fired: vec!["ctx✓"],
                exclude: false,
            };
        }
    }
    RecallMultiplier::pass()
}

/// Item 4 — participant-aware boost. Boost a memory that is *about* a character
/// present in the room this turn. A boost, never a filter: absent characters
/// still get discussed.
pub fn participant_multiplier(
    memory: &MemoryTagView,
    present_about_character_ids: &[String],
) -> RecallMultiplier {
    if let Some(about) = memory.about_character_id {
        if present_about_character_ids.iter().any(|c| c == about) {
            return RecallMultiplier {
                multiplier: PARTICIPANT_PRESENT,
                fired: vec!["present↑"],
                exclude: false,
            };
        }
    }
    RecallMultiplier::pass()
}

/// Anti-repetition — penalize a memory whispered in the last few turns of this
/// chat. A bounded multiplier, never a hard exclude: a still-best match keeps
/// winning, just not trivially. No set, or memory not in it → pass through.
/// `suspended` (retrospective turn): the user is deliberately re-asking —
/// penalizing the just-whispered memory here would bury the very entry they
/// want.
pub fn recently_whispered_multiplier(
    memory: &MemoryTagView,
    recently_whispered_ids: Option<&HashSet<String>>,
    suspended: bool,
) -> RecallMultiplier {
    if suspended {
        return RecallMultiplier::pass();
    }
    if let (Some(id), Some(set)) = (memory.id, recently_whispered_ids) {
        if set.contains(id) {
            return RecallMultiplier {
                multiplier: RECENTLY_WHISPERED,
                fired: vec!["repeat↓"],
                exclude: false,
            };
        }
    }
    RecallMultiplier::pass()
}

/// Combine every applicable recall multiplier for one memory into a single
/// clamped adjustment. Items 1 (scope+project) and 2 (temporal) read the
/// memory's own tags; items 3 (context) and 4 (participant) compare against the
/// turn-level signals, and anti-repetition reads the recently-whispered set. The
/// time-window boost and the unconditional fresh-event boost read the memory's
/// event time against the turn's window and clock. The product is clamped to
/// [MIN, MAX]. A cross-project narrow memory under the `exclude` policy
/// short-circuits to `{ exclude: true }`.
///
/// The float multiplication order (scope · temporal · context · participant ·
/// recent · window · fresh, left-associative — fresh LAST) is preserved exactly
/// so the f64 result is bit-equal to the TS oracle.
pub fn combine_recall_multipliers(
    memory: &MemoryTagView,
    ctx: &RecallContext,
) -> CombinedRecallAdjustment {
    let tags = parse_targeting_tags(memory.keywords);

    let scope = scope_project_multiplier(
        tags,
        memory.project_id,
        ctx.current_project_id,
        ctx.scope_policy,
    );
    if scope.exclude {
        return CombinedRecallAdjustment {
            multiplier: 0.0,
            fired: scope.fired,
            exclude: true,
        };
    }

    let retrospective = ctx.turn_retrospective;
    let temporal = temporal_multiplier(tags, retrospective);
    let context = context_multiplier(tags, ctx.turn_context);
    let participant = participant_multiplier(memory, ctx.present_about_character_ids);
    let recent = recently_whispered_multiplier(memory, ctx.recently_whispered_ids, retrospective);
    let window = occurred_within_multiplier(memory, ctx.occurred_within);
    let fresh = fresh_event_multiplier(memory, ctx.now_ms, ctx.current_chat_id);

    let product = scope.multiplier
        * temporal.multiplier
        * context.multiplier
        * participant.multiplier
        * recent.multiplier
        * window.multiplier
        * fresh.multiplier;
    // Mirrors TS `Math.max(MIN, Math.min(MAX, product))`; `.clamp` is identical
    // for all finite inputs (the only inputs a product of finite multipliers can
    // produce — no NaN path here).
    let clamped = product.clamp(MULTIPLIER_CLAMP_MIN, MULTIPLIER_CLAMP_MAX);

    let mut fired = Vec::new();
    fired.extend(scope.fired);
    fired.extend(temporal.fired);
    fired.extend(context.fired);
    fired.extend(participant.fired);
    fired.extend(recent.fired);
    fired.extend(window.fired);
    fired.extend(fresh.fired);

    CombinedRecallAdjustment {
        multiplier: clamped,
        fired,
        exclude: false,
    }
}

#[cfg(test)]
mod retrospective_tests {
    //! Case-for-case port of v4 `lib/memory/__tests__/recall-tags-retrospective.test.ts`
    //! — retrospective turn handling in the multiplier loop, plus the §3
    //! inert-path regression guard.

    use super::*;

    fn past_memory_keywords() -> Vec<String> {
        ["harbor", "past", "scope: wide", "history"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn past_memory<'a>(keywords: &'a [String]) -> MemoryTagView<'a> {
        MemoryTagView {
            id: Some("mem-1"),
            project_id: None,
            keywords,
            about_character_id: None,
            occurred_at: Some("2026-07-14T00:00:00.000Z"),
            created_at: Some("2026-07-14T01:00:00.000Z"),
            chat_id: None,
        }
    }

    #[test]
    fn penalizes_past_memories_on_ordinary_turns() {
        let kw = past_memory_keywords();
        let tags = parse_targeting_tags(&kw);
        let result = temporal_multiplier(tags, false);
        assert_eq!(result.multiplier, TEMPORAL_PAST);
        assert_eq!(result.fired, vec!["past↓"]);
    }

    #[test]
    fn boosts_past_memories_on_retrospective_turns() {
        let kw = past_memory_keywords();
        let tags = parse_targeting_tags(&kw);
        let result = temporal_multiplier(tags, true);
        assert_eq!(result.multiplier, TEMPORAL_PAST_RETROSPECTIVE);
        assert_eq!(result.fired, vec!["past↑retro"]);
    }

    #[test]
    fn stops_penalizing_moment_memories_on_retrospective_turns() {
        let kw: Vec<String> = ["moment", "scope: wide", "information"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let moment_tags = parse_targeting_tags(&kw);
        assert_eq!(
            temporal_multiplier(moment_tags, false).multiplier,
            TEMPORAL_MOMENT
        );
        assert_eq!(
            temporal_multiplier(moment_tags, true).multiplier,
            TEMPORAL_MOMENT_RETROSPECTIVE
        );
    }

    #[test]
    fn penalizes_recently_whispered_on_ordinary_turns() {
        let kw = past_memory_keywords();
        let mem = past_memory(&kw);
        let whispered: HashSet<String> = ["mem-1".to_string()].into_iter().collect();
        let result = recently_whispered_multiplier(&mem, Some(&whispered), false);
        assert_eq!(result.multiplier, RECENTLY_WHISPERED);
    }

    #[test]
    fn suspends_the_penalty_on_retrospective_turns() {
        let kw = past_memory_keywords();
        let mem = past_memory(&kw);
        let whispered: HashSet<String> = ["mem-1".to_string()].into_iter().collect();
        let result = recently_whispered_multiplier(&mem, Some(&whispered), true);
        assert_eq!(result.multiplier, 1.0);
        assert!(result.fired.is_empty());
    }

    fn window() -> TimeWindow {
        TimeWindow {
            from: "2026-07-13T00:00:00.000Z".to_string(),
            to: "2026-07-19T23:59:59.999Z".to_string(),
        }
    }

    #[test]
    fn boosts_a_memory_inside_the_window() {
        let kw = past_memory_keywords();
        let mem = past_memory(&kw);
        let w = window();
        let result = occurred_within_multiplier(&mem, Some(&w));
        assert_eq!(result.multiplier, OCCURRED_WITHIN_WINDOW);
        assert_eq!(result.fired, vec!["window↑"]);
    }

    #[test]
    fn falls_back_to_created_at_when_occurred_at_absent() {
        let kw = past_memory_keywords();
        let mut mem = past_memory(&kw);
        mem.occurred_at = None;
        let w = window();
        assert_eq!(
            occurred_within_multiplier(&mem, Some(&w)).multiplier,
            OCCURRED_WITHIN_WINDOW
        );
    }

    #[test]
    fn passes_through_outside_the_window_or_with_no_window() {
        let kw = past_memory_keywords();
        let mut outside = past_memory(&kw);
        outside.occurred_at = Some("2026-01-01T00:00:00.000Z");
        outside.created_at = Some("2026-01-01T00:00:00.000Z");
        let w = window();
        assert_eq!(
            occurred_within_multiplier(&outside, Some(&w)).multiplier,
            1.0
        );
        let mem = past_memory(&kw);
        assert_eq!(occurred_within_multiplier(&mem, None).multiplier, 1.0);
    }

    #[test]
    fn combine_applies_flip_suspension_and_window_in_one_clamped_loop() {
        let kw = past_memory_keywords();
        let mem = past_memory(&kw);
        let whispered: HashSet<String> = ["mem-1".to_string()].into_iter().collect();
        let w = window();
        let ctx = RecallContext {
            current_project_id: None,
            scope_policy: ScopePolicy::DownWeight,
            turn_retrospective: true,
            recently_whispered_ids: Some(&whispered),
            occurred_within: Some(&w),
            ..Default::default()
        };
        let result = combine_recall_multipliers(&mem, &ctx);
        // past↑retro (1.15) × window↑ (1.3); repeat↓ suspended.
        let expected = TEMPORAL_PAST_RETROSPECTIVE * OCCURRED_WITHIN_WINDOW;
        assert!((result.multiplier - expected).abs() < 1e-10);
        assert!(result.fired.contains(&"past↑retro"));
        assert!(result.fired.contains(&"window↑"));
        assert!(!result.fired.contains(&"repeat↓"));
    }

    // ---- fresh-event boost (v4 `505dcb1f`) ----

    /// 2026-07-29T02:44:00.000Z — the diagnosed chat's instant.
    const FRESH_NOW: f64 = 1_785_293_040_000.0;
    const HOUR: f64 = 3_600_000.0;
    const CHAT: &str = "chat-current";

    fn ago(hours: f64) -> String {
        crate::clock::iso_from_unix_ms((FRESH_NOW - hours * HOUR) as i64)
    }

    fn fresh_of(
        occurred: Option<&str>,
        created: Option<&str>,
        chat: Option<&str>,
    ) -> MemoryTagView<'static> {
        // Leaked strings keep the borrow simple in these table-driven tests.
        MemoryTagView {
            occurred_at: occurred.map(|s| &*Box::leak(s.to_string().into_boxed_str())),
            created_at: created.map(|s| &*Box::leak(s.to_string().into_boxed_str())),
            chat_id: chat.map(|s| &*Box::leak(s.to_string().into_boxed_str())),
            ..Default::default()
        }
    }

    #[test]
    fn fresh_bands_are_inclusive_at_both_edges() {
        let m = fresh_of(Some(&ago(6.0)), None, None);
        let r = fresh_event_multiplier(&m, Some(FRESH_NOW), Some(CHAT));
        assert_eq!(r.multiplier, FRESH_EVENT_24H);
        assert_eq!(r.fired, vec!["fresh24↑"]);
        let m = fresh_of(Some(&ago(24.0)), None, None);
        assert_eq!(
            fresh_event_multiplier(&m, Some(FRESH_NOW), Some(CHAT)).fired,
            vec!["fresh24↑"]
        );
        let m = fresh_of(Some(&ago(30.0)), None, None);
        let r = fresh_event_multiplier(&m, Some(FRESH_NOW), Some(CHAT));
        assert_eq!(r.multiplier, FRESH_EVENT_48H);
        assert_eq!(r.fired, vec!["fresh48↑"]);
        let m = fresh_of(Some(&ago(48.0)), None, None);
        assert_eq!(
            fresh_event_multiplier(&m, Some(FRESH_NOW), Some(CHAT)).fired,
            vec!["fresh48↑"]
        );
        let m = fresh_of(Some(&ago(49.0)), None, None);
        assert_eq!(
            fresh_event_multiplier(&m, Some(FRESH_NOW), Some(CHAT)).multiplier,
            1.0
        );
    }

    #[test]
    fn fresh_passes_through_on_missing_data() {
        // No clock (both v4 nullish arms and NaN).
        let m = fresh_of(Some(&ago(1.0)), None, None);
        assert_eq!(fresh_event_multiplier(&m, None, Some(CHAT)).multiplier, 1.0);
        assert_eq!(
            fresh_event_multiplier(&m, Some(f64::NAN), Some(CHAT)).multiplier,
            1.0
        );
        // createdAt stands in for a missing/null occurredAt.
        let m = fresh_of(None, Some(&ago(2.0)), None);
        assert_eq!(
            fresh_event_multiplier(&m, Some(FRESH_NOW), Some(CHAT)).fired,
            vec!["fresh24↑"]
        );
        // No event time at all, and an unparsable one.
        let m = fresh_of(None, None, None);
        assert_eq!(
            fresh_event_multiplier(&m, Some(FRESH_NOW), Some(CHAT)).multiplier,
            1.0
        );
        let m = fresh_of(Some("not a date"), None, None);
        assert_eq!(
            fresh_event_multiplier(&m, Some(FRESH_NOW), Some(CHAT)).multiplier,
            1.0
        );
        // A future event time is never penalized (house rule).
        let m = fresh_of(Some(&ago(-1.0)), None, None);
        assert_eq!(
            fresh_event_multiplier(&m, Some(FRESH_NOW), Some(CHAT)).multiplier,
            1.0
        );
    }

    #[test]
    fn fresh_echo_guard_reads_truthiness() {
        let same = fresh_of(Some(&ago(1.0)), None, Some(CHAT));
        assert_eq!(
            fresh_event_multiplier(&same, Some(FRESH_NOW), Some(CHAT)).multiplier,
            1.0
        );
        let other = fresh_of(Some(&ago(1.0)), None, Some("chat-other"));
        assert_eq!(
            fresh_event_multiplier(&other, Some(FRESH_NOW), Some(CHAT)).fired,
            vec!["fresh24↑"]
        );
        let unowned = fresh_of(Some(&ago(1.0)), None, None);
        assert_eq!(
            fresh_event_multiplier(&unowned, Some(FRESH_NOW), Some(CHAT)).fired,
            vec!["fresh24↑"]
        );
        // An empty string on EITHER side is falsy in v4, so the guard is off.
        let empty_mem = fresh_of(Some(&ago(1.0)), None, Some(""));
        assert_eq!(
            fresh_event_multiplier(&empty_mem, Some(FRESH_NOW), Some("")).fired,
            vec!["fresh24↑"]
        );
        assert_eq!(
            fresh_event_multiplier(&same, Some(FRESH_NOW), Some("")).fired,
            vec!["fresh24↑"]
        );
        assert_eq!(
            fresh_event_multiplier(&same, Some(FRESH_NOW), None).fired,
            vec!["fresh24↑"]
        );
    }

    #[test]
    fn combine_multiplies_fresh_last_and_labels_it_last() {
        let kw: Vec<String> = ["moment", "scope: narrow", "history"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let occurred = ago(6.0);
        let mem = MemoryTagView {
            id: Some("M1"),
            project_id: Some("P1"),
            keywords: &kw,
            occurred_at: Some(&occurred),
            chat_id: Some("chat-other"),
            ..Default::default()
        };
        let ctx = RecallContext {
            current_project_id: Some("P1"),
            current_chat_id: Some(CHAT),
            now_ms: Some(FRESH_NOW),
            ..Default::default()
        };
        let r = combine_recall_multipliers(&mem, &ctx);
        let expected = SCOPE_NARROW_SAME_PROJECT * TEMPORAL_MOMENT * FRESH_EVENT_24H;
        assert!((r.multiplier - expected).abs() < 1e-10);
        assert_eq!(r.fired, vec!["narrow✓", "moment↓", "fresh24↑"]);

        // No clock on the context → the boost is inert (the pre-drift path).
        let inert = RecallContext {
            current_project_id: Some("P1"),
            ..Default::default()
        };
        let r = combine_recall_multipliers(&mem, &inert);
        assert!(!r.fired.contains(&"fresh24↑"));
    }

    #[test]
    fn inert_path_regression_guard() {
        let kw = past_memory_keywords();
        let mem = past_memory(&kw);
        let whispered: HashSet<String> = ["mem-1".to_string()].into_iter().collect();
        let ctx = RecallContext {
            current_project_id: None,
            scope_policy: ScopePolicy::DownWeight,
            recently_whispered_ids: Some(&whispered),
            ..Default::default()
        };
        let result = combine_recall_multipliers(&mem, &ctx);
        // Historical: past↓ (0.85) × repeat↓ (0.6); no window term.
        let expected = TEMPORAL_PAST * RECENTLY_WHISPERED;
        assert!((result.multiplier - expected).abs() < 1e-10);
        assert_eq!(result.fired, vec!["past↓", "repeat↓"]);
    }
}
