/**
 * The memory badge's re-extract action — v4's `handleReextractMemories`, which
 * v4 keeps as two byte-identical copies (`app/salon/SalonListView.tsx:119-145`
 * and `components/character/character-conversations-tab.tsx:148-176` at
 * `97b25fc53`). One home here, so the two lists cannot drift; callers own their
 * own list refresh, exactly as they do for `confirmAndDeleteChat`.
 *
 * v4 sends `DELETE /api/v1/memories?chatId=` then `POST /api/v1/chats/{id}
 * ?action=queue-memories`; the SPA dispatches the two verbs that are the same
 * server code (`memoryDeleteByChat`, `chatQueueMemories`), IN SEQUENCE. No core
 * change. The character tab's POST body carries `characterId`/`characterName`,
 * which the handler ignores (`ChatQueueMemories`'s doc in `api/types.rs`), so the
 * verb's `chatId` is the whole live surface on both.
 *
 * Failure semantics are v4's: the DELETE's status is NEVER read (`fetch` throws
 * only on a network failure), so a refused delete does not stop the queue call;
 * a refused queue is the server's own `error` text, else `Failed to queue memory
 * extraction`; anything thrown is its message, else `Failed to re-extract
 * memories`.
 *
 * @module screens/salon/memory-reextract.api
 */

import { CoreDispatchError } from '../../core/core-contract';
import type { CoreClient } from '../../core/core-client';
import { notifyQueueChange } from '../../layout/queue-status.logic';

/** v4's confirmation sentence, byte for byte. */
export const REEXTRACT_CONFIRM =
  'This will delete all existing memories from this chat and re-extract them from the conversation. Are you sure?';

/** The toast surface the action needs (a structural slice of `ToastService`). */
export interface ReextractToasts {
  showSuccess(message: string): unknown;
  showError(message: string): unknown;
}

/**
 * Confirm, delete the chat's memories, queue re-extraction, toast, wake the
 * queue badges. Answers whether the extraction was queued (v4's `res.ok`
 * branch — the list's cue to refresh).
 *
 * The gate is `window.confirm`, the established idiom (v5 has no counterpart to
 * v4's promise-based `showConfirmation`; see `chat-delete.api.ts`).
 */
export async function confirmAndReextractMemories(
  core: CoreClient,
  toasts: ReextractToasts,
  chatId: string,
): Promise<boolean> {
  if (!window.confirm(REEXTRACT_CONFIRM)) return false;
  try {
    try {
      await core.dispatchData({ type: 'memoryDeleteByChat', chatId });
    } catch (err) {
      // v4 never reads the DELETE's status: a non-ok answer falls through to the
      // queue call. Only a transport failure (a non-dispatch error) aborts.
      if (!(err instanceof CoreDispatchError)) throw err;
    }
    let queued: Record<string, unknown>;
    try {
      queued = await core.dispatchData({ type: 'chatQueueMemories', chatId });
    } catch (err) {
      if (err instanceof CoreDispatchError) {
        toasts.showError(err.message || 'Failed to queue memory extraction');
        return false;
      }
      throw err;
    }
    toasts.showSuccess(`Queued ${queued['jobCount']} memory extraction jobs`);
    notifyQueueChange();
    return true;
  } catch (err) {
    toasts.showError(err instanceof Error && err.message ? err.message : 'Failed to re-extract memories');
    return false;
  }
}
