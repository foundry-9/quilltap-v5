//! A stable sort that does **not** validate its comparator — the Rust-side
//! stand-in for V8's `Array.prototype.sort` when the ported comparator is one of
//! v4's non-total ones.
//!
//! ## Why this exists (work order P4.14)
//!
//! Several of v4's comparators are not total orders. The memory injector's is
//! the load-bearing one: *"if the weight gap exceeds 0.05 compare weights, else
//! compare scores"* makes weights 0.00 / 0.04 / 0.08 give `a == b`, `b == c`,
//! and `a < c`. That is v4's own logic, ported verbatim, and it is equally
//! broken on both sides — but the two runtimes react differently:
//!
//! * **V8's TimSort never checks its comparator.** It produces some
//!   arbitrary-but-deterministic order and returns.
//! * **Rust's `slice::sort_by` (driftsort) does check**, in `smallsort`'s
//!   bidirectional merge, and **panics**: `user-provided comparison function
//!   does not correctly implement a total order`. On a live Salon send that
//!   panic killed the turn (and, because `run_summary_check` runs at the end of
//!   `process_message`, the context-summary fold with it).
//!
//! Fixing the comparator is not an option: its decisions ARE v4's ranking
//! output. So the sort changes instead. For any **self-consistent** comparator
//! every stable sort yields the identical permutation, so this function is
//! v4-identical everywhere the comparator forms a total order. Inside the
//! contradictory region both engines already produce arbitrary garbage that
//! nothing semantic depends on, and which no committed differential slate
//! reaches — see the P4.14 lane record.
//!
//! ## The implementation
//!
//! A classic bottom-up stable merge sort, run over **indices** so no element is
//! moved until the final permutation is applied with plain `slice::swap`. That
//! keeps the whole thing safe code (there is no `unsafe` here — "unchecked"
//! refers only to the comparator's total-order contract going unverified) and
//! bounds the allocation at `2 * len` `usize`.
//!
//! Ties take from the left half, which is what makes the merge stable and
//! matches the tie rule `slice::sort_by` uses — so on well-behaved comparators
//! this is a drop-in replacement, as the property test below asserts.

use std::cmp::Ordering;

/// Stably sort `slice` with `compare`, never panicking on a comparator that
/// fails to implement a total order.
///
/// On any self-consistent comparator this produces exactly what
/// `slice::sort_by` would. On an inconsistent one it produces a deterministic
/// permutation of the input rather than aborting the caller.
pub fn stable_sort_by_unchecked<T, F>(slice: &mut [T], mut compare: F)
where
    F: FnMut(&T, &T) -> Ordering,
{
    let n = slice.len();
    if n < 2 {
        return;
    }

    // `src[k]` = the original index of the element currently ranked k-th.
    let mut src: Vec<usize> = (0..n).collect();
    let mut dst: Vec<usize> = vec![0; n];

    let mut width = 1usize;
    while width < n {
        let mut lo = 0usize;
        while lo < n {
            let mid = (lo + width).min(n);
            let hi = (lo + 2 * width).min(n);
            let (mut l, mut r, mut k) = (lo, mid, lo);
            while l < mid && r < hi {
                // Take from the left unless the left is strictly greater — the
                // stable tie rule.
                if compare(&slice[src[l]], &slice[src[r]]) == Ordering::Greater {
                    dst[k] = src[r];
                    r += 1;
                } else {
                    dst[k] = src[l];
                    l += 1;
                }
                k += 1;
            }
            while l < mid {
                dst[k] = src[l];
                l += 1;
                k += 1;
            }
            while r < hi {
                dst[k] = src[r];
                r += 1;
                k += 1;
            }
            lo = hi;
        }
        std::mem::swap(&mut src, &mut dst);
        width *= 2;
    }

    // Invert `src` into `to`: the final position of the element that started at
    // each original index. Then walk the cycles, applying it with swaps —
    // `to` rides along, so `to[i]` always names the home of whatever now sits
    // at `i`, and every swap seats at least one element for good.
    let mut to = dst;
    for (rank, &original) in src.iter().enumerate() {
        to[original] = rank;
    }
    for i in 0..n {
        while to[i] != i {
            let j = to[i];
            slice.swap(i, j);
            to.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic LCG — the tests must not vary run to run.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }
    }

    /// A self-consistent comparator over the key only, so the payload index
    /// makes ties observable: equality of the whole output against
    /// `slice::sort_by` proves order AND stability at once.
    fn by_key(a: &(u32, usize), b: &(u32, usize)) -> Ordering {
        a.0.cmp(&b.0)
    }

    #[test]
    fn matches_slice_sort_by_on_self_consistent_comparators() {
        let mut rng = Lcg(0x5EED_1234_9ABC_DEF0);
        // Sweep lengths across driftsort's fast-path boundaries (insertion sort
        // under ~20, smallsort merges above) and both tie densities.
        for len in [0usize, 1, 2, 3, 7, 19, 20, 21, 40, 41, 64, 128, 200, 501] {
            for &distinct in &[1u32, 3, 17, 100_000] {
                let input: Vec<(u32, usize)> = (0..len)
                    .map(|i| ((rng.next() % u64::from(distinct)) as u32, i))
                    .collect();

                let mut expected = input.clone();
                expected.sort_by(by_key);

                let mut actual = input.clone();
                stable_sort_by_unchecked(&mut actual, by_key);

                assert_eq!(
                    actual, expected,
                    "len={len} distinct={distinct}: diverged from slice::sort_by"
                );
            }
        }
    }

    #[test]
    fn preserves_input_order_within_ties() {
        let mut rng = Lcg(0xC0FF_EE00_1111_2222);
        let mut v: Vec<(u32, usize)> = (0..300).map(|i| ((rng.next() % 5) as u32, i)).collect();
        stable_sort_by_unchecked(&mut v, by_key);
        for w in v.windows(2) {
            assert!(w[0].0 <= w[1].0, "not sorted: {:?} then {:?}", w[0], w[1]);
            if w[0].0 == w[1].0 {
                assert!(w[0].1 < w[1].1, "tie reordered: {:?} then {:?}", w[0], w[1]);
            }
        }
    }

    /// The memory injector's comparator, reduced to its arithmetic: the 0.05
    /// epsilon window makes near neighbours compare by score and distant ones by
    /// weight, so `a == b`, `b == c`, `a < c`. `slice::sort_by` panics on this
    /// slate (see `memory_injector::intransitive_comparator_regression`); this
    /// must not.
    #[test]
    fn survives_the_epsilon_gap_comparator() {
        let mut rng = Lcg(0xDEAD_BEEF_0000_0001);
        let mut v: Vec<(f64, f64)> = (0..200)
            .map(|i| ((i as f64) * 0.04, 1.0 - (i as f64) * 0.001))
            .collect();
        for i in (1..v.len()).rev() {
            v.swap(i, (rng.next() as usize) % (i + 1));
        }
        let before = v.clone();

        stable_sort_by_unchecked(&mut v, |a, b| {
            let weight_diff = b.0 - a.0;
            if weight_diff.abs() > 0.05 {
                return weight_diff.partial_cmp(&0.0).unwrap();
            }
            (b.1 - a.1).partial_cmp(&0.0).unwrap()
        });

        // No panic, and nothing lost or duplicated.
        let mut sorted_before = before;
        let mut sorted_after = v;
        let key = |x: &(f64, f64)| x.0.to_bits();
        sorted_before.sort_by_key(key);
        sorted_after.sort_by_key(key);
        assert_eq!(
            sorted_before, sorted_after,
            "the output is not a permutation of the input"
        );
    }

    /// A comparator that answers `Equal` to everything — the degenerate
    /// non-order. Stability means the input comes back untouched.
    #[test]
    fn all_equal_comparator_is_an_identity() {
        let input: Vec<usize> = (0..200).collect();
        let mut v = input.clone();
        stable_sort_by_unchecked(&mut v, |_, _| Ordering::Equal);
        assert_eq!(v, input);
    }
}
