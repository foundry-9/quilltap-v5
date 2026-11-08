/**
 * The raised hand's STATUS machine (v4 `9eab8d2da`, `useSSEStreaming.ts:234-255`
 * and `:637-662`, `:1650-1677`) — what the composer's chip says about the
 * operator's raised hand, computed from the stream's floor frames.
 *
 * raised → asked (a `handAsked` frame: the hand is being put to the next
 * speaker) → yielded / declined (a `handAnswered` frame) — or, when the stream
 * ends with the hand still only raised, `unasked` ("the room fell quiet before
 * anyone was asked"). Before `9eab8d2da` the hand was one boolean and a decline
 * was a toast; the chip now says each step, so neither the asking nor a
 * decline passes without a word, and the toast is gone.
 *
 * Pure: the Salon holds the `RaisedHandStatus | null` signal and applies these.
 *
 * @module chat/raised-hand-status
 */

// === P4.D384 LANE-REMOVE (C4 H2): P4.D385's chat/raised-hand.ts owns this type; the unifier replaces this block with `import type { RaisedHandStatus } from './raised-hand'` ===
export type RaisedHandStatus =
  | { kind: 'raised' }
  | { kind: 'asked'; characterName: string }
  | { kind: 'yielded'; characterName: string }
  | { kind: 'declined'; characterName: string }
  | { kind: 'unasked' }
// === end P4.D384 LANE-REMOVE (C4 H2) ===

/** The hand is still up: raised, or being put to someone (v4 `isHandUp`). */
export function isHandUp(status: RaisedHandStatus | null): boolean {
  return status?.kind === 'raised' || status?.kind === 'asked';
}

/**
 * The stream is over (v4 `settleHandAtStreamEnd`, applied in each stream's
 * `finally`): a hand never asked about tells the operator so; a hand mid-ask
 * has nobody left to ask and simply goes down; an answer stays on show.
 */
export function settleHandAtStreamEnd(status: RaisedHandStatus | null): RaisedHandStatus | null {
  if (status?.kind === 'raised') return { kind: 'unasked' };
  if (status?.kind === 'asked') return null;
  return status;
}

/** The operator dismissed the chip (v4 `dismissHandStatus`): an up hand stays. */
export function dismissedHandStatus(status: RaisedHandStatus | null): RaisedHandStatus | null {
  return isHandUp(status) ? status : null;
}

/**
 * A raise answered `active: false` (nobody speaking), or a raise that failed:
 * lower the hand ONLY if it is still merely raised — a `handAsked` that beat
 * the reply must survive it (v4's `prev?.kind === 'raised' ? null : prev`).
 */
export function lowerIfOnlyRaised(status: RaisedHandStatus | null): RaisedHandStatus | null {
  return status?.kind === 'raised' ? null : status;
}

/** A floor frame's hand transition (v4 `applyFloorFrame`'s two hand arms). */
export function handStatusFromFloor(
  frame:
    | { kind: 'handAsked'; characterName: string }
    | { kind: 'handAnswered'; characterName: string; yielded: boolean },
): RaisedHandStatus {
  if (frame.kind === 'handAsked') {
    return { kind: 'asked', characterName: frame.characterName || 'The next speaker' };
  }
  return {
    kind: frame.yielded ? 'yielded' : 'declined',
    characterName: frame.characterName || 'The speaker',
  };
}
