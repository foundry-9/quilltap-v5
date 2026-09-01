import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';

import { CoreClient, coreErrorMessage } from '../../../core/core-client';
import type { DedupPreviewDto } from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';
import { Modal } from '../../../ui/modal';
import { ToastService } from '../../../ui/toast.service';

type Step = 'idle' | 'analyzing' | 'preview' | 'running' | 'complete';

/**
 * The Memory Deduplication card (v4 `components/tools/memory-dedup-card.tsx`): a
 * similarity-threshold slider (0.70–0.95, default 0.80 — narrower than the API's
 * accepted 0.5–1.0, carried faithfully), an Analyze step that opens a preview
 * dialog (per-character table + totals), and a Run step that applies the merge.
 *
 * The dialog rides `qt-modal` (the `qt-dialog-overlay` backdrop convention). Run
 * success/failure toast v4's exact sentences; preview failures are inline-only,
 * deliberately NOT toasted.
 */
@Component({
  selector: 'qt-memory-dedup-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Modal],
  template: `
    <div>
      <div class="flex items-start gap-4 mb-6">
        <div class="flex-1">
          <h2 class="qt-heading-2 mb-1">Memory Deduplication</h2>
          <p class="qt-text-small">
            Find and merge duplicate memories across all characters. Semantically similar memories
            are clustered, the best version is kept, and unique details from duplicates are preserved
            as footnotes.
          </p>
        </div>
        <div class="flex-shrink-0 text-primary">
          <qt-icon name="cpu" class="w-8 h-8" />
        </div>
      </div>

      <div class="mb-4">
        <label class="qt-text-small font-medium block mb-2">
          Similarity Threshold: {{ threshold().toFixed(2) }}
        </label>
        <input
          type="range"
          min="0.70"
          max="0.95"
          step="0.01"
          class="qt-range w-full"
          [value]="threshold()"
          (input)="threshold.set(parseThreshold($any($event.target).value))"
        />
        <div class="flex justify-between qt-text-xs mt-1">
          <span>0.70 (more aggressive)</span>
          <span>0.95 (more conservative)</span>
        </div>
      </div>

      <button type="button" class="qt-button qt-button-primary" (click)="analyze()">
        Analyze Memories
      </button>
    </div>

    @if (showDialog()) {
      <qt-modal [title]="dialogTitle()" maxWidth="lg" (close)="closeDialog()">
        @switch (step()) {
          @case ('analyzing') {
            <div class="flex flex-col items-center justify-center py-8 space-y-4">
              <div class="qt-spinner text-primary"></div>
              <p class="qt-text-small">Analyzing memories for duplicates...</p>
            </div>
          }
          @case ('preview') {
            @if (error()) {
              <div class="p-3 qt-bg-destructive/10 border qt-border-destructive/30 rounded-lg">
                <p class="text-sm qt-text-destructive">{{ error() }}</p>
              </div>
            } @else if (preview(); as p) {
              <div class="space-y-4">
                <p class="qt-text-small">
                  Analysis at threshold <span class="font-semibold">{{ p.threshold.toFixed(2) }}</span>:
                </p>

                <div class="max-h-64 overflow-y-auto border qt-border-default rounded-lg">
                  <table class="w-full text-sm">
                    <thead class="qt-bg-muted sticky top-0">
                      <tr>
                        <th class="text-left px-3 py-2 qt-text-secondary font-medium">Character</th>
                        <th class="text-right px-3 py-2 qt-text-secondary font-medium">Memories</th>
                        <th class="text-right px-3 py-2 qt-text-secondary font-medium">Clusters</th>
                        <th class="text-right px-3 py-2 qt-text-secondary font-medium">Removable</th>
                        <th class="text-right px-3 py-2 qt-text-secondary font-medium">Details</th>
                      </tr>
                    </thead>
                    <tbody>
                      @for (char of p.characters; track char.characterId) {
                        <tr [class.opacity-50]="char.clustersFound === 0">
                          <td class="px-3 py-2 text-foreground truncate max-w-[150px]">{{ char.characterName }}</td>
                          <td class="text-right px-3 py-2 qt-text-primary">{{ char.originalCount }}</td>
                          <td class="text-right px-3 py-2 qt-text-primary">{{ char.clustersFound }}</td>
                          <td class="text-right px-3 py-2 qt-text-primary">{{ char.removedCount }}</td>
                          <td class="text-right px-3 py-2 qt-text-primary">{{ char.mergedDetailCount }}</td>
                        </tr>
                      }
                    </tbody>
                    <tfoot class="qt-bg-muted border-t qt-border-default">
                      <tr class="font-semibold">
                        <td class="px-3 py-2 text-foreground">Total</td>
                        <td class="text-right px-3 py-2 text-foreground">{{ p.totalOriginal }}</td>
                        <td class="text-right px-3 py-2 text-foreground">{{ totalClusters(p) }}</td>
                        <td class="text-right px-3 py-2 text-foreground">{{ p.totalRemoved }}</td>
                        <td class="text-right px-3 py-2 text-foreground">{{ p.totalMergedDetails }}</td>
                      </tr>
                    </tfoot>
                  </table>
                </div>

                @if (totalRemovable() > 0) {
                  <div class="qt-bg-primary/10 border qt-border-primary/30 rounded-lg p-3">
                    <p class="text-sm text-foreground">
                      <span class="font-semibold">{{ p.totalRemoved }}</span> duplicate memories can
                      be removed. <span class="font-semibold">{{ p.totalMergedDetails }}</span> unique
                      details will be preserved as footnotes in surviving memories.
                    </p>
                  </div>
                } @else {
                  <div class="qt-bg-muted rounded-lg p-3">
                    <p class="text-sm qt-text-secondary">
                      No duplicate memories found at this threshold. Try lowering the threshold to
                      find more matches.
                    </p>
                  </div>
                }
              </div>
            }
          }
          @case ('running') {
            <div class="flex flex-col items-center justify-center py-8 space-y-4">
              <div class="qt-spinner text-primary"></div>
              <p class="qt-text-small">Deduplicating memories...</p>
              <p class="qt-text-xs">Merging unique details and removing duplicates</p>
            </div>
          }
          @case ('complete') {
            @if (result(); as r) {
              <div class="space-y-4">
                <div class="flex items-center justify-center">
                  <div class="w-16 h-16 qt-bg-success/10 rounded-full flex items-center justify-center">
                    <qt-icon name="check" class="w-8 h-8 qt-text-success" />
                  </div>
                </div>

                <div class="max-h-64 overflow-y-auto border qt-border-default rounded-lg">
                  <table class="w-full text-sm">
                    <thead class="qt-bg-muted sticky top-0">
                      <tr>
                        <th class="text-left px-3 py-2 qt-text-secondary font-medium">Character</th>
                        <th class="text-right px-3 py-2 qt-text-secondary font-medium">Before</th>
                        <th class="text-right px-3 py-2 qt-text-secondary font-medium">Removed</th>
                        <th class="text-right px-3 py-2 qt-text-secondary font-medium">After</th>
                      </tr>
                    </thead>
                    <tbody>
                      @for (char of removedRows(r); track char.characterId) {
                        <tr>
                          <td class="px-3 py-2 text-foreground truncate max-w-[150px]">{{ char.characterName }}</td>
                          <td class="text-right px-3 py-2 qt-text-primary">{{ char.originalCount }}</td>
                          <td class="text-right px-3 py-2 qt-text-destructive">{{ char.removedCount }}</td>
                          <td class="text-right px-3 py-2 qt-text-primary">{{ char.finalCount }}</td>
                        </tr>
                      }
                    </tbody>
                    <tfoot class="qt-bg-muted border-t qt-border-default">
                      <tr class="font-semibold">
                        <td class="px-3 py-2 text-foreground">Total</td>
                        <td class="text-right px-3 py-2 text-foreground">{{ r.totalOriginal }}</td>
                        <td class="text-right px-3 py-2 qt-text-destructive">{{ r.totalRemoved }}</td>
                        <td class="text-right px-3 py-2 text-foreground">{{ r.totalFinal }}</td>
                      </tr>
                    </tfoot>
                  </table>
                </div>

                <p class="text-center qt-text-xs">
                  {{ r.totalMergedDetails }} unique details preserved as footnotes in surviving
                  memories.
                </p>
              </div>
            }
          }
        }

        <div qt-modal-footer>
          @if (step() === 'preview') {
            <div class="flex gap-3 justify-end">
              <button type="button" class="qt-button qt-button-secondary" (click)="closeDialog()">
                Cancel
              </button>
              <button
                type="button"
                class="qt-button qt-button-primary"
                [disabled]="!preview() || totalRemovable() === 0"
                (click)="run()"
              >
                Run Deduplication
              </button>
            </div>
          } @else if (step() === 'complete') {
            <div class="flex gap-3 justify-end">
              <button type="button" class="qt-button qt-button-primary" (click)="closeDialog()">
                Done
              </button>
            </div>
          }
        </div>
      </qt-modal>
    }
  `,
})
export class MemoryDedupCard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  protected readonly showDialog = signal(false);
  protected readonly step = signal<Step>('idle');
  protected readonly threshold = signal(0.8);
  protected readonly preview = signal<DedupPreviewDto | null>(null);
  protected readonly result = signal<DedupPreviewDto | null>(null);
  protected readonly error = signal<string | null>(null);

  protected readonly totalRemovable = computed(() => this.preview()?.totalRemoved ?? 0);
  protected readonly dialogTitle = computed(() =>
    this.step() === 'complete' ? 'Deduplication Complete' : 'Memory Deduplication',
  );

  /** Clamp NaN out of the range input (v4 `parseFloat`). */
  protected parseThreshold(value: string): number {
    const n = Number.parseFloat(value);
    return Number.isNaN(n) ? this.threshold() : n;
  }

  protected totalClusters(p: DedupPreviewDto): number {
    return p.characters.reduce((sum, c) => sum + c.clustersFound, 0);
  }

  protected removedRows(r: DedupPreviewDto): DedupPreviewDto['characters'] {
    return r.characters.filter((c) => c.removedCount > 0);
  }

  protected async analyze(): Promise<void> {
    this.showDialog.set(true);
    this.step.set('analyzing');
    this.preview.set(null);
    this.result.set(null);
    this.error.set(null);
    try {
      const data = await this.core.memoryDedupPreview(this.threshold());
      this.preview.set(data);
      this.step.set('preview');
    } catch (err) {
      // Preview failures are inline-only (v4 :60-65 — deliberately NOT toasted).
      this.error.set(coreErrorMessage(err, 'Failed to analyze memories'));
      this.step.set('preview');
    }
  }

  protected async run(): Promise<void> {
    if (!this.preview()) return;
    this.step.set('running');
    this.error.set(null);
    try {
      const data = await this.core.memoryDedupRun(this.threshold());
      this.result.set(data);
      this.step.set('complete');
      this.toasts.showSuccess(
        `Removed ${data.totalRemoved} duplicate memories, merged ${data.totalMergedDetails} details`,
      );
    } catch (err) {
      const msg = coreErrorMessage(err, 'Failed to deduplicate memories');
      this.error.set(msg);
      this.step.set('preview');
      this.toasts.showError(msg);
    }
  }

  protected closeDialog(): void {
    this.showDialog.set(false);
    this.step.set('idle');
    this.preview.set(null);
    this.result.set(null);
    this.error.set(null);
  }
}
