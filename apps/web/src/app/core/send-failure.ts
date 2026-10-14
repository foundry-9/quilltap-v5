/**
 * The failed-send toast (P4.D301 — v4 bug 241, `93c5af6ad`), ported byte for
 * byte from `app/salon/[id]/hooks/useSSEStreaming.ts:222-234` at the pin.
 *
 * A send that died before its row was written used to take the operator's
 * words with it; the Salon now puts them back in the composer and says so in
 * the ONE toast the failure raises, held long enough to read.
 */

/** Appended to the failure toast when the typed text was put back in the composer. */
export const SUBMISSION_RESTORED_NOTE = 'Your message is back in the composer.';

/** The failure toast's text, with the restored note as its own sentence. */
export function sendFailureToastText(message: string, restored: boolean): string {
  if (!restored) return message;
  const trimmed = message.trimEnd();
  const sentence = /[.!?…]$/.test(trimmed) ? trimmed : `${trimmed}.`;
  return `${sentence} ${SUBMISSION_RESTORED_NOTE}`;
}

/** How long a failed-send toast stays up — long enough to actually read it. */
export const SEND_FAILURE_TOAST_MS = 8000;

/**
 * The `error` half of the send shell's frame (`quilltap-host`'s
 * `emit_send_error_frame`, v4 `handleStreamError`'s literal). The Salon composes
 * v4's reader text `${error}: ${details}` from it and the rejection's message —
 * the same two values the frame carries — so the toast reads the same whichever
 * of the frame and the dispatch reply arrives first.
 */
export const STREAM_FAILURE_ERROR = 'Failed to generate response';
