import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { ChatInformsListResult, PendingInformBatch } from '../core/core-contract';
import { RealtimeService } from '../core/realtime.service';
import { Icon } from '../ui/icon';
import { ToastService } from '../ui/toast.service';
import { chatKeys } from './chat-keys';

/** Cadence of the offline fallback poll, in ms (v4 `FALLBACK_POLL_MS`). */
const FALLBACK_POLL_MS = 60_000;

/** The first non-blank line of the passage, for the chip's hover title (v4 `firstLine`). */
export function firstInformLine(markdown: string): string {
  const line = markdown.split('\n').find((l) => l.trim().length > 0);
  return (line ?? '').trim();
}

/**
 * PendingInformChips — the notes still waiting in the wings (v4
 * `components/chat/PendingInformChips.tsx`, `e7d77bb60`).
 *
 * One chip per pending Inform batch, sitting in the composer just above the
 * form: *Informing Alice, Bob before their next turn*, with a × that cancels the
 * batch and a hover title carrying the passage's first line.
 *
 * Realtime rides the existing `chats` topic. Posting an inform inserts the Host
 * record, consuming one rides the assistant-message insert, and the cancel
 * handler publishes `chats` explicitly — and `chatKeys.informs(id)` sits under
 * the `['chat', id]` prefix that row already invalidates, so that is the whole
 * refresh story. The interval below is only the offline fallback, gated by
 * {@link RealtimeService.refetchInterval} so it stops the moment the socket is
 * up.
 *
 * @module chat/pending-inform-chips
 */
@Component({
  selector: 'qt-pending-inform-chips',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    @if (chips().length > 0) {
      <div class="qt-chat-attachment-list mb-2">
        @for (chip of chips(); track chip.batchId) {
          <div class="qt-chat-tool-result-chip" [title]="chip.hover">
            <qt-icon
              name="info"
              class="qt-chat-attachment-chip-icon qt-chat-attachment-chip-icon-info"
            />
            <span class="text-foreground max-w-[280px] truncate">{{ chip.label }}</span>
            <button
              type="button"
              class="qt-chat-attachment-chip-remove"
              title="Withdraw this inform"
              [attr.aria-label]="'Withdraw the inform for ' + chip.names"
              [disabled]="cancelling()"
              (click)="onCancel(chip.batchId)"
            >
              <qt-icon name="close" class="w-4 h-4" />
            </button>
          </div>
        }
      </div>
    }
  `,
})
export class PendingInformChips {
  private readonly core = inject(CoreClient);
  private readonly realtime = inject(RealtimeService);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  readonly chatId = input.required<string>();
  /** Seat display names, keyed by chat participant id (v4 `participantNames`). */
  readonly participantNames = input<Record<string, string>>({});

  /** v4's `cancel.isPending` — one withdrawal at a time. */
  protected readonly cancelling = signal(false);

  private readonly query = injectQuery(() => ({
    queryKey: chatKeys.informs(this.chatId()),
    queryFn: async (): Promise<ChatInformsListResult> => {
      const data = await this.core.dispatchData({
        type: 'chatInformsList',
        chatId: this.chatId(),
      });
      return data as unknown as ChatInformsListResult;
    },
    enabled: Boolean(this.chatId()),
    refetchInterval: this.realtime.refetchInterval(FALLBACK_POLL_MS),
  }));

  /**
   * The chips to draw. A batch whose seats have all left the chat has nothing
   * left to name, so it is skipped rather than shown nameless (v4 `:96-98`).
   */
  protected readonly chips = computed(() => {
    const names = this.participantNames();
    const batches: PendingInformBatch[] = this.query.data()?.batches ?? [];
    return batches.flatMap((batch) => {
      const seatNames = batch.pendingParticipantIds
        .map((id) => names[id])
        .filter((name): name is string => Boolean(name));
      if (seatNames.length === 0) return [];
      const joined = seatNames.join(', ');
      return [
        {
          batchId: batch.batchId,
          names: joined,
          label: `Informing ${joined} before their next turn`,
          hover: firstInformLine(batch.contentMarkdown),
        },
      ];
    });
  });

  protected async onCancel(batchId: string): Promise<void> {
    this.cancelling.set(true);
    try {
      await this.core.dispatchData({
        type: 'chatInformCancel',
        chatId: this.chatId(),
        batchId,
      });
      this.toasts.showSuccess('The note has been withdrawn');
    } catch (err) {
      this.toasts.showError(
        (err instanceof Error && err.message) || 'Failed to withdraw the inform',
      );
    } finally {
      // v4's `onSettled`: the re-read runs on both arms, because a cancel that
      // reported an error may still have removed rows.
      this.cancelling.set(false);
      void this.queryClient.invalidateQueries({
        queryKey: chatKeys.informs(this.chatId()),
      });
    }
  }
}
