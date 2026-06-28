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
//! NOTE on scope vs the Phase-2/3 expansion path: the TS module also declares
//! `RELATED_EXPANSION`, `expandRelated`, and `turnTemporal`, but NO function in
//! it consumes them — one-hop related expansion runs inside
//! `searchMemoriesSemantic`, not here. They are deliberately omitted from this
//! port and will land with item-5 expansion (and its own oracle test) in a
//! later phase, rather than as untested dead constants here.

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
    fn from_kw(kw: &str) -> Option<Self> {
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
    fn from_kw(kw: &str) -> Option<Self> {
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
    fn from_kw(kw: &str) -> Option<Self> {
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
pub const RECENTLY_WHISPERED: f64 = 0.6;

/// Clamp on the *combined* multiplier so no single memory can explode the
/// ranking. (With the current constants the product never actually reaches the
/// ceiling — max stacked boosts = 1.15*1.1*1.2 = 1.518 — so `max` is a forward
/// safety net, not a live branch; `min` only ever binds via the exclude path,
/// which short-circuits before the clamp. Ported faithfully regardless.)
pub const MULTIPLIER_CLAMP_MIN: f64 = 0.0;
pub const MULTIPLIER_CLAMP_MAX: f64 = 4.0;

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
    /// Memory IDs whispered in the last few turns of this chat.
    pub recently_whispered_ids: Option<&'a HashSet<String>>,
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

/// Item 2 — temporal down-weighting.
///
/// `past` facts rarely should outrank live ones; `moment` facts are true only
/// at a single instant. Recall always runs BEFORE the current turn's
/// extraction, so any recalled `moment` memory was produced on a prior turn —
/// the "only when not the producing turn" condition is therefore always
/// satisfied on this path, and the penalty applies unconditionally.
/// `present`/`future` pass through.
pub fn temporal_multiplier(tags: TargetingTags) -> RecallMultiplier {
    match tags.temporal {
        TemporalTag::Past => RecallMultiplier {
            multiplier: TEMPORAL_PAST,
            fired: vec!["past↓"],
            exclude: false,
        },
        TemporalTag::Moment => RecallMultiplier {
            multiplier: TEMPORAL_MOMENT,
            fired: vec!["moment↓"],
            exclude: false,
        },
        _ => RecallMultiplier::pass(),
    }
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
pub fn recently_whispered_multiplier(
    memory: &MemoryTagView,
    recently_whispered_ids: Option<&HashSet<String>>,
) -> RecallMultiplier {
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
/// product is clamped to [MIN, MAX]. A cross-project narrow memory under the
/// `exclude` policy short-circuits to `{ exclude: true }`.
///
/// The float multiplication order (scope · temporal · context · participant ·
/// recent, left-associative) is preserved exactly so the f64 result is bit-equal
/// to the TS oracle.
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

    let temporal = temporal_multiplier(tags);
    let context = context_multiplier(tags, ctx.turn_context);
    let participant = participant_multiplier(memory, ctx.present_about_character_ids);
    let recent = recently_whispered_multiplier(memory, ctx.recently_whispered_ids);

    let product = scope.multiplier
        * temporal.multiplier
        * context.multiplier
        * participant.multiplier
        * recent.multiplier;
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

    CombinedRecallAdjustment {
        multiplier: clamped,
        fired,
        exclude: false,
    }
}
