import type { CoreClient } from '../core/core-client';
import { notifyQueueChange } from '../layout/queue-status.logic';
import type { ToastService } from '../ui/toast.service';
import { describeRetryRefusalError } from './concierge-retry';

/** What the controller needs from its host — the Salon (v4 `useConciergeRetry`'s three arguments). */
export interface ConciergeRetryHost {
  core: Pick<CoreClient, 'dispatchData'>;
  toasts: Pick<ToastService, 'showInfo' | 'showSuccess' | 'showError'>;
  /** The open chat's id, read at CALL time (the Salon may change chats under a live controller). */
  chatId: () => string | null;
  /** v4 `fetchChat` — the new TOOL row arrives by refetch. */
  refetchChat: () => Promise<void>;
  /** v4 `startBackgroundPolling` — the backdrop arrives through the ordinary story-background watch. */
  startBackgroundPolling: () => void;
}

const PICTURE_FAILED = 'The uncensored desk could not produce the picture';
const BACKDROP_FAILED = 'Failed to queue the backdrop';

/**
 * v4's `errorFrom(res, fallback)`: a 409 is the Concierge declining — Locked,
 * or nobody to send it to — and is said in words; anything else shows the
 * server's own message, else the fallback. Over v5's dispatch the 409 is
 * `kind === 'conflict'` and the sentence is keyed by the error's MESSAGE (the
 * bare token, §S.3) — see {@link describeRetryRefusalError}.
 */
function errorMessage(err: unknown, fallback: string): string {
  const message = err instanceof Error ? err.message : '';
  return describeRetryRefusalError(err) || message || fallback;
}

/**
 * "Try uncensored" on pictures — the `generate_image` redraw and the Lantern's
 * backdrop (v4 `app/salon/[id]/hooks/useConciergeRetry.ts`, new at
 * `ce2f1dabf` #77). The text retry rides the regeneration controller instead
 * (`RegenerationController.regenerate`'s `request` option).
 *
 * Both are one `chatRetryImageUncensored` dispatch (§S.3; v4's POST to
 * `?action=retry-image-uncensored`). The picture itself arrives through the
 * ordinary refetch, the backdrop through the ordinary background poll — no
 * query-key invalidation beyond what those already do (v4 identical).
 *
 * A plain class the Salon constructs once (no DI): its handlers are handed to
 * every transcript row, which compares them by identity.
 */
export class ConciergeRetryController {
  /** One redraw per picture at a time; a second press while it paints is lost. */
  private readonly inFlight = new Set<string>();

  constructor(private readonly host: ConciergeRetryHost) {}

  async retryPicture(toolMessageId: string): Promise<void> {
    const chatId = this.host.chatId();
    if (!chatId) return;
    if (this.inFlight.has(toolMessageId)) return;
    this.inFlight.add(toolMessageId);
    // v4 raises the info toast BEFORE the request.
    this.host.toasts.showInfo('The Concierge has taken the commission across the street…');
    try {
      try {
        await this.host.core.dispatchData({
          type: 'chatRetryImageUncensored',
          chatId,
          body: { toolMessageId },
        });
      } catch (err) {
        throw new Error(errorMessage(err, PICTURE_FAILED));
      }
      await this.host.refetchChat();
      this.host.toasts.showSuccess('The uncensored desk has delivered the picture');
    } catch (err) {
      this.host.toasts.showError((err instanceof Error && err.message) || PICTURE_FAILED);
    } finally {
      this.inFlight.delete(toolMessageId);
    }
  }

  /** No in-flight guard — v4 has none here (the server answers "already in progress"). */
  async retryBackground(): Promise<void> {
    const chatId = this.host.chatId();
    if (!chatId) return;
    try {
      try {
        await this.host.core.dispatchData({
          type: 'chatRetryImageUncensored',
          chatId,
          body: { kind: 'background' },
        });
      } catch (err) {
        throw new Error(errorMessage(err, BACKDROP_FAILED));
      }
      this.host.toasts.showSuccess('Backdrop commissioned from the uncensored desk');
      notifyQueueChange();
      this.host.startBackgroundPolling();
    } catch (err) {
      this.host.toasts.showError((err instanceof Error && err.message) || BACKDROP_FAILED);
    }
  }
}
