/**
 * Established facts — the predicates that recognise a fact row (Deus Ex
 * Machina, v4 `db6190b44`).
 *
 * A transcription of the client-safe half of v4's `lib/chat/established-fact.ts`
 * (at the pin `624d0d25c`), character for character. The operator can declare
 * something **true** in a Salon chat two ways:
 *
 * - **A seat line.** Their own USER row, flagged `isEstablishedFact`. It keeps
 *   its speaker; the frame says what follows is true and known to everyone.
 * - **A machine fact.** A Host row (`systemSender: 'host'`,
 *   `systemKind: 'established-fact'`, no participant, public). It belongs to no
 *   speaker.
 *
 * Only the predicates and the kind constant live here (ruling PL-1). v4's
 * module also carries the frames, the reminder header, the transcript rule and
 * the speaker label — every one of them MODEL-FACING, so they live on the
 * server (`quilltap-core`, P4.D372) beside the guard that keeps the UI name out
 * of every model's prompt. The SPA never builds a frame.
 */

/** `systemKind` of a machine-delivered fact (a Host row). */
export const ESTABLISHED_FACT_KIND = 'established-fact';

export type EstablishedFactKind = 'seat' | 'announcement';

/** The fields a fact predicate reads. Any message shape carries them. */
export interface EstablishedFactFields {
  isEstablishedFact?: boolean | null;
  systemSender?: string | null;
  systemKind?: string | null;
}

/** True only for the machine-delivered kind (a Host fact row). */
export function isEstablishedFactAnnouncement(m: EstablishedFactFields): boolean {
  return m.systemSender === 'host' && m.systemKind === ESTABLISHED_FACT_KIND;
}

/** True only for a seat line (an operator USER row posted as fact). */
export function isEstablishedFactSeatLine(m: EstablishedFactFields): boolean {
  return m.isEstablishedFact === true && !m.systemSender;
}

/** True for either kind: a flagged seat line, or a Host fact row. */
export function isEstablishedFact(m: EstablishedFactFields): boolean {
  return isEstablishedFactSeatLine(m) || isEstablishedFactAnnouncement(m);
}

/** Which kind a row is, or null when it is not a fact. */
export function establishedFactKindOf(m: EstablishedFactFields): EstablishedFactKind | null {
  if (isEstablishedFactAnnouncement(m)) return 'announcement';
  if (isEstablishedFactSeatLine(m)) return 'seat';
  return null;
}
