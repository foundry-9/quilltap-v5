import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { BackfillProgress } from '../../../core/core-contract';
import { fetchBackfillProgress, memoryKeys, startBackfill } from '../../../memory/memory.api';
import { ToastService } from '../../../ui/toast.service';

/**
 * The Repair Missing Embeddings card (v4 `components/tools/memory-backfill-card.tsx`):
 * polls the instance-wide backfill progress (`memoryBackfillProgress`) every 4s
 * and enqueues up to 500 embedding jobs on demand (`memoryBackfillStart`).
 */
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

        <div class="qt-text-body">
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
          <p class="qt-text-small qt-text-error">{{ msg }}</p>
        }
      </div>
    }
  `,
})
export class MemoryBackfillCard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  protected readonly running = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly progressQuery = injectQuery(() => ({
    queryKey: memoryKeys.backfill(),
    queryFn: (): Promise<BackfillProgress> => fetchBackfillProgress(this.core),
    // v4 polls every 4s so the counts drain live.
    refetchInterval: 4000,
  }));

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
