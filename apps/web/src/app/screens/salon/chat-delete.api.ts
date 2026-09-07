/**
 * The chat-delete client api (dogfood finding #117) — v4
 * `lib/chat-utils.ts`'s `confirmAndDeleteChat` (`:148-159`), which is the ONE
 * place v4's two chat lists share: the Salon list and a character's
 * Conversations tab both call it and then own their own refresh.
 *
 * v4 sends `DELETE /api/v1/chats/{id}`; v5's SPA dispatches the `chatDelete`
 * verb (§B.4) instead, which is the same server code — the REST edge
 * `chats_routes::chat_delete` is a thin adapter over the same
 * `chat_delete_dispatch`. The CLI and curl keep the URL.
 *
 * @module screens/salon/chat-delete.api
 */

import { CoreDispatchError, type ChatDeleteRequest } from '../../core/core-contract';
import type { CoreClient } from '../../core/core-client';

/** v4's body: `NextResponse.json({ success: true })`. */
export interface ChatDeleteResult {
  success: boolean;
}

/**
 * v4 `confirmAndDeleteChat` (`lib/chat-utils.ts:148-159`), whole: confirm,
 * DELETE, and report whether the delete happened. Failures surface as a toast
 * and the function answers `false`; **callers own their list refresh**, exactly
 * as v4's do (`SalonListView.tsx:113-117` re-fetches, the Conversations tab
 * filters its local array).
 *
 * The gate is `window.confirm` — v5 has no counterpart to v4's promise-based
 * `showConfirmation` modal, and `window.confirm` is the established idiom here
 * (the photos / almanack / wardrobe precedent, ~19 call sites). The SENTENCE is
 * v4's, byte for byte.
 *
 * The failure sentence is v4's too, and the split is deliberate:
 * `confirmAndDeleteChat` throws its OWN `new Error('Failed to delete chat')`
 * on a non-ok response and only then toasts `err.message`, so a server error's
 * text never reaches the operator — while a network-layer rejection's message
 * does. A `CoreDispatchError` is v5's "non-ok response", so it takes the fixed
 * sentence; anything else is the transport failing and keeps its own.
 */
export async function confirmAndDeleteChat(
  core: CoreClient,
  toastError: (message: string) => void,
  chatId: string,
): Promise<boolean> {
  if (!window.confirm('Are you sure you want to delete this chat?')) {
    return false;
  }
  try {
    const req: ChatDeleteRequest = { type: 'chatDelete', chatId };
    await core.dispatchData(req);
    return true;
  } catch (err) {
    toastError(
      err instanceof CoreDispatchError || !(err instanceof Error)
        ? 'Failed to delete chat'
        : err.message,
    );
    return false;
  }
}
