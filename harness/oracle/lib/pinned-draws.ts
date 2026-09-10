/**
 * The ordered `Math.random` pin (P4.D172).
 *
 * Every pre-rotation oracle froze `Math.random` to a SCALAR, because every
 * `Math.random()` site drew exactly once. v4 `2aca73ad6`'s `drawCycleOrder`
 * broke that: it draws once per remaining candidate — N draws for an N-seat
 * room, over a shrinking pool with a recomputed total weight — so one pinned
 * value makes every position of the permutation land on the same relative
 * offset, which is neither what v4 does nor anything a test can compare.
 *
 * The pin is therefore a SEQUENCE. It REPEATS ITS LAST VALUE once exhausted,
 * which is what makes a one-element array behave exactly like the old scalar
 * freeze — so re-spelling `Math.random = () => 0` as `pinDraws([0])` is
 * byte-neutral for every existing corpus, and an arm that needs a real sequence
 * later needs no recipe change and no oracle-shape change.
 *
 * Mirrors `quilltap_core::weighted_random::DrawSource::sequence` exactly,
 * including the empty case (`0`).
 */
export interface PinnedDraws {
  /** The draws actually consumed, in order — a comparand, not decoration. */
  readonly consumed: number[];
  /** Restore the real `Math.random`. */
  restore(): void;
}

/**
 * Pins `Math.random` to `values` and records what gets consumed. Always pair
 * with `restore()` in a `finally`.
 */
export function pinDraws(values: number[]): PinnedDraws {
  const real = Math.random;
  const consumed: number[] = [];
  let i = 0;
  Math.random = () => {
    const v = values.length === 0 ? 0 : values[Math.min(i, values.length - 1)];
    i += 1;
    consumed.push(v);
    return v;
  };
  return {
    consumed,
    restore() {
      Math.random = real;
    },
  };
}
