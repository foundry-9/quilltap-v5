import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { CharacterMemoryCount, HousekeepingConfig } from '../../../core/core-contract';
import {
  fetchCharacterMemoryCounts,
  fetchHousekeepingConfig,
  memoryKeys,
  runHousekeepingSweep,
  setHousekeepingConfig,
} from '../../../memory/memory.api';
import { ToastService } from '../../../ui/toast.service';

const DEFAULT_CONFIG: HousekeepingConfig = {
  enabled: false,
  perCharacterCap: 2000,
  perCharacterCapOverrides: {},
  autoMergeSimilarThreshold: 0.9,
  mergeSimilar: false,
};

/**
 * The Memory Housekeeping card (v4 `components/tools/memory-housekeeping-card.tsx`):
 * the instance-wide auto-housekeeping toggle, the per-character cap, the
 * collapsible per-character overrides (each validated 100–100,000, blank clears),
 * the merge-similar toggle, and a "Run housekeeping now" sweep. Config writes are
 * merge-patch (`memoryHousekeepingConfigSet`, changed fields only).
 */
@Component({
  selector: 'qt-memory-housekeeping-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (configQuery.isPending()) {
      <p class="qt-text-small qt-text-muted">Loading housekeeping settings…</p>
    } @else {
      <div class="space-y-4">
        <p class="qt-text-small qt-text-muted">
          Automatic housekeeping prunes low-importance, stale memories once a character approaches its
          cap. High-importance, manually added, recently accessed, and well-reinforced memories are
          never touched. Off by default — toggle on once you've reviewed the limits below.
        </p>

        <label class="flex items-center gap-3 qt-body">
          <input
            type="checkbox"
            class="qt-checkbox"
            [checked]="config().enabled"
            [disabled]="saving()"
            (change)="toggleEnabled()"
          />
          <span>Enable automatic housekeeping</span>
        </label>

        <div class="flex items-center gap-3">
          <label class="qt-text-small qt-text-muted" for="perCharacterCap">Per-character cap</label>
          <input
            id="perCharacterCap"
            type="number"
            min="100"
            max="100000"
            step="100"
            class="qt-input w-32"
            [value]="config().perCharacterCap"
            [disabled]="saving()"
            (input)="capDraft.set($any($event.target).value)"
            (blur)="onCapBlur($any($event.target).value)"
          />
          <span class="qt-text-small qt-text-muted">memories per character</span>
        </div>

        @if (characters().length > 0) {
          <div>
            <button
              type="button"
              class="qt-text-small qt-text-muted underline-offset-2 hover:underline"
              (click)="showOverrides.set(!showOverrides())"
            >
              {{ showOverrides() ? '▾' : '▸' }} Per-character overrides ({{ overrideCount() }} active)
            </button>

            @if (showOverrides()) {
              <div class="mt-3 space-y-2">
                <p class="qt-text-small qt-text-muted">
                  Set a different cap for individual characters. Leave blank to fall back to the global
                  cap ({{ config().perCharacterCap.toLocaleString() }}).
                </p>
                <div class="space-y-1">
                  @for (character of characters(); track character.id) {
                    <div class="flex items-center gap-3">
                      <div class="flex-1 qt-body">
                        <span>{{ character.name }}</span>
                        <span
                          [class]="
                            character.memoryCount > effectiveCap(character.id)
                              ? 'qt-text-destructive qt-text-small'
                              : 'qt-text-muted qt-text-small'
                          "
                        >
                          ({{ character.memoryCount.toLocaleString() }} memories)
                        </span>
                      </div>
                      <input
                        type="number"
                        min="100"
                        max="100000"
                        step="100"
                        class="qt-input w-28"
                        [placeholder]="config().perCharacterCap.toString()"
                        [value]="draftValue(character.id)"
                        [disabled]="saving()"
                        [attr.aria-label]="'Cap override for ' + character.name"
                        (input)="setDraft(character.id, $any($event.target).value)"
                        (blur)="onOverrideBlur(character.id, $any($event.target).value)"
                      />
                    </div>
                  }
                </div>
              </div>
            }
          </div>
        }

        <label class="flex items-center gap-3 qt-body">
          <input
            type="checkbox"
            class="qt-checkbox"
            [checked]="config().mergeSimilar"
            [disabled]="saving()"
            (change)="toggleMergeSimilar()"
          />
          <span>Also merge semantically similar memories during the sweep</span>
        </label>

        <div class="flex items-center gap-3">
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="saving()"
            (click)="runNow()"
          >
            Run housekeeping now
          </button>
          <span class="qt-text-small qt-text-muted">
            Sweeps every character. Runs in the background.
          </span>
        </div>

        @if (error(); as msg) {
          <p class="qt-text-small qt-text-destructive">{{ msg }}</p>
        }
      </div>
    }
  `,
})
export class MemoryHousekeepingCard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly showOverrides = signal(false);
  protected readonly capDraft = signal<string | null>(null);
  private readonly overrideDrafts = signal<Record<string, string>>({});

  /** Local echo of the saved config (seeded from the query, updated on save). */
  private readonly local = signal<HousekeepingConfig | null>(null);

  protected readonly configQuery = injectQuery(() => ({
    queryKey: memoryKeys.housekeepingConfig(),
    queryFn: async (): Promise<HousekeepingConfig> => {
      const settings = await fetchHousekeepingConfig(this.core);
      return { ...DEFAULT_CONFIG, ...settings };
    },
  }));

  protected readonly charactersQuery = injectQuery(() => ({
    queryKey: memoryKeys.characterCounts(),
    queryFn: (): Promise<CharacterMemoryCount[]> => fetchCharacterMemoryCounts(this.core),
  }));

  protected readonly config = computed<HousekeepingConfig>(
    () => this.local() ?? this.configQuery.data() ?? DEFAULT_CONFIG,
  );
  protected readonly characters = computed(() => this.charactersQuery.data() ?? []);
  protected readonly overrideCount = computed(
    () => Object.keys(this.config().perCharacterCapOverrides).length,
  );

  protected effectiveCap(characterId: string): number {
    return this.config().perCharacterCapOverrides[characterId] ?? this.config().perCharacterCap;
  }

  protected draftValue(characterId: string): string {
    const drafts = this.overrideDrafts();
    if (characterId in drafts) {
      return drafts[characterId];
    }
    const override = this.config().perCharacterCapOverrides[characterId];
    return override === undefined ? '' : String(override);
  }

  protected setDraft(characterId: string, value: string): void {
    this.overrideDrafts.update((d) => ({ ...d, [characterId]: value }));
  }

  protected toggleEnabled(): void {
    const next = !this.config().enabled;
    this.local.set({ ...this.config(), enabled: next });
    void this.save({ enabled: next });
  }

  protected toggleMergeSimilar(): void {
    const next = !this.config().mergeSimilar;
    this.local.set({ ...this.config(), mergeSimilar: next });
    void this.save({ mergeSimilar: next });
  }

  protected onCapBlur(raw: string): void {
    const value = Number(raw);
    // v4: reject out-of-range or unchanged.
    if (!Number.isFinite(value) || value < 100 || value > 100000) return;
    if (value === this.config().perCharacterCap) return;
    void this.save({ perCharacterCap: Math.floor(value) });
  }

  protected onOverrideBlur(characterId: string, rawValue: string): void {
    const trimmed = rawValue.trim();
    const nextOverrides: Record<string, number> = { ...this.config().perCharacterCapOverrides };

    if (trimmed === '') {
      // Empty input removes any override for this character.
      if (characterId in nextOverrides) {
        delete nextOverrides[characterId];
      } else {
        return; // nothing to save
      }
    } else {
      const parsed = Math.floor(Number(trimmed));
      if (!Number.isFinite(parsed) || parsed < 100 || parsed > 100000) {
        this.error.set(
          `Override for character ${characterId.slice(0, 8)} must be between 100 and 100,000`,
        );
        return;
      }
      if (nextOverrides[characterId] === parsed) return; // no change
      nextOverrides[characterId] = parsed;
    }

    this.overrideDrafts.update((d) => {
      const next = { ...d };
      delete next[characterId];
      return next;
    });
    void this.save({ perCharacterCapOverrides: nextOverrides });
  }

  protected async runNow(): Promise<void> {
    this.saving.set(true);
    this.error.set(null);
    try {
      await runHousekeepingSweep(this.core);
      this.toasts.showSuccess('Housekeeping job enqueued — it will run in the background');
    } catch (err) {
      const message =
        err instanceof Error && err.message ? err.message : 'Failed to run housekeeping';
      this.error.set(message);
      this.toasts.showError(message);
    } finally {
      this.saving.set(false);
    }
  }

  private async save(patch: Partial<HousekeepingConfig>): Promise<void> {
    this.saving.set(true);
    this.error.set(null);
    try {
      const settings = await setHousekeepingConfig(this.core, patch);
      this.local.set({ ...DEFAULT_CONFIG, ...settings });
      this.toasts.showSuccess('Housekeeping settings saved');
    } catch (err) {
      const message =
        err instanceof Error && err.message ? err.message : 'Failed to save housekeeping settings';
      this.error.set(message);
      this.toasts.showError(message);
    } finally {
      this.saving.set(false);
    }
  }
}
