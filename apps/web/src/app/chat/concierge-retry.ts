import {
  CoreDispatchError,
  type MessageDto,
  type MessageRetryUncensoredRequest,
} from '../core/core-contract';

/**
 * "Try uncensored" — the client-side helpers shared by the text, picture and
 * backdrop retries (v4 `app/salon/[id]/concierge-retry.ts`, new at
 * `ce2f1dabf` #77; the server half is P4.D228's `retry_uncensored.rs`).
 *
 * Pure and client-safe; pinned against v4's REAL module by the recorder
 * `harness/oracle/cases/concierge-retry.mjs` → `concierge-retry.v4.json`.
 */

/**
 * The request that re-rolls an assistant line on the uncensored desk, narrated
 * — v4's `retryUncensoredTurnUrl(chatId, messageId)`
 * (`/api/v1/chats/${chatId}/messages/${messageId}?action=retry-uncensored&stream=1`).
 * v5 has no URL: the retry is the `messageRetryUncensored` dispatch verb
 * (round shared contract §S.3) whose `swipeProgress` frames are keyed by the
 * target message id, exactly as `messageSwipe`'s are — so the regeneration
 * controller narrates it with the same code. The chat id rides the message.
 */
export function retryUncensoredTurnRequest(messageId: string): MessageRetryUncensoredRequest {
  return { type: 'messageRetryUncensored', messageId, stream: true };
}

/**
 * The operator-facing sentence for a refused retry, or null when the error
 * names no reason we know. v4 reads the 409 body's `error`; over v5's dispatch
 * the refusal is a `CoreDispatchError` whose MESSAGE is the bare token
 * (`locked` / `no-understudy`, §S.3). Callers with the thrown error use
 * {@link describeRetryRefusalError}, which adds v4's 409 gate.
 */
export function describeRetryRefusal(error: unknown): string | null {
  switch (error) {
    case 'no-understudy':
      return 'There is no uncensored desk to send this to — appoint one under Settings → The Concierge.';
    case 'locked':
      return 'This conversation is Locked to the usual desks; set it to Moderated should you wish the Concierge to take things elsewhere.';
    default:
      return null;
  }
}

/**
 * The refusal sentence for a thrown dispatch error, or null. v4 gates on the
 * STATUS — `res.status === 409 ? describeRetryRefusal(info?.error) : null`
 * (`useRegeneration.ts:136`, `useConciergeRetry.ts:36` at `97b25fc53`) — so the
 * bare token only means "refused" on a 409. Over v5's dispatch the 409 is
 * `CoreDispatchError.kind === 'conflict'` (core maps the refusal to
 * `("conflict", "no-understudy" | "locked")`), which is the gate here: a
 * non-conflict error whose message merely reads `locked` (`kind: 'locked'` is
 * the vault-locked 503) is NOT reworded.
 */
export function describeRetryRefusalError(err: unknown): string | null {
  return err instanceof CoreDispatchError && err.kind === 'conflict'
    ? describeRetryRefusal(err.message)
    : null;
}

/**
 * Whether a row is the Lantern's report that its painter refused the backdrop.
 * `systemKind` ONLY — no content inference, so a legacy row written before the
 * kind existed gets no button (v4's own rule; `system-message-labels`' wording
 * sniff is for the LABEL, a different question).
 */
export function isLanternBackgroundRefusal(
  message: Pick<MessageDto, 'systemSender' | 'systemKind'>,
): boolean {
  return message.systemSender === 'lantern' && message.systemKind === 'background-refused';
}

/**
 * The retry callbacks the transcript offers; ABSENT on a Locked chat, which
 * hides every "Try uncensored" button. The buttons key off the PRESENCE of this
 * object, and the rows compare it by identity (OnPush inputs), so the Salon
 * hands down one stable object.
 */
export interface ConciergeRetryHandlers {
  onRetryTurn: (messageId: string) => void;
  onRetryPicture: (toolMessageId: string) => void;
  onRetryBackground: () => void;
}
