/**
 * The raised hand's status — what the composer's raised-hand chip says while a
 * multi-character turn runs (v4 `9eab8d2da`).
 *
 * A port of the type and predicate v4 keeps beside its stream reducer
 * (`hooks/useSSEStreaming.ts:228-244` at the pin `624d0d25c`): the operator
 * raises a hand, the next speaker is asked to yield (`asked`), and the turn
 * either hands the floor back (`yielded`), carries on regardless (`declined`),
 * or falls quiet before anyone was asked (`unasked`).
 *
 * This module is the SHARED contract of P4.D385's HANDOFF H2: the chip
 * (`raised-hand-chip.ts`) renders a status, and the Salon spine (P4.D384)
 * computes one from the stream's frames. `settleHandAtStreamEnd` — the rule
 * that lowers a hand when a stream ends — is the reducer's, so it lives with
 * P4.D384's spine, not here.
 */

export type RaisedHandStatus =
  | { kind: 'raised' }
  | { kind: 'asked'; characterName: string }
  | { kind: 'yielded'; characterName: string }
  | { kind: 'declined'; characterName: string }
  | { kind: 'unasked' };

/**
 * True while the operator's hand is up: raised and waiting, or raised and being
 * weighed by the next speaker. The three settled states mean the hand is down.
 */
export function isHandUp(status: RaisedHandStatus | null): boolean {
  return status?.kind === 'raised' || status?.kind === 'asked';
}
