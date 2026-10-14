//! Port of v4 `lib/memory/scoring/salience.ts` — how much a memory is worth
//! SAYING.
//!
//! `salience = reinforcedImportance × 0.5^(daysSince(max(createdAt,
//! lastReinforcedAt)) / 30)`, with NO floor. v4's why, carried: it is the
//! only "worth" used to order anything a model will see — the recall blend,
//! the fallback head, Carina, the inter-character section, archive fill,
//! first-message context and the search tool's final sort. It replaces the
//! old floored `calculateEffectiveWeight`, whose 0.70 floor leaked into
//! ranking at four call sites. Whether a memory is worth KEEPING is a
//! different question, answered by [`super::protection`].

use super::js_sort::js_array_sort_by;
use crate::jsnum::math_max;

/// Days for the decay factor to reach 0.5.
pub const SALIENCE_HALF_LIFE_DAYS: f64 = 30.0;

const DAY_MS: f64 = 86_400_000.0;

/// The fields salience reads (v4 `SalienceInput`). Times are epoch ms as v4's
/// `new Date(x).getTime()` reads them — NaN for an unparseable stamp; build
/// with [`SalienceInput::from_iso`] to get v4's parsing and truthiness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SalienceInput {
    pub importance: f64,
    /// `reinforcedImportance` — `None` for null / absent (v4 `??`).
    pub reinforced_importance: Option<f64>,
    pub created_at_ms: f64,
    /// `lastReinforcedAt` — `None` when absent or empty (JS-falsy).
    pub last_reinforced_at_ms: Option<f64>,
}

impl SalienceInput {
    /// Build from the row's stored strings with v4's semantics: `createdAt`
    /// through `new Date()` (NaN when unparseable), `lastReinforcedAt` only
    /// when truthy.
    pub fn from_iso(
        importance: f64,
        reinforced_importance: Option<f64>,
        created_at: &str,
        last_reinforced_at: Option<&str>,
    ) -> Self {
        SalienceInput {
            importance,
            reinforced_importance,
            created_at_ms: super::date_ms(created_at),
            last_reinforced_at_ms: super::truthy_date_ms(last_reinforced_at),
        }
    }
}

/// v4 `SalienceBreakdown`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SalienceBreakdown {
    /// The ordering key: `base × decay`.
    pub salience: f64,
    /// `reinforcedImportance ?? importance`.
    pub base: f64,
    /// `0.5^(daysSince / 30)`.
    pub decay: f64,
    /// Days since the decay reference time (never negative).
    pub days_since: f64,
}

/// Decay reference point for a memory: `max(createdAt, lastReinforcedAt)`
/// (v4 `referenceTimeMs`, moved byte-identical from the deleted weighting
/// module).
///
/// v4's why, carried: passive retrieval (`lastAccessedAt`) never resets the
/// decay clock — only creation or reinforcement with new information does.
/// Every decay or age calculation in the scoring module derives its
/// reference time from here. An absent `lastReinforcedAt` reads as 0, and
/// JS `Math.max` propagates a NaN `createdAt`.
pub fn reference_time_ms(created_at_ms: f64, last_reinforced_at_ms: Option<f64>) -> f64 {
    math_max(created_at_ms, last_reinforced_at_ms.unwrap_or(0.0))
}

/// How much a memory is worth saying at `now_ms` (v4 `salience`).
pub fn salience(m: &SalienceInput, now_ms: f64) -> SalienceBreakdown {
    let base = m.reinforced_importance.unwrap_or(m.importance);
    let days_since = math_max(
        0.0,
        (now_ms - reference_time_ms(m.created_at_ms, m.last_reinforced_at_ms)) / DAY_MS,
    );
    let decay = 0.5_f64.powf(days_since / SALIENCE_HALF_LIFE_DAYS);
    SalienceBreakdown {
        salience: base * decay,
        base,
        decay,
        days_since,
    }
}

/// Order items by salience, highest first, stable for ties (v4
/// `sortBySalience`). ONE `now_ms` serves the whole sort. v4's comparator is
/// `b.s − a.s || a.index − b.index`: a NaN difference is falsy, so a NaN key
/// falls through to the index order — a non-total comparator, so the sort is
/// V8's own algorithm ([`super::js_sort`]), never Rust's `sort_by` (which
/// panics on one).
pub fn sort_by_salience<T>(
    items: Vec<T>,
    view: impl Fn(&T) -> SalienceInput,
    now_ms: f64,
) -> Vec<T> {
    let mut keyed: Vec<(usize, f64, T)> = items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let s = salience(&view(&item), now_ms).salience;
            (index, s, item)
        })
        .collect();
    js_array_sort_by(&mut keyed, |a, b| {
        let d = b.1 - a.1;
        // `d || index-difference`: NaN and ±0 are falsy.
        if d.is_nan() || d == 0.0 {
            a.0 as f64 - b.0 as f64
        } else {
            d
        }
    });
    keyed.into_iter().map(|(_, _, item)| item).collect()
}
