//! The maintenance ledger — v4 `lib/memory/maintenance/ledger.ts`
//! (`104e986c7`, at the `83334b9a5` pin).
//!
//! v4's why, carried: "One record per character: the last run of each phase,
//! the last demotion's effectiveness, and the watermark debounce. It replaces
//! the job child's housekeeping outcome cache, consolidation's parent-side
//! debounce map, and the four throttles that sat between them;
//! `shouldEnqueue` is the one question every trigger asks. In-memory by
//! design. A restart forgets it; the first question after a boot seeds it once
//! from the recent `MEMORY_MAINTENANCE` job rows, so a restart does not
//! unleash a storm of watermark runs on a character that was just
//! maintained."
//!
//! v5 shape: a process-global `OnceLock<Mutex<…>>` (the retired
//! `housekeeping_outcome_cache.rs` precedent); v4's `processSingleton` (the
//! bug-235 Next.js module-instance workaround) is NO-PORT. Entries keep v4's
//! `Map` insertion order (the status read lists them in it), and each entry's
//! `lastRun` keeps its object's first-assignment key order. v4 shares one
//! in-flight seeding promise between concurrent callers; here two concurrent
//! seeders both read and the first to take the lock applies the rows (the
//! second sees `seeded`) — the same rows land once.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use serde_json::{json, Map, Value};

use super::json_text;
use super::types::{MaintenancePhase, MaintenanceSummary};
use crate::db::background_jobs::BackgroundJobsRepository;
use crate::db::runtime::Db;
use crate::episodic::js_date_parse_ms;
use crate::services::queue_service::MEMORY_MAINTENANCE;

const MODULE: &str = "memory:maintenance:ledger";

/// v4 `CAP_WATERMARK_FRACTION`: the fraction of the per-character cap at
/// which the cap watermark trips.
pub const CAP_WATERMARK_FRACTION: f64 = 0.9;

/// v4 `WATERMARK_CHECK_DEBOUNCE_MS`: "minimum gap between two watermark
/// checks for one character (each check is two COUNTs)".
pub const WATERMARK_CHECK_DEBOUNCE_MS: f64 = 2.0 * 60.0 * 1000.0;

/// v4 `DEMOTE_THROTTLE_MS`: "a cap-driven run is not re-enqueued while the
/// last demotion is younger than this".
pub const DEMOTE_THROTTLE_MS: f64 = 15.0 * 60.0 * 1000.0;

/// v4 `INEFFECTIVE_BACKOFF_MS`: "…nor for this long after a demotion that was
/// practically ineffective".
pub const INEFFECTIVE_BACKOFF_MS: f64 = 60.0 * 60.0 * 1000.0;

/// v4 `CONSOLIDATE_THROTTLE_MS`: "a watermark consolidation is not
/// re-enqueued while the last one is younger than this".
pub const CONSOLIDATE_THROTTLE_MS: f64 = 6.0 * 60.0 * 60.0 * 1000.0;

/// v4 `MIN_EFFECTIVE_DEMOTIONS`: "a demotion is effective when it moved at
/// least this many rows…"
pub const MIN_EFFECTIVE_DEMOTIONS: f64 = 10.0;

/// v4 `EXCESS_RATIO_THRESHOLD`: "…or this fraction of the excess, whichever
/// is larger".
pub const EXCESS_RATIO_THRESHOLD: f64 = 0.01;

/// v4 `PhaseRecord {at, reason}` — `reason` carried as v4 carries it (a
/// seeded payload's `reason ?? 'manual'`, unvalidated).
#[derive(Clone, Debug, PartialEq)]
struct PhaseRecord {
    at: f64,
    reason: Value,
}

/// v4 `LedgerEntry`.
#[derive(Clone, Debug, Default, PartialEq)]
struct LedgerEntry {
    /// When each phase last ran (live runs only) — first-assignment order.
    last_run: Vec<(String, PhaseRecord)>,
    /// The last run's summary, as reported.
    last_summary: Option<Value>,
    /// The last demotion: `(at, demoted, excess)`.
    last_demotion: Option<(f64, Value, Value)>,
    /// When a watermark run asking for each phase was last enqueued.
    last_enqueued: HashMap<String, f64>,
    /// When the watermark last looked at this character.
    last_watermark_check: Option<f64>,
}

impl LedgerEntry {
    fn set_last_run(&mut self, phase: &str, record: PhaseRecord) {
        match self.last_run.iter_mut().find(|(p, _)| p == phase) {
            Some((_, r)) => *r = record,
            None => self.last_run.push((phase.to_string(), record)),
        }
    }

    fn last_run_at(&self, phase: &str) -> Option<f64> {
        self.last_run
            .iter()
            .find(|(p, _)| p == phase)
            .map(|(_, r)| r.at)
    }
}

#[derive(Default)]
struct LedgerState {
    order: Vec<String>,
    entries: HashMap<String, LedgerEntry>,
    seeded: bool,
}

impl LedgerState {
    /// v4 `entryFor(characterId)` — creates the entry (at the end of the
    /// order) when absent.
    fn entry_for(&mut self, character_id: &str) -> &mut LedgerEntry {
        if !self.entries.contains_key(character_id) {
            self.order.push(character_id.to_string());
        }
        self.entries.entry(character_id.to_string()).or_default()
    }
}

fn state() -> MutexGuard<'static, LedgerState> {
    static STATE: OnceLock<Mutex<LedgerState>> = OnceLock::new();
    STATE
        .get_or_init(|| Mutex::new(LedgerState::default()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// v4 `resetMaintenanceLedgerForTests()`.
pub fn reset_maintenance_ledger_for_tests() {
    let mut s = state();
    *s = LedgerState::default();
}

/// v4 `ensureSeeded()`: "seed the ledger once per process from recent
/// maintenance jobs — the durable floor under the in-memory record." v4's
/// `findRecentByType` is a fallback `safeQuery` over `findByFilter` (a failed
/// read logs `Error finding entities by filter` and answers `[]`), so the
/// seed's own catch — WARN `Maintenance ledger could not seed from recent
/// jobs`, "a failed read leaves it unseeded" — is unreachable through v4's
/// real code: a failed read seeds the ledger with nothing. Pinned negative.
fn ensure_seeded(db: &Db) {
    if state().seeded {
        return;
    }
    let recent = db
        .read_main(|c| {
            Ok(crate::db::fallback::find_by_filter_or_empty(
                "background_jobs",
                || BackgroundJobsRepository::new(c).find_recent_by_type(MEMORY_MAINTENANCE, 100),
            ))
        })
        .unwrap_or_default();
    let mut s = state();
    if s.seeded {
        return;
    }
    for job in &recent {
        if job.status != "COMPLETED" && job.status != "PROCESSING" {
            continue;
        }
        let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
        let Some(character_id) = payload
            .get("characterId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Some(at) = js_date_parse_ms(&job.updated_at) else {
            continue;
        };
        let at = at as f64;
        let reason = match payload.get("reason") {
            None | Some(Value::Null) => json!("manual"),
            Some(v) => v.clone(),
        };
        let phases: Vec<String> = match payload.get("phases") {
            Some(Value::Array(list)) => list
                .iter()
                .filter_map(|p| p.as_str().map(str::to_string))
                .collect(),
            _ => super::types::phase_names(&MaintenancePhase::ALL),
        };
        let entry = s.entry_for(character_id);
        for phase in &phases {
            let prior = entry.last_run_at(phase);
            if prior.is_none_or(|p| p < at) {
                entry.set_last_run(
                    phase,
                    PhaseRecord {
                        at,
                        reason: reason.clone(),
                    },
                );
            }
        }
    }
    s.seeded = true;
    tracing::debug!(
        module = MODULE,
        jobs = recent.len(),
        characters = s.entries.len(),
        "Maintenance ledger seeded from recent jobs"
    );
}

/// v4 `recordMaintenanceOutcome(summary)`: "record a finished live run. Dry
/// runs and skipped runs change nothing." `at = Date.parse(finishedAt) ||
/// Date.now()` (NaN or 0 → now).
pub fn record_maintenance_outcome(summary: &MaintenanceSummary) {
    if summary.dry_run || summary.skipped {
        return;
    }
    let at = match js_date_parse_ms(&summary.finished_at) {
        Some(ms) if ms != 0 => ms as f64,
        _ => crate::clock::now_unix_ms() as f64,
    };
    let ran = summary.ran();
    {
        let mut s = state();
        let entry = s.entry_for(&summary.character_id);
        for phase in &ran {
            entry.set_last_run(
                phase.as_str(),
                PhaseRecord {
                    at,
                    reason: json!(summary.reason.as_str()),
                },
            );
        }
        entry.last_summary = Some(summary.to_json());
        if let Some((demoted, excess)) = summary.demote() {
            entry.last_demotion = Some((at, demoted, excess));
        }
    }
    tracing::debug!(
        module = MODULE,
        characterId = %summary.character_id,
        reason = summary.reason.as_str(),
        ranJson = %json_text(&json!(ran.iter().map(|p| p.as_str()).collect::<Vec<_>>())),
        demoted = %summary.count("demote", "demoted"),
        "Maintenance outcome recorded"
    );
}

/// v4 `WatermarkReading` — what the watermark measured for one character.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WatermarkReading {
    /// Hot AUTO rows (what the cap measures).
    pub demotable: i64,
    pub cap: i64,
    /// Hot, never-considered, non-digest rows.
    pub unconsidered: i64,
    pub consolidation_watermark: i64,
    pub maintenance_enabled: bool,
    pub consolidation_enabled: bool,
}

fn number_of(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}

/// v4 `lastDemotionIneffective(entry, nowMs)`: "true when the last demotion
/// moved too few rows to be worth repeating soon. That includes a demotion
/// with nothing to move: a character between 90% and 100% of its cap trips
/// the watermark on every insert, and a run that finds no excess has nothing
/// to do there for the next hour either."
fn last_demotion_ineffective(entry: &LedgerEntry, now_ms: f64) -> bool {
    let Some((at, demoted, excess)) = &entry.last_demotion else {
        return false;
    };
    if now_ms - at >= INEFFECTIVE_BACKOFF_MS {
        return false;
    }
    let needed = crate::jsnum::math_max(
        MIN_EFFECTIVE_DEMOTIONS,
        (number_of(excess) * EXCESS_RATIO_THRESHOLD).floor(),
    );
    number_of(demoted) < needed
}

/// v4 `ranWithin(entry, phase, windowMs, nowMs)`: "true when the phase ran,
/// or a run asking for it was enqueued, inside the window".
fn ran_within(entry: &LedgerEntry, phase: MaintenancePhase, window_ms: f64, now_ms: f64) -> bool {
    let last = entry
        .last_run_at(phase.as_str())
        .unwrap_or(f64::NEG_INFINITY)
        .max(
            entry
                .last_enqueued
                .get(phase.as_str())
                .copied()
                .unwrap_or(f64::NEG_INFINITY),
        );
    now_ms - last < window_ms
}

/// The trigger reasons [`should_enqueue`] distinguishes.
pub use super::types::MaintenanceReason;

/// v4 `shouldEnqueue(characterId, reason, reading?, nowMs)` (`ledger.ts:
/// 196-231`): "the one trigger question: should a run be enqueued for this
/// character, and with which phases? Returns null when nothing is due."
/// `scheduled` / `manual` → every phase; `watermark` needs a reading: the cap
/// watermark (demotable ≥ 90% of the cap) asks for fold → demote → retire
/// unless a demotion ran in the last 15 minutes or the last one was
/// ineffective within the hour; the consolidation watermark (unconsidered >
/// `watermark`) asks for consolidate unless one ran in the last six hours.
/// The mirror rides with consolidate.
pub fn should_enqueue(
    db: &Db,
    character_id: &str,
    reason: MaintenanceReason,
    reading: Option<&WatermarkReading>,
    now_ms: f64,
) -> Option<Vec<MaintenancePhase>> {
    if reason != MaintenanceReason::Watermark {
        return Some(MaintenancePhase::ALL.to_vec());
    }
    let reading = reading?;
    ensure_seeded(db);
    let mut s = state();
    let entry = s.entry_for(character_id).clone();
    drop(s);
    let mut phases: Vec<MaintenancePhase> = Vec::new();

    if reading.maintenance_enabled
        && reading.demotable as f64 >= (reading.cap as f64 * CAP_WATERMARK_FRACTION).floor()
    {
        if ran_within(&entry, MaintenancePhase::Demote, DEMOTE_THROTTLE_MS, now_ms) {
            tracing::debug!(
                module = MODULE,
                characterId = %character_id,
                demotable = reading.demotable,
                cap = reading.cap,
                "Cap watermark reached but a demotion ran recently"
            );
        } else if last_demotion_ineffective(&entry, now_ms) {
            let last = entry
                .last_demotion
                .as_ref()
                .map(|(at, d, e)| {
                    json!({ "at": crate::db::js_number_to_json(*at), "demoted": d, "excess": e })
                })
                .unwrap_or(Value::Null);
            let last_json = json_text(&last);
            tracing::debug!(
                module = MODULE,
                characterId = %character_id,
                lastDemotionJson = %last_json,
                "Cap watermark reached but the last demotion was ineffective; backing off"
            );
        } else {
            phases.extend([
                MaintenancePhase::FoldNearDuplicates,
                MaintenancePhase::Demote,
                MaintenancePhase::Retire,
            ]);
        }
    }
    if reading.consolidation_enabled && reading.unconsidered > reading.consolidation_watermark {
        if ran_within(
            &entry,
            MaintenancePhase::Consolidate,
            CONSOLIDATE_THROTTLE_MS,
            now_ms,
        ) {
            tracing::debug!(
                module = MODULE,
                characterId = %character_id,
                unconsidered = reading.unconsidered,
                "Consolidation watermark reached but a consolidation ran recently"
            );
        } else {
            phases.extend([MaintenancePhase::Consolidate, MaintenancePhase::Mirror]);
        }
    }
    if phases.is_empty() {
        None
    } else {
        Some(
            MaintenancePhase::ALL
                .into_iter()
                .filter(|p| phases.contains(p))
                .collect(),
        )
    }
}

/// v4 `takeWatermarkCheck(characterId, nowMs)`: "debounce for the watermark:
/// true (and stamped) when this character may be measured now".
pub fn take_watermark_check(character_id: &str, now_ms: f64) -> bool {
    let mut s = state();
    let entry = s.entry_for(character_id);
    if entry
        .last_watermark_check
        .is_some_and(|last| now_ms - last < WATERMARK_CHECK_DEBOUNCE_MS)
    {
        return false;
    }
    entry.last_watermark_check = Some(now_ms);
    true
}

/// v4 `noteEnqueued(characterId, phases, nowMs)`: "a watermark run of
/// `phases` has been enqueued: its throttles start now".
pub fn note_enqueued(character_id: &str, phases: &[MaintenancePhase], now_ms: f64) {
    let mut s = state();
    let entry = s.entry_for(character_id);
    for phase in phases {
        entry
            .last_enqueued
            .insert(phase.as_str().to_string(), now_ms);
    }
}

/// v4 `LedgerStatus` — the ledger as the status surfaces show it:
/// `{characterId, lastRun {<phase>: {at, reason}}, lastSummary, lastDemotion
/// {at, demoted, excess, effective} | null}`.
#[derive(Clone, Debug, PartialEq)]
pub struct LedgerStatus {
    pub json: Value,
}

impl LedgerStatus {
    pub fn to_json(&self) -> Value {
        self.json.clone()
    }
}

fn iso(at: f64) -> String {
    crate::clock::iso_from_unix_ms(at as i64)
}

/// v4 `getMaintenanceLedgerStatus(characterId?)`: seeds; the one id asked (an
/// unknown id answers an EMPTY entry, not created) or every entry in
/// insertion order (an empty id is JS-falsy: all). `effective` is evaluated AT
/// the demotion (`min(now, lastDemotion.at)` — the hour window always
/// applies).
pub fn get_maintenance_ledger_status(db: &Db, character_id: Option<&str>) -> Vec<LedgerStatus> {
    ensure_seeded(db);
    let now_ms = crate::clock::now_unix_ms() as f64;
    let s = state();
    let ids: Vec<String> = match character_id.filter(|c| !c.is_empty()) {
        Some(id) => vec![id.to_string()],
        None => s.order.clone(),
    };
    ids.into_iter()
        .map(|id| {
            let entry = s.entries.get(&id).cloned().unwrap_or_default();
            let mut last_run = Map::new();
            for (phase, record) in &entry.last_run {
                last_run.insert(
                    phase.clone(),
                    json!({ "at": iso(record.at), "reason": record.reason }),
                );
            }
            let last_demotion = match &entry.last_demotion {
                Some((at, demoted, excess)) => json!({
                    "at": iso(*at),
                    "demoted": demoted,
                    "excess": excess,
                    "effective": !last_demotion_ineffective(&entry, now_ms.min(*at)),
                }),
                None => Value::Null,
            };
            let mut m = Map::new();
            m.insert("characterId".into(), json!(id));
            m.insert("lastRun".into(), Value::Object(last_run));
            m.insert(
                "lastSummary".into(),
                entry.last_summary.clone().unwrap_or(Value::Null),
            );
            m.insert("lastDemotion".into(), last_demotion);
            LedgerStatus {
                json: Value::Object(m),
            }
        })
        .collect()
}
