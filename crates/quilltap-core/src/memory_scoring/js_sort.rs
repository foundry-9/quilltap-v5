//! V8's `Array.prototype.sort` for the memory ports' NON-TOTAL comparators.
//!
//! `sortBySalience` (`b.s − a.s || a.index − b.index`) and admission's decide
//! (`b.cosine − a.cosine`) are not total orders once a key is NaN: a NaN
//! difference is falsy (salience falls through to the index) or neither `< 0`
//! nor `>= 0` (the decide). Rust's `sort_by` panics on such a comparator, and
//! [`crate::stable_sort::stable_sort_by_unchecked`] (a merge sort) avoids the
//! panic but produces a DIFFERENT permutation from V8's on exactly those
//! inputs (measured on the `nan-key` row of the `memory_scoring` family).
//!
//! V8's TimSort sorts any array shorter than its minimum run (64) with
//! `CountAndMakeRun` followed by `BinaryInsertionSort` — no merging — so for
//! `len < 64` this module reproduces V8 step for step (the comparator returns
//! a JS number and every branch tests it exactly as V8's Torque does: `order
//! < 0`, `order >= 0`, so NaN takes V8's side of each test). Longer arrays go
//! through the unchecked stable merge sort: identical to V8 for every total
//! comparator; for a NaN-keyed array of 64+ the permutation is a recorded
//! residual (admission's neighbour list is at most 20; a NaN salience needs
//! an unparseable `createdAt`).

use crate::stable_sort::stable_sort_by_unchecked;
use std::cmp::Ordering;

/// V8's minimum run length below which the whole array is one
/// binary-insertion-sorted run.
const V8_MIN_RUN: usize = 64;

/// Sort `items` as V8's `Array.prototype.sort(compare)` would, where
/// `compare` returns the JS comparator's number.
pub fn js_array_sort_by<T, F>(items: &mut [T], mut compare: F)
where
    F: FnMut(&T, &T) -> f64,
{
    let n = items.len();
    if n < 2 {
        return;
    }
    if n >= V8_MIN_RUN {
        stable_sort_by_unchecked(items, |a, b| {
            let order = compare(a, b);
            if order < 0.0 {
                Ordering::Less
            } else if order > 0.0 {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        });
        return;
    }
    let run = count_and_make_run(items, &mut compare);
    if run < n {
        binary_insertion_sort(items, run, &mut compare);
    }
}

/// V8 `CountAndMakeRun(0, n)`: the length of the leading run, reversing it in
/// place when it is strictly descending.
fn count_and_make_run<T, F>(items: &mut [T], compare: &mut F) -> usize
where
    F: FnMut(&T, &T) -> f64,
{
    let n = items.len();
    if n == 1 {
        return 1;
    }
    let mut run = 2usize;
    let is_descending = compare(&items[1], &items[0]) < 0.0;
    let mut previous = 1usize;
    for idx in 2..n {
        let order = compare(&items[idx], &items[previous]);
        if is_descending {
            if order >= 0.0 {
                break;
            }
        } else if order < 0.0 {
            break;
        }
        previous = idx;
        run += 1;
    }
    if is_descending {
        items[..run].reverse();
    }
    run
}

/// V8 `BinaryInsertionSort(low = 0, start, high = n)`.
fn binary_insertion_sort<T, F>(items: &mut [T], start: usize, compare: &mut F)
where
    F: FnMut(&T, &T) -> f64,
{
    let n = items.len();
    let mut start = if start == 0 { 1 } else { start };
    while start < n {
        let (mut left, mut right) = (0usize, start);
        while left < right {
            let mid = left + ((right - left) >> 1);
            let order = compare(&items[start], &items[mid]);
            if order < 0.0 {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        items[left..=start].rotate_right(1);
        start += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::js_array_sort_by;

    #[test]
    fn total_comparators_sort_stably() {
        let mut v = vec![(3, 'a'), (1, 'b'), (3, 'c'), (2, 'd'), (1, 'e')];
        js_array_sort_by(&mut v, |a, b| (a.0 - b.0) as f64);
        assert_eq!(v, vec![(1, 'b'), (1, 'e'), (2, 'd'), (3, 'a'), (3, 'c')]);
    }

    #[test]
    fn a_descending_run_is_reversed() {
        let mut v = vec![5, 4, 3, 2, 1];
        js_array_sort_by(&mut v, |a, b| (*a - *b) as f64);
        assert_eq!(v, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn long_arrays_take_the_merge_path_and_still_sort() {
        let mut v: Vec<i64> = (0..200).rev().collect();
        js_array_sort_by(&mut v, |a, b| (*a - *b) as f64);
        assert_eq!(v, (0..200).collect::<Vec<_>>());
    }
}
