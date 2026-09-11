import { parseCarinaQuery } from '../carina-parser';

/**
 * In Their Own Words — the pure gate (v4
 * `app/salon/[id]/hooks/useImpersonationVoice.ts:23-70`).
 *
 * Five rules decide whether a composer submit becomes a rehearsal. Four of them
 * are about staying out of the way: an owner seat has no voice of its own to
 * consult, an attachment-only send has nothing to restate, a Carina address is
 * machinery rather than a line, and a "Send as written" resubmit has already
 * been through the dialog once.
 *
 * @module chat/impersonation-voice/gate
 */

/** The minimum a gate decision needs to know about the speaking seat. */
export interface RehearsalSeat {
  id: string;
  type: 'CHARACTER';
  controlledBy?: 'llm' | 'user';
}

export interface ShouldRehearseArgs {
  /** `chatSettings.impersonationVoiceRewrite`. */
  enabled: boolean;
  /** The seat the composer will attribute this message to (`speakingSeat`). */
  seat: RehearsalSeat | null;
  impersonatingParticipantIds: readonly string[];
  text: string;
  /** True when the send carries only attachments / tool results and no prose. */
  hasAttachmentsOnly: boolean;
  /** Set for the one resubmit that "Send as written" triggers. */
  bypassOnce: boolean;
}

/**
 * The gate. True only when every condition holds; pure, so it can be tested and
 * reasoned about without a render.
 *
 * An owner seat (`controlledBy: 'user'`) never qualifies, even when it also
 * appears in the overlay list: the overlay is the one signal that says "this
 * character has a voice of their own that a model normally supplies".
 *
 * The early-return ORDER is v4's, kept verbatim. It is deliberately
 * unobservable — every arm answers the same `false` — so no spec can pin it and
 * the transcription is the only guarantee (recorded in the lane record with the
 * mutation that proves the corpus cannot see it).
 */
export function shouldRehearseImpersonatedLine({
  enabled,
  seat,
  impersonatingParticipantIds,
  text,
  hasAttachmentsOnly,
  bypassOnce,
}: ShouldRehearseArgs): boolean {
  if (!enabled) return false;
  if (bypassOnce) return false;
  if (!seat) return false;
  if (seat.type !== 'CHARACTER') return false;
  if (seat.controlledBy === 'user') return false;
  if (!impersonatingParticipantIds.includes(seat.id)) return false;
  if (text.trim().length === 0) return false;
  if (hasAttachmentsOnly) return false;
  // A Carina address is machinery, not a line: `@Name:` routes to an answerer
  // and must survive verbatim. Rewriting it would rewrite the address.
  if (parseCarinaQuery(text)) return false;
  return true;
}
