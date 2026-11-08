/**
 * Message Navigation Utilities — the client twin of v4
 * `lib/chat/message-navigation.ts` (`4f5047241`, pinned at `624d0d25c`).
 *
 * Handles navigation to specific messages in chat: the chat-scoped reveal
 * request every `?msg=` link resolves to. (v4's DOM-only `scrollToMessage`
 * helper is twinned by `MessageList.scrollToMessage` — v5's transcript is
 * virtualized, so it asks the virtualizer, not the DOM.)
 *
 * `revealRequestFromSalonHref` is proven row for row against v4's real module by
 * `message-navigation.oracle.spec.ts`; the storage / event pair by
 * `message-navigation.spec.ts` against v4's own test cases.
 *
 * @module chat/message-navigation
 */

export const PENDING_REVEAL_KEY = 'pendingMessageReveal';

/** Window event announcing a new reveal request; `detail` is a {@link MessageRevealRequest}. */
export const MESSAGE_REVEAL_EVENT = 'quilltap:reveal-message';

/** "Scroll this chat to this message", waiting for whichever Salon shows the chat. */
export interface MessageRevealRequest {
  chatId: string;
  messageId: string;
}

/**
 * Ask the Salon showing `chatId` to scroll to `messageId` and flash it.
 *
 * The request is parked in sessionStorage, so a Salon that is still to mount
 * (a new tab, a full page load) takes it once its transcript has loaded, and is
 * announced with {@link MESSAGE_REVEAL_EVENT}, so a Salon already open — a
 * kept-alive workspace tab — takes it at once. Either way it is taken once,
 * and only by that chat. Every way into a `/salon/<id>?msg=<messageId>` link
 * (workspace links, the workspace redirect, the legacy route) ends here.
 */
export function requestMessageReveal(chatId: string, messageId: string): void {
  const request: MessageRevealRequest = { chatId, messageId };
  try {
    sessionStorage.setItem(PENDING_REVEAL_KEY, JSON.stringify(request));
  } catch {
    // Storage unavailable: the event still reaches an open Salon.
  }
  window.dispatchEvent(
    new CustomEvent<MessageRevealRequest>(MESSAGE_REVEAL_EVENT, { detail: request }),
  );
}

/**
 * Take the pending reveal for `chatId`, if there is one, clearing it. A request
 * for a different chat is left for that chat.
 */
export function takePendingMessageReveal(chatId: string): string | null {
  let raw: string | null = null;
  try {
    raw = sessionStorage.getItem(PENDING_REVEAL_KEY);
  } catch {
    return null;
  }
  if (!raw) return null;
  try {
    const request = JSON.parse(raw) as Partial<MessageRevealRequest>;
    if (request.chatId !== chatId || typeof request.messageId !== 'string') return null;
    sessionStorage.removeItem(PENDING_REVEAL_KEY);
    return request.messageId;
  } catch {
    sessionStorage.removeItem(PENDING_REVEAL_KEY);
    return null;
  }
}

/** The message a Salon href (`/salon/<id>?msg=<messageId>`) points at, if any. */
export function revealRequestFromSalonHref(href: string): MessageRevealRequest | null {
  const match = href.match(/^\/salon\/([^/?#]+)\/?\?([^#]*)/);
  if (!match || match[1] === 'new') return null;
  const messageId = new URLSearchParams(match[2]).get('msg');
  return messageId ? { chatId: decodeURIComponent(match[1]), messageId } : null;
}
