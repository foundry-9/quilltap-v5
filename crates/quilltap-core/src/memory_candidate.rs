//! Port of v4 `lib/memory/candidate.ts` (`e0ac7493a`, Phase A of the memory
//! system refactor; pin `83334b9a5`) — the memory candidate: the one shape
//! every producer emits and the writer accepts (memory refactor §3.2).
//!
//! v4's why, carried: an extractor's parser yields a draft (what the model
//! said: text, keywords, importance, targeting, kind, `when`, entities —
//! [`MemoryDraftView`], v4's `MemoryDraftLike`). The producer that knows who
//! holds the memory, whom it is about and where it came from turns the draft
//! into a [`MemoryCandidate`] with [`candidate_from_draft`]; the episode pass,
//! the manual API and the others build one directly. Admission turns a
//! candidate into its row with [`to_admission_row`].
//!
//!  - **Importance** is defaulted once, by the parser ([`coerce_importance`]):
//!    a number is clamped to [0, 1], anything else is 0.5. Nothing downstream
//!    defaults it again, so a model's 0 stays 0.
//!  - **Targeting tags** ride on the candidate as `targeting`, never inside
//!    `keywords`; [`normalise_targeting`] is the one place a raw value becomes
//!    a tag. The row keeps storing them in `keywords` as it always has (the
//!    whisper and `parseTargetingTags` read them there) — [`to_admission_row`]
//!    writes them in.
//!
//! Pure; safe in the forked job child.
//!
//! **Coexistence (R-AH).** `crate::memory_tasks::MemoryCandidate` is v4's OLD
//! type (now v4's `MemoryDraft`); this module's [`MemoryCandidate`] is the new
//! Zod schema's. The two coexist by path until the extraction lane deletes
//! the old one.
//!
//! **The schema twin.** [`parse_memory_candidate`] is
//! `MemoryCandidateSchema.safeParse`: zod's issue list, in schema key order,
//! built ONLY through `api::zod_issues`' constructors (v4's
//! `validationError(parsed.error)` puts the list on the manual-create wire,
//! `app/api/v1/memories/route.ts:576-583`). JSON cannot carry a non-finite
//! number, so zod's non-finite refusal (an `invalid_type` carrying a
//! `received: "Infinity"` key) never reaches the twin over a `Value`; the
//! typed draft path refuses a non-finite anchor before it builds the value
//! ([`candidate_from_draft`]), exactly where v4's parse answers `null`.

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{json, Map, Value};

use crate::admission::types::AdmissionRow;
use crate::api::zod_issues::{key, ZodIssue, MEMORY_KIND, MEMORY_SOURCE, MEMORY_WITNESSED_CONTEXT};
use crate::jsnum::{math_max, math_min};
use crate::jsstr::js_trim;
use crate::recall_tags::{
    ContextTag, ScopeTag, TemporalTag, DEFAULT_CONTEXT, DEFAULT_SCOPE, DEFAULT_TEMPORAL,
};

/// v4 `TargetingTags` — ONE type with recall's (LR-11): v4's own
/// `candidate.ts` imports its vocabularies and defaults from `recall/tags`.
pub use crate::recall_tags::TargetingTags;

// ---------------------------------------------------------------------------
// Targeting
// ---------------------------------------------------------------------------

/// What an untagged memory reads as: present / wide / information (built
/// from `recall_tags`' defaults, as v4 builds it from `recall/tags`).
pub const DEFAULT_TARGETING: TargetingTags = TargetingTags {
    temporal: DEFAULT_TEMPORAL,
    scope: DEFAULT_SCOPE,
    context: DEFAULT_CONTEXT,
};

/// v4 `Partial<TargetingTags>` — the fallback [`normalise_targeting`] takes
/// and the tags [`split_targeting_keywords`] found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PartialTargetingTags {
    pub temporal: Option<TemporalTag>,
    pub scope: Option<ScopeTag>,
    pub context: Option<ContextTag>,
}

/// v4 `pick(raw, vocabulary)`: a string, trimmed and lower-cased, when it is
/// in the closed vocabulary; anything else (a non-string included) is none.
fn pick<T>(raw: Option<&Value>, from_kw: fn(&str) -> Option<T>) -> Option<T> {
    match raw {
        Some(Value::String(s)) => from_kw(&js_trim(s).to_lowercase()),
        _ => None,
    }
}

/// Normalise raw targeting values (a model's `temporal` / `scope` /
/// `context`) against the closed vocabularies. An invalid or missing value
/// takes the fallback's, else the default. The one normaliser for every
/// producer. `raw` is any JSON (v4 reads `raw?.temporal` — a non-object
/// carries no axis).
pub fn normalise_targeting(raw: Option<&Value>, fallback: &PartialTargetingTags) -> TargetingTags {
    let axis = |name: &str| raw.and_then(|r| r.get(name));
    TargetingTags {
        temporal: pick(axis("temporal"), TemporalTag::from_kw)
            .or(fallback.temporal)
            .unwrap_or(DEFAULT_TARGETING.temporal),
        scope: pick(axis("scope"), ScopeTag::from_kw)
            .or(fallback.scope)
            .unwrap_or(DEFAULT_TARGETING.scope),
        context: pick(axis("context"), ContextTag::from_kw)
            .or(fallback.context)
            .unwrap_or(DEFAULT_TARGETING.context),
    }
}

/// The row's keyword form of the tags: bare temporal and context words,
/// `scope: <value>`.
pub fn targeting_keywords(targeting: &TargetingTags) -> Vec<String> {
    vec![
        targeting.temporal.as_str().to_string(),
        format!("scope: {}", targeting.scope.as_str()),
        targeting.context.as_str().to_string(),
    ]
}

/// v4 `splitTargetingKeywords`'s `{ free, targeting }`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SplitTargetingKeywords {
    pub free: Vec<String>,
    pub targeting: PartialTargetingTags,
}

/// Split a keyword list into free words and the targeting tags it carries
/// (the last of each axis wins, as `parseTargetingTags` reads them). For
/// producers whose model emits tags inside its keywords (consolidation).
/// Non-strings are skipped; a `scope:` word with an unknown value is dropped
/// (neither a tag nor a free word); a blank word is dropped.
pub fn split_targeting_keywords(keywords: &[Value]) -> SplitTargetingKeywords {
    let mut out = SplitTargetingKeywords::default();
    for raw in keywords {
        let Value::String(raw) = raw else { continue };
        let k = js_trim(raw);
        let lower = k.to_lowercase();
        // `'scope:'` is six ASCII bytes, so the byte slice is v4's UTF-16
        // `slice('scope:'.length)`.
        if let Some(rest) = lower.strip_prefix("scope:") {
            if let Some(scope) = ScopeTag::from_kw(&js_trim(rest).to_lowercase()) {
                out.targeting.scope = Some(scope);
            }
            continue;
        }
        if let Some(temporal) = TemporalTag::from_kw(&lower) {
            out.targeting.temporal = Some(temporal);
            continue;
        }
        if let Some(context) = ContextTag::from_kw(&lower) {
            out.targeting.context = Some(context);
            continue;
        }
        if !k.is_empty() {
            out.free.push(k.to_string());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Importance
// ---------------------------------------------------------------------------

/// The one importance default: a finite number clamped to [0, 1], else 0.5.
/// (A JSON value never carries a non-finite number — `JSON.parse` cannot
/// produce one — so every non-number, absence included, is the 0.5 arm.)
pub fn coerce_importance(raw: Option<&Value>) -> f64 {
    match raw.and_then(|v| if v.is_number() { v.as_f64() } else { None }) {
        Some(n) if n.is_finite() => math_min(1.0, math_max(0.0, n)),
        _ => 0.5,
    }
}

// ---------------------------------------------------------------------------
// The candidate
// ---------------------------------------------------------------------------

/// Most entities a memory carries: the candidate's bound, and every fold's.
pub const MEMORY_ENTITY_CAP: usize = 12;

/// v4 `CandidatePassEnum`, in schema order (the issue's `values`).
pub const CANDIDATE_PASS_VALUES: [&str; 8] = [
    "self-turn",
    "other-turn",
    "other-fold",
    "episode-fold",
    "carina",
    "manual",
    "consolidation",
    "import",
];

/// v4 `CandidatePass`. `Consolidation` and `Import` are enumerated (zod's
/// enum refuses anything else) but no producer builds them at the pin (O11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandidatePass {
    SelfTurn,
    OtherTurn,
    OtherFold,
    EpisodeFold,
    Carina,
    Manual,
    Consolidation,
    Import,
}

impl CandidatePass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SelfTurn => "self-turn",
            Self::OtherTurn => "other-turn",
            Self::OtherFold => "other-fold",
            Self::EpisodeFold => "episode-fold",
            Self::Carina => "carina",
            Self::Manual => "manual",
            Self::Consolidation => "consolidation",
            Self::Import => "import",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        Some(match s {
            "self-turn" => Self::SelfTurn,
            "other-turn" => Self::OtherTurn,
            "other-fold" => Self::OtherFold,
            "episode-fold" => Self::EpisodeFold,
            "carina" => Self::Carina,
            "manual" => Self::Manual,
            "consolidation" => Self::Consolidation,
            "import" => Self::Import,
            _ => return None,
        })
    }
}

/// v4 `MemorySourceEnum` (`lib/schemas/memory.types.ts:29`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemorySource {
    #[serde(rename = "AUTO")]
    Auto,
    #[serde(rename = "MANUAL")]
    Manual,
    #[serde(rename = "CONSOLIDATED")]
    Consolidated,
}

impl MemorySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::Manual => "MANUAL",
            Self::Consolidated => "CONSOLIDATED",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        Some(match s {
            "AUTO" => Self::Auto,
            "MANUAL" => Self::Manual,
            "CONSOLIDATED" => Self::Consolidated,
            _ => return None,
        })
    }
}

/// v4 `WitnessedContextEnum` (`memory.types.ts:43`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WitnessedContext {
    UserPresent,
    AutonomousRoom,
    Manual,
}

impl WitnessedContext {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserPresent => "user_present",
            Self::AutonomousRoom => "autonomous_room",
            Self::Manual => "manual",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        Some(match s {
            "user_present" => Self::UserPresent,
            "autonomous_room" => Self::AutonomousRoom,
            "manual" => Self::Manual,
            _ => return None,
        })
    }
}

/// v4 `MemoryKindEnum` (`memory.types.ts:55`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryKind {
    Semantic,
    Episodic,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Semantic => "semantic",
            Self::Episodic => "episodic",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        Some(match s {
            "semantic" => Self::Semantic,
            "episodic" => Self::Episodic,
            _ => return None,
        })
    }
}

/// v4 `MemoryCandidate['subject']`: who holds the memory, whom it is about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateSubject {
    pub holder_id: String,
    pub about_id: Option<String>,
}

/// v4 `MemoryCandidate['provenance']`: where the memory came from.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateProvenance {
    pub source: MemorySource,
    pub witnessed_context: Option<WitnessedContext>,
    pub chat_id: Option<String>,
    pub project_id: Option<String>,
    pub source_message_id: Option<String>,
    /// The message clock (ms) the clock resolves `when` against; see
    /// [`anchor_time_ms_from`].
    #[serde(serialize_with = "ser_js_number")]
    pub anchor_time_ms: f64,
    pub pass: CandidatePass,
}

/// v4 `MemoryCandidate` — `MemoryCandidateSchema`'s output. Serialises as
/// zod's parsed data does: schema key order, nulls kept, numbers as
/// `JSON.stringify` writes them.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCandidate {
    pub content: String,
    pub summary: String,
    /// Free words; targeting tags are NOT here.
    pub keywords: Vec<String>,
    /// The targeting tags. `None` for an untagged memory (a hand-written
    /// one), which reads as [`DEFAULT_TARGETING`] and stores no tag keywords.
    #[serde(serialize_with = "ser_targeting")]
    pub targeting: Option<TargetingTags>,
    /// The parser's default (0.5) is the only default.
    #[serde(serialize_with = "ser_js_number")]
    pub importance: f64,
    pub kind: MemoryKind,
    /// The model's time phrase; the memory clock resolves it.
    pub when: Option<String>,
    pub entities: Vec<String>,
    /// Group-level tags (UUIDs) — the memory editor's, not the targeting ones.
    pub tags: Vec<String>,
    pub subject: CandidateSubject,
    pub provenance: CandidateProvenance,
}

fn ser_js_number<S: Serializer>(n: &f64, s: S) -> Result<S::Ok, S::Error> {
    crate::db::js_number_to_json(*n).serialize(s)
}

fn targeting_value(t: &TargetingTags) -> Value {
    json!({
        "temporal": t.temporal.as_str(),
        "scope": t.scope.as_str(),
        "context": t.context.as_str(),
    })
}

fn ser_targeting<S: Serializer>(t: &Option<TargetingTags>, s: S) -> Result<S::Ok, S::Error> {
    t.as_ref().map(targeting_value).serialize(s)
}

/// What a parser yields (v4 `MemoryDraftLike`): the model's part of a
/// candidate, before subject and provenance. `None` is v4's `undefined`.
/// The fields a model can send as any JSON stay `Value`-shaped:
/// `importance` (v4 inspects its `typeof`), `targeting` (v4 picks string
/// axes out of anything), and the `keywords` / `entities` elements (the
/// schema refuses a non-string one).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryDraftView {
    pub content: Option<String>,
    pub summary: Option<String>,
    pub keywords: Option<Vec<Value>>,
    pub importance: Option<Value>,
    pub targeting: Option<Value>,
    pub kind: Option<String>,
    pub when: Option<String>,
    pub entities: Option<Vec<Value>>,
}

/// The first of `iso_candidates` that parses as a time (JS `Date.parse`), in
/// ms; else `fallback_ms` (v4's default is now — the caller passes it). A
/// source message with an empty or malformed timestamp still yields a
/// candidate rather than a NaN the schema refuses.
pub fn anchor_time_ms_from(iso_candidates: &[Option<&str>], fallback_ms: f64) -> f64 {
    for iso in iso_candidates.iter().flatten() {
        if iso.is_empty() {
            continue;
        }
        if let Some(ms) = crate::episodic::js_date_parse_ms(iso) {
            return ms as f64;
        }
    }
    fallback_ms
}

/// The candidate for a parsed draft, given who holds it, whom it is about
/// and where it came from. Missing or blank text falls back to the other
/// field — the UNTRIMMED value is kept. `None` when the draft cannot be a
/// candidate (no text in either field, or a value the schema refuses): the
/// caller skips it, as the parser skips a draft with neither field — it
/// never aborts the turn.
pub fn candidate_from_draft(
    draft: &MemoryDraftView,
    subject: CandidateSubject,
    provenance: CandidateProvenance,
) -> Option<MemoryCandidate> {
    let non_blank = |s: &Option<String>| s.as_ref().filter(|s| !js_trim(s).is_empty()).cloned();
    let content = non_blank(&draft.content)
        .or_else(|| non_blank(&draft.summary))
        .unwrap_or_default();
    let summary = non_blank(&draft.summary).unwrap_or_else(|| content.clone());
    // Zod 4's number gate refuses a non-finite anchor (`received:
    // "Infinity"`), which no JSON value can carry: refuse it here, where
    // v4's `safeParse` answers null.
    let anchor = serde_json::Number::from_f64(provenance.anchor_time_ms)?;
    let entities: Vec<Value> = draft
        .entities
        .as_deref()
        .unwrap_or_default()
        .iter()
        .take(MEMORY_ENTITY_CAP)
        .cloned()
        .collect();
    let kind = if draft.kind.as_deref() == Some("episodic") {
        MemoryKind::Episodic
    } else {
        MemoryKind::Semantic
    };
    let input = json!({
        "content": content,
        "summary": summary,
        "keywords": draft.keywords.clone().unwrap_or_default(),
        // Idempotent on a parsed draft; a hand-built one still gets the defaults.
        "targeting": targeting_value(&normalise_targeting(
            draft.targeting.as_ref(),
            &PartialTargetingTags::default(),
        )),
        "importance": crate::db::js_number_to_json(coerce_importance(draft.importance.as_ref())),
        "kind": kind.as_str(),
        "when": draft.when,
        "entities": entities,
        "tags": [],
        "subject": subject,
        "provenance": {
            "source": provenance.source.as_str(),
            "witnessedContext": provenance.witnessed_context.map(WitnessedContext::as_str),
            "chatId": provenance.chat_id,
            "projectId": provenance.project_id,
            "sourceMessageId": provenance.source_message_id,
            "anchorTimeMs": Value::Number(anchor),
            "pass": provenance.pass.as_str(),
        },
    });
    parse_memory_candidate(&input).ok()
}

/// The anchors the clock resolved for a candidate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedCandidateTime {
    pub occurred_at: Option<String>,
    pub narrative_time: Option<String>,
    /// The row's `createdAt` override (a source message's own time), when the
    /// producer has one.
    pub source_message_timestamp: Option<String>,
}

/// The row a candidate becomes at admission: the targeting tags are written
/// into the row's keywords here, AFTER the free words.
pub fn to_admission_row(
    candidate: &MemoryCandidate,
    resolved: &ResolvedCandidateTime,
) -> AdmissionRow {
    let mut keywords = candidate.keywords.clone();
    if let Some(t) = &candidate.targeting {
        keywords.extend(targeting_keywords(t));
    }
    AdmissionRow {
        character_id: candidate.subject.holder_id.clone(),
        about_character_id: candidate.subject.about_id.clone(),
        chat_id: candidate.provenance.chat_id.clone(),
        project_id: candidate.provenance.project_id.clone(),
        content: candidate.content.clone(),
        summary: candidate.summary.clone(),
        keywords: Some(keywords),
        tags: Some(candidate.tags.clone()),
        importance: Some(candidate.importance),
        source: Some(candidate.provenance.source.as_str().to_string()),
        // v4 `sourceMessageId ?? undefined` — AdmissionRow's None is both.
        source_message_id: candidate.provenance.source_message_id.clone(),
        source_message_timestamp: resolved.source_message_timestamp.clone(),
        witnessed_context: candidate
            .provenance
            .witnessed_context
            .map(|w| w.as_str().to_string()),
        occurred_at: resolved.occurred_at.clone(),
        narrative_time: resolved.narrative_time.clone(),
        entities: Some(candidate.entities.clone()),
        kind: Some(candidate.kind.as_str().to_string()),
    }
}

// ---------------------------------------------------------------------------
// The schema twin
// ---------------------------------------------------------------------------

fn at(prefix: &[Value], k: &str) -> Vec<Value> {
    let mut p = prefix.to_vec();
    p.push(key(k));
    p
}

/// `z.string().min(1)` (v4's `Id` and the two text fields). `nullable` adds
/// `.nullable()`. A type miss aborts the field (no length check).
fn min1_string(
    obj: &Map<String, Value>,
    prefix: &[Value],
    k: &str,
    nullable: bool,
    issues: &mut Vec<ZodIssue>,
) -> Option<Option<String>> {
    match obj.get(k) {
        Some(Value::Null) if nullable => Some(None),
        Some(Value::String(s)) if s.is_empty() => {
            issues.push(ZodIssue::too_small_string(json!(1), at(prefix, k)));
            None
        }
        Some(Value::String(s)) => Some(Some(s.clone())),
        got => {
            issues.push(ZodIssue::invalid_type("string", at(prefix, k), got));
            None
        }
    }
}

/// `z.array(z.string())` with `.default([])`, and `.max(cap)` when given:
/// absent defaults; anything but an array (`null` included — a default
/// replaces `undefined` only) is `invalid_type array`; an array past `cap`
/// answers ONLY its `too_big` before any element (Zod 4.6.7's `capSize`);
/// otherwise each non-string element is its own issue at `[k, index]`.
fn string_array(
    obj: &Map<String, Value>,
    k: &str,
    cap: Option<usize>,
    issues: &mut Vec<ZodIssue>,
) -> Option<Vec<String>> {
    let items = match obj.get(k) {
        None => return Some(Vec::new()),
        Some(Value::Array(items)) => items,
        got => {
            issues.push(ZodIssue::invalid_type("array", vec![key(k)], got));
            return None;
        }
    };
    // Zod 4.6.7's `capSize` (P4.D311, measured at the `25b8fab6a` pin): an
    // array PAST its leading `.max(cap)` answers ONLY its own `too_big`, before
    // any element is parsed (4.6.5 reported the element issues first).
    if let Some(cap) = cap {
        if items.len() > cap {
            issues.push(ZodIssue::too_big_array(json!(cap), vec![key(k)]));
            return None;
        }
    }
    let mut ok = true;
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        match item {
            Value::String(s) => out.push(s.clone()),
            got => {
                issues.push(ZodIssue::invalid_type(
                    "string",
                    vec![key(k), json!(i)],
                    Some(got),
                ));
                ok = false;
            }
        }
    }
    ok.then_some(out)
}

/// A `z.enum([...])` (no separate type gate: a wrong type, a missing key and
/// an out-of-domain string all answer `invalid_value`); `nullable` adds
/// `.nullable()`.
fn enum_value<T>(
    obj: &Map<String, Value>,
    prefix: &[Value],
    k: &str,
    values: &[&str],
    from_wire: fn(&str) -> Option<T>,
    nullable: bool,
    issues: &mut Vec<ZodIssue>,
) -> Option<Option<T>> {
    match obj.get(k) {
        Some(Value::Null) if nullable => return Some(None),
        Some(Value::String(s)) => {
            if let Some(v) = from_wire(s) {
                return Some(Some(v));
            }
        }
        _ => {}
    }
    issues.push(ZodIssue::invalid_value(values, at(prefix, k)));
    None
}

/// A `z.object({...})` gate: anything but a plain object is `invalid_type
/// object` (an array included). `nullable` adds `.nullable()`.
fn object_at<'a>(
    v: Option<&'a Value>,
    path: Vec<Value>,
    issues: &mut Vec<ZodIssue>,
) -> Option<&'a Map<String, Value>> {
    match v {
        Some(Value::Object(m)) => Some(m),
        got => {
            issues.push(ZodIssue::invalid_type("object", path, got));
            None
        }
    }
}

fn targeting_tags(m: &Map<String, Value>, issues: &mut Vec<ZodIssue>) -> Option<TargetingTags> {
    let prefix = [key("targeting")];
    let temporal = enum_value(
        m,
        &prefix,
        "temporal",
        &crate::recall_tags::TEMPORAL_VALUES,
        TemporalTag::from_kw,
        false,
        issues,
    );
    let scope = enum_value(
        m,
        &prefix,
        "scope",
        &["narrow", "wide"],
        ScopeTag::from_kw,
        false,
        issues,
    );
    let context = enum_value(
        m,
        &prefix,
        "context",
        &crate::recall_tags::CONTEXT_VALUES,
        ContextTag::from_kw,
        false,
        issues,
    );
    Some(TargetingTags {
        temporal: temporal??,
        scope: scope??,
        context: context??,
    })
}

/// v4 `MemoryCandidateSchema.safeParse(v)` (`candidate.ts:150-179`): the
/// parsed candidate (defaults applied — `keywords` / `entities` / `tags` →
/// `[]` when absent; unknown keys stripped), or zod's issue list in schema
/// key order, byte-identical to `error.issues` under `serde_json::to_string`.
pub fn parse_memory_candidate(v: &Value) -> Result<MemoryCandidate, Vec<ZodIssue>> {
    let mut issues = Vec::new();
    let Some(obj) = object_at(Some(v), vec![], &mut issues) else {
        return Err(issues);
    };
    let content = min1_string(obj, &[], "content", false, &mut issues);
    let summary = min1_string(obj, &[], "summary", false, &mut issues);
    let keywords = string_array(obj, "keywords", None, &mut issues);
    // `TargetingTagsSchema.nullable()` — a missing key is NOT null.
    let targeting = match obj.get("targeting") {
        Some(Value::Null) => Some(None),
        got => object_at(got, vec![key("targeting")], &mut issues)
            .and_then(|m| targeting_tags(m, &mut issues))
            .map(Some),
    };
    // `z.number().min(0).max(1)`.
    let importance = match obj.get("importance") {
        Some(n @ Value::Number(_)) => {
            let x = n.as_f64().unwrap_or(f64::NAN);
            if x < 0.0 {
                issues.push(ZodIssue::too_small_number(
                    json!(0),
                    vec![key("importance")],
                ));
                None
            } else if x > 1.0 {
                issues.push(ZodIssue::too_big_number(json!(1), vec![key("importance")]));
                None
            } else {
                Some(x)
            }
        }
        got => {
            issues.push(ZodIssue::invalid_type(
                "number",
                vec![key("importance")],
                got,
            ));
            None
        }
    };
    let kind = enum_value(
        obj,
        &[],
        "kind",
        &MEMORY_KIND,
        MemoryKind::from_wire,
        false,
        &mut issues,
    );
    // `z.string().nullable()`.
    let when = match obj.get("when") {
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Some(Some(s.clone())),
        got => {
            issues.push(ZodIssue::invalid_type("string", vec![key("when")], got));
            None
        }
    };
    let entities = string_array(obj, "entities", Some(MEMORY_ENTITY_CAP), &mut issues);
    let tags = string_array(obj, "tags", None, &mut issues);
    let subject = object_at(obj.get("subject"), vec![key("subject")], &mut issues).and_then(|m| {
        let prefix = [key("subject")];
        let holder_id = min1_string(m, &prefix, "holderId", false, &mut issues);
        let about_id = min1_string(m, &prefix, "aboutId", true, &mut issues);
        Some(CandidateSubject {
            holder_id: holder_id??,
            about_id: about_id?,
        })
    });
    let provenance = object_at(obj.get("provenance"), vec![key("provenance")], &mut issues)
        .and_then(|m| provenance_of(m, &mut issues));

    if !issues.is_empty() {
        return Err(issues);
    }
    // Every field parsed (no issue was raised), so each is `Some`.
    match (
        content, summary, keywords, targeting, importance, kind, when, entities, tags, subject,
        provenance,
    ) {
        (
            Some(Some(content)),
            Some(Some(summary)),
            Some(keywords),
            Some(targeting),
            Some(importance),
            Some(Some(kind)),
            Some(when),
            Some(entities),
            Some(tags),
            Some(subject),
            Some(provenance),
        ) => Ok(MemoryCandidate {
            content,
            summary,
            keywords,
            targeting,
            importance,
            kind,
            when,
            entities,
            tags,
            subject,
            provenance,
        }),
        _ => Err(issues),
    }
}

fn provenance_of(
    m: &Map<String, Value>,
    issues: &mut Vec<ZodIssue>,
) -> Option<CandidateProvenance> {
    let prefix = [key("provenance")];
    let source = enum_value(
        m,
        &prefix,
        "source",
        &MEMORY_SOURCE,
        MemorySource::from_wire,
        false,
        issues,
    );
    let witnessed_context = enum_value(
        m,
        &prefix,
        "witnessedContext",
        &MEMORY_WITNESSED_CONTEXT,
        WitnessedContext::from_wire,
        true,
        issues,
    );
    let chat_id = min1_string(m, &prefix, "chatId", true, issues);
    let project_id = min1_string(m, &prefix, "projectId", true, issues);
    let source_message_id = min1_string(m, &prefix, "sourceMessageId", true, issues);
    // `z.number().finite()` — a JSON number is always finite.
    let anchor_time_ms = match m.get("anchorTimeMs") {
        Some(n @ Value::Number(_)) => n.as_f64(),
        got => {
            issues.push(ZodIssue::invalid_type(
                "number",
                at(&prefix, "anchorTimeMs"),
                got,
            ));
            None
        }
    };
    let pass = enum_value(
        m,
        &prefix,
        "pass",
        &CANDIDATE_PASS_VALUES,
        CandidatePass::from_wire,
        false,
        issues,
    );
    Some(CandidateProvenance {
        source: source??,
        witnessed_context: witnessed_context?,
        chat_id: chat_id?,
        project_id: project_id?,
        source_message_id: source_message_id?,
        anchor_time_ms: anchor_time_ms?,
        pass: pass??,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Value {
        json!({
            "content": "x", "summary": "x", "targeting": null, "importance": 0.5,
            "kind": "semantic", "when": null,
            "subject": { "holderId": "h", "aboutId": null },
            "provenance": {
                "source": "MANUAL", "witnessedContext": "manual", "chatId": "chat-1",
                "projectId": null, "sourceMessageId": "msg-1",
                "anchorTimeMs": 1_791_659_600_000_i64, "pass": "manual"
            }
        })
    }

    #[test]
    fn parsing_a_parsed_candidate_changes_nothing() {
        let c = parse_memory_candidate(&base()).expect("parses");
        assert_eq!(c.keywords, Vec::<String>::new());
        let again = parse_memory_candidate(&serde_json::to_value(&c).unwrap()).expect("re-parses");
        assert_eq!(again, c);
    }

    #[test]
    fn refuses_no_text_or_out_of_range_importance() {
        let mut v = base();
        v["content"] = json!("");
        assert!(parse_memory_candidate(&v).is_err());
        let mut v = base();
        v["importance"] = json!(1.2);
        assert!(parse_memory_candidate(&v).is_err());
    }

    #[test]
    fn coerce_importance_keeps_a_models_zero() {
        assert_eq!(coerce_importance(Some(&json!(0))), 0.0);
        assert_eq!(coerce_importance(Some(&json!(5))), 1.0);
        assert_eq!(coerce_importance(Some(&json!(-1))), 0.0);
        assert_eq!(coerce_importance(Some(&json!("0.9"))), 0.5);
        assert_eq!(coerce_importance(None), 0.5);
    }

    #[test]
    fn a_non_finite_anchor_is_no_candidate() {
        let prov = CandidateProvenance {
            source: MemorySource::Auto,
            witnessed_context: Some(WitnessedContext::UserPresent),
            chat_id: None,
            project_id: None,
            source_message_id: None,
            anchor_time_ms: f64::INFINITY,
            pass: CandidatePass::SelfTurn,
        };
        let subject = CandidateSubject {
            holder_id: "h".into(),
            about_id: None,
        };
        let draft = MemoryDraftView {
            content: Some("c".into()),
            ..Default::default()
        };
        assert!(candidate_from_draft(&draft, subject, prov).is_none());
    }
}

#[cfg(test)]
mod jest_twins {
    //! Case-for-case port of v4 `__tests__/unit/lib/memory/memory-candidate.test.ts`
    //! (pin `83334b9a5`). The cases that drive `runPass` through the mocked
    //! extraction context (the producers' async half — the admission mock,
    //! the cheap-LLM mock, the turn window) are not expressible against this
    //! pure module and are listed in the P4.D289 Tier-2 report; a case whose
    //! first half is pure keeps that half here.

    use super::*;
    use crate::consolidation_plan::normalize_digest_keywords;
    use crate::recall_tags::{parse_targeting_tags, ContextTag, ScopeTag, TemporalTag};

    const HOLDER: &str = "char-holder";
    const OTHER: &str = "char-other";

    fn turn_ms() -> f64 {
        crate::episodic::js_date_parse_ms("2026-10-12T19:00:00.000Z").expect("parses") as f64
    }

    /// v4's `provenance(pass, source)` helper, as the struct.
    fn provenance(pass: CandidatePass, source: MemorySource) -> CandidateProvenance {
        CandidateProvenance {
            source,
            witnessed_context: Some(if source == MemorySource::Manual {
                WitnessedContext::Manual
            } else {
                WitnessedContext::UserPresent
            }),
            chat_id: Some("chat-1".into()),
            project_id: None,
            source_message_id: Some("msg-1".into()),
            anchor_time_ms: turn_ms(),
            pass,
        }
    }

    /// v4's `provenance(pass, source)` helper, as the JSON a schema parse reads.
    fn provenance_json(pass: &str, source: &str) -> Value {
        json!({
            "source": source,
            "witnessedContext": if source == "MANUAL" { "manual" } else { "user_present" },
            "chatId": "chat-1",
            "projectId": null,
            "sourceMessageId": "msg-1",
            "anchorTimeMs": turn_ms() as i64,
            "pass": pass,
        })
    }

    fn subject(holder: &str, about: Option<&str>) -> CandidateSubject {
        CandidateSubject {
            holder_id: holder.into(),
            about_id: about.map(str::to_string),
        }
    }

    fn strs(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| s.to_string()).collect()
    }

    fn values(words: &[String]) -> Vec<Value> {
        words.iter().map(|s| Value::String(s.clone())).collect()
    }

    /// v4's `roundTrip(candidate)` helper.
    fn round_trip(candidate: &MemoryCandidate) -> AdmissionRow {
        // Parsing a parsed candidate changes nothing.
        let reparsed = parse_memory_candidate(&serde_json::to_value(candidate).unwrap())
            .expect("a parsed candidate re-parses");
        assert_eq!(&reparsed, candidate);
        let row = to_admission_row(candidate, &ResolvedCandidateTime::default());
        assert_eq!(row.character_id, candidate.subject.holder_id);
        assert_eq!(row.about_character_id, candidate.subject.about_id);
        assert_eq!(
            row.source.as_deref(),
            Some(candidate.provenance.source.as_str())
        );
        assert_eq!(row.importance, Some(candidate.importance));
        // The row's keywords: the free words, then the tags — read back unchanged.
        let keywords = row.keywords.clone().unwrap_or_default();
        assert_eq!(
            parse_targeting_tags(&keywords),
            candidate.targeting.unwrap_or(DEFAULT_TARGETING)
        );
        assert_eq!(
            split_targeting_keywords(&values(&keywords)).free,
            candidate.keywords
        );
        row
    }

    // --- describe('targeting') ---

    /// "normalises against the closed vocabularies, with fallbacks"
    #[test]
    fn normalises_against_the_closed_vocabularies_with_fallbacks() {
        assert_eq!(
            normalise_targeting(
                Some(&json!({ "temporal": "PAST", "scope": " narrow ", "context": "gossip" })),
                &PartialTargetingTags::default()
            ),
            TargetingTags {
                temporal: TemporalTag::Past,
                scope: ScopeTag::Narrow,
                context: ContextTag::Information,
            }
        );
        assert_eq!(
            normalise_targeting(
                Some(&Value::Null),
                &PartialTargetingTags {
                    temporal: Some(TemporalTag::Past),
                    ..Default::default()
                }
            ),
            TargetingTags {
                temporal: TemporalTag::Past,
                ..DEFAULT_TARGETING
            }
        );
    }

    /// "materialises and splits symmetrically"
    #[test]
    fn materialises_and_splits_symmetrically() {
        let t = TargetingTags {
            temporal: TemporalTag::Moment,
            scope: ScopeTag::Narrow,
            context: ContextTag::Banter,
        };
        assert_eq!(
            targeting_keywords(&t),
            strs(&["moment", "scope: narrow", "banter"])
        );
        let mut kws = strs(&["Harbour"]);
        kws.extend(targeting_keywords(&t));
        assert_eq!(
            split_targeting_keywords(&values(&kws)),
            SplitTargetingKeywords {
                free: strs(&["Harbour"]),
                targeting: PartialTargetingTags {
                    temporal: Some(t.temporal),
                    scope: Some(t.scope),
                    context: Some(t.context),
                },
            }
        );
    }

    // --- describe('importance: the parser default is the only default') ---

    /// `it.each(...)('%p → %p')` — every row but `[Number.NaN, 0.5]`: no JSON
    /// value carries a NaN (`serde_json::Number` refuses one, and
    /// `json!(f64::NAN)` is `null`), so that row is not expressible against
    /// `coerce_importance(Option<&Value>)` (listed in the report).
    #[test]
    fn importance_the_parser_default_is_the_only_default() {
        let cases: [(Option<Value>, f64); 6] = [
            (Some(json!(0)), 0.0),
            (Some(json!(0.73)), 0.73),
            (Some(json!(5)), 1.0),
            (Some(json!(-1)), 0.0),
            (None, 0.5),
            (Some(json!("0.9")), 0.5),
        ];
        for (raw, expected) in cases {
            assert_eq!(
                coerce_importance(raw.as_ref()),
                expected,
                "{raw:?} → {expected}"
            );
        }
    }

    // --- describe('producers') ---

    /// "per-turn SELF/OTHER: the draft becomes the admitted row, a model 0
    /// stays 0" — the pure half only (the closing `roundTrip(candidateFromDraft
    /// (...))`); the `runPass(otherTurnPass(), …)` half drives the mocked
    /// extraction context and is not expressible here.
    #[test]
    fn per_turn_self_other_the_draft_becomes_the_admitted_row_a_model_0_stays_0() {
        let draft = MemoryDraftView {
            content: Some("Amy is afraid of the dark.".into()),
            summary: Some("Amy fears the dark".into()),
            keywords: Some(vec![json!("dark")]),
            importance: Some(json!(0)),
            targeting: Some(
                json!({ "temporal": "present", "scope": "wide", "context": "relationships" }),
            ),
            kind: Some("semantic".into()),
            ..Default::default()
        };
        let candidate = candidate_from_draft(
            &draft,
            subject(HOLDER, Some(OTHER)),
            provenance(CandidatePass::OtherTurn, MemorySource::Auto),
        )
        .expect("a candidate");
        assert_eq!(candidate.importance, 0.0);
        round_trip(&candidate);
    }

    /// "fold episode: past / narrow / history, about nobody in particular"
    #[test]
    fn fold_episode_past_narrow_history_about_nobody_in_particular() {
        let candidate = parse_memory_candidate(&json!({
            "content": "They went down to the cellar and found the crates.",
            "summary": "the cellar expedition",
            "keywords": ["cellar", "crates"],
            "targeting": { "temporal": "past", "scope": "narrow", "context": "history" },
            "importance": 0.7,
            "kind": "episodic",
            "when": "last night",
            "entities": ["Cellar", "Crates"],
            "subject": { "holderId": HOLDER, "aboutId": null },
            "provenance": provenance_json("episode-fold", "AUTO"),
        }))
        .expect("parses");
        let row = round_trip(&candidate);
        assert_eq!(
            row.keywords,
            Some(strs(&[
                "cellar",
                "crates",
                "past",
                "scope: narrow",
                "history"
            ]))
        );
        assert_eq!(row.kind.as_deref(), Some("episodic"));
    }

    /// "manual: no targeting tags are written, and the row reads as the defaults"
    #[test]
    fn manual_no_targeting_tags_are_written_and_the_row_reads_as_the_defaults() {
        let candidate = parse_memory_candidate(&json!({
            "content": "Holly prefers tea.",
            "summary": "prefers tea",
            "keywords": ["tea"],
            "targeting": null,
            "importance": 0.6,
            "kind": "semantic",
            "when": null,
            "subject": { "holderId": HOLDER, "aboutId": null },
            "provenance": provenance_json("manual", "MANUAL"),
        }))
        .expect("parses");
        let row = round_trip(&candidate);
        assert_eq!(row.keywords, Some(strs(&["tea"])));
        assert_eq!(row.witnessed_context.as_deref(), Some("manual"));
    }

    /// "consolidation: a digest's keywords normalise through the same targeting rules"
    #[test]
    fn consolidation_a_digests_keywords_normalise_through_the_same_targeting_rules() {
        let keywords = normalize_digest_keywords(
            &strs(&["Harbour", "harbour", "SCOPE: narrow", "banter"]),
            &[
                strs(&["present", "scope: wide", "history"]),
                strs(&["past", "scope: wide", "history"]),
            ],
            "semantic",
        );
        assert_eq!(
            keywords,
            strs(&["harbour", "present", "scope: narrow", "banter"])
        );
        let SplitTargetingKeywords { free, targeting } =
            split_targeting_keywords(&values(&keywords));
        // v4 hands `normaliseTargeting` the split's `Partial<TargetingTags>`
        // object: the axes it found, as strings.
        let mut partial = Map::new();
        if let Some(t) = targeting.temporal {
            partial.insert("temporal".into(), json!(t.as_str()));
        }
        if let Some(s) = targeting.scope {
            partial.insert("scope".into(), json!(s.as_str()));
        }
        if let Some(c) = targeting.context {
            partial.insert("context".into(), json!(c.as_str()));
        }
        let normalised = normalise_targeting(
            Some(&Value::Object(partial)),
            &PartialTargetingTags::default(),
        );
        let candidate = parse_memory_candidate(&json!({
            "content": "A digest.",
            "summary": "digest",
            "keywords": free,
            "targeting": targeting_value(&normalised),
            "importance": 0.8,
            "kind": "semantic",
            "when": null,
            "subject": { "holderId": HOLDER, "aboutId": OTHER },
            "provenance": provenance_json("consolidation", "AUTO"),
        }))
        .expect("parses");
        round_trip(&candidate);
    }

    /// "a blank draft is not a candidate, and skips alone rather than aborting
    /// the turn" — the pure half only (the two `candidateFromDraft`
    /// expectations); the `runPass(selfTurnPass(), …)` half is not expressible
    /// here.
    #[test]
    fn a_blank_draft_is_not_a_candidate_and_skips_alone_rather_than_aborting_the_turn() {
        let blank = MemoryDraftView {
            content: Some("   ".into()),
            ..Default::default()
        };
        assert!(candidate_from_draft(
            &blank,
            subject(HOLDER, None),
            provenance(CandidatePass::SelfTurn, MemorySource::Auto)
        )
        .is_none());
        // Blank content with a usable summary falls back to the summary.
        let fallback = MemoryDraftView {
            content: Some("  ".into()),
            summary: Some("kept".into()),
            ..Default::default()
        };
        assert_eq!(
            candidate_from_draft(
                &fallback,
                subject(HOLDER, None),
                provenance(CandidatePass::SelfTurn, MemorySource::Auto)
            )
            .map(|c| c.content)
            .as_deref(),
            Some("kept")
        );
    }

    /// "an empty or malformed source timestamp falls through to the next anchor"
    #[test]
    fn an_empty_or_malformed_source_timestamp_falls_through_to_the_next_anchor() {
        // v4's default fallback is `Date.now()`; the first case never reaches it.
        assert_eq!(
            anchor_time_ms_from(
                &[
                    Some(""),
                    Some("not a date"),
                    Some("2026-10-12T19:00:00.000Z")
                ],
                0.0
            ),
            turn_ms()
        );
        assert_eq!(anchor_time_ms_from(&[None, None, Some("")], 42.0), 42.0);
    }

    /// "refuses a candidate with no text or an out-of-range importance"
    #[test]
    fn refuses_a_candidate_with_no_text_or_an_out_of_range_importance() {
        let base = json!({
            "content": "x",
            "summary": "x",
            "targeting": null,
            "importance": 0.5,
            "kind": "semantic",
            "when": null,
            "subject": { "holderId": HOLDER, "aboutId": null },
            "provenance": provenance_json("manual", "MANUAL"),
        });
        let mut no_text = base.clone();
        no_text["content"] = json!("");
        assert!(parse_memory_candidate(&no_text).is_err());
        let mut too_important = base.clone();
        too_important["importance"] = json!(1.2);
        assert!(parse_memory_candidate(&too_important).is_err());
        assert!(parse_memory_candidate(&base).is_ok());
    }
}
