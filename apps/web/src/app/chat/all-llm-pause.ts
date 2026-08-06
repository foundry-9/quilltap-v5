/**
 * All-LLM pause thresholds — a client-safe TypeScript twin of v4
 * `lib/chat/turn-manager/all-llm-pause.ts` (its pure `nextPauseAt` family; v4's
 * unused server logger import is dropped). The Rust twins that actually drive the
 * pause live in `crates/quilltap-core/src/all_llm_pause.rs`; these exist only so
 * the AllLLMPauseModal can name the next threshold (v4 `ChatModals.tsx:436`).
 *
 * Pause thresholds double: 3, 6, 12, 24, 48, 96, 192…
 */

/** Initial pause interval (number of turns before the first pause). */
export const INITIAL_PAUSE_INTERVAL = 3;

/**
 * The next pause interval using logarithmic doubling.
 * Sequence: 3, 6, 12, 24, 48, 96…
 */
export function getNextPauseInterval(currentInterval: number): number {
  if (currentInterval === 0) {
    return INITIAL_PAUSE_INTERVAL;
  }
  return currentInterval * 2;
}

/**
 * Whether the chat should pause at this turn count — true when it exactly
 * matches a pause threshold (3, 6, 12, 24, 48, 96…).
 */
export function shouldPauseForAllLLM(turnCount: number): boolean {
  if (turnCount <= 0) {
    return false;
  }

  let threshold = INITIAL_PAUSE_INTERVAL;

  while (threshold <= turnCount) {
    if (turnCount === threshold) {
      return true;
    }
    threshold *= 2;
  }

  return false;
}

/**
 * The current pause threshold for a turn count — the last threshold that was or
 * should have been reached, or 0 if none.
 */
export function getCurrentPauseThreshold(turnCount: number): number {
  if (turnCount < INITIAL_PAUSE_INTERVAL) {
    return 0;
  }

  let threshold = INITIAL_PAUSE_INTERVAL;
  let lastThreshold = 0;

  while (threshold <= turnCount) {
    lastThreshold = threshold;
    threshold *= 2;
  }

  return lastThreshold;
}

/** The next pause threshold for a turn count. */
export function getNextPauseThreshold(turnCount: number): number {
  let threshold = INITIAL_PAUSE_INTERVAL;

  while (threshold <= turnCount) {
    threshold *= 2;
  }

  return threshold;
}

/** How many turns remain until the next pause. */
export function getTurnsUntilNextPause(turnCount: number): number {
  const nextThreshold = getNextPauseThreshold(turnCount);
  return nextThreshold - turnCount;
}
