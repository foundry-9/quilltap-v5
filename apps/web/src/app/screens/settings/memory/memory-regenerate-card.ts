import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { RegenerateAllStatus } from '../../../core/core-contract';
import { RealtimeService } from '../../../core/realtime.service';
import { notifyQueueChange } from '../../../layout/queue-status.logic';
import { fetchRegenerateStatus, memoryKeys, regenerateAllMemories } from '../../../memory/memory.api';
import { ToastService } from '../../../ui/toast.service';

/** Fallback poll cadence, used only while the realtime channel is down. */
const POLL_INTERVAL_MS = 5000;

/**
 * The Regenerate Memories card (v4 `components/tools/memory-regenerate-card.tsx`):
 * a destructive "wipe every chat-linked memory and re-run the extraction
 * pipeline" action behind an inline confirm, plus an in-flight status line that
 * polls (`memoryRegenerateAllStatus`) every 5s while a sweep is draining.
 */
@Component({
  selector: 'qt-memory-regenerate-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="space-y-4">
      <p class="qt-text-small qt-text-muted">
        Wipes every memory linked to a conversation and re-runs the current extraction pipeline
        against the chat history. Manual memories that aren't tied to a chat are left alone. Memories
        whose chat has already been deleted are removed too. The work runs in the background; close
        this tab and come back whenever.
      </p>

      @if (inFlight() > 0) {
        <p class="qt-text-small qt-text-muted">
          In flight:
          @if (status().inFlightFanOut > 0) {
            {{ status().inFlightFanOut }} fan-out{{ status().inFlightFanOut === 1 ? '' : 's' }}
            (building chat list),
          }
          {{ status().inFlightWipes }} chat wipe{{ status().inFlightWipes === 1 ? '' : 's' }},
          {{ status().inFlightExtractions }} extraction{{
            status().inFlightExtractions === 1 ? '' : 's'
          }}.
        </p>
      }

      @if (!confirming()) {
        <div class="flex items-center gap-3">
          <button
            type="button"
            class="qt-button qt-button-danger"
            [disabled]="submitting()"
            (click)="confirming.set(true)"
          >
            Delete and regenerate all memories
          </button>
          <span class="qt-text-small qt-text-muted"
            >Affects every chat-linked memory across all characters.</span
          >
        </div>
      } @else {
        <div class="flex items-center gap-3">
          <span class="qt-body">
            This will delete and rebuild every chat-linked memory. Continue?
          </span>
          <button
            type="button"
            class="qt-button qt-button-danger"
            [disabled]="submitting()"
            (click)="confirm()"
          >
            {{ submitting() ? 'Enqueuing…' : 'Yes, regenerate' }}
          </button>
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="submitting()"
            (click)="confirming.set(false)"
          >
            Cancel
          </button>
        </div>
      }

      @if (error(); as msg) {
        <p class="qt-text-small qt-text-destructive">{{ msg }}</p>
      }
    </div>
  `,
})
export class MemoryRegenerateCard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();
  private readonly realtime = inject(RealtimeService);

  protected readonly confirming = signal(false);
  protected readonly submitting = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly statusQuery = injectQuery(() => ({
    queryKey: memoryKeys.regenerateStatus(),
    queryFn: (): Promise<RegenerateAllStatus> => fetchRegenerateStatus(this.core),
    // Fallback only, and only while a sweep is in flight — v4's scope exactly,
    // now gated on channel health as well (`f3892158d`). ⚠ The channel gate is
    // read HERE, in the reactive options factory, not inside the function form:
    // TanStack recomputes a function-form interval only on subscribe/options/
    // cache changes, so a `connected()` read in there is untracked and a
    // mid-drain channel drop would never re-arm the poll (the §3 unification
    // review's catch; the badges' factory-level gate is the pattern).
    refetchInterval: this.realtime.connected()
      ? (false as const)
      : (query): number | false => {
          const data = query.state.data as RegenerateAllStatus | undefined;
          const inFlight = !!data && data.inFlight > 0;
          return inFlight ? POLL_INTERVAL_MS : false;
        },
  }));

  constructor() {
    // The sweep is a fan-out of background jobs, so it drains visibly on the
    // `jobs` topic. Only while something is actually in flight, matching what
    // the old poll did — an idle card has no reason to re-read on every
    // unrelated job (v4's own comment).
    this.realtime.onTopic('jobs', () => {
      if (this.inFlight() > 0) {
        void this.queryClient.invalidateQueries({ queryKey: memoryKeys.regenerateStatus() });
      }
    });
  }

  protected readonly status = computed<RegenerateAllStatus>(
    () =>
      this.statusQuery.data() ?? {
        inFlightFanOut: 0,
        inFlightWipes: 0,
        inFlightExtractions: 0,
        inFlight: 0,
      },
  );
  protected readonly inFlight = computed(() => this.status().inFlight);

  protected async confirm(): Promise<void> {
    this.submitting.set(true);
    this.error.set(null);
    try {
      const result = await regenerateAllMemories(this.core);
      // v4 `:82` prefers the server's sentence.
      this.toasts.showSuccess(
        (result as { message?: string } | undefined)?.message ||
          'Regeneration enqueued — chats will rebuild in the background',
      );
      this.confirming.set(false);
      // v4 `memory-regenerate-card.tsx:83` wakes the toolbar queue badges too
      // (the sweep rides the MEMORY_REGENERATE_* queue).
      notifyQueueChange();
      // Light the in-flight badge immediately.
      await this.queryClient.invalidateQueries({ queryKey: memoryKeys.regenerateStatus() });
    } catch (err) {
      const message =
        err instanceof Error && err.message ? err.message : 'Failed to start regeneration';
      this.error.set(message);
      this.toasts.showError(message);
    } finally {
      this.submitting.set(false);
    }
  }
}
