import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient, coreErrorMessage } from '../../../core/core-client';
import { notifyQueueChange } from '../../../layout/queue-status.logic';
import { ToastService } from '../../../ui/toast.service';

const POLL_INTERVAL_MS = 5000;
const STATUS_KEY = ['conversation-summaries', 'status'] as const;

/**
 * The Regenerate Conversation Summaries card (v4 `components/tools/
 * conversation-summary-regenerate-card.tsx`): a single-click enqueue that
 * re-mirrors every summarised chat into its participants' vaults, with an
 * in-flight line that polls every 5s while a run drains and stops at zero.
 *
 * Status failures are swallowed — the button still works (v4 :23-25). Enqueue
 * success toasts the server's message and wakes the toolbar queue badges.
 */
@Component({
  selector: 'qt-conversation-summary-regenerate-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="space-y-4">
      <p class="qt-text-small qt-text-muted">
        Re-mirrors every summarised conversation into each participant&rsquo;s vault under
        <code>Conversation Summaries/</code>, where the Commonplace Book draws the &ldquo;relevant
        past conversations&rdquo; it offers a character before their turn. Run this to seed those
        files for older chats, or to repair them after a format change. Nothing is deleted that
        isn&rsquo;t replaced; the work runs in the background, so you may close this tab and come
        back whenever.
      </p>

      @if (inFlight() > 0) {
        <p class="qt-text-small qt-text-muted">
          In flight: {{ inFlight() }} regeneration{{ inFlight() === 1 ? '' : 's' }}.
        </p>
      }

      <div class="flex items-center gap-3">
        <button
          type="button"
          class="qt-button qt-button-primary"
          [disabled]="submitting()"
          (click)="regenerate()"
        >
          {{ submitting() ? 'Enqueuing…' : 'Regenerate conversation summaries' }}
        </button>
        <span class="qt-text-small qt-text-muted"
          >Re-mirrors every summarised chat across all characters.</span
        >
      </div>

      @if (error(); as msg) {
        <p class="qt-text-small qt-text-error">{{ msg }}</p>
      }
    </div>
  `,
})
export class ConversationSummaryRegenerateCard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  protected readonly submitting = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly statusQuery = injectQuery(() => ({
    queryKey: STATUS_KEY,
    // v4 swallows status failures ("the button still works"): a failed read
    // reports zero in flight rather than surfacing an error.
    queryFn: (): Promise<number> => this.core.conversationSummariesStatus().catch(() => 0),
    // Poll only while a run is draining (v4 stops at zero).
    refetchInterval: (query): number | false => {
      const data = query.state.data as number | undefined;
      return data && data > 0 ? POLL_INTERVAL_MS : false;
    },
  }));

  protected readonly inFlight = computed(() => this.statusQuery.data() ?? 0);

  protected async regenerate(): Promise<void> {
    this.submitting.set(true);
    this.error.set(null);
    try {
      const { message } = await this.core.regenerateConversationSummaries();
      this.toasts.showSuccess(
        message || 'Conversation summaries are being re-mirrored in the background',
      );
      notifyQueueChange();
      // Light the in-flight line immediately (v4 `await loadStatus()`).
      await this.queryClient.invalidateQueries({ queryKey: STATUS_KEY });
    } catch (err) {
      const msg = coreErrorMessage(err, 'Failed to start regeneration');
      this.error.set(msg);
      this.toasts.showError(msg);
    } finally {
      this.submitting.set(false);
    }
  }
}
