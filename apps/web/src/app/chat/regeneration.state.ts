import { Injectable, computed, inject, signal } from '@angular/core';

import { CoreClient } from '../core/core-client';
import { isSwipeProgressEvent, type ResponseStatus } from '../core/core-contract';
import { notifyQueueChange } from '../layout/queue-status.logic';
import { ToastService } from '../ui/toast.service';

/** What the message row needs to render a line that is being re-rolled. */
export interface RegenerationState {
  /** The message whose place the new line will take. */
  messageId: string;
  /**
   * `preparing` while the server is still gathering context and waiting on the
   * model — the plate is up and the old line shows through, dimmed. `streaming`
   * from the first token on, when `content` is worth reading and replaces it.
   */
  stage: 'preparing' | 'streaming';
  /** The new line so far. */
  content: string;
  /** Live cumulative reasoning. DISPLAY ONLY, and only if the chat shows it. */
  reasoning: string;
}

/** v4's default failure sentence, used for every arm that has nothing better. */
const FAILED = 'Failed to generate alternative response';

/**
 * Regeneration — the live re-roll of a line that has already been spoken (v4
 * `app/salon/[id]/hooks/useRegeneration.ts`, `f564b0de3`).
 *
 * A regeneration used to be a silent blocking dispatch: the operator pressed the
 * refresh icon, nothing whatever changed on screen, and some seconds later the
 * transcript quietly held a different line. There was no way to tell a slow
 * model from a dead one, and nothing stopped a second press — or a fresh message
 * typed into the composer — from landing on top of a turn already in flight.
 *
 * So a regeneration now narrates itself, in the same three places a first-time
 * turn does:
 *
 *   1. the composer is shut for the duration (see {@link isRegenerating}),
 *   2. the line being replaced dims and wears a "Regenerating..." plate, which
 *      gives way to the new prose the instant the first token lands,
 *   3. the status strip above the transcript carries the stage, updated as the
 *      server reports it — exactly as it does the first time round.
 *
 * **Provided at the Salon component**, never `providedIn: 'root'` — it holds one
 * chat's in-flight re-roll, the `ImpersonationVoiceState` precedent (and the
 * same dogfood-#105 NG0201 trap otherwise).
 *
 * Two transport divergences from v4's hook, both forced by the boundary:
 *
 *  - **Events, not SSE.** v4 `fetch`es `?action=swipe&stream=1` and hand-parses
 *    `data: ` lines out of the response body, keeping a partial tail across
 *    network chunks. v5 dispatches `messageSwipe { stream: true }` and reads
 *    `swipeProgress` frames off the ONE Event channel, scoped by `progressId` —
 *    which §S.2 fixes as the target message's own id, so nothing is minted. The
 *    `streamGenerator` helper in `screens/characters/generators/` is the
 *    precedent; the frames themselves are v4's SSE payloads verbatim, so
 *    everything downstream of the parse is v4's own handling.
 *  - **One error arm, not two.** v4 branches on `!res.ok` (the stream never
 *    opened, so the body is an ordinary JSON error) versus a `{ error }` frame
 *    inside the stream. v5's refusals ARRIVE as a rejected dispatch carrying the
 *    envelope's message, so the first arm is the dispatch's rejection and the
 *    second is still the frame — the visible behavior, an error toast and
 *    nothing changed, is the same on both.
 *
 * @module chat/regeneration.state
 */
@Injectable()
export class RegenerationController {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  /** The in-flight regeneration, or null when none is running. */
  readonly regeneration = signal<RegenerationState | null>(null);
  /** The stage line for the strip above the composer. */
  readonly regenerationStatus = signal<ResponseStatus | null>(null);
  /** Whether a regeneration holds the floor (shuts the composer). */
  readonly isRegenerating = computed(() => this.regeneration() !== null);

  /**
   * One at a time. Read through a plain field as well as the signal so a second
   * press in the same tick — before the disabled button has repainted — still
   * loses.
   */
  private inFlight = false;

  /**
   * Tokens can arrive faster than the view can paint them. Buffer in a field and
   * flush at most once per frame, the same treatment the send path's stream
   * gets. Kept from v4 deliberately: an Angular signal write per token is
   * cheaper than React's was, but the row it feeds re-renders the whole markdown
   * pipeline, which is the cost v4's coalescing was actually avoiding.
   */
  private contentBuffer = '';
  private contentFrame: number | null = null;

  private cancelPendingFlush(): void {
    if (this.contentFrame !== null) {
      cancelAnimationFrame(this.contentFrame);
      this.contentFrame = null;
    }
  }

  private scheduleContent(content: string): void {
    this.contentBuffer = content;
    if (this.contentFrame !== null) return;
    this.contentFrame = requestAnimationFrame(() => {
      this.contentFrame = null;
      this.regeneration.update((prev) =>
        prev ? { ...prev, stage: 'streaming', content: this.contentBuffer } : prev,
      );
    });
  }

  /**
   * Start a regeneration. A no-op while another is already running.
   *
   * @param messageId The message to re-roll — also the `progressId` its frames
   *   carry.
   * @param refetch Read the authoritative transcript back once the new line is
   *   persisted.
   * @param selectSwipeVariant Put the freshly-made variant on screen once the
   *   refetch lands. Without it the operator would watch a line arrive and then
   *   be shown a different one, because reconciliation carries their previous
   *   swipe selection (v4 bug (b)).
   */
  async regenerate(
    messageId: string,
    refetch: () => Promise<void>,
    selectSwipeVariant?: (newSwipeId: string) => void,
  ): Promise<void> {
    if (this.inFlight) return;
    this.inFlight = true;
    this.cancelPendingFlush();
    this.contentBuffer = '';
    this.regeneration.set({ messageId, stage: 'preparing', content: '', reasoning: '' });
    this.regenerationStatus.set({ stage: 'regenerating', message: 'Regenerating...' });

    let fullContent = '';
    let streamError: string | null = null;
    let newSwipeId: string | null = null;

    const sub = this.core.events$.subscribe((frame) => {
      if (!isSwipeProgressEvent(frame, messageId)) return;
      const data = frame.frame;

      if (data['status']) {
        this.regenerationStatus.set(data['status'] as ResponseStatus);
      }
      if (data['content']) {
        fullContent += String(data['content']);
        this.scheduleContent(fullContent);
      }
      if (typeof data['reasoning'] === 'string') {
        const reasoning = data['reasoning'];
        this.regeneration.update((prev) => (prev ? { ...prev, reasoning } : prev));
      }
      if (data['error']) {
        streamError = data['details']
          ? `${String(data['error'])}: ${String(data['details'])}`
          : String(data['error']);
      }
      if (data['done']) {
        // Show the finished line rather than whatever the last frame left, so
        // the hand-off to the persisted swipe is not a visible twitch.
        this.cancelPendingFlush();
        const posted = data['message'] as { id?: string; content?: string } | undefined;
        newSwipeId = posted?.id ?? null;
        const finalContent = posted?.content;
        this.regeneration.update((prev) =>
          prev ? { ...prev, stage: 'streaming', content: finalContent ?? fullContent } : prev,
        );
      }
    });

    try {
      const answered = await this.core
        .dispatchData({ type: 'messageSwipe', messageId, stream: true })
        .finally(() => sub.unsubscribe());

      if (streamError) throw new Error(streamError);

      // The dispatch answers the same `201 { message }` the non-streaming call
      // does (§S.2), and it is the authoritative copy of the persisted row: the
      // `done` frame rides the Event channel and the subscription above is torn
      // down the instant the dispatch settles, so a terminal frame that loses
      // that race would leave `newSwipeId` null and the operator on the line
      // they just replaced. Prefer the frame's id (it arrives first in practice)
      // and fall back to the response's — the unification review's finding.
      if (!newSwipeId) {
        const posted = answered?.['message'] as { id?: string } | null | undefined;
        newSwipeId = posted?.id ?? null;
      }

      // Hold the live text in place until the authoritative transcript is in
      // hand; clearing first would flash the line we just replaced.
      await refetch();
      if (newSwipeId) selectSwipeVariant?.(newSwipeId);
      notifyQueueChange();
    } catch (err) {
      this.toasts.showError((err instanceof Error && err.message) || FAILED);
    } finally {
      this.cancelPendingFlush();
      this.contentBuffer = '';
      this.inFlight = false;
      this.regeneration.set(null);
      this.regenerationStatus.set(null);
    }
  }
}
