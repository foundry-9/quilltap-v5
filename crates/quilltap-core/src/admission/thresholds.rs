//! Port of the admission-threshold block of v4
//! `lib/memory/settings-defaults.ts` (`8af764514`; the TF-IDF set and
//! `admissionThresholdsForProvider` from `4ae0c4ca6`) plus `ADMISSION_TOP_K`
//! (v4 `lib/memory/admission/neighbours.ts:35` — homed here because
//! `neighbours` is the stateful half, P4.D290's).
//!
//! v4's why, carried: the admission thresholds are not user-settable and live
//! in one place so there is one place to read them. `resolveClusterThreshold`
//! and the `MEMORY_SETTING_DEFAULTS` façade (consolidation / maintenance
//! settings) are not this module's (work order P4.D289 LR-14 "not this lane").

use serde::Serialize;

use super::types::serialize_js_number;
use crate::embedding_scale::is_lexical_embedding_provider;
use crate::reinforcement_footnotes::MAX_REINFORCEMENT_FOOTNOTES;

/// Admission's cut-offs: the cosine bands plus the date guard and footnote cap
/// (v4 `AdmissionThresholds`).
///
/// Derives `PartialEq` because v4's `thresholdScale` (P4.D290) is the IDENTITY
/// test `thresholds === ADMISSION_THRESHOLDS`, reproduced here by value — the
/// two sets differ on every cosine threshold, so value equality and identity
/// agree for every set this module hands out.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdmissionThresholds {
    /// At or above: the candidate is absorbed as a near-duplicate.
    #[serde(serialize_with = "serialize_js_number")]
    pub near_duplicate: f64,
    /// At or above: the candidate reinforces the existing row.
    #[serde(serialize_with = "serialize_js_number")]
    pub reinforce: f64,
    /// At or above (and under `reinforce`): the candidate is linked to the row.
    #[serde(serialize_with = "serialize_js_number")]
    pub related: f64,
    /// Two rows more than this many days apart are different occasions.
    #[serde(serialize_with = "serialize_js_number")]
    pub date_guard_days: f64,
    pub footnote_cap: usize,
}

/// Admission thresholds for neural embeddings (v4 `ADMISSION_THRESHOLDS`): the
/// cosines at which a candidate is absorbed as a near-duplicate, reinforces an
/// existing row, or is linked to it, plus the date guard (two rows more than
/// this many days apart are different occasions) and the footnote cap.
pub const ADMISSION_THRESHOLDS: AdmissionThresholds = AdmissionThresholds {
    near_duplicate: 0.90,
    reinforce: 0.85,
    related: 0.70,
    date_guard_days: 7.0,
    footnote_cap: MAX_REINFORCEMENT_FOOTNOTES,
};

/// Admission thresholds for the Built-in (TF-IDF) profile (v4
/// `ADMISSION_THRESHOLDS_TFIDF`). A bag of stemmed words scores a restatement
/// of one fact ~0.4–0.6 and different facts about one subject ≤0.1, where a
/// neural model gives 0.85+ and ~0.5; only near-verbatim text reaches 0.8.
/// Calibrated offline on the plugin's own vectorizer
/// (builtin-tfidf-memory-tuning §1, §3). The date guard and footnote cap are
/// shared.
pub const ADMISSION_THRESHOLDS_TFIDF: AdmissionThresholds = AdmissionThresholds {
    near_duplicate: 0.80,
    reinforce: 0.40,
    related: 0.20,
    ..ADMISSION_THRESHOLDS
};

/// How many neighbours admission decides over (v4 `ADMISSION_TOP_K`).
pub const ADMISSION_TOP_K: usize = 5;

/// The thresholds for vectors made by `provider` (v4
/// `admissionThresholdsForProvider`; `BUILTIN` is TF-IDF). An unknown or
/// missing provider reads as neural.
pub fn admission_thresholds_for_provider(provider: Option<&str>) -> &'static AdmissionThresholds {
    if is_lexical_embedding_provider(provider) {
        &ADMISSION_THRESHOLDS_TFIDF
    } else {
        &ADMISSION_THRESHOLDS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors v4's `provider-thresholds.test.ts` "picks the TF-IDF set …" and
    /// "keeps the bands ordered …".
    #[test]
    fn picks_the_tfidf_set_for_builtin_only() {
        assert_eq!(
            *admission_thresholds_for_provider(Some("BUILTIN")),
            ADMISSION_THRESHOLDS_TFIDF
        );
        assert_eq!(
            *admission_thresholds_for_provider(Some("OPENAI")),
            ADMISSION_THRESHOLDS
        );
        assert_eq!(
            *admission_thresholds_for_provider(None),
            ADMISSION_THRESHOLDS
        );
        let t = ADMISSION_THRESHOLDS_TFIDF;
        assert!(t.related < t.reinforce && t.reinforce < t.near_duplicate);
        assert_eq!(t.date_guard_days, ADMISSION_THRESHOLDS.date_guard_days);
        assert_eq!(t.footnote_cap, ADMISSION_THRESHOLDS.footnote_cap);
        assert_ne!(ADMISSION_THRESHOLDS, ADMISSION_THRESHOLDS_TFIDF);
    }
}
