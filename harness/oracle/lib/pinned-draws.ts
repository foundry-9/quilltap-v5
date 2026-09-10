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
 *
 * # Per-CASE draws (P4.87)
 *
 * The two tier-3 families pin ONCE per file and consume across every case, so a
 * corpus row that wants its own sequence needs the cursor rewound between cases.
 * The handle therefore carries [`PinnedDraws.reset`] rather than a
 * `withDraws(seq, fn)` scope: the oracles already own the pin's lifetime (one
 * `pinDraws` at the top, one `restore()` in the tail), and a scope would have
 * meant nesting a second install inside that one. `reset` rewinds the cursor,
 * swaps the values, and clears `consumed` — so a case's `consumed` is ITS draws,
 * not the file's running total.
 *
 * On the Rust side the mirror is ONE `DrawSource` per case, cloned into every
 * carrier that case reaches (the initial `ProcessClock`, each chained turn's,
 * and `execute_turn_chain`'s own) — clones share the cursor, so the draws arrive
 * in one order exactly as v4's single pinned `Math.random` delivers them across
 * `handleSendMessage`. Building a fresh sequence per carrier restarts at index
 * 0 and is only invisible while the array has one element.
 */
export interface PinnedDraws {
  /** The draws actually consumed, in order — a comparand, not decoration. */
  readonly consumed: number[];
  /** Restore the real `Math.random`. */
  restore(): void;
  /**
   * Rewind to the start of `values` (or of a NEW sequence when one is given)
   * and forget what was consumed. Called once per corpus case so a row's draws
   * are its own; `reset([0])` is the no-op every untouched row wants.
   */
  reset(values?: number[]): void;
}

/**
 * Pins `Math.random` to `values` and records what gets consumed. Always pair
 * with `restore()` in a `finally`.
 */
export function pinDraws(values: number[]): PinnedDraws {
  const real = Math.random;
  const consumed: number[] = [];
  let current = values;
  let i = 0;
  Math.random = () => {
    const v = current.length === 0 ? 0 : current[Math.min(i, current.length - 1)];
    i += 1;
    consumed.push(v);
    return v;
  };
  return {
    consumed,
    restore() {
      Math.random = real;
    },
    reset(next?: number[]) {
      if (next !== undefined) current = next;
      i = 0;
      // In place: `consumed` is handed out by reference above.
      consumed.length = 0;
    },
  };
}
