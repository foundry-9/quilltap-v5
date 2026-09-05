/**
 * The help-chat streaming consumer — the transport half of v4's
 * `useHelpChatStreaming` (its state machine is {@link reduceHelpFrame}).
 *
 * v4 opens an SSE response and reads it; v5's frames ride the GLOBAL event
 * channel scope-tagged by `chatId` (§B), exactly like `ChatSend` and
 * `BrahmaConsoleSend`, so this subscribes to that channel BEFORE dispatching and
 * unsubscribes when the dispatch resolves — the `send` verb resolves at run
 * completion.
 *
 * v4's `onMessageComplete(messageId)` becomes a callback fired off the fold's
 * `completedMessageIds` growing, so the pure machine stays pure.
 *
 * @module help/help-streaming.service
 */

import { Injectable, computed, inject, signal } from '@angular/core';
import { filter } from 'rxjs';

import { CoreClient } from '../core/core-client';
import {
  initialHelpStreamState,
  reduceHelpFrame,
  type HelpStreamState,
  type NavigationLink,
} from './help-stream';
import { HelpApi } from './help-wire';

@Injectable({ providedIn: 'root' })
export class HelpStreamingService {
  private readonly core = inject(CoreClient);
  private readonly api = inject(HelpApi);

  /** The live folded stream state; null when no turn is in flight. */
  private readonly state = signal<HelpStreamState | null>(null);

  readonly isStreaming = computed(() => this.state()?.isStreaming ?? false);
  readonly isExecutingTools = computed(() => this.state()?.isExecutingTools ?? false);
  readonly streamingContent = computed(() => this.state()?.streamingContent ?? '');
  readonly streamingParticipantId = computed(() => this.state()?.streamingParticipantId ?? null);
  readonly streamingNavigationLinks = computed<NavigationLink[]>(
    () => this.state()?.streamingNavigationLinks ?? [],
  );
  readonly suggestedLinks = computed<NavigationLink[]>(() => this.state()?.suggestedLinks ?? []);
  readonly error = signal<string | null>(null);

  /** Drop the live overlay (the dialog calls this after reconciling). */
  reset(): void {
    this.state.set(null);
  }

  /**
   * Send one message and fold its frames.
   *
   * `onMessageComplete` fires per `done` frame carrying a message id, which is
   * where v4 reloads the transcript.
   */
  async sendMessage(
    chatId: string,
    content: string,
    onMessageComplete?: (messageId: string) => void,
    fileIds?: string[],
  ): Promise<void> {
    this.error.set(null);
    let s = initialHelpStreamState();
    this.state.set(s);
    let seenCompletions = 0;

    const sub = this.core.events$
      .pipe(filter((frame) => frame.chatId === chatId))
      .subscribe((frame) => {
        s = reduceHelpFrame(s, frame);
        this.state.set(s);
        if (s.error) this.error.set(s.error);
        while (seenCompletions < s.completedMessageIds.length) {
          onMessageComplete?.(s.completedMessageIds[seenCompletions]);
          seenCompletions += 1;
        }
      });

    try {
      await this.api.chatSend(chatId, content, fileIds);
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to send message');
    } finally {
      sub.unsubscribe();
      // v4's "stream ended normally" arm: the run is over whatever the last
      // frame said, so the streaming flag comes down here rather than relying
      // on a terminal frame having arrived.
      this.state.update((prev) => (prev ? { ...prev, isStreaming: false, streamingContent: '' } : prev));
    }
  }
}
