//! One typed read of every memory knob — P4.D288 (contract C1-5), porting v4
//! `lib/memory/settings.ts` (`e0ac7493a`, memory refactor §3.1) at the
//! `83334b9a5` pin.
//!
//! "Every memory setting lives in `instance_settings` — `memoryRecall`,
//! `memoryConsolidation`, `memoryExtractionMode`, `memoryExtractionLimits` and
//! `memoryMaintenance` — and is read here, in one call, with its defaults from
//! the schemas. `capFor` is the one per-character cap resolver (override →
//! `perCharacterCap` → the schema default); nothing else resolves a cap."
//!
//! [`resolve_memory_settings`] is the ONE home of the façade (P4.D296's
//! extraction and P4.D297's maintenance consume it); [`resolve_memory_
//! maintenance`] is v4's maintenance-only read. The cap resolver itself is
//! [`MemoryMaintenanceSettings::cap_for`]. Each getter keeps its own WARN +
//! defaults (`db::instance_settings::read_json_setting`); a failing read never
//! fails the façade.

use rusqlite::Connection;

use crate::admission::thresholds::{
    AdmissionThresholds, ADMISSION_THRESHOLDS, ADMISSION_THRESHOLDS_TFIDF,
};
use crate::db::instance_settings::{
    get_memory_consolidation_settings, get_memory_extraction_limits_settings,
    get_memory_extraction_mode_settings, get_memory_maintenance_settings,
    get_memory_recall_settings, MemoryConsolidationSettings, MemoryExtractionLimits,
    MemoryExtractionModeSettings, MemoryMaintenanceSettings, MemoryRecallSettings,
};
use crate::memory_scoring::relevance::{DEFAULT_MIN_COSINE_NEURAL, DEFAULT_MIN_COSINE_TFIDF};
use crate::recall_tuning::{ResolvedRecallTuning, DEFAULT_RECALL_TUNING};

/// v4 `MemorySettings.extraction` — the rate limiter and the OTHER pass's grain.
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryExtractionSettings {
    pub limits: MemoryExtractionLimits,
    pub mode: MemoryExtractionModeSettings,
}

/// v4 `MemorySettings.recall.minCosine` — the per-scale relevance floors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinCosine {
    pub neural: f64,
    pub tfidf: f64,
}

/// v4 `MemorySettings.recall` — the stored recall settings with the default
/// tuning and the cosine floors attached.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedMemoryRecall {
    pub settings: MemoryRecallSettings,
    pub tuning: ResolvedRecallTuning,
    pub min_cosine: MinCosine,
}

/// v4 `ResolvedMemoryMaintenance & { consolidation, coldRetentionDays }` — the
/// maintenance settings (with [`MemoryMaintenanceSettings::cap_for`], v4's
/// `capFor`), the consolidation settings, and the retention window
/// (`consolidation.coldRetentionDays`, read here so maintenance reads one
/// place).
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedMemoryMaintenance {
    pub settings: MemoryMaintenanceSettings,
    pub consolidation: MemoryConsolidationSettings,
    pub cold_retention_days: Option<i64>,
}

impl ResolvedMemoryMaintenance {
    /// v4 `capFor(characterId)`.
    pub fn cap_for(&self, character_id: &str) -> i64 {
        self.settings.cap_for(character_id)
    }
}

/// v4 `MemorySettings` — every memory knob, typed, defaults filled.
#[derive(Clone, Debug, PartialEq)]
pub struct MemorySettings {
    pub extraction: MemoryExtractionSettings,
    /// Neural-embedding admission thresholds.
    pub admission: AdmissionThresholds,
    /// Built-in (TF-IDF) admission thresholds.
    pub admission_tfidf: AdmissionThresholds,
    pub recall: ResolvedMemoryRecall,
    pub maintenance: ResolvedMemoryMaintenance,
}

/// v4 `resolveMemoryMaintenance()` (`settings.ts:90-92`) — only the maintenance
/// settings (one instance-setting read), for paths that need nothing but the
/// cap.
pub fn resolve_memory_maintenance(main: &Connection) -> MemoryMaintenanceSettings {
    get_memory_maintenance_settings(main)
}

/// v4 `resolveMemorySettings(userId?)` (`settings.ts:99-130`): the five
/// instance-wide reads, then DEBUG `[MemorySettings] Resolved memory settings
/// {userId, maintenanceEnabled, perCharacterCap, capOverrides, consolidationEnabled,
/// otherPass}` (`userId` is `userId ?? null` — accepted "so callers need not
/// change when a knob becomes per-user"; `capOverrides` is the override COUNT).
pub fn resolve_memory_settings(main: &Connection, user_id: Option<&str>) -> MemorySettings {
    let limits = get_memory_extraction_limits_settings(main);
    let mode = get_memory_extraction_mode_settings(main);
    let recall = get_memory_recall_settings(main).unwrap_or_default();
    let consolidation = get_memory_consolidation_settings(main);
    let maintenance = get_memory_maintenance_settings(main);
    tracing::debug!(
        userId = %user_id.unwrap_or("null"),
        maintenanceEnabled = maintenance.enabled,
        perCharacterCap = maintenance.per_character_cap,
        capOverrides = maintenance.per_character_cap_overrides.len(),
        consolidationEnabled = consolidation.enabled,
        otherPass = %mode.other_pass,
        "[MemorySettings] Resolved memory settings"
    );
    let cold_retention_days = consolidation.cold_retention_days;
    MemorySettings {
        extraction: MemoryExtractionSettings { limits, mode },
        admission: ADMISSION_THRESHOLDS,
        admission_tfidf: ADMISSION_THRESHOLDS_TFIDF,
        recall: ResolvedMemoryRecall {
            settings: recall,
            tuning: DEFAULT_RECALL_TUNING,
            min_cosine: MinCosine {
                neural: DEFAULT_MIN_COSINE_NEURAL,
                tfidf: DEFAULT_MIN_COSINE_TFIDF,
            },
        },
        maintenance: ResolvedMemoryMaintenance {
            settings: maintenance,
            consolidation,
            cold_retention_days,
        },
    }
}
