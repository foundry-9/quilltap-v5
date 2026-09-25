/**
 * Per-chat Concierge helpers — the client twin of v4
 * `lib/services/dangerous-content/chat-override.ts` (three states since v4
 * `4d370a90f`, #75; read at `ce2f1dabf`), which v4 imports into BOTH its server
 * code and its React components.
 *
 * A chat is in one of three states, stored in `chats.conciergeMode`:
 *
 * ```text
 *   | State         | Text / cheap LLM / images   | Failover on refusal | Concierge may move it |
 *   | 'moderated'   | ordinary providers first    | yes                 | yes (to Unmoderated)  |
 *   | 'unmoderated' | the uncensored desk only    | n/a (already there) | n/a                   |
 *   | 'locked'      | ordinary providers only     | never               | never                 |
 * ```
 *
 * Who put the chat in its state — the operator, or the Concierge after
 * refusals or on the classifier's reading — is *provenance*
 * (`conciergeModeSetBy` / `conciergeModeReason`). It is a note on the badge
 * and in the helper text, never a separate state and never a colour.
 *
 * The legacy pair (`conciergeOverride`, `isDangerousChat`) is no longer read
 * by any routing or display decision — v4's test "ignores the legacy pair
 * entirely" pins it, and so does this twin's recorded oracle. (v4's
 * `deriveConciergeModeFromLegacy` / `withConciergeModeFromLegacy` are the
 * import/restore mapping — SERVER-only, the Rust core's to port, not this
 * module's.)
 *
 * NOTHING outside this module should read the stored columns or the payload
 * keys raw. Derive everything from {@link getConciergeState}, or ask one of the
 * purpose-named questions:
 *
 *   - "Take the uncensored routes right now?" → {@link shouldUseUncensoredRoute}
 *     (or {@link conciergeStateUsesUncensoredRoute}, given a derived state)
 *   - "Paint danger styling in the UI?"        → {@link shouldShowDangerStyling}
 *   - "May the Concierge move this chat?"      → {@link isClassifierOnDuty}
 *   - "May a refusal be rerouted?"             → {@link mayFailOver}
 *     (or {@link conciergeStateMayFailOver}, given a derived state)
 *
 * Every function here is pinned against v4's REAL module, executed and emitted
 * to `concierge-state.v4.json` by `harness/oracle/cases/concierge-chat-override.mjs`
 * — see `concierge-state.oracle.spec.ts`.
 */

import type { ConciergeProvenance, ConciergeReason, ConciergeState } from '../core/core-contract';

export type { ConciergeProvenance, ConciergeReason, ConciergeState } from '../core/core-contract';

/** Every state, in the order the controls list them. */
export const CONCIERGE_STATES: readonly ConciergeState[] = ['moderated', 'unmoderated', 'locked'];

/**
 * A chat row (the stored columns) or a chat payload the server already derived
 * (the chat GET and every list carry `conciergeState` / `conciergeSetBy` /
 * `conciergeReason`, never the columns). The helpers read whichever is
 * present, COLUMNS FIRST, so client and server ask the same functions.
 */
export interface ConciergeChatView {
  conciergeMode?: ConciergeState | null;
  conciergeModeSetBy?: Exclude<ConciergeProvenance, null> | null;
  conciergeModeReason?: ConciergeReason | null;
  conciergeState?: ConciergeState | null;
  conciergeSetBy?: Exclude<ConciergeProvenance, null> | null;
  conciergeReason?: ConciergeReason | null;
}

/**
 * THE canonical derivation of a chat's Concierge state. A missing or NULL
 * column reads as `'moderated'`.
 */
export function getConciergeState(chat: ConciergeChatView | null | undefined): ConciergeState {
  const mode = chat?.conciergeMode ?? chat?.conciergeState;
  return mode === 'unmoderated' || mode === 'locked' ? mode : 'moderated';
}

/**
 * Who put the chat in its current state. Always `null` for a Moderated chat —
 * Moderated is where every chat starts and where the operator returns it, so
 * there is nothing to attribute. An unknown or missing setBy on a
 * non-Moderated chat reads as the operator's.
 */
export function getConciergeProvenance(chat: ConciergeChatView | null | undefined): ConciergeProvenance {
  if (getConciergeState(chat) === 'moderated') return null;
  const by = chat?.conciergeModeSetBy ?? chat?.conciergeSetBy;
  return by === 'concierge' || by === 'operator' ? by : 'operator';
}

/** Why the chat is in its current state; `null` for a Moderated chat. */
export function getConciergeReason(chat: ConciergeChatView | null | undefined): ConciergeReason | null {
  if (getConciergeState(chat) === 'moderated') return null;
  return chat?.conciergeModeReason ?? chat?.conciergeReason ?? null;
}

/**
 * Does this state take the uncensored route? The state-only twin of
 * {@link shouldUseUncensoredRoute}, for callers that already hold a derived
 * state (list payloads carry `conciergeState` rather than the columns). THE
 * one place that says which state takes the uncensored route.
 */
export function conciergeStateUsesUncensoredRoute(state: ConciergeState): boolean {
  return state === 'unmoderated';
}

/**
 * Should this chat take the Concierge's uncensored routes right now? True only
 * for Unmoderated, whoever set it.
 */
export function shouldUseUncensoredRoute(chat: ConciergeChatView | null | undefined): boolean {
  return conciergeStateUsesUncensoredRoute(getConciergeState(chat));
}

/**
 * Should the UI paint this chat with danger styling? True for Unmoderated,
 * regardless of provenance: the provenance goes in the tooltip and helper
 * text, never in colour. (This INVERTS the four-state rule, under which an
 * operator-set Uncensored chat was deliberately left unpainted.)
 */
export function shouldShowDangerStyling(chat: ConciergeChatView | null | undefined): boolean {
  return getConciergeState(chat) === 'unmoderated';
}

/**
 * May the Concierge act on this chat of his own accord — the classifier job,
 * the scheduled scan, the per-turn trigger and the refusal ledger's
 * auto-switch? True only for Moderated.
 */
export function isClassifierOnDuty(chat: ConciergeChatView | null | undefined): boolean {
  return getConciergeState(chat) === 'moderated';
}

/**
 * May a refusal in this state be rerouted to an uncensored understudy? The
 * state-only twin of {@link mayFailOver}. False only for Locked, which must
 * never reach the uncensored desk.
 */
export function conciergeStateMayFailOver(state: ConciergeState): boolean {
  return state !== 'locked';
}

/**
 * May a refusal on this chat be rerouted to an uncensored understudy? A
 * chatless call (no chat to ask) reads as Moderated.
 */
export function mayFailOver(chat: ConciergeChatView | null | undefined): boolean {
  return conciergeStateMayFailOver(getConciergeState(chat));
}
