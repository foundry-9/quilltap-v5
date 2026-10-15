//! `run_maintenance` — the one maintenance run for one character: v4
//! `lib/memory/maintenance/run.ts` (`104e986c7`, at the `83334b9a5` pin).
//!
//! v4's why, carried: "Reads the character's corpus once, then runs the
//! phases in their fixed order — fold near-duplicates → consolidate → demote →
//! retire → mirror — each planning against the corpus as the earlier ones
//! left it. A dry run plans every phase the same way and writes nothing
//! (invariant 10)." And: "All effects after commit. In the job child this run
//! announces nothing: the dispatcher's commit hook invalidates the frozen
//! archives, publishes `memories`, writes the `Commonplace/` mirror from the
//! committed rows (bug 208) and records the outcome in the parent's ledger.
//! In the parent the writes have already committed when the run finishes, so
//! it does the same itself."
//!
//! v5 has no job child: [`MaintenanceMode`] stands for v4's `isJobChild()`
//! (R-M — every job run reports as v4's CHILD does: the mirror deferred to the
//! commit hook, no in-run announce), and the commit hook is
//! [`super::after_job::after_job_completed`].

use std::time::Instant;

use serde_json::{json, Map, Value};

use super::corpus::MaintenanceCorpus;
use super::demote::{plan_demotion, plan_retention};
use super::fold_near_duplicates::{
    apply_folds_to_corpus, apply_near_duplicate_folds, plan_near_duplicate_folds,
    ApplyNearDuplicateFoldsRequest,
};
use super::ledger::record_maintenance_outcome;
use super::types::{
    ElapsedClock, FoldPhaseReport, MaintenanceContext, MaintenanceMode, MaintenancePhase,
    MaintenanceReport, MaintenanceRequest, MaintenanceSkip, MaintenanceSummary, MirrorPhaseReport,
    PhaseOutcome, PhaseSkipReason, PhaseStatus, RetirePhaseReport,
};
use super::{js_num, json_text};
use crate::admission::thresholds::admission_thresholds_for_provider;
use crate::db::memories::TierExtra;
use crate::db::runtime::Db;
use crate::db::vector_store::Tier;
use crate::db::{characters_read, DbError};
use crate::memory_settings::{resolve_memory_settings, ResolvedMemoryMaintenance};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::consolidation::RunConsolidationOptions;
use crate::services::memory_retire::{retire_and_announce, RetireOptions, RetireReason};

const MODULE: &str = "memory:maintenance";

/// v4 `FOLD_BUDGET_SHARE`: "the share of a run's time budget the
/// near-duplicate fold may spend; consolidation gets the rest."
pub const FOLD_BUDGET_SHARE: f64 = 0.4;

/// v4 `exemptPhasesFor(request)` (`run.ts:62-68`) — the phases a request
/// exempts from the switches: `request.exemptPhases` if present; else an
/// automatic request exempts nothing; else a manual one exempts the phases it
/// names (all five when it names none / an empty list). Set semantics: the
/// first occurrence of each name, in order.
pub fn exempt_phases_for(
    phases: Option<&[String]>,
    reason: super::types::MaintenanceReason,
    exempt_phases: Option<&[String]>,
) -> Vec<String> {
    let dedupe = |list: &[String]| {
        let mut out: Vec<String> = Vec::new();
        for p in list {
            if !out.contains(p) {
                out.push(p.clone());
            }
        }
        out
    };
    if let Some(exempt) = exempt_phases {
        return dedupe(exempt);
    }
    if reason != super::types::MaintenanceReason::Manual {
        return Vec::new();
    }
    match phases {
        Some(list) if !list.is_empty() => dedupe(list),
        _ => super::types::phase_names(&MaintenancePhase::ALL),
    }
}

/// What [`resolve_phases`] reads of the resolved settings (v4
/// `Pick<MemorySettings, 'maintenance'>`, after the run's consolidation
/// override is spread in).
pub struct PhaseSettings {
    pub maintenance_enabled: bool,
    pub fold_near_duplicates: bool,
    pub demote: bool,
    pub retire: bool,
    /// `maintenance.consolidation.enabled`, JS-truthy.
    pub consolidation_enabled: bool,
    /// `maintenance.coldRetentionDays` (a JS number or `null`).
    pub cold_retention_days: Option<f64>,
}

impl PhaseSettings {
    /// From the façade's maintenance block; `consolidation_enabled` is the
    /// merged (override-spread) value.
    pub fn from_resolved(m: &ResolvedMemoryMaintenance, consolidation_enabled: bool) -> Self {
        Self {
            maintenance_enabled: m.settings.enabled,
            fold_near_duplicates: m.settings.phases.fold_near_duplicates,
            demote: m.settings.phases.demote,
            retire: m.settings.phases.retire,
            consolidation_enabled,
            cold_retention_days: m.cold_retention_days.map(|d| d as f64),
        }
    }
}

/// v4 `resolvePhases(request, settings)` (`run.ts:77-104`): "which phases run,
/// and why the others do not. A phase asked for by hand (exempt) runs whatever
/// the enabled switches and per-phase toggles say; every other phase —
/// anything a scheduled or watermark request asked for, even when merged into
/// a manual job — obeys them. `retire` always needs a retention window."
pub fn resolve_phases(
    phases: Option<&[String]>,
    reason: super::types::MaintenanceReason,
    exempt_phases: Option<&[String]>,
    settings: &PhaseSettings,
) -> Vec<(MaintenancePhase, Option<PhaseSkipReason>)> {
    let explicit: Option<&[String]> = phases.filter(|p| !p.is_empty());
    let exempt = exempt_phases_for(phases, reason, exempt_phases);
    let is_exempt = |p: MaintenancePhase| exempt.iter().any(|e| e == p.as_str());
    MaintenancePhase::ALL
        .into_iter()
        .map(|phase| {
            let mut skip: Option<PhaseSkipReason> = None;
            if explicit.is_some_and(|list| !list.iter().any(|e| e == phase.as_str())) {
                skip = Some(PhaseSkipReason::NotRequested);
            } else if phase == MaintenancePhase::Consolidate {
                if !is_exempt(phase) && !settings.consolidation_enabled {
                    skip = Some(PhaseSkipReason::ConsolidationDisabled);
                }
            } else if phase != MaintenancePhase::Mirror {
                if !is_exempt(phase) {
                    let on = match phase {
                        MaintenancePhase::FoldNearDuplicates => settings.fold_near_duplicates,
                        MaintenancePhase::Demote => settings.demote,
                        _ => settings.retire,
                    };
                    if !settings.maintenance_enabled {
                        skip = Some(PhaseSkipReason::MaintenanceDisabled);
                    } else if !on {
                        skip = Some(PhaseSkipReason::PhaseDisabled);
                    }
                }
                // `!(m.coldRetentionDays && m.coldRetentionDays > 0)`.
                if skip.is_none()
                    && phase == MaintenancePhase::Retire
                    && !settings.cold_retention_days.is_some_and(|d| d > 0.0)
                {
                    skip = Some(PhaseSkipReason::NoRetentionWindow);
                }
            }
            (phase, skip)
        })
        .collect()
}

/// v4 `summarizeMaintenance(report)` (`run.ts:107-152`): "the report without
/// row contents, for the job result and the ledger" — `characterId, reason,
/// dryRun, finishedAt, durationMs, [skippedReason], phases, [fold], [consolidation],
/// [demote minus demotedIds], [retire], [mirror]`.
pub fn summarize_maintenance(report: &MaintenanceReport) -> MaintenanceSummary {
    let mut m = Map::new();
    m.insert("characterId".into(), json!(report.character_id));
    m.insert("reason".into(), json!(report.reason.as_str()));
    m.insert("dryRun".into(), json!(report.dry_run));
    m.insert("finishedAt".into(), json!(report.finished_at));
    m.insert("durationMs".into(), json!(report.duration_ms));
    if let Some(s) = report.skipped_reason {
        m.insert("skippedReason".into(), json!(s.as_str()));
    }
    m.insert(
        "phases".into(),
        Value::Array(report.phases.iter().map(PhaseOutcome::to_json).collect()),
    );
    if let Some(f) = &report.fold {
        let mut o = Map::new();
        o.insert("eligible".into(), json!(f.eligible));
        o.insert("groups".into(), json!(f.groups.len()));
        o.insert("folded".into(), json!(f.folded));
        o.insert("groupsFailed".into(), json!(f.groups_failed));
        o.insert("groupsDeferred".into(), json!(f.groups_deferred));
        m.insert("fold".into(), Value::Object(o));
    }
    if let Some(c) = &report.consolidation {
        let mut o = Map::new();
        o.insert("digestsCreated".into(), json!(c.stats.digests_created));
        o.insert("digestsUpdated".into(), json!(c.stats.digests_updated));
        o.insert(
            "membersSuperseded".into(),
            json!(c.stats.members_superseded),
        );
        o.insert("clustersFailed".into(), json!(c.stats.clusters_failed));
        o.insert("clustersDeferred".into(), json!(c.stats.clusters_deferred));
        if let Some(r) = c.skipped_reason {
            o.insert("skippedReason".into(), json!(r));
        }
        m.insert("consolidation".into(), Value::Object(o));
    }
    if let Some(d) = &report.demote {
        let mut full = match d.to_json() {
            Value::Object(o) => o,
            _ => Map::new(),
        };
        full.shift_remove("demotedIds");
        m.insert("demote".into(), Value::Object(full));
    }
    if let Some(r) = &report.retire {
        let mut o = Map::new();
        o.insert("retentionDays".into(), js_value(r.retention_days));
        o.insert("retired".into(), json!(r.retired));
        m.insert("retire".into(), Value::Object(o));
    }
    if let Some(mi) = &report.mirror {
        let mut o = Map::new();
        o.insert("subjects".into(), json!(mi.subjects.len()));
        o.insert("filesWritten".into(), json!(mi.files_written));
        o.insert("deferredToCommit".into(), json!(mi.deferred_to_commit));
        m.insert("mirror".into(), Value::Object(o));
    }
    MaintenanceSummary {
        character_id: report.character_id.clone(),
        reason: report.reason,
        dry_run: report.dry_run,
        finished_at: report.finished_at.clone(),
        skipped: report.skipped_reason.is_some(),
        phases: report.phases.clone(),
        json: Value::Object(m),
    }
}

fn js_value(x: f64) -> Value {
    crate::db::js_number_to_json(x)
}

/// v4 `changedRows(report)` — true when a live run changed rows.
pub fn changed_rows(report: &MaintenanceReport) -> bool {
    report.fold.as_ref().is_some_and(|f| f.folded > 0)
        || report.consolidation.as_ref().is_some_and(|c| {
            c.stats.digests_created
                + c.stats.digests_updated
                + c.stats.members_superseded
                + c.stats.rows_marked_considered
                > 0
        })
        || report.demote.as_ref().is_some_and(|d| d.demoted > 0)
        || report.retire.as_ref().is_some_and(|r| r.retired > 0)
}

/// v4 `announceMaintenance(summary, {rowsChanged, mirrored})` (`run.ts:171-186`):
/// "the effects of a live run whose writes have committed: drop the frozen
/// archives, publish the realtime hints, record the outcome." Returns at once
/// in a job or for a dry run. v4's catch (WARN `Maintenance announce failed
/// (non-fatal)`) guards three calls none of which can fail in v5 (and none
/// throws through v4's real code) — unreachable, no home.
pub fn announce_maintenance(
    summary: &MaintenanceSummary,
    rows_changed: bool,
    mirrored: bool,
    mode: MaintenanceMode,
) {
    if mode == MaintenanceMode::Job || summary.dry_run {
        return;
    }
    if rows_changed {
        crate::services::frozen_archive::invalidate_frozen_archive(&summary.character_id);
        crate::realtime::bus::publish_realtime(
            crate::realtime::types::RealtimeTopic::Memories,
            None,
        );
    }
    if mirrored {
        crate::realtime::bus::publish_realtime(
            crate::realtime::types::RealtimeTopic::MountPoints,
            None,
        );
    }
    record_maintenance_outcome(summary);
}

/// What a run answers: the report, the digest subjects it touched (the
/// commit hook mirrors them in a job — R-M), and whether a live run changed
/// rows.
#[derive(Clone, Debug)]
pub struct MaintenanceRun {
    pub report: MaintenanceReport,
    pub touched_subjects: Vec<String>,
    pub rows_changed: bool,
}

fn or_null(v: Option<&str>) -> String {
    v.map(str::to_string).unwrap_or_else(|| "null".to_string())
}

fn opt_count(v: Option<usize>) -> String {
    v.map(|n| n.to_string())
        .unwrap_or_else(|| "null".to_string())
}

/// v4 `runMaintenance(request, ctx)` (`run.ts:189-385`): run maintenance for
/// one character. "Throws only when its corpus cannot be read" — in v5 also
/// when a phase's write cannot commit (the writer's `Err`).
pub async fn run_maintenance<C, E>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    request: MaintenanceRequest,
    ctx: MaintenanceContext,
) -> Result<MaintenanceRun, DbError>
where
    C: CompletionProvider,
    E: EmbeddingProvider + Sync,
{
    let started = Instant::now();
    let run_elapsed: ElapsedClock = ctx
        .elapsed
        .clone()
        .unwrap_or_else(|| std::sync::Arc::new(move || started.elapsed().as_millis() as i64));
    let now_ms = ctx.now_ms.unwrap_or_else(crate::clock::now_unix_ms);
    let now_iso = crate::clock::iso_from_unix_ms(now_ms);
    let dry_run = request.dry_run;
    let character_id = request.character_id.clone();
    let reason = request.reason;
    let mut report = MaintenanceReport {
        character_id: character_id.clone(),
        character_name: None,
        reason,
        dry_run,
        started_at: now_iso.clone(),
        finished_at: now_iso.clone(),
        duration_ms: 0,
        skipped_reason: None,
        phases: Vec::new(),
        fold: None,
        consolidation: None,
        demote: None,
        retire: None,
        mirror: None,
    };
    let finish = |report: &mut MaintenanceReport| {
        report.finished_at = crate::clock::now_iso();
        report.duration_ms = started.elapsed().as_millis() as i64;
    };

    // "Raw read on purpose: the archived tombstone lives on the row, and a
    // broken vault must not hide a character that still holds memories."
    let cid = character_id.clone();
    let holder_raw = db
        .read_main(|c| Ok(characters_read::find_by_id_raw_or_none(c, &cid)))
        .ok()
        .flatten();
    let archived = holder_raw
        .as_ref()
        .is_some_and(crate::api::characters::is_archived);
    if holder_raw.is_none() || archived {
        let skip = if holder_raw.is_some() {
            MaintenanceSkip::Archived
        } else {
            MaintenanceSkip::CharacterNotFound
        };
        report.skipped_reason = Some(skip);
        report.character_name = holder_raw
            .as_ref()
            .and_then(|h| h.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string);
        for phase in MaintenancePhase::ALL {
            report.phases.push(PhaseOutcome {
                phase,
                status: PhaseStatus::Skipped,
                skipped_because: None,
                duration_ms: 0,
            });
        }
        tracing::info!(
            module = MODULE,
            characterId = %character_id,
            reason = reason.as_str(),
            skippedReason = skip.as_str(),
            "Maintenance skipped"
        );
        finish(&mut report);
        return Ok(MaintenanceRun {
            report,
            touched_subjects: Vec::new(),
            rows_changed: false,
        });
    }
    let holder_raw = holder_raw.expect("checked above");
    report.character_name = holder_raw
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string);
    let holder = {
        let id = character_id.clone();
        db.read_main(|main| {
            db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &id))
        })
        .ok()
        .flatten()
        .unwrap_or_else(|| holder_raw.clone())
    };
    let user_id = ctx
        .user_id
        .clone()
        .or_else(|| {
            holder_raw
                .get("userId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();

    let uid = user_id.clone();
    // The façade never fails; only the pool can.
    let settings = db.read_main(|c| Ok(resolve_memory_settings(c, Some(&uid))))?;
    // `settings.maintenance.consolidation = {...stored, ...request.settings.consolidation}`
    // — a run override `{enabled: true}` lifts `consolidation-disabled`, and
    // its `matureAfterDays` is the loader's.
    let mut consolidation_json = settings.maintenance.consolidation.to_json();
    if let (Some(over), Value::Object(m)) = (
        request
            .settings
            .as_ref()
            .and_then(|s| s.consolidation.as_ref()),
        &mut consolidation_json,
    ) {
        for (k, v) in over {
            m.insert(k.clone(), v.clone());
        }
    }
    let consolidation_enabled = js_truthy(consolidation_json.get("enabled"));
    let mature_after_days = consolidation_json
        .get("matureAfterDays")
        .and_then(Value::as_f64)
        .unwrap_or(f64::NAN);
    let phase_settings = PhaseSettings::from_resolved(&settings.maintenance, consolidation_enabled);
    let mut plan = resolve_phases(
        request.phases.as_deref(),
        reason,
        request.exempt_phases.as_deref(),
        &phase_settings,
    );
    let runs = |plan: &[(MaintenancePhase, Option<PhaseSkipReason>)], p: MaintenancePhase| {
        plan.iter().any(|(q, s)| *q == p && s.is_none())
    };
    let running: Vec<&str> = MaintenancePhase::ALL
        .into_iter()
        .filter(|p| runs(&plan, *p))
        .map(MaintenancePhase::as_str)
        .collect();
    tracing::debug!(
        module = MODULE,
        characterId = %character_id,
        reason = reason.as_str(),
        dryRun = dry_run,
        phasesJson = %json_text(&json!(running)),
        "Maintenance starting"
    );

    let mut corpus = if MaintenancePhase::ALL
        .into_iter()
        .any(|p| p != MaintenancePhase::Mirror && runs(&plan, p))
    {
        MaintenanceCorpus::load(db, &character_id)?
    } else {
        MaintenanceCorpus::new(&character_id)
    };
    let skip_of = |plan: &[(MaintenancePhase, Option<PhaseSkipReason>)], p: MaintenancePhase| {
        plan.iter().find(|(q, _)| *q == p).and_then(|(_, s)| *s)
    };
    let skipped = |report: &mut MaintenanceReport, phase: MaintenancePhase, why| {
        report.phases.push(PhaseOutcome {
            phase,
            status: PhaseStatus::Skipped,
            skipped_because: Some(why),
            duration_ms: 0,
        });
    };
    let ran = |report: &mut MaintenanceReport, phase: MaintenancePhase, t0: Instant| {
        report.phases.push(PhaseOutcome {
            phase,
            status: PhaseStatus::Ran,
            skipped_because: None,
            duration_ms: t0.elapsed().as_millis() as i64,
        });
    };

    // ── 1. Fold near-duplicates ───────────────────────────────────────────
    match skip_of(&plan, MaintenancePhase::FoldNearDuplicates) {
        Some(why) => skipped(&mut report, MaintenancePhase::FoldNearDuplicates, why),
        None => {
            let t0 = Instant::now();
            // "The vectors are the default profile's; fold on its scale
            // (TF-IDF cosines run lower)." v4's `findDefault` is a safeQuery
            // over a lazily-created table, so its catch (DEBUG `Could not read
            // the default embedding profile; folding on the neural scale`) is
            // unreachable through v4's real code — v5's `Err` is the `null`.
            let uid = user_id.clone();
            let provider = db
                .read_main(|c| crate::db::embedding_profiles::find_default(c, &uid))
                .ok()
                .flatten()
                .map(|p| p.provider);
            let thresholds = admission_thresholds_for_provider(provider.as_deref());
            let planned = plan_near_duplicate_folds(&corpus.all(), thresholds, now_ms as f64);
            let folding: usize = planned.groups.iter().map(|g| g.absorbed_ids.len()).sum();
            let mut fold = FoldPhaseReport {
                eligible: planned.eligible,
                deferred: planned.deferred,
                groups: planned.groups.clone(),
                folded: if dry_run { folding } else { 0 },
                groups_failed: 0,
                groups_deferred: 0,
            };
            if dry_run || planned.groups.is_empty() {
                apply_folds_to_corpus(&mut corpus, &planned.groups, None);
            } else {
                // "Each group re-embeds its survivor; under a budget the fold
                // gets its share and leaves the rest of its groups for the next
                // run."
                let deadline_ms = ctx.time_budget_ms.map(|b| b as f64 * FOLD_BUDGET_SHARE);
                let clock = run_elapsed.clone();
                let deadline = move || deadline_ms.is_some_and(|d| clock() as f64 >= d);
                let applied = apply_near_duplicate_folds(
                    db,
                    Some(embedding),
                    ApplyNearDuplicateFoldsRequest {
                        character_id: &character_id,
                        groups: &planned.groups,
                        user_id: &user_id,
                        deadline_passed: Some(&deadline),
                        host_zone_fallback: &ctx.host_zone_fallback,
                    },
                )
                .await?;
                fold.folded = applied.retired_ids.len();
                fold.groups_failed = applied.groups_failed;
                fold.groups_deferred = applied.groups_deferred;
                apply_folds_to_corpus(&mut corpus, &planned.groups, Some(&applied));
            }
            report.fold = Some(fold);
            ran(&mut report, MaintenancePhase::FoldNearDuplicates, t0);
        }
    }

    // ── 2. Consolidate ────────────────────────────────────────────────────
    let mut touched_subjects: Vec<String> = Vec::new();
    match skip_of(&plan, MaintenancePhase::Consolidate) {
        Some(why) => skipped(&mut report, MaintenancePhase::Consolidate, why),
        None => {
            let t0 = Instant::now();
            // "What the run's budget has left after the fold."
            let offset = run_elapsed();
            let budget_left = ctx.time_budget_ms.map(|b| 0i64.max(b - offset));
            let elapsed = ctx.elapsed.as_ref().map(|clock| {
                let clock = clock.clone();
                let f: ElapsedClock = std::sync::Arc::new(move || clock() - offset);
                f
            });
            let run = super::consolidate::run_consolidate_phase(
                db,
                completion,
                embedding,
                executor,
                &mut corpus,
                &holder,
                mature_after_days,
                RunConsolidationOptions {
                    dry_run,
                    // "Only this run's overrides: `consolidate` reads the
                    // stored settings itself and picks the cluster threshold
                    // for the profile's scale unless the run names one."
                    settings_override: request
                        .settings
                        .as_ref()
                        .and_then(|s| s.consolidation.clone()),
                    max_clusters_per_run: request
                        .settings
                        .as_ref()
                        .and_then(|s| s.max_clusters_per_run),
                    user_id: Some(user_id.clone()),
                    time_budget_ms: budget_left,
                    trigger: reason,
                    now_ms: Some(now_ms),
                    elapsed,
                    host_zone_fallback: ctx.host_zone_fallback.clone(),
                },
            )
            .await?;
            touched_subjects = run.touched_subjects;
            report.consolidation = Some(run.report);
            ran(&mut report, MaintenancePhase::Consolidate, t0);
        }
    }

    // ── 3. Demote ─────────────────────────────────────────────────────────
    match skip_of(&plan, MaintenancePhase::Demote) {
        Some(why) => skipped(&mut report, MaintenancePhase::Demote, why),
        None => {
            let t0 = Instant::now();
            let cap = request
                .settings
                .as_ref()
                .and_then(|s| s.cap)
                .unwrap_or_else(|| settings.maintenance.cap_for(&character_id) as f64);
            let demotion = plan_demotion(&corpus.all(), cap, now_ms as f64);
            if !dry_run && !demotion.demoted_ids.is_empty() {
                let (cid, ids) = (character_id.clone(), demotion.demoted_ids.clone());
                db.write(move |w| {
                    crate::services::memory_index::set_tier(
                        w.main().connection(),
                        &cid,
                        &ids,
                        Tier::Cold,
                        TierExtra::default(),
                    )
                })
                .await?;
            }
            let cold: Map<String, Value> =
                [("tier".to_string(), json!("cold"))].into_iter().collect();
            for id in &demotion.demoted_ids {
                corpus.patch(id, &cold);
            }
            tracing::debug!(
                module = MODULE,
                characterId = %character_id,
                cap = %js_num(cap),
                demotable = demotion.demotable,
                protected = demotion.protected,
                demoted = demotion.demoted,
                highestScoreDemoted = %demotion
                    .highest_score_demoted
                    .map(js_num)
                    .unwrap_or_else(|| "null".to_string()),
                "Demotion planned"
            );
            report.demote = Some(demotion);
            ran(&mut report, MaintenancePhase::Demote, t0);
        }
    }

    // ── 4. Retire ─────────────────────────────────────────────────────────
    match skip_of(&plan, MaintenancePhase::Retire) {
        Some(why) => skipped(&mut report, MaintenancePhase::Retire, why),
        None => {
            let t0 = Instant::now();
            let retention_days = phase_settings.cold_retention_days.unwrap_or(f64::NAN);
            let expired = plan_retention(&corpus.all(), retention_days, now_ms as f64);
            let mut retired_ids = expired.clone();
            if !dry_run && !expired.is_empty() {
                // "Retention never takes a digest with its last member."
                retired_ids =
                    retire_and_announce(db, expired, RetireOptions::new(RetireReason::Retention))
                        .await?
                        .retired_ids;
            }
            corpus.remove(&retired_ids);
            report.retire = Some(RetirePhaseReport {
                retention_days,
                retired: retired_ids.len(),
                retired_ids,
            });
            ran(&mut report, MaintenancePhase::Retire, t0);
        }
    }

    // ── 5. Mirror ─────────────────────────────────────────────────────────
    if runs(&plan, MaintenancePhase::Mirror) && touched_subjects.is_empty() {
        for entry in plan.iter_mut() {
            if entry.0 == MaintenancePhase::Mirror {
                entry.1 = Some(PhaseSkipReason::NothingTouched);
            }
        }
    }
    match skip_of(&plan, MaintenancePhase::Mirror) {
        Some(why) => skipped(&mut report, MaintenancePhase::Mirror, why),
        None => {
            let t0 = Instant::now();
            let deferred = !dry_run && ctx.mode == MaintenanceMode::Job;
            let files_written = if dry_run || deferred {
                0
            } else {
                crate::services::commonplace_mirror::mirror_commonplace_for_subjects(
                    db,
                    &character_id,
                    &touched_subjects,
                    Some(&now_iso),
                )
                .await
            };
            report.mirror = Some(MirrorPhaseReport {
                subjects: touched_subjects.clone(),
                files_written,
                deferred_to_commit: deferred,
            });
            if deferred {
                tracing::debug!(
                    module = MODULE,
                    characterId = %character_id,
                    subjects = touched_subjects.len(),
                    "Commonplace mirror deferred to the commit hook"
                );
            }
            ran(&mut report, MaintenancePhase::Mirror, t0);
        }
    }

    finish(&mut report);
    let rows_changed = changed_rows(&report);
    if !dry_run && ctx.mode == MaintenanceMode::InProcess {
        announce_maintenance(
            &summarize_maintenance(&report),
            rows_changed,
            report.mirror.as_ref().is_some_and(|m| m.files_written > 0),
            ctx.mode,
        );
    }

    tracing::info!(
        module = MODULE,
        characterId = %character_id,
        characterName = %or_null(report.character_name.as_deref()),
        reason = reason.as_str(),
        dryRun = dry_run,
        ranJson = %json_text(&json!(report.ran().iter().map(|p| p.as_str()).collect::<Vec<_>>())),
        folded = %opt_count(report.fold.as_ref().map(|f| f.folded)),
        digestsCreated = %opt_count(report.consolidation.as_ref().map(|c| c.stats.digests_created)),
        membersSuperseded = %opt_count(report.consolidation.as_ref().map(|c| c.stats.members_superseded)),
        demoted = %opt_count(report.demote.as_ref().map(|d| d.demoted)),
        retired = %opt_count(report.retire.as_ref().map(|r| r.retired)),
        durationMs = report.duration_ms,
        "Maintenance run complete"
    );
    Ok(MaintenanceRun {
        report,
        touched_subjects,
        rows_changed,
    })
}

/// JS truthiness of an optional JSON value.
fn js_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}
