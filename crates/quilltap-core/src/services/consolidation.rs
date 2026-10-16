//! Memory consolidation — the Commonplace Book's missing verb, as the
//! planner and the model route. v4 `lib/memory/consolidation.ts` at the
//! `83334b9a5` pin (`104e986c7` moved its loader out; bug 246 from
//! `64743ee13`; `tfidfClusterThreshold` from `4ae0c4ca6`).
//!
//! The book could append, reinforce, link, and delete; it could not *combine*.
//! This service folds clusters of a character's hot memories about one subject
//! into **digest** rows (`source: 'CONSOLIDATED'`) and sends the members to the
//! cold tier, superseded by the digest that replaced them
//! (docs/developer/features/complete/memory-consolidation-and-tiers.md §4).
//!
//! "This module is the planner and the model route; the maintenance job's
//! `consolidate` phase (`maintenance/consolidate.ts`) feeds it and announces
//! what it wrote." One call of [`consolidate`], per character:
//!
//!   1. **Buckets** arrive from the phase's loader
//!      ([`crate::services::memory_maintenance::consolidate`]): candidates are
//!      AUTO rows older than `matureAfterDays` with an embedding — hot rows,
//!      and cold rows demoted by the cap with no successor (`supersededById`
//!      null, bug 211) — bucketed by subject (self / another character / no
//!      subject), each with its hot digests as cluster seeds. MANUAL rows are
//!      never candidates.
//!   2. **Cluster** each bucket ([`crate::consolidation_clustering`], pure) and
//!      pick the qualifying clusters by total `reinforcedImportance`, up to
//!      `maxClustersPerRun`, at the cluster threshold for the default embedding
//!      profile's scale ([`resolve_cluster_threshold`]). Clusters below the
//!      size floor have their never-considered rows stamped `consolidatedAt` so
//!      they are not rescanned every run; they re-qualify when a new neighbour
//!      arrives.
//!   3. **Consolidate**: one model call per cluster
//!      ([`crate::consolidation_tasks`]), on
//!      `memoryConsolidation.connectionProfileId` or else the cheap LLM, with
//!      an output budget of `max(4000, the routed profile's)` (bug 246). A call
//!      that fails, times out, or answers out of schema skips its cluster —
//!      nothing is written for it, and its rows stay unconsidered so the next
//!      run retries them.
//!   4. **Plan, then write** ([`crate::consolidation_plan`], pure). The whole
//!      batch is computed before a single write is issued, then committed as
//!      ONE main-partition write (P4.D271 R-D — v4's job batch commits
//!      atomically). Digests are inserted directly with their embedding —
//!      never through admission, which would absorb a digest into its own
//!      members.
//!
//! "It announces nothing: the frozen-archive invalidation, the realtime hint
//! and the `Commonplace/` mirror are the maintenance run's effects, after
//! commit. A dry run does steps 1–3 and plans step 4, and writes nothing."
//!
//! Concierge: consolidation reads across chats, so it inherits the posture of
//! the chats its members came from. A Locked member chat wins (no reroute, no
//! failover — the operator said never); otherwise an Unmoderated one routes the
//! call to the uncensored desk; otherwise the global Moderated policy applies.
//!
//! **v5 shapes (P4.D271 rulings, as they stand after P4.D297):** R-D — the
//! plan is ONE `Db::write` closure, so its lines run on the writer thread;
//! R-F — the OTHER subject canon is the extraction pass's hand canon
//! (`HANDOFF(P4.D270)`; §S.5 (8) re-points it with P4.D296's canon loaders);
//! R-J — `now_ms` drives `startedAt`, the selection floor and the `nowIso`
//! stamps, while `finishedAt` / `durationMs` stay wall-clock and the time
//! budget reads an injectable monotonic clock. R-C's `in_job` and R-E's
//! runner slot are GONE (L-11 / R-N): the maintenance run's mode decides the
//! mirror and the announce, and the process-global runner is
//! [`crate::services::memory_maintenance::runner`]'s.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Map, Value};

use crate::canon::{render_other_canon_block, render_self_canon_block};
use crate::cheap_llm::{
    resolve_uncensored_cheap_llm_selection, selection_from_profile, CheapLlmProfile,
    CheapLlmSelection, UncensoredFallbackOptions,
};
use crate::clock;
use crate::consolidation_clustering::{
    cluster_bucket, select_clusters, AsCluster, Cluster, ClusterItem, ClusterKind, ClusterParams,
    SelectClustersOptions,
};
use crate::consolidation_plan::{
    plan_consolidation_writes, ConsolidationWritePlan, LinkIndexRow, OrderedMap, PlanInput,
    PlannedDigest, ResolvedClusterOutcome,
};
use crate::consolidation_tasks::{
    consolidate_memory_cluster, ConsolidationBucketKind, ConsolidationCallInput,
    ConsolidationContradiction, ConsolidationMemberInput, ConsolidationValidation,
};
use crate::db::instance_settings::{
    get_memory_consolidation_settings, MemoryConsolidationSettings,
};
use crate::db::memories::{CreateOptions, MemUpdate, TierExtra};
use crate::db::runtime::Db;
use crate::db::vector_store::Tier;
use crate::db::{characters_read, chats_read, DbError};
use crate::episodic::js_date_parse_ms;
use crate::memory_embedding_text::{
    embedding_text_for, lexical_embedding_text_for, AnchorEntities, EmbeddingTextSource,
};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::{EmbeddingCallOptions, EmbeddingProvider};
use crate::services::canon_loaders::load_canon_for_self_with_commonplace;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::dangerous_content::chat_override::{
    get_concierge_state, should_use_uncensored_route, ConciergeState,
};
use crate::services::dangerous_content::resolver::resolve_concierge_settings;
use crate::services::memory_maintenance::types::MaintenanceReason;

const MODULE: &str = "memory:consolidation";

// ============================================================================
// Public types
// ============================================================================

/// Which subject bucket a cluster belongs to (v4 `ConsolidationBucketRef`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationBucketRef {
    pub kind: ConsolidationBucketKind,
    /// The subject's character id (the holder's for self; `None` for none).
    pub subject_character_id: Option<String>,
    pub subject_name: String,
}

impl ConsolidationBucketRef {
    pub fn to_json(&self) -> Value {
        json!({
            "kind": self.kind.as_str(),
            "subjectCharacterId": self.subject_character_id,
            "subjectName": self.subject_name,
        })
    }
}

/// One digest as reported (v4 `ConsolidationDigestReport`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationDigestReport {
    pub id: String,
    pub action: &'static str,
    pub content: String,
    pub summary: String,
    pub keywords: Vec<String>,
    pub importance: f64,
    pub reinforced_importance: f64,
    pub reinforcement_count: f64,
    pub kind: String,
    pub occurred_at: Option<String>,
    pub member_ids: Vec<String>,
}

impl ConsolidationDigestReport {
    fn from_planned(d: &PlannedDigest) -> Self {
        Self {
            id: d.id.clone(),
            action: d.action.as_str(),
            content: d.content.clone(),
            summary: d.summary.clone(),
            keywords: d.keywords.clone(),
            importance: d.importance,
            reinforced_importance: d.reinforced_importance,
            reinforcement_count: d.reinforcement_count,
            kind: d.kind.clone(),
            occurred_at: d.occurred_at.clone(),
            member_ids: d.member_ids.clone(),
        }
    }

    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("id".into(), json!(self.id));
        m.insert("action".into(), json!(self.action));
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
        m.insert("memberIds".into(), json!(self.member_ids));
        Value::Object(m)
    }
}

/// One attempted (or dry-run) cluster (v4 `ConsolidationClusterReport`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationClusterReport {
    pub bucket: ConsolidationBucketRef,
    pub cluster_kind: ClusterKind,
    /// Non-digest member ids, oldest first.
    pub member_ids: Vec<String>,
    /// Member contents, same order as `member_ids`.
    pub member_contents: Vec<String>,
    pub existing_digest_id: Option<String>,
    pub existing_digest_content: Option<String>,
    pub digests: Vec<ConsolidationDigestReport>,
    pub keep_standalone: Vec<String>,
    pub contradictions: Vec<ConsolidationContradiction>,
    /// `planned` | `written` | `failed` | `invalid` | `embedding-failed`.
    pub status: &'static str,
    pub error: Option<String>,
}

impl ConsolidationClusterReport {
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("bucket".into(), self.bucket.to_json());
        m.insert("clusterKind".into(), json!(self.cluster_kind.as_str()));
        m.insert("memberIds".into(), json!(self.member_ids));
        m.insert("memberContents".into(), json!(self.member_contents));
        m.insert("existingDigestId".into(), json!(self.existing_digest_id));
        m.insert(
            "existingDigestContent".into(),
            json!(self.existing_digest_content),
        );
        m.insert(
            "digests".into(),
            Value::Array(
                self.digests
                    .iter()
                    .map(ConsolidationDigestReport::to_json)
                    .collect(),
            ),
        );
        m.insert("keepStandalone".into(), json!(self.keep_standalone));
        m.insert(
            "contradictions".into(),
            Value::Array(
                self.contradictions
                    .iter()
                    .map(ConsolidationContradiction::to_json)
                    .collect(),
            ),
        );
        m.insert("status".into(), json!(self.status));
        if let Some(e) = &self.error {
            m.insert("error".into(), json!(e));
        }
        Value::Object(m)
    }
}

/// v4 `ConsolidationStats` (key order = `createConsolidationStats()`; v4
/// exports the factory for the maintenance loader — here [`Default`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConsolidationStats {
    pub rows_loaded: usize,
    pub candidates: usize,
    /// Candidates that were cold — demoted by the cap, never superseded (bug
    /// 211).
    pub demoted_candidates: usize,
    pub digests_loaded: usize,
    pub immature_skipped: usize,
    pub no_embedding_skipped: usize,
    pub buckets_scanned: usize,
    pub bucket_rows_capped: usize,
    pub clusters_found: usize,
    pub clusters_qualified: usize,
    pub clusters_selected: usize,
    pub clusters_deferred: usize,
    pub clusters_below_min: usize,
    pub clusters_stale: usize,
    pub clusters_attempted: usize,
    pub clusters_succeeded: usize,
    pub clusters_failed: usize,
    pub clusters_lost_to_timeout: usize,
    pub digests_created: usize,
    pub digests_updated: usize,
    pub members_superseded: usize,
    pub contradictions_applied: usize,
    pub rows_marked_considered: usize,
    pub links_rewired: usize,
    /// True when the run stopped issuing calls because its time budget ran out.
    pub budget_exhausted: bool,
    pub duration_ms: i64,
}

impl ConsolidationStats {
    pub fn to_json(&self) -> Value {
        json!({
            "rowsLoaded": self.rows_loaded,
            "candidates": self.candidates,
            "demotedCandidates": self.demoted_candidates,
            "digestsLoaded": self.digests_loaded,
            "immatureSkipped": self.immature_skipped,
            "noEmbeddingSkipped": self.no_embedding_skipped,
            "bucketsScanned": self.buckets_scanned,
            "bucketRowsCapped": self.bucket_rows_capped,
            "clustersFound": self.clusters_found,
            "clustersQualified": self.clusters_qualified,
            "clustersSelected": self.clusters_selected,
            "clustersDeferred": self.clusters_deferred,
            "clustersBelowMin": self.clusters_below_min,
            "clustersStale": self.clusters_stale,
            "clustersAttempted": self.clusters_attempted,
            "clustersSucceeded": self.clusters_succeeded,
            "clustersFailed": self.clusters_failed,
            "clustersLostToTimeout": self.clusters_lost_to_timeout,
            "digestsCreated": self.digests_created,
            "digestsUpdated": self.digests_updated,
            "membersSuperseded": self.members_superseded,
            "contradictionsApplied": self.contradictions_applied,
            "rowsMarkedConsidered": self.rows_marked_considered,
            "linksRewired": self.links_rewired,
            "budgetExhausted": self.budget_exhausted,
            "durationMs": self.duration_ms,
        })
    }
}

/// v4 `ConsolidationReport`.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationReport {
    pub character_id: String,
    pub character_name: Option<String>,
    pub dry_run: bool,
    pub trigger: MaintenanceReason,
    pub started_at: String,
    pub finished_at: String,
    /// The settings the run actually used (stored settings + overrides, the
    /// cluster threshold RESOLVED for the profile's scale), as v4's spread
    /// object.
    pub settings: Value,
    pub clusters: Vec<ConsolidationClusterReport>,
    pub stats: ConsolidationStats,
    /// `no-llm` — the loader's `character-not-found` / `archived` skips are
    /// the maintenance run's now (`Maintenance skipped`).
    pub skipped_reason: Option<&'static str>,
}

impl ConsolidationReport {
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("characterId".into(), json!(self.character_id));
        m.insert("characterName".into(), json!(self.character_name));
        m.insert("dryRun".into(), json!(self.dry_run));
        m.insert("trigger".into(), json!(self.trigger.as_str()));
        m.insert("startedAt".into(), json!(self.started_at));
        m.insert("finishedAt".into(), json!(self.finished_at));
        m.insert("settings".into(), self.settings.clone());
        m.insert(
            "clusters".into(),
            Value::Array(
                self.clusters
                    .iter()
                    .map(ConsolidationClusterReport::to_json)
                    .collect(),
            ),
        );
        m.insert("stats".into(), self.stats.to_json());
        if let Some(r) = self.skipped_reason {
            m.insert("skippedReason".into(), json!(r));
        }
        Value::Object(m)
    }
}

pub use crate::services::memory_maintenance::types::ElapsedClock;

/// v4 `RunConsolidationOptions`, plus the injectable budget clock (P4.D271
/// R-J) and the host's zone fallback (the digest embeds read the memory
/// calendar — v4 `refreshMemoryTimeZone()`).
#[derive(Clone)]
pub struct RunConsolidationOptions {
    /// Cluster and call the model, but write nothing.
    pub dry_run: bool,
    /// Override `maxClustersPerRun`.
    pub max_clusters_per_run: Option<f64>,
    /// Override any stored setting for this run (v4's spread — a
    /// `Partial<MemoryConsolidationSettings>`).
    pub settings_override: Option<Map<String, Value>>,
    /// The user whose profiles and embedding provider to use; defaults to the
    /// character's owner.
    pub user_id: Option<String>,
    /// Stop issuing model calls after this long (ms).
    pub time_budget_ms: Option<i64>,
    /// Recorded in the report and logs (v4 `trigger ?? 'manual'`).
    pub trigger: MaintenanceReason,
    /// v4's `opts.now` clock override, in ms.
    pub now_ms: Option<i64>,
    /// R-J: elapsed ms since the call started; `None` → a real `Instant`.
    pub elapsed: Option<ElapsedClock>,
    /// The host's IANA zone (the memory calendar's last resort).
    pub host_zone_fallback: String,
}

impl Default for RunConsolidationOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            max_clusters_per_run: None,
            settings_override: None,
            user_id: None,
            time_budget_ms: None,
            trigger: MaintenanceReason::Manual,
            now_ms: None,
            elapsed: None,
            host_zone_fallback: "UTC".to_string(),
        }
    }
}

// ============================================================================
// Internals
// ============================================================================

fn num(x: f64) -> Value {
    crate::db::js_number_to_json(x)
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn num_field(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}

fn id_of(v: &Value) -> String {
    str_field(v, "id").unwrap_or_default()
}

/// JS `Date.parse` (NaN → `None`).
fn parse_ms(s: Option<&str>) -> Option<f64> {
    s.and_then(js_date_parse_ms).map(|v| v as f64)
}

/// The memory JSON's `{"0":v,…}` `Float32Array` object → the f32s, in index
/// order (each value is an f32 promoted exactly — R-H). `None` when absent or
/// empty.
fn embedding_of(memory: &Value) -> Option<Vec<f32>> {
    let obj = memory.get("embedding")?.as_object()?;
    if obj.is_empty() {
        return None;
    }
    let mut out = vec![0f32; obj.len()];
    for (k, v) in obj {
        let i: usize = k.parse().ok()?;
        *out.get_mut(i)? = v.as_f64()? as f32;
    }
    Some(out)
}

/// v4 `hasEmbedding(memory)` — a non-empty `Float32Array` (exported for the
/// maintenance loader).
pub fn has_embedding(memory: &Value) -> bool {
    memory
        .get("embedding")
        .and_then(Value::as_object)
        .is_some_and(|o| !o.is_empty())
}

/// v4 `eventTimeMs`: `occurredAt ?? createdAt`, NaN → `None`.
pub fn event_time_ms(memory: &Value) -> Option<f64> {
    let iso = str_field(memory, "occurredAt").or_else(|| str_field(memory, "createdAt"));
    parse_ms(iso.as_deref())
}

fn to_cluster_item(memory: &Value, embedding: Vec<f32>) -> ClusterItem {
    let created = parse_ms(str_field(memory, "createdAt").as_deref());
    let considered = str_field(memory, "consolidatedAt")
        .filter(|s| !s.is_empty())
        .and_then(|s| parse_ms(Some(&s)));
    ClusterItem {
        id: id_of(memory),
        embedding,
        kind: if str_field(memory, "kind").as_deref() == Some("episodic") {
            ClusterKind::Episodic
        } else {
            ClusterKind::Semantic
        },
        is_digest: str_field(memory, "source").as_deref() == Some("CONSOLIDATED"),
        event_time_ms: event_time_ms(memory),
        created_at_ms: created.unwrap_or(0.0),
        considered_at_ms: considered,
        weight: num_field(memory, "reinforcedImportance")
            .or_else(|| num_field(memory, "importance"))
            .unwrap_or(0.5),
    }
}

/// v4 `ConsolidationBucket` — one subject bucket: its candidate rows and the
/// hot digests that seed its clusters.
#[derive(Clone, Debug, PartialEq)]
pub struct ConsolidationBucket {
    pub key: String,
    pub reference: ConsolidationBucketRef,
    /// `aboutCharacterId` digests in this bucket carry.
    pub about_character_id: Option<String>,
    pub rows: Vec<Value>,
    pub digests: Vec<Value>,
}

#[derive(Clone)]
struct TaggedCluster {
    cluster: Cluster,
    bucket_key: String,
}

impl AsCluster for TaggedCluster {
    fn as_cluster(&self) -> &Cluster {
        &self.cluster
    }
}

struct ModelRoute {
    base: CheapLlmSelection,
    available_profiles: Vec<CheapLlmProfile>,
    chat_settings: Option<Value>,
}

/// The run's settings: the stored object with the override spread over it
/// (v4 `{...stored, ...opts.settings}` — stored keys keep their places, new
/// keys append).
fn merge_settings(
    stored: &MemoryConsolidationSettings,
    overrides: Option<&Map<String, Value>>,
) -> Value {
    let mut merged = stored.to_json();
    if let (Some(o), Value::Object(m)) = (overrides, &mut merged) {
        for (k, v) in o {
            m.insert(k.clone(), v.clone());
        }
    }
    merged
}

/// v4 `resolveClusterThreshold(settings, provider, override)`
/// (`settings-defaults.ts:82-89`, `4ae0c4ca6`): "the cosine at which
/// consolidation joins two memories, for vectors made by `provider`:
/// `tfidfClusterThreshold` for the Built-in (TF-IDF) profile,
/// `clusterThreshold` otherwise. An explicit per-run `override` (the API's
/// dry-run knob) wins on either provider." `override_` is v4's `typeof
/// override === 'number'` arm.
pub fn resolve_cluster_threshold(
    cluster_threshold: f64,
    tfidf_cluster_threshold: f64,
    provider: Option<&str>,
    override_: Option<f64>,
) -> f64 {
    if let Some(o) = override_ {
        return o;
    }
    if crate::embedding_scale::is_lexical_embedding_provider(provider) {
        tfidf_cluster_threshold
    } else {
        cluster_threshold
    }
}

struct RunSettings {
    connection_profile_id: Option<String>,
    cluster_threshold: f64,
    min_cluster_size: f64,
    max_cluster_size: f64,
    mature_after_days: f64,
    max_clusters_per_run: f64,
}

fn run_settings(merged: &Value) -> RunSettings {
    let n = |k: &str| merged.get(k).and_then(Value::as_f64).unwrap_or(f64::NAN);
    RunSettings {
        connection_profile_id: str_field(merged, "connectionProfileId").filter(|s| !s.is_empty()),
        cluster_threshold: n("clusterThreshold"),
        min_cluster_size: n("minClusterSize"),
        max_cluster_size: n("maxClusterSize"),
        mature_after_days: n("matureAfterDays"),
        max_clusters_per_run: n("maxClustersPerRun"),
    }
}

/// v4 `resolveModelRoute`: the configured consolidation profile, else the
/// user's cheap LLM.
fn resolve_model_route(db: &Db, settings: &RunSettings, user_id: &str) -> Option<ModelRoute> {
    let uid = user_id.to_string();
    let chat_settings = db
        .read_main(|c| crate::db::chat_settings::find_by_user_id(c, &uid))
        .ok()
        .flatten();
    let profiles: Vec<Value> = db
        .read_main(|c| crate::db::connection_profiles::find_by_user_id(c, &uid))
        .unwrap_or_default();
    let available_profiles: Vec<CheapLlmProfile> = profiles
        .iter()
        .map(crate::services::image_job_common::cheap_llm_profile_from_value)
        .collect();

    if let Some(configured) = &settings.connection_profile_id {
        if let Some(profile) = available_profiles.iter().find(|p| &p.id == configured) {
            tracing::debug!(
                module = MODULE,
                connectionProfileId = %profile.id,
                provider = %profile.provider,
                model = %profile.model_name,
                "Consolidation using its configured connection profile"
            );
            return Some(ModelRoute {
                base: selection_from_profile(profile, true),
                available_profiles,
                chat_settings,
            });
        }
        tracing::warn!(
            module = MODULE,
            connectionProfileId = %configured,
            "Configured consolidation profile no longer exists; falling back to the cheap LLM"
        );
    }

    let cheap_settings = chat_settings
        .as_ref()
        .and_then(|s| s.get("cheapLLMSettings"));
    let base =
        crate::services::image_job_common::build_cheap_llm_selection(&profiles, cheap_settings)?;
    Some(ModelRoute {
        base,
        available_profiles,
        chat_settings,
    })
}

struct RoutedCall {
    selection: CheapLlmSelection,
    policy: crate::services::dangerous_content::resolver::ResolvedConciergePolicy,
    failover: bool,
    dangerous: bool,
    policy_chat_id: Option<String>,
}

/// v4 `profileMaxTokensFor(route, selection)` (bug 246): "the output budget
/// of the profile a cluster's call was routed to, as extraction reads it
/// (`resolveMaxTokens`). Undefined when the selection names no known profile;
/// the call then keeps the consolidation floor."
fn profile_max_tokens_for(route: &ModelRoute, selection: &CheapLlmSelection) -> Option<i64> {
    let id = selection.connection_profile_id.as_deref()?;
    let profile = route.available_profiles.iter().find(|p| p.id == id)?;
    Some(crate::context_budget::resolve_max_tokens(
        profile.max_tokens.map(|v| v as i64),
        profile.model_class.as_deref(),
    ))
}

/// v4 `routeForCluster`: route one cluster's call under the Concierge posture
/// of its members' chats (a per-run chat cache).
fn route_for_cluster(
    db: &Db,
    route: &ModelRoute,
    members: &[&Value],
    chat_cache: &mut HashMap<String, Option<Value>>,
) -> RoutedCall {
    let mut chat_ids: Vec<String> = Vec::new();
    for m in members {
        if let Some(id) = str_field(m, "chatId").filter(|s| !s.is_empty()) {
            if !chat_ids.contains(&id) {
                chat_ids.push(id);
            }
        }
    }
    let mut locked: Option<Value> = None;
    let mut unmoderated: Option<Value> = None;
    for chat_id in &chat_ids {
        let chat = match chat_cache.get(chat_id) {
            Some(c) => c.clone(),
            None => {
                let id = chat_id.clone();
                let read = db
                    .read_main(|c| Ok(chats_read::find_by_id_or_none(c, &id)))
                    .ok()
                    .flatten();
                chat_cache.insert(chat_id.clone(), read.clone());
                read
            }
        };
        let Some(chat) = chat else { continue };
        let state = get_concierge_state(Some(&chat));
        if state == ConciergeState::Locked && locked.is_none() {
            locked = Some(chat.clone());
        }
        if state == ConciergeState::Unmoderated && unmoderated.is_none() {
            unmoderated = Some(chat.clone());
        }
    }
    let policy_chat = locked.or(unmoderated);
    let policy = resolve_concierge_settings(route.chat_settings.as_ref(), policy_chat.as_ref());
    let dangerous = policy_chat.is_some() && should_use_uncensored_route(policy_chat.as_ref());
    let selection = if dangerous {
        resolve_uncensored_cheap_llm_selection(
            route.base.clone(),
            true,
            Some(&policy),
            &route.available_profiles,
        )
    } else {
        route.base.clone()
    };
    RoutedCall {
        selection,
        failover: policy.failover_allowed,
        policy,
        dangerous,
        policy_chat_id: policy_chat.as_ref().map(id_of),
    }
}

/// v4 `findById` (the overlay read) — `None` on absence AND on a throw.
fn character_overlay(db: &Db, id: &str) -> Option<Value> {
    let id = id.to_string();
    db.read_main(|main| db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &id)))
        .ok()
        .flatten()
}

/// v4 `describeBuckets`: fill in subject names and canon blocks for the buckets
/// that will be consolidated.
fn describe_buckets(
    db: &Db,
    holder: &Value,
    buckets: &mut [ConsolidationBucket],
    keys: &HashSet<String>,
) -> HashMap<String, Option<String>> {
    let mut canon: HashMap<String, Option<String>> = HashMap::new();
    let holder_id = id_of(holder);
    let holder_name = str_field(holder, "name").unwrap_or_default();
    for bucket in buckets.iter_mut() {
        if !keys.contains(&bucket.key) {
            continue;
        }
        match bucket.reference.kind {
            ConsolidationBucketKind::SelfBucket => {
                bucket.reference.subject_name = holder_name.clone();
                // "The card fields plus `Commonplace/Self.md`, as the
                // extractor reads them" (v4 `loadCanonForSelfWithCommonplace`,
                // `104e986c7`).
                let block = render_self_canon_block(&load_canon_for_self_with_commonplace(
                    db,
                    &holder_id,
                    &holder_name,
                    holder.get("manifesto").and_then(Value::as_str),
                    holder.get("personality").and_then(Value::as_str),
                    holder.get("description").and_then(Value::as_str),
                    holder.get("identity").and_then(Value::as_str),
                    holder
                        .get("characterDocumentMountPointId")
                        .and_then(Value::as_str),
                ));
                canon.insert(bucket.key.clone(), Some(block));
            }
            ConsolidationBucketKind::None => {
                bucket.reference.subject_name = "(no subject)".to_string();
                canon.insert(bucket.key.clone(), None);
            }
            ConsolidationBucketKind::Other => {
                let subject_id = bucket
                    .reference
                    .subject_character_id
                    .clone()
                    .unwrap_or_default();
                // The overlay read carries the vault-managed identity/description; a
                // broken vault throws, so fall back to the raw row for name-only canon.
                let subject = character_overlay(db, &subject_id).or_else(|| {
                    let sid = subject_id.clone();
                    db.read_main(|c| Ok(characters_read::find_by_id_raw_or_none(c, &sid)))
                        .ok()
                        .flatten()
                });
                let Some(subject) = subject else {
                    bucket.reference.subject_name = "an unnamed acquaintance".to_string();
                    canon.insert(bucket.key.clone(), None);
                    continue;
                };
                let subject_name = str_field(&subject, "name").unwrap_or_default();
                bucket.reference.subject_name = subject_name.clone();
                // The cluster is handed its digest directly; the mirror would only echo it.
                // R-F: the OTHER subject canon with `includeCommonplace: false`
                // — EXACTLY the extraction pass's hand canon (P4.D270's `pub`
                // loader; the cluster is handed its digest directly).
                let source = crate::services::canon_loaders::load_canon_for_observer_about_subject(
                    db,
                    holder
                        .get("characterDocumentMountPointId")
                        .and_then(Value::as_str),
                    &holder_id,
                    &crate::services::canon_loaders::CanonSubject {
                        id: id_of(&subject),
                        name: subject_name.clone(),
                        identity: subject
                            .get("identity")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        description: subject
                            .get("description")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    },
                    false,
                );
                canon.insert(bucket.key.clone(), Some(render_other_canon_block(&source)));
            }
        }
    }
    canon
}

/// R-D: v4 `executePlan` as ONE main-partition write. Lines inside run on the
/// writer thread.
async fn execute_plan(
    db: &Db,
    character_id: &str,
    plan: &ConsolidationWritePlan,
    extra_considered: &[String],
    now_iso: &str,
) -> Result<(), DbError> {
    struct CreateJob {
        id: String,
        data: crate::db::memories::MemCreate,
        embedding: Option<Vec<f32>>,
        members: usize,
    }
    let creates: Vec<CreateJob> = plan
        .creates
        .iter()
        .map(|c| CreateJob {
            id: c.digest.id.clone(),
            data: clone_mem_create(&c.data),
            embedding: c.digest.embedding.clone(),
            members: c.digest.member_ids.len(),
        })
        .collect();
    let updates: Vec<(String, MemUpdate, Option<Vec<f32>>, usize)> = plan
        .updates
        .iter()
        .map(|u| {
            let mut patch = clone_mem_update(&u.patch);
            if let Some(e) = &u.digest.embedding {
                patch.embedding = Some(Some(e.clone()));
            }
            (
                u.digest.id.clone(),
                patch,
                u.digest.embedding.clone(),
                u.digest.member_ids.len(),
            )
        })
        .collect();
    let moves: Vec<(String, Vec<String>)> = plan
        .tier_moves
        .iter()
        .map(|m| (m.superseded_by_id.clone(), m.ids.clone()))
        .collect();
    let mut considered: Vec<String> = Vec::new();
    for id in plan.considered.iter().chain(extra_considered.iter()) {
        if !considered.contains(id) {
            considered.push(id.clone());
        }
    }
    let rewrites: Vec<(String, Vec<String>)> = plan
        .link_rewrites
        .iter()
        .map(|r| (r.id.clone(), r.related_memory_ids.clone()))
        .collect();
    let character_id = character_id.to_string();
    let now_iso = now_iso.to_string();

    db.write(move |writers| {
        let main = writers.main();
        for c in &creates {
            // "The vector rides in the create itself, so a digest never exists
            // without it; the index then caches it without a second write."
            let mut data = clone_mem_create(&c.data);
            if let Some(e) = &c.embedding {
                data.embedding = Some(e.clone());
            }
            main.memories().create(
                &data,
                &CreateOptions {
                    id: c.id.clone(),
                    created_at: now_iso.clone(),
                    updated_at: now_iso.clone(),
                },
            )?;
            if let Some(e) = &c.embedding {
                crate::services::memory_index::index_inserted(
                    &character_id,
                    &[crate::services::memory_index::MemoryIndexEntry {
                        id: c.id.clone(),
                        embedding: e.clone(),
                        tier: Some(Tier::Hot),
                    }],
                );
            }
            tracing::debug!(
                module = MODULE,
                characterId = %character_id,
                memoryId = %c.id,
                members = c.members,
                "Digest created"
            );
        }

        for (id, patch, embedding, members) in &updates {
            // "The revised text and its vector land in one row update."
            match embedding {
                Some(e) => {
                    crate::services::memory_index::upsert(
                        main.connection(),
                        &character_id,
                        vec![crate::services::memory_index::MemoryIndexUpsert {
                            id: id.clone(),
                            embedding: e.clone(),
                            tier: Some(Tier::Hot),
                            alongside: clone_mem_update(patch),
                        }],
                    )?;
                }
                None => {
                    main.memories()
                        .update_for_character(&character_id, id, patch)?;
                }
            }
            tracing::debug!(
                module = MODULE,
                characterId = %character_id,
                memoryId = %id,
                newMembers = *members,
                "Digest revised in place"
            );
        }

        for (target, ids) in &moves {
            crate::services::memory_index::set_tier(
                main.connection(),
                &character_id,
                ids,
                Tier::Cold,
                TierExtra {
                    superseded_by_id: Some(Some(target.clone())),
                    consolidated_at: Some(Some(now_iso.clone())),
                },
            )?;
        }

        if !considered.is_empty() {
            main.memories()
                .mark_considered(&character_id, &considered, &now_iso)?;
        }

        for (id, links) in &rewrites {
            let patch = MemUpdate {
                related_memory_ids: Some(links.clone()),
                ..Default::default()
            };
            main.memories()
                .update_for_character(&character_id, id, &patch)?;
        }
        Ok(())
    })
    .await
}

fn clone_mem_create(d: &crate::db::memories::MemCreate) -> crate::db::memories::MemCreate {
    crate::db::memories::MemCreate {
        character_id: d.character_id.clone(),
        about_character_id: d.about_character_id.clone(),
        chat_id: d.chat_id.clone(),
        project_id: d.project_id.clone(),
        content: d.content.clone(),
        summary: d.summary.clone(),
        keywords: d.keywords.clone(),
        tags: d.tags.clone(),
        importance: d.importance,
        embedding: d.embedding.clone(),
        source: d.source.clone(),
        witnessed_context: d.witnessed_context.clone(),
        occurred_at: d.occurred_at.clone(),
        narrative_time: d.narrative_time.clone(),
        entities: d.entities.clone(),
        kind: d.kind.clone(),
        source_message_id: d.source_message_id.clone(),
        last_accessed_at: d.last_accessed_at.clone(),
        reinforcement_count: d.reinforcement_count,
        last_reinforced_at: d.last_reinforced_at.clone(),
        related_memory_ids: d.related_memory_ids.clone(),
        reinforced_importance: d.reinforced_importance,
        tier: d.tier.clone(),
        superseded_by_id: d.superseded_by_id.clone(),
        superseded_at: d.superseded_at.clone(),
        consolidated_from: d.consolidated_from.clone(),
        consolidated_at: d.consolidated_at.clone(),
    }
}

fn clone_mem_update(p: &MemUpdate) -> MemUpdate {
    MemUpdate {
        content: p.content.clone(),
        summary: p.summary.clone(),
        keywords: p.keywords.clone(),
        tags: p.tags.clone(),
        related_memory_ids: p.related_memory_ids.clone(),
        importance: p.importance,
        source: p.source.clone(),
        reinforcement_count: p.reinforcement_count,
        reinforced_importance: p.reinforced_importance,
        about_character_id: p.about_character_id.clone(),
        chat_id: p.chat_id.clone(),
        project_id: p.project_id.clone(),
        witnessed_context: p.witnessed_context.clone(),
        occurred_at: p.occurred_at.clone(),
        narrative_time: p.narrative_time.clone(),
        entities: p.entities.clone(),
        kind: p.kind.clone(),
        source_message_id: p.source_message_id.clone(),
        last_accessed_at: p.last_accessed_at.clone(),
        last_reinforced_at: p.last_reinforced_at.clone(),
        embedding: p.embedding.clone(),
        tier: p.tier.clone(),
        superseded_by_id: p.superseded_by_id.clone(),
        superseded_at: p.superseded_at.clone(),
        consolidated_from: p.consolidated_from.clone(),
        consolidated_at: p.consolidated_at.clone(),
        updated_at: p.updated_at.clone(),
    }
}

/// v4 `touchedSubjects(buckets, plan, bucketKeyByOutcome)`: "the digest
/// subjects a plan touches — the buckets whose Commonplace files the run
/// changes. The no-subject bucket has no mirror and is left out." Deduped,
/// first-seen (creates, then updates).
fn touched_subjects(
    buckets: &[ConsolidationBucket],
    plan: &ConsolidationWritePlan,
    bucket_key_by_outcome: &[String],
) -> Vec<String> {
    let mut subjects: Vec<String> = Vec::new();
    for d in plan
        .creates
        .iter()
        .map(|c| &c.digest)
        .chain(plan.updates.iter().map(|u| &u.digest))
    {
        let key = &bucket_key_by_outcome[d.cluster_index];
        let about = buckets
            .iter()
            .find(|b| &b.key == key)
            .and_then(|b| b.about_character_id.clone())
            .filter(|a| !a.is_empty());
        if let Some(about) = about {
            if !subjects.contains(&about) {
                subjects.push(about);
            }
        }
    }
    subjects
}

/// Render an optional string as v4's JSON-ish log value (`null` when absent).
fn or_null(v: Option<&str>) -> String {
    v.map(str::to_string).unwrap_or_else(|| "null".to_string())
}

// ============================================================================
// Entry point
// ============================================================================

/// v4 `ConsolidationInput` — what [`consolidate`] is handed: the holder and
/// its bucketed corpus.
pub struct ConsolidationInput {
    /// The holder, with its vault overlay when it could be read.
    pub holder: Value,
    /// Every row the holder has (links only), for the link rewrites, in load
    /// order.
    pub row_index: OrderedMap<LinkIndexRow>,
    /// The subject buckets, in first-seen order.
    pub buckets: Vec<ConsolidationBucket>,
    /// Stats seeded by the loader (rows, candidates, skips).
    pub stats: ConsolidationStats,
}

/// v4 `ConsolidationRun`.
pub struct ConsolidationRun {
    pub report: ConsolidationReport,
    /// The writes the run issued, or would have issued in a dry run.
    pub plan: ConsolidationWritePlan,
    /// Rows of clusters too small to fold, stamped considered with the plan.
    pub below_min_considered: Vec<String>,
    /// Digest subjects whose Commonplace files the run changes.
    pub touched_subjects: Vec<String>,
}

/// v4 `consolidate(input, opts)`: "consolidate one character's bucketed
/// candidates: cluster, call the model per cluster, plan, embed the digests,
/// and (unless a dry run) issue the writes. It announces nothing — the
/// maintenance run does that after the writes commit. Never throws for an
/// ordinary failure (a bad call, a missing vault)." Fails only when the plan
/// cannot be committed.
pub async fn consolidate<C: CompletionProvider, E: EmbeddingProvider>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    input: ConsolidationInput,
    opts: RunConsolidationOptions,
) -> Result<ConsolidationRun, DbError> {
    let started = Instant::now();
    let elapsed: ElapsedClock = opts
        .elapsed
        .clone()
        .unwrap_or_else(|| Arc::new(move || started.elapsed().as_millis() as i64));
    let now_ms = opts.now_ms.unwrap_or_else(clock::now_unix_ms);
    let now_iso = clock::iso_from_unix_ms(now_ms);
    let dry_run = opts.dry_run;
    let trigger = opts.trigger;
    let stored = db
        .read_main(|c| Ok(get_memory_consolidation_settings(c)))
        .unwrap_or_default();
    let mut merged_settings = merge_settings(&stored, opts.settings_override.as_ref());
    let ConsolidationInput {
        holder,
        row_index,
        mut buckets,
        mut stats,
    } = input;
    let character_id = id_of(&holder);
    let character_id = character_id.as_str();
    let holder_name = str_field(&holder, "name");
    let user_id = opts
        .user_id
        .clone()
        .or_else(|| str_field(&holder, "userId"))
        .unwrap_or_default();
    let mut cluster_reports: Vec<ConsolidationClusterReport> = Vec::new();

    // "The clustered vectors are the default profile's; cluster on its
    // scale." v4's `findDefault` is a safeQuery over a table its repository
    // creates on first access, so a failed read answers `null` there and the
    // caller's catch (DEBUG `Could not read the default embedding profile;
    // clustering on the neural scale`) is unreachable through v4's real code —
    // v5's `Err` is that `null`, with no line of this module's.
    let uid = user_id.clone();
    let embedding_provider: Option<String> = db
        .read_main(|c| crate::db::embedding_profiles::find_default(c, &uid))
        .ok()
        .flatten()
        .map(|p| p.provider);
    let override_threshold = opts
        .settings_override
        .as_ref()
        .and_then(|o| o.get("clusterThreshold"))
        .and_then(Value::as_f64);
    let resolved_threshold = resolve_cluster_threshold(
        merged_settings
            .get("clusterThreshold")
            .and_then(Value::as_f64)
            .unwrap_or(f64::NAN),
        merged_settings
            .get("tfidfClusterThreshold")
            .and_then(Value::as_f64)
            .unwrap_or(f64::NAN),
        embedding_provider.as_deref(),
        override_threshold,
    );
    if let Value::Object(m) = &mut merged_settings {
        m.insert("clusterThreshold".into(), num(resolved_threshold));
    }
    let settings = run_settings(&merged_settings);
    let max_clusters = opts
        .max_clusters_per_run
        .unwrap_or(settings.max_clusters_per_run);

    let finish = |stats: &mut ConsolidationStats,
                  clusters: Vec<ConsolidationClusterReport>,
                  skipped_reason: Option<&'static str>| {
        stats.duration_ms = started.elapsed().as_millis() as i64;
        ConsolidationReport {
            character_id: character_id.to_string(),
            character_name: holder_name.clone(),
            dry_run,
            trigger,
            started_at: now_iso.clone(),
            finished_at: clock::now_iso(),
            settings: merged_settings.clone(),
            clusters,
            stats: stats.clone(),
            skipped_reason,
        }
    };

    tracing::debug!(
        module = MODULE,
        characterId = %character_id,
        trigger = trigger.as_str(),
        dryRun = dry_run,
        maxClusters = max_clusters,
        clusterThreshold = settings.cluster_threshold,
        embeddingProvider = %or_null(embedding_provider.as_deref()),
        minClusterSize = settings.min_cluster_size,
        maxClusterSize = settings.max_cluster_size,
        matureAfterDays = settings.mature_after_days,
        timeBudgetMs = %opts.time_budget_ms.map(|b| b.to_string()).unwrap_or_else(|| "null".to_string()),
        "Consolidation starting"
    );

    let mut rows_by_id: HashMap<String, Value> = HashMap::new();
    let mut digests_by_id: HashMap<String, Value> = HashMap::new();
    for bucket in &buckets {
        for row in &bucket.rows {
            rows_by_id.insert(id_of(row), row.clone());
        }
        for digest in &bucket.digests {
            digests_by_id.insert(id_of(digest), digest.clone());
        }
    }

    // ── 2. Cluster ────────────────────────────────────────────────────────────
    let mut items_by_id: HashMap<String, ClusterItem> = HashMap::new();
    let mut all_clusters: Vec<TaggedCluster> = Vec::new();
    for bucket in &buckets {
        if bucket.rows.is_empty() {
            continue;
        }
        stats.buckets_scanned += 1;
        let items: Vec<ClusterItem> = bucket
            .digests
            .iter()
            .chain(bucket.rows.iter())
            .filter_map(|m| embedding_of(m).map(|e| to_cluster_item(m, e)))
            .collect();
        for item in &items {
            items_by_id.insert(item.id.clone(), item.clone());
        }
        let clusters = cluster_bucket(
            &items,
            &ClusterParams {
                threshold: settings.cluster_threshold,
                max_cluster_size: settings.max_cluster_size,
                episodic_window_ms: None,
            },
        );
        let found = clusters.len();
        for cluster in clusters {
            all_clusters.push(TaggedCluster {
                cluster,
                bucket_key: bucket.key.clone(),
            });
        }
        tracing::debug!(
            module = MODULE,
            characterId = %character_id,
            bucket = %bucket.key,
            rows = bucket.rows.len(),
            digests = bucket.digests.len(),
            clusters = found,
            "Bucket clustered"
        );
    }
    stats.clusters_found = all_clusters.len();

    let selection = select_clusters(
        &all_clusters,
        &items_by_id,
        &SelectClustersOptions {
            min_cluster_size: settings.min_cluster_size,
            mature_after_days: settings.mature_after_days,
            max_clusters,
            now_ms: now_ms as f64,
        },
    );
    stats.clusters_qualified = selection.selected.len() + selection.deferred.len();
    stats.clusters_selected = selection.selected.len();
    stats.clusters_deferred = selection.deferred.len();
    stats.clusters_below_min = selection.below_min.len();
    stats.clusters_stale = selection.stale.len();

    // Rows in clusters too small to fold: stamp them so they are not rescanned.
    let below_min_considered: Vec<String> = selection
        .below_min
        .iter()
        .flat_map(|c| {
            c.cluster
                .member_ids
                .iter()
                .filter(|id| {
                    items_by_id
                        .get(*id)
                        .is_some_and(|i| i.considered_at_ms.is_none())
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect();

    // ── 3. Consolidate ────────────────────────────────────────────────────────
    let mut outcomes: Vec<ResolvedClusterOutcome> = Vec::new();
    let mut outcome_report_index: Vec<usize> = Vec::new();
    let mut outcome_bucket_key: Vec<String> = Vec::new();
    let mut route: Option<ModelRoute> = None;
    if !selection.selected.is_empty() {
        route = resolve_model_route(db, &settings, &user_id);
        if route.is_none() {
            tracing::warn!(
                module = MODULE,
                characterId = %character_id,
                "Consolidation has clusters to fold but no model to fold them with"
            );
        }
    }

    let canon_by_bucket: HashMap<String, Option<String>> = if route.is_some() {
        let keys: HashSet<String> = selection
            .selected
            .iter()
            .map(|c| c.bucket_key.clone())
            .collect();
        describe_buckets(db, &holder, &mut buckets, &keys)
    } else {
        HashMap::new()
    };
    let mut chat_cache: HashMap<String, Option<Value>> = HashMap::new();

    if let Some(route) = &route {
        for (i, cluster) in selection.selected.iter().enumerate() {
            if let Some(budget) = opts.time_budget_ms {
                if elapsed() > budget {
                    let remaining = selection.selected.len() - i;
                    stats.budget_exhausted = true;
                    stats.clusters_deferred += remaining;
                    tracing::info!(
                        module = MODULE,
                        characterId = %character_id,
                        remaining,
                        timeBudgetMs = budget,
                        "Consolidation time budget spent; remaining clusters wait for the next run"
                    );
                    break;
                }
            }
            let bucket = buckets
                .iter()
                .find(|b| b.key == cluster.bucket_key)
                .expect("a selected cluster's bucket");
            let mut members: Vec<&Value> = cluster
                .cluster
                .member_ids
                .iter()
                .filter_map(|id| rows_by_id.get(id))
                .collect();
            members.sort_by(|a, b| {
                let d = event_time_ms(a).unwrap_or(0.0) - event_time_ms(b).unwrap_or(0.0);
                d.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal)
            });
            let existing_digest: Option<&Value> = cluster
                .cluster
                .digest_id
                .as_ref()
                .and_then(|id| digests_by_id.get(id));
            let mut handle_to_id: HashMap<String, String> = HashMap::new();
            let call_input = ConsolidationCallInput {
                holder_name: holder_name.clone().unwrap_or_default(),
                subject_name: bucket.reference.subject_name.clone(),
                bucket: bucket.reference.kind,
                cluster_kind: cluster.cluster.kind.as_str().to_string(),
                canon_block: canon_by_bucket.get(&bucket.key).cloned().flatten(),
                existing_digest: existing_digest.and_then(|d| str_field(d, "content")),
                members: members
                    .iter()
                    .enumerate()
                    .map(|(idx, m)| {
                        let handle = format!("m{}", idx + 1);
                        handle_to_id.insert(handle.clone(), id_of(m));
                        ConsolidationMemberInput {
                            handle,
                            when: str_field(m, "occurredAt").or_else(|| str_field(m, "createdAt")),
                            importance: num_field(m, "reinforcedImportance")
                                .or_else(|| num_field(m, "importance"))
                                .unwrap_or(f64::NAN),
                            reinforcement_count: num_field(m, "reinforcementCount").unwrap_or(1.0),
                            content: str_field(m, "content").unwrap_or_default(),
                        }
                    })
                    .collect(),
            };

            cluster_reports.push(ConsolidationClusterReport {
                bucket: bucket.reference.clone(),
                cluster_kind: cluster.cluster.kind,
                member_ids: members.iter().map(|m| id_of(m)).collect(),
                member_contents: members
                    .iter()
                    .map(|m| str_field(m, "content").unwrap_or_default())
                    .collect(),
                existing_digest_id: existing_digest.map(id_of),
                existing_digest_content: existing_digest.and_then(|d| str_field(d, "content")),
                digests: Vec::new(),
                keep_standalone: Vec::new(),
                contradictions: Vec::new(),
                status: "failed",
                error: None,
            });
            stats.clusters_attempted += 1;
            let report_idx = cluster_reports.len() - 1;

            let route_members: Vec<&Value> = existing_digest
                .into_iter()
                .chain(members.iter().copied())
                .collect();
            let routed = route_for_cluster(db, route, &route_members, &mut chat_cache);
            let fallback = routed.failover.then(|| UncensoredFallbackOptions {
                concierge_policy: &routed.policy,
                available_profiles: &route.available_profiles,
                is_dangerous_chat: Some(routed.dangerous),
            });
            let result = consolidate_memory_cluster(
                executor,
                completion,
                &call_input,
                &routed.selection,
                character_id,
                fallback.as_ref(),
                profile_max_tokens_for(route, &routed.selection),
            )
            .await;

            let validation = match (result.success, result.result) {
                (true, Some(v)) => v,
                _ => {
                    let error = result
                        .error
                        .clone()
                        .unwrap_or_else(|| "no result".to_string());
                    let report = &mut cluster_reports[report_idx];
                    report.status = "failed";
                    report.error = Some(error.clone());
                    stats.clusters_failed += 1;
                    if result.timed_out {
                        stats.clusters_lost_to_timeout += 1;
                    }
                    tracing::warn!(
                        module = MODULE,
                        characterId = %character_id,
                        bucket = %bucket.key,
                        members = members.len(),
                        timedOut = result.timed_out,
                        error = %error,
                        "Consolidation call failed; cluster skipped"
                    );
                    continue;
                }
            };
            let validated = match validation {
                ConsolidationValidation::Ok(v) => v,
                ConsolidationValidation::Invalid { reason } => {
                    let report = &mut cluster_reports[report_idx];
                    report.status = "invalid";
                    report.error = Some(reason.clone());
                    stats.clusters_failed += 1;
                    tracing::warn!(
                        module = MODULE,
                        characterId = %character_id,
                        bucket = %bucket.key,
                        members = members.len(),
                        reason = %reason,
                        "Consolidation answer failed validation; cluster skipped"
                    );
                    continue;
                }
            };

            let to_id = |h: &String| handle_to_id.get(h).cloned().unwrap_or_default();
            let outcome = ResolvedClusterOutcome {
                about_character_id: bucket.about_character_id.clone(),
                cluster_kind: cluster.cluster.kind,
                existing_digest: existing_digest.cloned(),
                members: members.iter().map(|m| (*m).clone()).collect(),
                digests: validated
                    .digests
                    .iter()
                    .map(|d| {
                        let mut d = d.clone();
                        d.member_ids = d.member_ids.iter().map(to_id).collect();
                        d
                    })
                    .collect(),
                keep_standalone: validated.keep_standalone.iter().map(to_id).collect(),
                contradictions: validated
                    .contradictions
                    .iter()
                    .map(|c| ConsolidationContradiction {
                        older_id: to_id(&c.older_id),
                        newer_id: to_id(&c.newer_id),
                        note: c.note.clone(),
                    })
                    .collect(),
                embeddings: validated.digests.iter().map(|_| None).collect(),
            };
            {
                let report = &mut cluster_reports[report_idx];
                report.keep_standalone = outcome.keep_standalone.clone();
                report.contradictions = outcome.contradictions.clone();
                report.status = if dry_run { "planned" } else { "written" };
            }
            tracing::debug!(
                module = MODULE,
                characterId = %character_id,
                bucket = %bucket.key,
                clusterKind = cluster.cluster.kind.as_str(),
                members = members.len(),
                digests = outcome.digests.len(),
                standalone = outcome.keep_standalone.len(),
                unlisted = validated.unlisted.len(),
                contradictions = outcome.contradictions.len(),
                policyChatId = %or_null(routed.policy_chat_id.as_deref()),
                "Cluster consolidated"
            );
            outcomes.push(outcome);
            outcome_report_index.push(report_idx);
            outcome_bucket_key.push(bucket.key.clone());
        }
    }

    // ── 4. Plan (and embed), then write ──────────────────────────────────────
    let make_plan = |list: &[ResolvedClusterOutcome]| {
        let mut new_id = || uuid::Uuid::new_v4().to_string();
        plan_consolidation_writes(PlanInput {
            character_id,
            clusters: list,
            row_index: &row_index,
            now_iso: &now_iso,
            new_id: &mut new_id,
        })
    };

    let mut final_report_index = outcome_report_index.clone();
    let mut final_bucket_key = outcome_bucket_key.clone();
    let mut plan = make_plan(&outcomes);
    let mut final_outcomes_len = outcomes.len();

    if !dry_run && !outcomes.is_empty() {
        // Embed every planned digest from its final fields; a digest that cannot
        // be embedded would be invisible to recall and the gate, so its whole
        // cluster is dropped (no partial writes) and the plan rebuilt without it.
        let mut failed: Vec<usize> = Vec::new();
        let planned: Vec<PlannedDigest> = plan
            .creates
            .iter()
            .map(|c| c.digest.clone())
            .chain(plan.updates.iter().map(|u| u.digest.clone()))
            .collect();
        // "Anchor dates render on the instance's memory calendar."
        let memory_time_zone = crate::services::memory_time_zone::refresh_memory_time_zone(
            db,
            &opts.host_zone_fallback,
            now_ms as f64,
        );
        for digest in &planned {
            if failed.contains(&digest.cluster_index) {
                continue;
            }
            let source = EmbeddingTextSource {
                summary: &digest.summary,
                content: &digest.content,
                occurred_at: digest.occurred_at.as_deref(),
                narrative_time: digest.narrative_time.as_deref(),
                entities: AnchorEntities::Strings(&digest.entities),
            };
            let text = embedding_text_for(&source, &memory_time_zone);
            match embedding
                .generate_embedding_for_user(
                    &text,
                    &user_id,
                    None,
                    EmbeddingCallOptions::background()
                        .with_lexical_text(lexical_embedding_text_for(&source)),
                )
                .await
            {
                Ok(result) => {
                    let k = plan.digests_by_cluster[digest.cluster_index]
                        .iter()
                        .position(|d| d.id == digest.id)
                        .expect("a planned digest");
                    outcomes[digest.cluster_index].embeddings[k] = Some(result.embedding);
                }
                Err(error) => {
                    failed.push(digest.cluster_index);
                    tracing::warn!(
                        module = MODULE,
                        characterId = %character_id,
                        error = %error.message,
                        "Digest embedding failed; its cluster is skipped"
                    );
                }
            }
        }
        let mut final_outcomes: Vec<ResolvedClusterOutcome> = outcomes.clone();
        if !failed.is_empty() {
            for &index in &failed {
                let report = &mut cluster_reports[outcome_report_index[index]];
                report.status = "embedding-failed";
                report.error = Some("digest embedding failed".to_string());
                stats.clusters_failed += 1;
            }
            let keep: Vec<usize> = (0..outcomes.len())
                .filter(|i| !failed.contains(i))
                .collect();
            final_outcomes = keep.iter().map(|&i| outcomes[i].clone()).collect();
            final_report_index = keep.iter().map(|&i| outcome_report_index[i]).collect();
            final_bucket_key = keep
                .iter()
                .map(|&i| outcome_bucket_key[i].clone())
                .collect();
        }
        // Re-plan with the embeddings attached (and failed clusters gone).
        plan = make_plan(&final_outcomes);
        final_outcomes_len = final_outcomes.len();
        stats.contradictions_applied = final_outcomes.iter().map(|o| o.contradictions.len()).sum();
    } else {
        stats.contradictions_applied = outcomes.iter().map(|o| o.contradictions.len()).sum();
    }

    for (idx, digests) in plan.digests_by_cluster.iter().enumerate() {
        cluster_reports[final_report_index[idx]].digests = digests
            .iter()
            .map(ConsolidationDigestReport::from_planned)
            .collect();
    }
    stats.clusters_succeeded = final_outcomes_len;
    stats.digests_created = plan.creates.len();
    stats.digests_updated = plan.updates.len();
    stats.members_superseded = plan.tier_moves.iter().map(|m| m.ids.len()).sum();
    stats.links_rewired = plan.link_rewrites.len();
    let mut marked: HashSet<&String> = plan.considered.iter().collect();
    marked.extend(below_min_considered.iter());
    stats.rows_marked_considered = marked.len();

    if !dry_run {
        execute_plan(db, character_id, &plan, &below_min_considered, &now_iso).await?;
    }
    let touched = touched_subjects(&buckets, &plan, &final_bucket_key);

    let skipped = if route.is_some() || selection.selected.is_empty() {
        None
    } else {
        Some("no-llm")
    };
    let report = finish(&mut stats, cluster_reports, skipped);
    tracing::info!(
        module = MODULE,
        characterId = %character_id,
        characterName = %or_null(holder_name.as_deref()),
        trigger = trigger.as_str(),
        dryRun = dry_run,
        candidates = report.stats.candidates,
        demotedCandidates = report.stats.demoted_candidates,
        digestsLoaded = report.stats.digests_loaded,
        clustersFound = report.stats.clusters_found,
        clustersSelected = report.stats.clusters_selected,
        clustersSucceeded = report.stats.clusters_succeeded,
        clustersFailed = report.stats.clusters_failed,
        clustersDeferred = report.stats.clusters_deferred,
        digestsCreated = report.stats.digests_created,
        digestsUpdated = report.stats.digests_updated,
        membersSuperseded = report.stats.members_superseded,
        rowsMarkedConsidered = report.stats.rows_marked_considered,
        linksRewired = report.stats.links_rewired,
        budgetExhausted = report.stats.budget_exhausted,
        durationMs = report.stats.duration_ms,
        "Consolidation run complete"
    );
    Ok(ConsolidationRun {
        report,
        plan,
        below_min_considered,
        touched_subjects: touched,
    })
}
