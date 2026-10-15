//! Memory MAINTENANCE — v4 `lib/memory/maintenance/` (`104e986c7`, Phase F of
//! the memory refactor, at the `83334b9a5` pin): ONE `MEMORY_MAINTENANCE` job
//! per character running five ordered, individually skippable phases over ONE
//! corpus read — fold near-duplicates → consolidate → demote → retire →
//! mirror — answered by ONE parent-side ledger, swept daily by ONE scheduler.
//!
//! It replaces `MEMORY_HOUSEKEEPING` + `MEMORY_CONSOLIDATION`: the
//! housekeeping service, its outcome cache, the consolidation triggers, the
//! scheduled-housekeeping driver, both handlers and the System-Tools dedup
//! service are gone (v4 deleted their twins in `104e986c7`).
//!
//! Files (v4's): [`types`], [`corpus`], [`fold_near_duplicates`],
//! [`consolidate`], [`demote`], [`run`], [`ledger`], [`triggers`],
//! [`scheduler`]; v5's job seams: [`runner`] (R-N), [`job_results`] (R-M),
//! [`after_job`] (the completion region's one call).
//!
//! **Log lines** keep v4's child-logger bindings as a `module` field
//! (`memory:maintenance`, `…:corpus`, `…:fold`, `…:consolidate`, `…:ledger`,
//! `…:triggers`, `…:scheduler`); a structured array / object field renders
//! through the `…Json` file-layer convention (§R.5).

pub mod after_job;
pub mod consolidate;
pub mod corpus;
pub mod demote;
pub mod fold_near_duplicates;
pub mod job_results;
pub mod ledger;
pub mod run;
pub mod runner;
pub mod scheduler;
pub mod triggers;
pub mod types;

pub use after_job::after_job_completed;

use serde_json::Value;

/// A JS number as v4's logger renders it (`2000`, `0.5`, `NaN`).
pub(crate) fn js_num(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    crate::db::js_number_to_json(x).to_string()
}

/// A structured field's JSON text (the `…Json` convention).
pub(crate) fn json_text(v: &Value) -> String {
    v.to_string()
}
