//! Port of v4's lib/chat/turn-manager/weighted-random.ts — the one weighted
//! pick, plus the ordered draw source that feeds it.
//!
//! v4 `2aca73ad6` moved `pickWeightedRandom` out of `selection.ts` into its own
//! module precisely so three consumers could reach it without importing each
//! other: the per-turn speaker pick (`selection.ts`), the whole-cycle rotation
//! draw (`cycle-order.ts`), and the opening-character pick at chat creation.
//! This module is that home in v5.
//!
//! # Why the injection is a SEQUENCE
//!
//! Every earlier port of a `Math.random()` site injected a single `random01:
//! f64`, because every site drew exactly once. `drawCycleOrder` breaks that: it
//! calls `pickWeightedRandom` once per remaining candidate — N draws for an
//! N-seat room, over a shrinking pool with a recomputed total weight. A single
//! pinned value would make every position of the permutation land on the same
//! relative offset, which is not what v4 does and not a distribution any test
//! could compare.
//!
//! So the injection is an ordered draw source, [`DrawSource`], and the oracle
//! pins `Math.random` to an ARRAY, emitting the draws it actually consumed. A
//! ONE-element array is exactly the old constant pin — which is what makes
//! every pre-rotation family's expectations survive unchanged.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// The ordered source of `Math.random()` values a turn path consumes.
///
/// Cloneable, `Send + Sync`, and interior-mutable, because v5's carriers are not
/// a call stack: the value lives in [`crate::services::orchestrator::ProcessClock`]
/// and `StepDeps`, is cloned into spawned work, and crosses `.await` points. A
/// `&mut dyn FnMut() -> f64` cannot travel any of those; this handle can, while
/// still delivering the draws in order to whoever holds it.
///
/// Two constructors cover every caller:
///   * [`DrawSource::constant`] — the pre-rotation shape. Every call returns the
///     same value, exactly as pinning `Math.random` to a scalar did.
///   * [`DrawSource::sequence`] — the rotation shape. Returns each value in
///     turn, then REPEATS THE LAST one forever. The repeat is what makes a
///     one-element sequence identical to a constant, so a corpus row that pins
///     `[0.5]` and one that pins `0.5` are the same row.
#[derive(Clone)]
pub struct DrawSource(Arc<dyn Fn() -> f64 + Send + Sync>);

impl DrawSource {
    /// A source that answers `value` to every draw (the pre-rotation pin).
    pub fn constant(value: f64) -> Self {
        DrawSource(Arc::new(move || value))
    }

    /// A source that answers `values` in order, then repeats the last one.
    ///
    /// An EMPTY sequence answers `0.0` to every draw. That is the same answer
    /// `random01: 0.0` gave at the frozen-zero call sites, so an empty pin
    /// degrades to the shape those sites already had rather than panicking on a
    /// hot path.
    pub fn sequence(values: Vec<f64>) -> Self {
        let cursor = AtomicUsize::new(0);
        DrawSource(Arc::new(move || {
            if values.is_empty() {
                return 0.0;
            }
            let i = cursor.fetch_add(1, Ordering::Relaxed);
            values[i.min(values.len() - 1)]
        }))
    }

    /// An arbitrary draw source — the host's OS CSPRNG closure.
    pub fn from_fn(f: impl Fn() -> f64 + Send + Sync + 'static) -> Self {
        DrawSource(Arc::new(f))
    }

    /// One `Math.random()`.
    pub fn draw(&self) -> f64 {
        (self.0)()
    }
}

impl std::fmt::Debug for DrawSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DrawSource(..)")
    }
}

impl Default for DrawSource {
    /// `0.0` — the frozen-zero convention several v5 call sites already carried.
    fn default() -> Self {
        DrawSource::constant(0.0)
    }
}

/// The outcome of one [`pick_weighted_random`] call — v4's
/// `{item, weights, randomValue, equalWeights}`.
///
/// `weights` is PARALLEL TO `items` (v4's `number[]`), not keyed by anything:
/// the cycle draw has no ids to key by, and the per-turn pick builds its own
/// `{id: weight}` record from this array exactly as v4 does.
#[derive(Clone, Debug, PartialEq)]
pub struct WeightedPick<'a, T> {
    /// The chosen item.
    pub item: &'a T,
    /// Each item's weight, in `items` order. Reset to all-1 when
    /// `equal_weights`.
    pub weights: Vec<f64>,
    /// The draw, scaled by the total weight — v4's `randomValue`.
    pub random_value: f64,
    /// True when the weights summed to nothing and every item was made equally
    /// likely. v4's per-turn pick LOGS on this; the cycle draw discards it.
    pub equal_weights: bool,
}

/// Weighted-random pick: each item's chance is its weight over the total (v4
/// `pickWeightedRandom`, weighted-random.ts).
///
/// When the weights sum to nothing every item is equally likely instead, and
/// `equal_weights` says so. The gate is v4's `totalWeight <= 0`, NOT `== 0` — a
/// negative weight is a zero-total for this purpose, and v5's old per-turn copy
/// tested `== 0.0`, which would have let a negative total through to a scan that
/// can only fall out the bottom.
///
/// Exactly ONE value comes off `draws`, whatever the outcome — including the
/// unreachable empty-`items` case, so the consumed-draw count is faithful.
///
/// # Panics
/// On empty `items`, after the draw. v4 returns `items[-1]` (`undefined`) there
/// and every caller dereferences it immediately; no caller of either
/// implementation can reach it.
pub fn pick_weighted_random<'a, T, F>(
    items: &'a [T],
    weight_of: F,
    draws: &DrawSource,
) -> WeightedPick<'a, T>
where
    F: Fn(&T) -> f64,
{
    let mut weights: Vec<f64> = items.iter().map(&weight_of).collect();
    let mut total_weight: f64 = weights.iter().sum();
    let equal_weights = total_weight <= 0.0;
    if equal_weights {
        weights.iter_mut().for_each(|w| *w = 1.0);
        total_weight = items.len() as f64;
    }
    let random_value = draws.draw() * total_weight;
    let mut cumulative = 0.0;
    for (i, item) in items.iter().enumerate() {
        cumulative += weights[i];
        if random_value < cumulative {
            return WeightedPick {
                item,
                weights,
                random_value,
                equal_weights,
            };
        }
    }
    WeightedPick {
        item: items
            .last()
            .expect("pick_weighted_random over an empty item list"),
        weights,
        random_value,
        equal_weights,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_answers_the_same_value_forever() {
        let d = DrawSource::constant(0.42);
        assert_eq!(d.draw(), 0.42);
        assert_eq!(d.draw(), 0.42);
        assert_eq!(d.draw(), 0.42);
    }

    #[test]
    fn a_one_element_sequence_is_a_constant() {
        // The neutrality property every pre-rotation family relies on.
        let d = DrawSource::sequence(vec![0.5]);
        assert_eq!(d.draw(), 0.5);
        assert_eq!(d.draw(), 0.5);
    }

    #[test]
    fn a_sequence_is_ordered_then_repeats_its_last() {
        let d = DrawSource::sequence(vec![0.1, 0.9, 0.3]);
        assert_eq!(d.draw(), 0.1);
        assert_eq!(d.draw(), 0.9);
        assert_eq!(d.draw(), 0.3);
        assert_eq!(d.draw(), 0.3);
    }

    #[test]
    fn an_empty_sequence_answers_zero() {
        let d = DrawSource::sequence(Vec::new());
        assert_eq!(d.draw(), 0.0);
        assert_eq!(d.draw(), 0.0);
    }

    #[test]
    fn pick_scans_cumulatively_like_v4() {
        // Weights 0.9 / 0.3 / 0.8 (total 2.0) — the corpus trio.
        let items = ["A", "B", "C"];
        let w = |it: &&str| match *it {
            "A" => 0.9,
            "B" => 0.3,
            _ => 0.8,
        };
        // rv 0.2 < 0.9 → A
        let p = pick_weighted_random(&items, w, &DrawSource::constant(0.1));
        assert_eq!(*p.item, "A");
        assert!((p.random_value - 0.2).abs() < 1e-12);
        assert_eq!(p.weights, vec![0.9, 0.3, 0.8]);
        assert!(!p.equal_weights);
        // rv 1.0 → cumulative 0.9 no, 1.2 yes → B
        assert_eq!(
            *pick_weighted_random(&items, w, &DrawSource::constant(0.5)).item,
            "B"
        );
        // rv 1.9 → C
        assert_eq!(
            *pick_weighted_random(&items, w, &DrawSource::constant(0.95)).item,
            "C"
        );
    }

    #[test]
    fn a_nonpositive_total_resets_to_equal_weights() {
        let items = ["A", "B"];
        let p = pick_weighted_random(&items, |_| 0.0, &DrawSource::constant(0.6));
        assert!(p.equal_weights);
        assert_eq!(p.weights, vec![1.0, 1.0]);
        // rv 1.2 → cumulative 1.0 no, 2.0 yes → B
        assert_eq!(*p.item, "B");

        // v4's gate is `<= 0`, so a NEGATIVE total is an equal-weights total
        // too. v5's old per-turn copy tested `== 0.0` and would have scanned a
        // negative total, falling out of the loop onto the last item whatever
        // the draw.
        let neg = pick_weighted_random(&items, |_| -1.0, &DrawSource::constant(0.1));
        assert!(neg.equal_weights);
        assert_eq!(*neg.item, "A");
    }

    #[test]
    fn one_draw_per_call() {
        let d = DrawSource::sequence(vec![0.1, 0.9]);
        let items = ["A", "B"];
        // Two calls take exactly two values, in order.
        let first = pick_weighted_random(&items, |_| 1.0, &d);
        let second = pick_weighted_random(&items, |_| 1.0, &d);
        assert!((first.random_value - 0.2).abs() < 1e-12);
        assert!((second.random_value - 1.8).abs() < 1e-12);
    }

    #[test]
    fn a_clone_shares_the_cursor() {
        // The carriers CLONE this handle (ProcessClock is cloned into the chain
        // loop); a clone that restarted the sequence would silently re-draw the
        // first value for every chained turn.
        let d = DrawSource::sequence(vec![0.1, 0.9]);
        let c = d.clone();
        assert_eq!(d.draw(), 0.1);
        assert_eq!(c.draw(), 0.9);
    }
}
