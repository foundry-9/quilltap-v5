//! The maintenance job's vocabulary — v4 `lib/memory/maintenance/types.ts`
//! (`104e986c7`, memory refactor §3.9, at the `83334b9a5` pin).
//!
//! "One job, `MEMORY_MAINTENANCE`, runs these phases for one character, in
//! this order, each skippable: fold near-duplicates → consolidate → demote →
//! retire → mirror."
//!
//! The phase and reason enums are C1-6's ([`MaintenancePhase`],
//! [`MaintenanceReason`] — defined once in `queue_service` so the wire, the
//! SPA contract and this module share them); everything else is here.
//!
//! **Wire key orders.** v4's `JSON.stringify` follows each object's LITERAL
//! construction (`run.ts` / `demote.ts`), not its TypeScript interface — e.g.
//! the fold report emits `eligible, deferred, groups, folded, groupsFailed,
//! groupsDeferred` although the interface declares `groupsDeferred` first.
//! Every `to_json` below follows the construction.
//!
//! **Phase lists from a payload stay strings.** A stored job payload's
//! `phases` / `exemptPhases` are carried as v4 carries them (a string array it
//! never re-validates): a request naming only an unknown phase is still an
//! EXPLICIT list (every real phase `not-requested`), exactly as v4's
//! `new Set(request.phases)` reads it.

use std::sync::Arc;

use serde_json::{json, Map, Value};

pub use crate::services::queue_service::{MaintenancePhase, MaintenanceReason};

use crate::services::consolidation::ConsolidationReport;

/// A monotonic elapsed-milliseconds reading since the run started (P4.D271
/// R-J: the time budget's clock, injectable for the differential).
pub type ElapsedClock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// v4 `withImpliedPhases(phases)` (`types.ts:24-28`): "the Commonplace mirror
/// always follows consolidation, so naming `consolidate` brings `mirror` with
/// it." Returned in [`MaintenancePhase::ALL`] order, deduped. Called ONLY by
/// the API (`route.ts:746`) — the run never applies it.
pub fn with_implied_phases(phases: &[MaintenancePhase]) -> Vec<MaintenancePhase> {
    let consolidates = phases.contains(&MaintenancePhase::Consolidate);
    MaintenancePhase::ALL
        .into_iter()
        .filter(|p| phases.contains(p) || (consolidates && *p == MaintenancePhase::Mirror))
        .collect()
}

/// The phase names a list holds, as v4's `Set<MaintenancePhase>` of a stored
/// array reads them.
pub fn phase_names(phases: &[MaintenancePhase]) -> Vec<String> {
    phases.iter().map(|p| p.as_str().to_string()).collect()
}

/// v4 `MaintenanceRunSettings` (`types.ts:52-59`) — what one run may override.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaintenanceRunSettings {
    /// Consolidation knobs for this run (a dry run's tuning; bug 223's
    /// threshold) — a `Partial<MemoryConsolidationSettings>`, spread over the
    /// stored settings.
    pub consolidation: Option<Map<String, Value>>,
    /// Most clusters consolidated in this run.
    pub max_clusters_per_run: Option<f64>,
    /// The demotable-row cap for this run; otherwise the character's own
    /// (override → global).
    pub cap: Option<f64>,
}

impl MaintenanceRunSettings {
    /// The `settings` object of a stored payload (or the API's parsed body),
    /// read as v4's handler casts it: a non-object `consolidation` and a
    /// non-number `maxClustersPerRun` / `cap` are absent.
    pub fn from_value(v: &Value) -> Option<Self> {
        let obj = v.as_object()?;
        Some(Self {
            consolidation: obj.get("consolidation").and_then(Value::as_object).cloned(),
            max_clusters_per_run: obj.get("maxClustersPerRun").and_then(Value::as_f64),
            cap: obj.get("cap").and_then(Value::as_f64),
        })
    }
}

/// v4 `MaintenanceRequest` (`types.ts:33-49`).
#[derive(Clone, Debug, PartialEq)]
pub struct MaintenanceRequest {
    pub character_id: String,
    /// The phases asked for; `None` (or empty), all of them. Always run in
    /// [`MaintenancePhase::ALL`] order.
    pub phases: Option<Vec<String>>,
    pub reason: MaintenanceReason,
    /// Phases asked for by hand: they run whatever the enabled switches and
    /// the per-phase toggles say. Omitted, a manual request exempts the
    /// phases it names (all of them when it names none) and an automatic one
    /// exempts nothing.
    pub exempt_phases: Option<Vec<String>>,
    /// Plan every phase and write nothing.
    pub dry_run: bool,
    pub settings: Option<MaintenanceRunSettings>,
}

/// Where a run is (v4 `isJobChild()`): in a job a live run defers its mirror
/// to the commit hook and announces nothing (R-M — every v5 job run reports as
/// v4's CHILD does); in process it mirrors and announces itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaintenanceMode {
    InProcess,
    Job,
}

/// v4 `MaintenanceContext` (`types.ts:62-69`), plus v5's mode (R-M), the
/// injectable budget clock (P4.D271 R-J) and the host's zone fallback (the
/// fold's survivor re-embed and the digest embeds read the memory calendar —
/// v4 `refreshMemoryTimeZone()`, whose last resort is the host zone).
#[derive(Clone)]
pub struct MaintenanceContext {
    /// Defaults to the character's owner.
    pub user_id: Option<String>,
    /// Stop issuing consolidation model calls after this long (ms).
    pub time_budget_ms: Option<i64>,
    /// v4's `ctx.now` clock override, in ms.
    pub now_ms: Option<i64>,
    pub mode: MaintenanceMode,
    /// Elapsed ms since the run started; `None` → a real `Instant`.
    pub elapsed: Option<ElapsedClock>,
    /// The host's IANA zone (the memory calendar's last resort).
    pub host_zone_fallback: String,
}

impl MaintenanceContext {
    /// An in-process context with no budget, no clock overrides and UTC as
    /// the host zone.
    pub fn in_process() -> Self {
        Self {
            user_id: None,
            time_budget_ms: None,
            now_ms: None,
            mode: MaintenanceMode::InProcess,
            elapsed: None,
            host_zone_fallback: "UTC".to_string(),
        }
    }
}

/// v4 `PhaseStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseStatus {
    Ran,
    Skipped,
}

impl PhaseStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ran => "ran",
            Self::Skipped => "skipped",
        }
    }
}

/// v4 `PhaseSkipReason` — why a phase did not run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseSkipReason {
    NotRequested,
    MaintenanceDisabled,
    ConsolidationDisabled,
    PhaseDisabled,
    NoRetentionWindow,
    NothingTouched,
}

impl PhaseSkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotRequested => "not-requested",
            Self::MaintenanceDisabled => "maintenance-disabled",
            Self::ConsolidationDisabled => "consolidation-disabled",
            Self::PhaseDisabled => "phase-disabled",
            Self::NoRetentionWindow => "no-retention-window",
            Self::NothingTouched => "nothing-touched",
        }
    }
}

/// v4 `PhaseOutcome`: `{phase, status, [skippedBecause], durationMs}` — the
/// character-missing / archived arm pushes `{phase, status: 'skipped',
/// durationMs: 0}` with NO `skippedBecause` key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhaseOutcome {
    pub phase: MaintenancePhase,
    pub status: PhaseStatus,
    pub skipped_because: Option<PhaseSkipReason>,
    pub duration_ms: i64,
}

impl PhaseOutcome {
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("phase".into(), json!(self.phase.as_str()));
        m.insert("status".into(), json!(self.status.as_str()));
        if let Some(r) = self.skipped_because {
            m.insert("skippedBecause".into(), json!(r.as_str()));
        }
        m.insert("durationMs".into(), json!(self.duration_ms));
        Value::Object(m)
    }
}

fn num(x: f64) -> Value {
    crate::db::js_number_to_json(x)
}

/// v4 `NearDuplicateGroup` — a survivor and the rows folded into it.
#[derive(Clone, Debug, PartialEq)]
pub struct NearDuplicateGroup {
    pub survivor_id: String,
    pub absorbed_ids: Vec<String>,
    /// The lowest cosine between the survivor and a row it absorbs.
    pub min_cosine: f64,
}

impl NearDuplicateGroup {
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("survivorId".into(), json!(self.survivor_id));
        m.insert("absorbedIds".into(), json!(self.absorbed_ids));
        m.insert("minCosine".into(), num(self.min_cosine));
        Value::Object(m)
    }
}

/// v4 `FoldPhaseReport`; the WIRE order is `run.ts:278-285`'s construction.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FoldPhaseReport {
    /// Hot AUTO rows with an embedding — the only rows the phase considers.
    pub eligible: usize,
    /// Eligible rows left uncompared this run because their bucket was over
    /// the comparison bound.
    pub deferred: usize,
    pub groups: Vec<NearDuplicateGroup>,
    /// Rows folded away (retired once their survivor took them in).
    pub folded: usize,
    /// Groups whose fold did not land; their rows were kept.
    pub groups_failed: usize,
    /// Groups left for a later run because the fold's time budget ran out.
    pub groups_deferred: usize,
}

impl FoldPhaseReport {
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("eligible".into(), json!(self.eligible));
        m.insert("deferred".into(), json!(self.deferred));
        m.insert(
            "groups".into(),
            Value::Array(
                self.groups
                    .iter()
                    .map(NearDuplicateGroup::to_json)
                    .collect(),
            ),
        );
        m.insert("folded".into(), json!(self.folded));
        m.insert("groupsFailed".into(), json!(self.groups_failed));
        m.insert("groupsDeferred".into(), json!(self.groups_deferred));
        Value::Object(m)
    }
}

/// v4 `DemotePhaseReport` (`demote.ts:37-46`'s literal order). `cap` and
/// `excess` are JS numbers (a per-run cap rides a stored payload unchecked).
#[derive(Clone, Debug, PartialEq)]
pub struct DemotePhaseReport {
    pub cap: f64,
    /// Hot rows before the phase (demotable + protected).
    pub hot_total: usize,
    /// Hot AUTO rows before the phase — the only rows the cap measures (bug
    /// 210).
    pub demotable: usize,
    /// Hot MANUAL rows and digests — never demoted, never counted.
    pub protected: usize,
    /// `max(0, demotable − cap)`.
    pub excess: f64,
    pub demoted: usize,
    pub demoted_ids: Vec<String>,
    /// Protection score of the last row demoted (the highest that went).
    pub highest_score_demoted: Option<f64>,
}

impl DemotePhaseReport {
    /// The report; `with_ids: false` is the summary's `demotedIds`-less copy
    /// (v4's `({ demotedIds, ...rest }) => rest` keeps the remaining order).
    fn to_json_with(&self, with_ids: bool) -> Value {
        let mut m = Map::new();
        m.insert("cap".into(), num(self.cap));
        m.insert("hotTotal".into(), json!(self.hot_total));
        m.insert("demotable".into(), json!(self.demotable));
        m.insert("protected".into(), json!(self.protected));
        m.insert("excess".into(), num(self.excess));
        m.insert("demoted".into(), json!(self.demoted));
        if with_ids {
            m.insert("demotedIds".into(), json!(self.demoted_ids));
        }
        m.insert(
            "highestScoreDemoted".into(),
            self.highest_score_demoted.map(num).unwrap_or(Value::Null),
        );
        Value::Object(m)
    }

    pub fn to_json(&self) -> Value {
        self.to_json_with(true)
    }
}

/// v4 `RetirePhaseReport` (`run.ts:351`).
#[derive(Clone, Debug, PartialEq)]
pub struct RetirePhaseReport {
    pub retention_days: f64,
    pub retired: usize,
    pub retired_ids: Vec<String>,
}

impl RetirePhaseReport {
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("retentionDays".into(), num(self.retention_days));
        m.insert("retired".into(), json!(self.retired));
        m.insert("retiredIds".into(), json!(self.retired_ids));
        Value::Object(m)
    }
}

/// v4 `MirrorPhaseReport` (`run.ts:359`).
#[derive(Clone, Debug, PartialEq)]
pub struct MirrorPhaseReport {
    /// Subject buckets whose Commonplace file the run changed.
    pub subjects: Vec<String>,
    pub files_written: usize,
    /// True in a job: the commit hook writes the files.
    pub deferred_to_commit: bool,
}

impl MirrorPhaseReport {
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("subjects".into(), json!(self.subjects));
        m.insert("filesWritten".into(), json!(self.files_written));
        m.insert("deferredToCommit".into(), json!(self.deferred_to_commit));
        Value::Object(m)
    }
}

/// v4 `MaintenanceReport['skippedReason']` — set when nothing ran at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaintenanceSkip {
    CharacterNotFound,
    Archived,
}

impl MaintenanceSkip {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CharacterNotFound => "character-not-found",
            Self::Archived => "archived",
        }
    }
}

/// v4 `MaintenanceReport` (`run.ts:199-208`): `characterId, characterName,
/// reason, dryRun, startedAt, finishedAt, durationMs, phases`, then appended
/// in execution order `skippedReason` (the skip arm) or the reports of the
/// phases that RAN — `fold`, `consolidation`, `demote`, `retire`, `mirror`.
/// Execution order IS that order, so [`MaintenanceReport::to_json`] emits it
/// fixed.
#[derive(Clone, Debug, PartialEq)]
pub struct MaintenanceReport {
    pub character_id: String,
    pub character_name: Option<String>,
    pub reason: MaintenanceReason,
    pub dry_run: bool,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: i64,
    pub skipped_reason: Option<MaintenanceSkip>,
    pub phases: Vec<PhaseOutcome>,
    pub fold: Option<FoldPhaseReport>,
    pub consolidation: Option<ConsolidationReport>,
    pub demote: Option<DemotePhaseReport>,
    pub retire: Option<RetirePhaseReport>,
    pub mirror: Option<MirrorPhaseReport>,
}

impl MaintenanceReport {
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("characterId".into(), json!(self.character_id));
        m.insert("characterName".into(), json!(self.character_name));
        m.insert("reason".into(), json!(self.reason.as_str()));
        m.insert("dryRun".into(), json!(self.dry_run));
        m.insert("startedAt".into(), json!(self.started_at));
        m.insert("finishedAt".into(), json!(self.finished_at));
        m.insert("durationMs".into(), json!(self.duration_ms));
        m.insert(
            "phases".into(),
            Value::Array(self.phases.iter().map(PhaseOutcome::to_json).collect()),
        );
        if let Some(s) = self.skipped_reason {
            m.insert("skippedReason".into(), json!(s.as_str()));
        }
        if let Some(f) = &self.fold {
            m.insert("fold".into(), f.to_json());
        }
        if let Some(c) = &self.consolidation {
            m.insert("consolidation".into(), c.to_json());
        }
        if let Some(d) = &self.demote {
            m.insert("demote".into(), d.to_json());
        }
        if let Some(r) = &self.retire {
            m.insert("retire".into(), r.to_json());
        }
        if let Some(mi) = &self.mirror {
            m.insert("mirror".into(), mi.to_json());
        }
        Value::Object(m)
    }

    /// The phases that RAN, in order (v4 `phases.filter(ran).map(phase)`).
    pub fn ran(&self) -> Vec<MaintenancePhase> {
        self.phases
            .iter()
            .filter(|p| p.status == PhaseStatus::Ran)
            .map(|p| p.phase)
            .collect()
    }
}

/// v4 `MaintenanceSummary` (`types.ts:164-184`, built by
/// `summarizeMaintenance`, `run.ts:107-152`): "a report without row contents:
/// what a job hands its parent and what the ledger keeps". Carried as its
/// JSON (the ledger stores and serves it verbatim); the typed reads the ledger
/// and the logs need are methods.
#[derive(Clone, Debug, PartialEq)]
pub struct MaintenanceSummary {
    pub character_id: String,
    pub reason: MaintenanceReason,
    pub dry_run: bool,
    pub finished_at: String,
    pub skipped: bool,
    pub phases: Vec<PhaseOutcome>,
    /// The summary's JSON, in v4's key order.
    pub json: Value,
}

impl MaintenanceSummary {
    pub fn to_json(&self) -> Value {
        self.json.clone()
    }

    /// The phases that RAN, in order.
    pub fn ran(&self) -> Vec<MaintenancePhase> {
        self.phases
            .iter()
            .filter(|p| p.status == PhaseStatus::Ran)
            .map(|p| p.phase)
            .collect()
    }

    fn section(&self, key: &str, field: &str) -> Option<&Value> {
        self.json.get(key).and_then(|s| s.get(field))
    }

    /// `summary.demote` present.
    pub fn demote(&self) -> Option<(Value, Value)> {
        Some((
            self.section("demote", "demoted")?.clone(),
            self.section("demote", "excess")?.clone(),
        ))
    }

    /// A count field of a section, as the logs read it (`?? null`).
    pub fn count(&self, key: &str, field: &str) -> Value {
        self.section(key, field).cloned().unwrap_or(Value::Null)
    }

    pub fn duration_ms(&self) -> Value {
        self.json.get("durationMs").cloned().unwrap_or(Value::Null)
    }
}
