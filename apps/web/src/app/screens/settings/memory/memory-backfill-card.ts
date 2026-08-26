import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import { RealtimeService } from '../../../core/realtime.service';
import type { BackfillProgress } from '../../../core/core-contract';
import { fetchBackfillProgress, memoryKeys, startBackfill } from '../../../memory/memory.api';
import { ToastService } from '../../../ui/toast.service';

/**
 * The Repair Missing Embeddings card (v4 `components/tools/memory-backfill-card.tsx`):
 * reads the instance-wide backfill progress (`memoryBackfillProgress`) and
 * enqueues up to 500 embedding jobs on demand (`memoryBackfillStart`).
 *
 * The backfill runs as background jobs, so every completion moves the `jobs`
 * topic and re-reads progress the moment it changes. The 4 s poll survives as
 * the fallback for a dropped channel (v4 `f3892158d`).
 */
/** Fallback poll cadence, used only while the realtime channel is down. */
const FALLBACK_POLL_INTERVAL_MS = 4_000;

@Component({
  selector: 'qt-memory-backfill-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (progressQuery.isPending()) {
      <p class="qt-text-small qt-text-muted">Loading backfill status…</p>
    } @else {
      <div class="space-y-4">
        <p class="qt-text-small qt-text-muted">
          Some older memories may not carry an embedding — usually because the pre-write gate fell
          back to a keyword check when the embedding provider was briefly unavailable, or because the
          memory was imported before the gate became embedding-aware. Such memories can't be found by
          semantic search and are invisible to the deduplication gate, which lets phrase-variants
          accumulate. Running the backfill enqueues an embedding job for each of them so they rejoin
          the fold.
        </p>

        <div class="qt-body">
          <div class="flex items-center gap-4">
            <div>
              <span class="qt-text-muted">Memories missing an embedding: </span>
              <strong>{{ remaining().toLocaleString() }}</strong>
            </div>
            <div>
              <span class="qt-text-muted">Embedding jobs in flight: </span>
              <strong>{{ inFlight().toLocaleString() }}</strong>
            </div>
          </div>
        </div>

        <div class="flex items-center gap-3">
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="running() || remaining() === 0"
            (click)="start()"
          >
            {{ remaining() === 0 ? 'Nothing to backfill' : 'Backfill up to 500 memories' }}
          </button>
          <span class="qt-text-small qt-text-muted">
            Run repeatedly for large backlogs. Jobs drain in the background.
          </span>
        </div>

        @if (error(); as msg) {
          <p class="qt-text-small qt-text-destructive">{{ msg }}</p>
        }
      </div>
    }
  `,
})
export class MemoryBackfillCard {
  private readonly core = inject(CoreClient);
  private readonly realtime = inject(RealtimeService);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  protected readonly running = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly progressQuery = injectQuery(() => ({
    queryKey: memoryKeys.backfill(),
    queryFn: (): Promise<BackfillProgress> => fetchBackfillProgress(this.core),
    // Fallback only: v4's 4 s cadence, gated on channel health.
    refetchInterval: this.realtime.refetchInterval(FALLBACK_POLL_INTERVAL_MS),
  }));

  constructor() {
    // Live path: the backfill runs as background jobs, so every completion
    // moves the `jobs` topic (v4 `useRealtimeTopic('jobs', fetchProgress)`).
    this.realtime.onTopic('jobs', () => {
      void this.queryClient.invalidateQueries({ queryKey: memoryKeys.backfill() });
    });
  }

  protected readonly remaining = computed(() => this.progressQuery.data()?.remaining ?? 0);
  protected readonly inFlight = computed(() => this.progressQuery.data()?.inFlight ?? 0);

  protected async start(): Promise<void> {
    this.running.set(true);
    this.error.set(null);
    try {
      const result = await startBackfill(this.core, 500);
      // v4 `:63` prefers the server's sentence, else names the count.
      this.toasts.showSuccess(
        (result as { message?: string; enqueued?: number }).message ||
          `Enqueued ${(result as { enqueued?: number }).enqueued ?? 0} embedding jobs`,
      );
      await this.queryClient.invalidateQueries({ queryKey: memoryKeys.backfill() });
    } catch (err) {
      const message =
        err instanceof Error && err.message ? err.message : 'Failed to start backfill';
      this.error.set(message);
      this.toasts.showError(message);
    } finally {
      this.running.set(false);
    }
  }
}
