//! Consolidation write planning — pure. v4 `lib/memory/consolidation-plan.ts`
//! (`d58548051`, byte-identical at the `01a83539d` pin).
//!
//! v4's consolidation job runs in the forked job child, where repository reads
//! never see the job's own buffered writes. So it computes *everything* first —
//! this module — and only then issues the writes (memory-consolidation-and-
//! tiers.md §C5). v5 has no child, but keeps the shape: keeping the planner pure
//! is also what makes the write rules testable without a database:
//!
//!   - **New digest** (`source: 'CONSOLIDATED'`): `consolidatedFrom` = its
//!     members; `reinforcementCount` = Σ member counts, capped at
//!     [`DIGEST_REINFORCEMENT_CAP`]; `importance` = the model's value clamped
//!     to `[min, max]` of the members' `reinforcedImportance`;
//!     `reinforcedImportance` by the gate's formula; `relatedMemoryIds` = the
//!     union of the members' links that point *outside* the cluster (re-aimed at
//!     a digest when they pointed at a row folded elsewhere in the same run);
//!     `occurredAt` = the earliest member event time for an episode digest, null
//!     for a standing fact; `chatId` / `projectId` only when every member shares
//!     one; keywords end with the three targeting tags.
//!   - **Existing digest** (the cluster formed around it): updated in place —
//!     the model's first digest is its revision, `consolidatedFrom` grows, the
//!     digest's own count and links are carried, and it is re-embedded.
//!   - **Members** go cold with `supersededById` = their digest and
//!     `consolidatedAt` = now.
//!   - **Contradictions**: the older row goes cold, superseded by the digest
//!     carrying the newer fact (or by the newer row itself when that row stands
//!     alone).
//!   - **Inbound links**: any surviving row that pointed at a superseded row now
//!     points at its digest, de-duplicated.
//!   - **Standalones** stay hot and only get `consolidatedAt`.
//!
//! Rows enter as the memory JSON (`serde_json::Value`) the C1 reads emit; every
//! `?? 1` / `?? 0.5` / `?? []` is v4's. v4's two `Map`s (`supersededBy`, the
//! tier moves) keep FIRST-insertion position on a re-set — [`OrderedMap`]
//! reproduces that. `localeCompare` is ICU ([`crate::collation`]); `.sort()` on
//! the ISO reference times is a code-unit sort (byte order is identical for the
//! ASCII ISO strings that reach it). Tier-1 exact against v4's REAL export
//! (`consolidation_plan_equivalence`).

use std::collections::{HashMap, HashSet};

use serde_json::{json, Map, Value};

use crate::consolidation_clustering::ClusterKind;
use crate::consolidation_tasks::{ConsolidationContradiction, ConsolidationDigestOutput};
use crate::db::memories::{MemCreate, MemUpdate};
use crate::episodic::js_date_parse_ms;
use crate::memory_gate::calculate_reinforced_importance;
use crate::recall_tags::{parse_targeting_tags, ContextTag, ScopeTag, TemporalTag};

/// Ceiling on a digest's `reinforcementCount`.
pub const DIGEST_REINFORCEMENT_CAP: f64 = 50.0;

/// The model's importance is first held to this band (spec: 0.2–1.0).
const IMPORTANCE_FLOOR: f64 = 0.2;

/// Bounds on the unions a digest inherits from its members.
const MAX_DIGEST_ENTITIES: usize = 12;
const MAX_FREE_KEYWORDS: usize = 8;

/// A JS `Map<string, V>`: insertion-ordered, and a re-`set` keeps the key's
/// FIRST position (never remove + insert).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrderedMap<V> {
    keys: Vec<String>,
    values: HashMap<String, V>,
}

impl<V> OrderedMap<V> {
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            values: HashMap::new(),
        }
    }

    pub fn set(&mut self, k: &str, v: V) {
        if self.values.insert(k.to_string(), v).is_none() {
            self.keys.push(k.to_string());
        }
    }

    pub fn get(&self, k: &str) -> Option<&V> {
        self.values.get(k)
    }

    pub fn get_mut(&mut self, k: &str) -> Option<&mut V> {
        self.values.get_mut(k)
    }

    pub fn has(&self, k: &str) -> bool {
        self.values.contains_key(k)
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Entries in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &V)> {
        self.keys.iter().map(move |k| (k, &self.values[k]))
    }
}

/// One cluster's validated outcome, mapped from handles back to memory ids.
#[derive(Debug, Clone)]
pub struct ResolvedClusterOutcome {
    /// `aboutCharacterId` the digests carry (holder id for self, `None` for the
    /// null bucket).
    pub about_character_id: Option<String>,
    pub cluster_kind: ClusterKind,
    /// The digest the cluster formed around, if any (memory JSON).
    pub existing_digest: Option<Value>,
    /// Non-digest members, as loaded (memory JSON).
    pub members: Vec<Value>,
    /// Model digests, `member_ids` already mapped to memory ids.
    pub digests: Vec<ConsolidationDigestOutput>,
    /// Member ids the model kept standalone (unlisted included).
    pub keep_standalone: Vec<String>,
    /// Contradictions in id space.
    pub contradictions: Vec<ConsolidationContradiction>,
    /// One embedding per digest, same order; `None` entries in a dry run.
    pub embeddings: Vec<Option<Vec<f32>>>,
}

/// `'create' | 'update'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestAction {
    Create,
    Update,
}

impl DigestAction {
    pub fn as_str(self) -> &'static str {
        match self {
            DigestAction::Create => "create",
            DigestAction::Update => "update",
        }
    }
}

/// A digest as the plan (and the dry-run report) describes it — v4
/// `PlannedDigest`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedDigest {
    pub id: String,
    pub action: DigestAction,
    pub cluster_index: usize,
    pub content: String,
    pub summary: String,
    pub keywords: Vec<String>,
    pub importance: f64,
    pub reinforced_importance: f64,
    pub reinforcement_count: f64,
    pub kind: String,
    pub occurred_at: Option<String>,
    pub narrative_time: Option<String>,
    pub entities: Vec<String>,
    /// Members this digest replaces in this run.
    pub member_ids: Vec<String>,
    pub about_character_id: Option<String>,
    pub embedding: Option<Vec<f32>>,
}

impl PlannedDigest {
    /// v4's key order (`id, action, clusterIndex, …, aboutCharacterId,
    /// embedding`); the embedding as its f32 values.
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("id".into(), json!(self.id));
        m.insert("action".into(), json!(self.action.as_str()));
        m.insert("clusterIndex".into(), json!(self.cluster_index));
        m.insert("content".into(), json!(self.content));
        m.insert("summary".into(), json!(self.summary));
        m.insert("keywords".into(), json!(self.keywords));
        m.insert("importance".into(), num(self.importance));
        m.insert(
            "reinforcedImportance".into(),
            num(self.reinforced_importance),
        );
        m.insert("reinforcementCount".into(), num(self.reinforcement_count));
        m.insert("kind".into(), json!(self.kind));
        m.insert("occurredAt".into(), json!(self.occurred_at));
        m.insert("narrativeTime".into(), json!(self.narrative_time));
        m.insert("entities".into(), json!(self.entities));
        m.insert("memberIds".into(), json!(self.member_ids));
        m.insert("aboutCharacterId".into(), json!(self.about_character_id));
        m.insert(
            "embedding".into(),
            match &self.embedding {
                Some(e) => Value::Array(e.iter().map(|x| json!(*x as f64)).collect()),
                None => Value::Null,
            },
        );
        Value::Object(m)
    }
}

/// v4 `PlannedDigestCreate` — the base plus the create `data`.
pub struct PlannedDigestCreate {
    pub digest: PlannedDigest,
    pub data: MemCreate,
}

impl PlannedDigestCreate {
    /// v4's `data` object in its literal key order.
    pub fn data_json(&self) -> Value {
        let d = &self.data;
        let mut m = Map::new();
        m.insert("characterId".into(), json!(d.character_id));
        m.insert("aboutCharacterId".into(), json!(d.about_character_id));
        m.insert("chatId".into(), json!(d.chat_id));
        m.insert("projectId".into(), json!(d.project_id));
        m.insert("content".into(), json!(d.content));
        m.insert("summary".into(), json!(d.summary));
        m.insert("keywords".into(), json!(d.keywords));
        m.insert("tags".into(), json!(d.tags));
        m.insert("importance".into(), num(d.importance));
        m.insert("source".into(), json!(d.source));
        m.insert("witnessedContext".into(), json!(d.witnessed_context));
        m.insert("occurredAt".into(), json!(d.occurred_at));
        m.insert("narrativeTime".into(), json!(d.narrative_time));
        m.insert("entities".into(), json!(d.entities));
        m.insert("kind".into(), json!(d.kind));
        m.insert("sourceMessageId".into(), json!(d.source_message_id));
        m.insert("lastAccessedAt".into(), json!(d.last_accessed_at));
        m.insert("reinforcementCount".into(), num(d.reinforcement_count));
        m.insert("lastReinforcedAt".into(), json!(d.last_reinforced_at));
        m.insert("relatedMemoryIds".into(), json!(d.related_memory_ids));
        m.insert("reinforcedImportance".into(), num(d.reinforced_importance));
        m.insert("tier".into(), json!(d.tier));
        m.insert("supersededById".into(), json!(d.superseded_by_id));
        m.insert("consolidatedFrom".into(), json!(d.consolidated_from));
        m.insert("consolidatedAt".into(), json!(d.consolidated_at));
        Value::Object(m)
    }
}

/// v4 `PlannedDigestUpdate` — the base plus the in-place `patch`.
pub struct PlannedDigestUpdate {
    pub digest: PlannedDigest,
    pub patch: MemUpdate,
    /// The patch's values in v4's key order (the `MemUpdate` is what the
    /// repository writes; this is the comparand and the report's source).
    pub patch_json: Value,
}

/// One tier move: rows sent cold under the row that supersedes them.
#[derive(Debug, Clone, PartialEq)]
pub struct TierMove {
    pub superseded_by_id: String,
    pub ids: Vec<String>,
}

/// One surviving row whose links are re-aimed at digests.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkRewrite {
    pub id: String,
    pub related_memory_ids: Vec<String>,
}

/// v4 `ConsolidationWritePlan`.
pub struct ConsolidationWritePlan {
    pub creates: Vec<PlannedDigestCreate>,
    pub updates: Vec<PlannedDigestUpdate>,
    /// Rows to send cold, grouped by the row that supersedes them.
    pub tier_moves: Vec<TierMove>,
    /// Standalone rows that stay hot and only get `consolidatedAt`.
    pub considered: Vec<String>,
    /// Surviving rows whose links are re-aimed at digests.
    pub link_rewrites: Vec<LinkRewrite>,
    /// Every superseded row → the row that supersedes it.
    pub superseded_by: OrderedMap<String>,
    /// Planned digests per input cluster, in cluster order (for the report).
    pub digests_by_cluster: Vec<Vec<PlannedDigest>>,
}

/// The minimal row view inbound-link rewiring needs.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkIndexRow {
    pub id: String,
    pub related_memory_ids: Vec<String>,
}

/// v4 `PlanInput`.
pub struct PlanInput<'a> {
    pub character_id: &'a str,
    pub clusters: &'a [ResolvedClusterOutcome],
    /// Every row the character holds (both tiers), for inbound-link rewiring,
    /// in LOAD order.
    pub row_index: &'a OrderedMap<LinkIndexRow>,
    pub now_iso: &'a str,
    /// Id factory for new digests.
    pub new_id: &'a mut dyn FnMut() -> String,
}

// ============================================================================
// Field helpers
// ============================================================================

fn num(x: f64) -> Value {
    crate::db::js_number_to_json(x)
}

/// A string field, `None` for null / absent / non-string.
fn str_field(row: &Value, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_string)
}

/// A number field, `None` for null / absent (`?? fallback`).
fn num_field(row: &Value, key: &str) -> Option<f64> {
    row.get(key).and_then(Value::as_f64)
}

/// A string-array field (`?? []`).
fn list_field(row: &Value, key: &str) -> Vec<String> {
    row.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn row_id(row: &Value) -> String {
    str_field(row, "id").unwrap_or_default()
}

/// v4 `eventTimeIso`: `occurredAt ?? createdAt`.
fn event_time_iso(row: &Value) -> String {
    str_field(row, "occurredAt")
        .or_else(|| str_field(row, "createdAt"))
        .unwrap_or_default()
}

/// v4 `referenceTimeIso`: `lastReinforcedAt` when it parses later than
/// `createdAt`, else `createdAt`. `None` is the JS `null` a falsy
/// `lastReinforcedAt` can only leak through on a pre-1970 `createdAt`.
fn reference_time_iso(row: &Value) -> Option<String> {
    let created = str_field(row, "createdAt");
    let created_ms = created
        .as_deref()
        .and_then(js_date_parse_ms)
        .map(|v| v as f64);
    let last = str_field(row, "lastReinforcedAt");
    let reinforced_ms = match last.as_deref() {
        Some(s) if !s.is_empty() => js_date_parse_ms(s).map(|v| v as f64),
        _ => Some(0.0),
    };
    let later = matches!((reinforced_ms, created_ms), (Some(r), Some(c)) if r > c);
    if later {
        last
    } else {
        created
    }
}

/// v4 `sharedValue`: the value every row shares, or `None` when they disagree
/// or any is empty.
pub fn shared_value(values: &[Option<String>]) -> Option<String> {
    let first = values.first()?.clone()?;
    if first.is_empty() {
        return None;
    }
    if values.iter().all(|v| v.as_deref() == Some(first.as_str())) {
        Some(first)
    } else {
        None
    }
}

/// v4 `deriveWitnessedContext`: the members' shared value; otherwise
/// `'user_present'` when the user witnessed any of it; otherwise null.
pub fn derive_witnessed_context(values: &[Option<String>]) -> Option<String> {
    if let Some(Some(first)) = values.first() {
        if !first.is_empty() && values.iter().all(|v| v.as_deref() == Some(first.as_str())) {
            return Some(first.clone());
        }
    }
    if values.iter().any(|v| v.as_deref() == Some("user_present")) {
        return Some("user_present".to_string());
    }
    None
}

fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// v4 `clampDigestImportance`: clamp the model's importance to 0.2–1.0, then to
/// `[min, max]` of the members' reinforced importance (`?? importance`).
pub fn clamp_digest_importance(model_importance: f64, rows: &[&Value]) -> f64 {
    let banded = js_min(1.0, js_max(IMPORTANCE_FLOOR, model_importance));
    if rows.is_empty() {
        return banded;
    }
    let values: Vec<f64> = rows
        .iter()
        .map(|r| {
            num_field(r, "reinforcedImportance")
                .or_else(|| num_field(r, "importance"))
                .unwrap_or(f64::NAN)
        })
        .collect();
    let lo = values.iter().copied().fold(f64::INFINITY, js_min);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, js_max);
    js_min(hi, js_max(lo, banded))
}

/// v4 `majority`: the first value (in first-seen order) reaching the strictly
/// highest count, else the fallback.
fn majority<'s>(values: &[&'s str], fallback: &'s str) -> &'s str {
    let mut order: Vec<&str> = Vec::new();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for v in values {
        let c = counts.entry(v).or_insert(0);
        if *c == 0 {
            order.push(v);
        }
        *c += 1;
    }
    let mut best = fallback;
    let mut best_count = 0;
    for v in order {
        if counts[v] > best_count {
            best = v;
            best_count = counts[v];
        }
    }
    best
}

/// v4 `normalizeDigestKeywords`: the model's free words (vocabulary words
/// stripped, de-duplicated, capped) followed by exactly one value per
/// targeting axis — the model's when it gave one, else the members' majority
/// (episode digests default `temporal` to `past`). Same materialization as the
/// extractor: `temporal` and `context` bare, `scope` as `scope: <value>`, at
/// the end.
pub fn normalize_digest_keywords(
    model_keywords: &[String],
    member_keywords: &[Vec<String>],
    kind: &str,
) -> Vec<String> {
    let mut temporal: Option<TemporalTag> = None;
    let mut scope: Option<ScopeTag> = None;
    let mut context: Option<ContextTag> = None;
    let mut free: Vec<String> = Vec::new();
    for raw in model_keywords {
        let kw = crate::jsstr::js_trim(raw).to_lowercase();
        if kw.is_empty() {
            continue;
        }
        if let Some(rest) = kw.strip_prefix("scope:") {
            if let Some(s) = ScopeTag::from_kw(crate::jsstr::js_trim(rest)) {
                scope = Some(s);
            }
            continue;
        }
        if let Some(t) = TemporalTag::from_kw(&kw) {
            temporal = Some(t);
            continue;
        }
        if let Some(c) = ContextTag::from_kw(&kw) {
            context = Some(c);
            continue;
        }
        if !free.contains(&kw) {
            free.push(kw);
        }
    }

    let member_tags: Vec<_> = member_keywords
        .iter()
        .map(|k| parse_targeting_tags(k))
        .collect();
    let final_temporal = match temporal {
        Some(t) => t.as_str(),
        None if kind == "episodic" => "past",
        None => majority(
            &member_tags
                .iter()
                .map(|t| t.temporal.as_str())
                .collect::<Vec<_>>(),
            "present",
        ),
    };
    let final_scope = match scope {
        Some(s) => s.as_str(),
        None => majority(
            &member_tags
                .iter()
                .map(|t| t.scope.as_str())
                .collect::<Vec<_>>(),
            "wide",
        ),
    };
    let final_context = match context {
        Some(c) => c.as_str(),
        None => majority(
            &member_tags
                .iter()
                .map(|t| t.context.as_str())
                .collect::<Vec<_>>(),
            "information",
        ),
    };

    let mut out: Vec<String> = free.into_iter().take(MAX_FREE_KEYWORDS).collect();
    out.push(final_temporal.to_string());
    out.push(format!("scope: {final_scope}"));
    out.push(final_context.to_string());
    out
}

/// v4 `union`: de-duplicated by JS `toLowerCase()`, keeping the first spelling;
/// stops at `cap`.
fn union(lists: &[Vec<String>], cap: Option<usize>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for list in lists {
        for v in list {
            let key = v.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);
            out.push(v.clone());
            if let Some(c) = cap {
                if out.len() >= c {
                    return out;
                }
            }
        }
    }
    out
}

// ============================================================================
// The planner
// ============================================================================

/// Plan every write for one run. Pure: the same input always yields the same
/// plan (given a deterministic `new_id`).
pub fn plan_consolidation_writes(input: PlanInput<'_>) -> ConsolidationWritePlan {
    let PlanInput {
        character_id,
        clusters,
        row_index,
        now_iso,
        new_id,
    } = input;

    // ── Pass 1: give every digest an id and record who it supersedes. ────────
    let mut digest_ids: Vec<Vec<String>> = Vec::with_capacity(clusters.len());
    let mut superseded_by: OrderedMap<String> = OrderedMap::new();
    let mut digest_of_member: HashMap<String, String> = HashMap::new();
    let mut planned_digest_ids: HashSet<String> = HashSet::new();
    for cluster in clusters {
        let ids: Vec<String> = (0..cluster.digests.len())
            .map(|k| match (&cluster.existing_digest, k) {
                (Some(existing), 0) => row_id(existing),
                _ => new_id(),
            })
            .collect();
        for (k, digest) in cluster.digests.iter().enumerate() {
            planned_digest_ids.insert(ids[k].clone());
            for member_id in &digest.member_ids {
                superseded_by.set(member_id, ids[k].clone());
                digest_of_member.insert(member_id.clone(), ids[k].clone());
            }
        }
        digest_ids.push(ids);
    }

    // Contradictions: the older row goes cold under whatever carries the newer fact.
    for cluster in clusters {
        for c in &cluster.contradictions {
            let target = digest_of_member
                .get(&c.newer_id)
                .cloned()
                .unwrap_or_else(|| c.newer_id.clone());
            if target == c.older_id {
                continue;
            }
            superseded_by.set(&c.older_id, target);
        }
    }

    let remap = |id: &str| -> String {
        let mut current = id.to_string();
        for _ in 0..5 {
            match superseded_by.get(&current) {
                Some(next) if !next.is_empty() && *next != current => current = next.clone(),
                _ => break,
            }
        }
        current
    };

    let link_target_exists =
        |id: &str| -> bool { row_index.has(id) || planned_digest_ids.contains(id) };

    // ── Pass 2: build each digest. ───────────────────────────────────────────
    let mut creates: Vec<PlannedDigestCreate> = Vec::new();
    let mut updates: Vec<PlannedDigestUpdate> = Vec::new();
    let mut digests_by_cluster: Vec<Vec<PlannedDigest>> = Vec::new();

    for (c, cluster) in clusters.iter().enumerate() {
        let mut members_by_id: HashMap<String, &Value> = HashMap::new();
        for m in &cluster.members {
            members_by_id.insert(row_id(m), m);
        }
        let mut planned: Vec<PlannedDigest> = Vec::new();

        for (k, digest) in cluster.digests.iter().enumerate() {
            let id = digest_ids[c][k].clone();
            let is_update = k == 0 && cluster.existing_digest.is_some();
            let existing = if is_update {
                cluster.existing_digest.as_ref()
            } else {
                None
            };
            let member_rows: Vec<&Value> = digest
                .member_ids
                .iter()
                .filter_map(|mid| members_by_id.get(mid).copied())
                .collect();
            let rows: Vec<&Value> = match existing {
                Some(e) => std::iter::once(e)
                    .chain(member_rows.iter().copied())
                    .collect(),
                None => member_rows.clone(),
            };

            let kind = if cluster.cluster_kind == ClusterKind::Episodic {
                "episodic".to_string()
            } else {
                digest.kind.clone()
            };
            let importance = clamp_digest_importance(digest.importance, &rows);
            let mut count_sum = 0.0f64;
            for r in &rows {
                count_sum += num_field(r, "reinforcementCount").unwrap_or(1.0);
            }
            let reinforcement_count = js_min(DIGEST_REINFORCEMENT_CAP, js_max(1.0, count_sum));
            let reinforced_importance =
                calculate_reinforced_importance(importance, reinforcement_count);

            // "Outside the cluster" = outside this digest's own membership: a link
            // between two of its members remaps to the digest itself and drops; a
            // link to a sibling kept standalone, or to a row folded elsewhere this
            // run (re-aimed at that digest), survives.
            let related_memory_ids = union(
                &[rows
                    .iter()
                    .flat_map(|r| list_field(r, "relatedMemoryIds"))
                    .map(|rid| remap(&rid))
                    .filter(|rid| *rid != id && link_target_exists(rid))
                    .collect()],
                None,
            );

            let mut sorted_rows = rows.clone();
            sorted_rows.sort_by(|a, b| {
                crate::collation::locale_compare(&event_time_iso(a), &event_time_iso(b))
            });
            let earliest_row = sorted_rows.first().copied();
            let occurred_at = match (kind.as_str(), earliest_row) {
                ("episodic", Some(e)) => Some(event_time_iso(e)),
                _ => None,
            };
            let narrative_time = if kind == "episodic" {
                earliest_row.and_then(|e| str_field(e, "narrativeTime"))
            } else {
                None
            };
            let keywords = normalize_digest_keywords(
                &digest.keywords,
                &rows
                    .iter()
                    .map(|r| list_field(r, "keywords"))
                    .collect::<Vec<_>>(),
                &kind,
            );
            let entities = union(
                &rows
                    .iter()
                    .map(|r| list_field(r, "entities"))
                    .collect::<Vec<_>>(),
                Some(MAX_DIGEST_ENTITIES),
            );
            let tags = union(
                &rows
                    .iter()
                    .map(|r| list_field(r, "tags"))
                    .collect::<Vec<_>>(),
                None,
            );
            let chat_id = shared_value(
                &rows
                    .iter()
                    .map(|r| str_field(r, "chatId"))
                    .collect::<Vec<_>>(),
            );
            let project_id = shared_value(
                &rows
                    .iter()
                    .map(|r| str_field(r, "projectId"))
                    .collect::<Vec<_>>(),
            );
            let witnessed_context = derive_witnessed_context(
                &rows
                    .iter()
                    .map(|r| str_field(r, "witnessedContext"))
                    .collect::<Vec<_>>(),
            );
            let consolidated_from = union(
                &[
                    existing
                        .map(|e| list_field(e, "consolidatedFrom"))
                        .unwrap_or_default(),
                    digest.member_ids.clone(),
                ],
                None,
            );
            let last_reinforced_at: String = if is_update {
                now_iso.to_string()
            } else {
                // `memberRows.map(referenceTimeIso).sort().at(-1) ?? nowIso` — a
                // JS default sort stringifies (`null` → "null").
                let mut refs: Vec<Option<String>> =
                    member_rows.iter().map(|r| reference_time_iso(r)).collect();
                refs.sort_by(|a, b| {
                    let ka = a.as_deref().unwrap_or("null");
                    let kb = b.as_deref().unwrap_or("null");
                    ka.encode_utf16().cmp(kb.encode_utf16())
                });
                match refs.last() {
                    Some(Some(s)) => s.clone(),
                    _ => now_iso.to_string(),
                }
            };

            let base = PlannedDigest {
                id: id.clone(),
                action: if is_update {
                    DigestAction::Update
                } else {
                    DigestAction::Create
                },
                cluster_index: c,
                content: digest.content.clone(),
                summary: digest.summary.clone(),
                keywords: keywords.clone(),
                importance,
                reinforced_importance,
                reinforcement_count,
                kind: kind.clone(),
                occurred_at: occurred_at.clone(),
                narrative_time: narrative_time.clone(),
                entities: entities.clone(),
                member_ids: digest.member_ids.clone(),
                about_character_id: cluster.about_character_id.clone(),
                embedding: cluster.embeddings.get(k).cloned().flatten(),
            };
            planned.push(base.clone());

            if is_update {
                let patch = MemUpdate {
                    content: Some(digest.content.clone()),
                    summary: Some(digest.summary.clone()),
                    keywords: Some(keywords.clone()),
                    tags: Some(tags.clone()),
                    importance: Some(importance),
                    reinforcement_count: Some(reinforcement_count),
                    reinforced_importance: Some(reinforced_importance),
                    last_reinforced_at: Some(Some(last_reinforced_at.clone())),
                    related_memory_ids: Some(related_memory_ids.clone()),
                    consolidated_from: Some(consolidated_from.clone()),
                    consolidated_at: Some(Some(now_iso.to_string())),
                    occurred_at: Some(occurred_at.clone()),
                    narrative_time: Some(narrative_time.clone()),
                    entities: Some(entities.clone()),
                    chat_id: Some(chat_id.clone()),
                    project_id: Some(project_id.clone()),
                    witnessed_context: Some(witnessed_context.clone()),
                    ..Default::default()
                };
                let mut pj = Map::new();
                pj.insert("content".into(), json!(digest.content));
                pj.insert("summary".into(), json!(digest.summary));
                pj.insert("keywords".into(), json!(keywords));
                pj.insert("tags".into(), json!(tags));
                pj.insert("importance".into(), num(importance));
                pj.insert("reinforcementCount".into(), num(reinforcement_count));
                pj.insert("reinforcedImportance".into(), num(reinforced_importance));
                pj.insert("lastReinforcedAt".into(), json!(last_reinforced_at));
                pj.insert("relatedMemoryIds".into(), json!(related_memory_ids));
                pj.insert("consolidatedFrom".into(), json!(consolidated_from));
                pj.insert("consolidatedAt".into(), json!(now_iso));
                pj.insert("occurredAt".into(), json!(occurred_at));
                pj.insert("narrativeTime".into(), json!(narrative_time));
                pj.insert("entities".into(), json!(entities));
                pj.insert("chatId".into(), json!(chat_id));
                pj.insert("projectId".into(), json!(project_id));
                pj.insert("witnessedContext".into(), json!(witnessed_context));
                updates.push(PlannedDigestUpdate {
                    digest: base,
                    patch,
                    patch_json: Value::Object(pj),
                });
            } else {
                creates.push(PlannedDigestCreate {
                    digest: base,
                    data: MemCreate {
                        character_id: character_id.to_string(),
                        about_character_id: cluster.about_character_id.clone(),
                        chat_id,
                        project_id,
                        content: digest.content.clone(),
                        summary: digest.summary.clone(),
                        keywords,
                        tags,
                        importance,
                        embedding: None,
                        source: "CONSOLIDATED".to_string(),
                        witnessed_context,
                        occurred_at,
                        narrative_time,
                        entities,
                        kind,
                        source_message_id: None,
                        last_accessed_at: None,
                        reinforcement_count,
                        last_reinforced_at: Some(last_reinforced_at),
                        related_memory_ids,
                        reinforced_importance,
                        tier: "hot".to_string(),
                        superseded_by_id: None,
                        consolidated_from,
                        consolidated_at: Some(now_iso.to_string()),
                    },
                });
            }
        }
        digests_by_cluster.push(planned);
    }

    // ── Pass 3: tier moves, standalones, inbound links. ──────────────────────
    let mut moves: OrderedMap<Vec<String>> = OrderedMap::new();
    for (row_id, target) in superseded_by.iter() {
        if planned_digest_ids.contains(row_id) {
            continue; // a digest never goes cold here
        }
        let final_target = remap(target);
        match moves.get_mut(&final_target) {
            Some(list) => list.push(row_id.clone()),
            None => moves.set(&final_target, vec![row_id.clone()]),
        }
    }
    let tier_moves: Vec<TierMove> = moves
        .iter()
        .map(|(k, ids)| TierMove {
            superseded_by_id: k.clone(),
            ids: ids.clone(),
        })
        .collect();

    let considered = union(
        &clusters
            .iter()
            .map(|cl| {
                cl.keep_standalone
                    .iter()
                    .filter(|id| !superseded_by.has(id))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        None,
    );

    let mut link_rewrites: Vec<LinkRewrite> = Vec::new();
    for (_, row) in row_index.iter() {
        if superseded_by.has(&row.id) || planned_digest_ids.contains(&row.id) {
            continue;
        }
        let links = &row.related_memory_ids;
        if !links.iter().any(|l| superseded_by.has(l)) {
            continue;
        }
        let rewired = union(
            &[links
                .iter()
                .map(|l| remap(l))
                .filter(|l| *l != row.id)
                .collect()],
            None,
        );
        link_rewrites.push(LinkRewrite {
            id: row.id.clone(),
            related_memory_ids: rewired,
        });
    }

    ConsolidationWritePlan {
        creates,
        updates,
        tier_moves,
        considered,
        link_rewrites,
        superseded_by,
        digests_by_cluster,
    }
}
