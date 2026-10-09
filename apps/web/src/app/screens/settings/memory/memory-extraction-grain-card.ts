import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { MemoryExtractionModeConfig } from '../../../core/core-contract';
import {
  fetchExtractionModeConfig,
  memoryKeys,
  setExtractionModeConfig,
} from '../../../memory/memory.api';
import { ToastService } from '../../../ui/toast.service';
import { readErrorText, writeErrorText } from './api-error-text';

type OtherPass = MemoryExtractionModeConfig['otherPass'];

/** v4 `DEFAULT_CONFIG` (`memory-extraction-grain-card.tsx:18-22`). */
const DEFAULT_CONFIG: MemoryExtractionModeConfig = {
  otherPass: 'hybrid',
  perTurnOtherFloor: 0.75,
  foldCandidatesPerSubject: 3,
};

/** v4 `OTHER_PASS_OPTIONS` (`:26-43`), in v4's order. */
const OTHER_PASS_OPTIONS: ReadonlyArray<{ value: OtherPass; label: string; description: string }> = [
  {
    value: 'hybrid',
    label: 'Hybrid (recommended)',
    description:
      'Observations of others are gathered when a conversation is folded into its summary, with only the weightiest — commitments, agreements, new standing facts — noted as they happen.',
  },
  {
    value: 'fold',
    label: 'At the fold only',
    description:
      'Observations of others are gathered solely when a conversation is folded into its summary. The thriftiest course.',
  },
  {
    value: 'turn',
    label: 'Every turn',
    description:
      'Observations of others are noted after each turn, as of old. The most thorough, and the most prone to repeating itself.',
  },
];

/**
 * The Extraction Grain card (v4 `components/tools/memory-extraction-grain-
 * card.tsx` at `01a83539d`, new in `d58548051`): the grain at which a
 * character's observations of OTHER characters are extracted — instance-wide
 * (`memoryExtractionModeConfigGet/Set`).
 *
 * Ported faithfully: one key per save, the server's echo shown over
 * `DEFAULT_CONFIG` (P4.D276 R-F — v4's server-side default-fill clobber stays
 * visible); the select snaps back to the stored mode on a failed save (R-E);
 * the blur rules clear the draft FIRST and coerce with `Number` (no trim), so a
 * BLANK floor saves `0` while a blank per-subject is refused — v4's
 * `Number('') === 0` asymmetry, kept. No realtime subscription (R-J).
 */
@Component({
  selector: 'qt-memory-extraction-grain-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (configQuery.isLoading()) {
      <p class="qt-text-small qt-text-muted">Loading extraction settings…</p>
    } @else {
      <div class="space-y-4">
        <p class="qt-text-small qt-text-muted">
          A character’s notes about <em>themselves</em> are always taken as the conversation goes.
          Notes about <em>other</em> characters are another matter: taken every turn, they pile up
          into a hundred ways of saying the same thing. Choose the grain.
        </p>

        <div class="space-y-1">
          <label for="extraction-other-pass" class="block qt-text-label">Observations of others</label>
          <select
            id="extraction-other-pass"
            class="qt-select"
            [disabled]="saving()"
            (change)="chooseOtherPass($any($event.target))"
          >
            @for (o of options; track o.value) {
              <option [value]="o.value" [selected]="config().otherPass === o.value">{{ o.label }}</option>
            }
          </select>
          @if (current(); as c) {
            <p class="qt-text-xs qt-text-muted">{{ c.description }}</p>
          }
        </div>

        <div class="grid gap-3 sm:grid-cols-2">
          <div class="space-y-1">
            <label for="extraction-floor" class="block qt-text-label">Per-turn importance floor</label>
            <input
              id="extraction-floor"
              type="number"
              min="0"
              max="1"
              step="0.05"
              class="qt-input"
              [disabled]="saving() || config().otherPass !== 'hybrid'"
              [value]="floorDraft() ?? floorText()"
              (input)="floorDraft.set($any($event.target).value)"
              (blur)="commitFloor()"
            />
            <p class="qt-text-xs qt-text-muted">
              In hybrid mode, only observations at or above this importance are noted turn by turn.
            </p>
          </div>
          <div class="space-y-1">
            <label for="extraction-per-subject" class="block qt-text-label">Fold candidates per subject</label>
            <input
              id="extraction-per-subject"
              type="number"
              min="1"
              step="1"
              class="qt-input"
              [disabled]="saving() || config().otherPass === 'turn'"
              [value]="perSubjectDraft() ?? perSubjectText()"
              (input)="perSubjectDraft.set($any($event.target).value)"
              (blur)="commitPerSubject()"
            />
            <p class="qt-text-xs qt-text-muted">
              At a fold, the most notes kept for any one observer and subject.
            </p>
          </div>
        </div>

        @if (error(); as msg) {
          <p class="qt-text-small qt-text-destructive">{{ msg }}</p>
        }
      </div>
    }
  `,
})
export class MemoryExtractionGrainCard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  protected readonly options = OTHER_PASS_OPTIONS;

  protected readonly config = signal<MemoryExtractionModeConfig>(DEFAULT_CONFIG);
  protected readonly floorDraft = signal<string | null>(null);
  protected readonly perSubjectDraft = signal<string | null>(null);
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly configQuery = injectQuery(() => ({
    queryKey: memoryKeys.extractionModeConfig(),
    queryFn: () => fetchExtractionModeConfig(this.core),
  }));

  /** v4 `:119`. */
  protected readonly current = computed(
    () => OTHER_PASS_OPTIONS.find((o) => o.value === this.config().otherPass) ?? null,
  );
  protected readonly floorText = computed(() => String(this.config().perTurnOtherFloor));
  protected readonly perSubjectText = computed(() => String(this.config().foldCandidatesPerSubject));

  constructor() {
    // v4 `:62-65` — seed the form from the query, only when `settings` came back.
    effect(() => {
      const loaded = this.configQuery.data();
      if (loaded) {
        this.config.set({ ...DEFAULT_CONFIG, ...loaded });
      }
    });
    // v4 `:67-70`.
    effect(() => {
      const loadError = this.configQuery.error();
      if (loadError) {
        this.error.set(readErrorText(loadError, 'Failed to load extraction settings'));
      }
    });
  }

  /** v4 `:136` — saved at once; R-E: the select snaps back to the stored mode. */
  protected async chooseOtherPass(el: HTMLSelectElement): Promise<void> {
    await this.save({ otherPass: el.value as OtherPass });
    el.value = this.config().otherPass;
  }

  /** v4 `commitFloor` (`:93-102`) — the draft cleared BEFORE validation; `Number`, no trim. */
  protected commitFloor(): void {
    const draft = this.floorDraft();
    if (draft === null) return;
    const value = Number(draft);
    this.floorDraft.set(null);
    if (!Number.isFinite(value) || value < 0 || value > 1) {
      this.error.set('The per-turn floor must be between 0 and 1');
      return;
    }
    if (value !== this.config().perTurnOtherFloor) void this.save({ perTurnOtherFloor: value });
  }

  /** v4 `commitPerSubject` (`:104-113`). */
  protected commitPerSubject(): void {
    const draft = this.perSubjectDraft();
    if (draft === null) return;
    const value = Number(draft);
    this.perSubjectDraft.set(null);
    if (!Number.isInteger(value) || value < 1) {
      this.error.set('Candidates per subject must be a whole number of at least 1');
      return;
    }
    if (value !== this.config().foldCandidatesPerSubject) {
      void this.save({ foldCandidatesPerSubject: value });
    }
  }

  /** v4 `save` (`:72-91`) — the ONE key, the echo over `DEFAULT_CONFIG` (R-F). */
  private async save(patch: Partial<MemoryExtractionModeConfig>): Promise<void> {
    this.saving.set(true);
    this.error.set(null);
    try {
      const settings = await setExtractionModeConfig(this.core, patch);
      this.config.set({ ...DEFAULT_CONFIG, ...settings });
      void this.queryClient.invalidateQueries({ queryKey: memoryKeys.extractionModeConfig() });
      this.toasts.showSuccess('Extraction settings saved');
    } catch (err) {
      const msg = writeErrorText(err, 'Failed to save extraction settings');
      this.error.set(msg);
      this.toasts.showError(msg);
    } finally {
      this.saving.set(false);
    }
  }
}
