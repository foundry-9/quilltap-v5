import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { HousekeepingPreview } from '../core/core-contract';
import { Modal } from '../ui/modal';
import { fetchHousekeepingPreview, runHousekeeping } from './memory.api';
import { ToastService } from '../ui/toast.service';

const PREVIEW_DEBOUNCE_MS = 300;

/**
 * The per-character memory cleanup dialog (v4 `components/memory/
 * housekeeping-dialog.tsx`): the options (max unprotected memories, max age,
 * min-importance threshold, merge-similar), a debounced preview
 * (`memoryHousekeepPreview`) with Keep / Delete / Merge stats and a changes
 * list, and a Run (`memoryHousekeep`, `dryRun:false`) that deletes and closes.
 * Run is disabled while loading or when the preview shows nothing to do.
 */
@Component({
  selector: 'qt-housekeeping-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Memory Cleanup" maxWidth="2xl" (close)="close.emit()">
      <p class="qt-dialog-description mb-4">
        Clean up old and low-importance memories to stay within limits.
      </p>

      <!-- Options -->
      <div class="space-y-4 pb-4 mb-4 border-b qt-border-default">
        <div class="grid grid-cols-2 gap-4">
          <div>
            <label class="qt-label mb-1 block">Maximum Unprotected Memories</label>
            <input
              type="number"
              min="10"
              max="10000"
              class="qt-input"
              [value]="maxMemories()"
              (input)="maxMemories.set(parseIntOr($any($event.target).value, 1000))"
            />
            <div class="mt-1 qt-text-xs space-y-0.5">
              <p>Not deleting memories that are:</p>
              <ul class="list-disc pl-4">
                <li>Importance &ge; 70%</li>
                <li>Reinforced 5+ times</li>
                <li>Manually created</li>
                <li>Accessed within last 3 months</li>
              </ul>
            </div>
          </div>

          <div>
            <label class="qt-label mb-1 block">Max Age (months)</label>
            <input
              type="number"
              min="1"
              max="120"
              class="qt-input"
              [value]="maxAgeMonths()"
              (input)="maxAgeMonths.set(parseIntOr($any($event.target).value, 6))"
            />
            <p class="mt-1 qt-text-xs">Delete old low-importance memories</p>
          </div>
        </div>

        <div>
          <label class="qt-label mb-1 block">Min Importance: {{ minImportancePct() }}%</label>
          <input
            type="range"
            min="0"
            max="0.7"
            step="0.1"
            class="qt-range w-full"
            [value]="minImportance()"
            (input)="minImportance.set(+$any($event.target).value)"
          />
          <div class="flex justify-between qt-text-xs mt-1">
            <span>0%</span>
            <span>Threshold for deletion</span>
            <span>70%</span>
          </div>
        </div>

        <label class="flex items-center gap-2">
          <input
            type="checkbox"
            class="w-4 h-4 rounded"
            [checked]="mergeSimilar()"
            (change)="mergeSimilar.set($any($event.target).checked)"
          />
          <span class="text-sm text-foreground">Merge similar memories (requires embeddings)</span>
        </label>
      </div>

      <!-- Preview -->
      <div>
        @if (loading()) {
          <div class="flex items-center justify-center py-8">
            <p class="qt-text-small">Loading preview...</p>
          </div>
        } @else if (error()) {
          <div class="qt-alert-error">{{ error() }}</div>
        } @else if (preview(); as p) {
          <div class="space-y-4">
            <div class="grid grid-cols-3 gap-4">
              <div class="qt-bg-success/10 border qt-border-success/30 rounded-lg p-4 text-center">
                <p class="text-2xl font-bold qt-text-success">{{ p.wouldKeep }}</p>
                <p class="text-sm qt-text-success">Keep</p>
              </div>
              <div class="qt-bg-destructive/10 border qt-border-destructive/30 rounded-lg p-4 text-center">
                <p class="text-2xl font-bold qt-text-destructive">{{ p.wouldDelete }}</p>
                <p class="text-sm qt-text-destructive">Delete</p>
              </div>
              <div class="qt-bg-warning/10 border qt-border-warning/30 rounded-lg p-4 text-center">
                <p class="text-2xl font-bold qt-text-warning">{{ p.wouldMerge }}</p>
                <p class="text-sm qt-text-warning">Merge</p>
              </div>
            </div>

            @if (p.wouldDelete > 0 || p.wouldMerge > 0) {
              <div>
                <h3 class="text-sm qt-text-primary mb-2">Changes Preview</h3>
                <div class="max-h-64 overflow-y-auto space-y-2">
                  @for (detail of changedDetails(); track detail.memoryId) {
                    <div
                      class="p-3 rounded-lg qt-text-small"
                      [class]="
                        detail.action === 'deleted'
                          ? 'qt-bg-destructive/10 border qt-border-destructive/30'
                          : 'qt-bg-warning/10 border qt-border-warning/30'
                      "
                    >
                      <p class="qt-text-primary line-clamp-1">
                        {{ detail.summary || 'Untitled memory' }}
                      </p>
                      <p class="qt-text-xs mt-1">{{ detail.reason }}</p>
                    </div>
                  }
                </div>
              </div>
            } @else {
              <div class="text-center py-8 qt-text-small">
                <p>No memories to clean up with current settings.</p>
                <p class="qt-text-xs mt-1">All memories are within retention policy.</p>
              </div>
            }
          </div>
        }
      </div>

      <div qt-modal-footer>
        <button type="button" class="qt-button qt-button-secondary" (click)="close.emit()">
          Cancel
        </button>
        <button
          type="button"
          class="qt-button qt-button-destructive"
          [disabled]="runDisabled()"
          (click)="run()"
        >
          {{ running() ? 'Running...' : 'Delete ' + (preview()?.wouldDelete ?? 0) + ' Memories' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class HousekeepingDialog {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly destroyRef = inject(DestroyRef);

  readonly characterId = input.required<string>();

  readonly close = output<void>();
  readonly complete = output<void>();

  protected readonly maxMemories = signal(1000);
  protected readonly maxAgeMonths = signal(6);
  protected readonly minImportance = signal(0.3);
  protected readonly mergeSimilar = signal(false);

  protected readonly loading = signal(true);
  protected readonly running = signal(false);
  protected readonly preview = signal<HousekeepingPreview | null>(null);
  protected readonly error = signal<string | null>(null);

  private previewTimer: ReturnType<typeof setTimeout> | null = null;

  protected readonly minImportancePct = computed(() => (this.minImportance() * 100).toFixed(0));

  protected readonly changedDetails = computed(
    () => this.preview()?.details.filter((d) => d.action !== 'kept') ?? [],
  );

  protected readonly runDisabled = computed(() => {
    const p = this.preview();
    return this.running() || this.loading() || !p || (p.wouldDelete === 0 && p.wouldMerge === 0);
  });

  constructor() {
    // v4: refetch the preview (debounced 300ms) whenever an option changes.
    effect(() => {
      const params = {
        characterId: this.characterId(),
        maxMemories: this.maxMemories(),
        maxAgeMonths: this.maxAgeMonths(),
        minImportance: this.minImportance(),
        mergeSimilar: this.mergeSimilar(),
      };
      if (this.previewTimer) {
        clearTimeout(this.previewTimer);
      }
      this.previewTimer = setTimeout(() => void this.loadPreview(params), PREVIEW_DEBOUNCE_MS);
    });

    this.destroyRef.onDestroy(() => {
      if (this.previewTimer) {
        clearTimeout(this.previewTimer);
      }
    });
  }

  private async loadPreview(params: {
    characterId: string;
    maxMemories: number;
    maxAgeMonths: number;
    minImportance: number;
    mergeSimilar: boolean;
  }): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      this.preview.set(await fetchHousekeepingPreview(this.core, params));
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to load preview');
    } finally {
      this.loading.set(false);
    }
  }

  protected async run(): Promise<void> {
    this.running.set(true);
    try {
      await runHousekeeping(this.core, {
        characterId: this.characterId(),
        maxMemories: this.maxMemories(),
        maxAgeMonths: this.maxAgeMonths(),
        minImportance: this.minImportance(),
        mergeSimilar: this.mergeSimilar(),
        dryRun: false,
      });
      // v4 `:89` names the count; v5's run reply carries none, so the sentence
      // is the count-less half of v4's — recorded in the census.
      this.toasts.showSuccess('Memory housekeeping complete');
      this.complete.emit();
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to run cleanup';
      this.error.set(message);
      this.toasts.showError(message);
    } finally {
      this.running.set(false);
    }
  }

  protected parseIntOr(value: string, fallback: number): number {
    const n = parseInt(value, 10);
    return Number.isNaN(n) ? fallback : n;
  }
}
