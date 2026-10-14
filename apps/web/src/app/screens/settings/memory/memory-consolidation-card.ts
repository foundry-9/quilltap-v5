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
import type {
  CharacterMemoryCount,
  ConnectionProfileDto,
  ConsolidationReport,
  MemoryConsolidationConfig,
} from '../../../core/core-contract';
import {
  consolidateMemories,
  fetchCharacterMemoryCounts,
  fetchConsolidationConfig,
  memoryKeys,
  setConsolidationConfig,
} from '../../../memory/memory.api';
import { ToastService } from '../../../ui/toast.service';
import { readErrorText, writeErrorText } from './api-error-text';
import { MemoryConsolidationReportDialog } from './memory-consolidation-report-dialog';

/** v4 `DEFAULT_CONFIG` (`memory-consolidation-card.tsx:25-35`). */
const DEFAULT_CONFIG: MemoryConsolidationConfig = {
  enabled: false,
  connectionProfileId: null,
  clusterThreshold: 0.72,
  tfidfClusterThreshold: 0.3,
  minClusterSize: 3,
  maxClusterSize: 30,
  matureAfterDays: 7,
  maxClustersPerRun: 40,
  watermark: 150,
  coldRetentionDays: null,
};

type NumericKey = Exclude<keyof MemoryConsolidationConfig, 'enabled' | 'connectionProfileId'>;

interface NumberFieldSpec {
  key: NumericKey;
  label: string;
  help: string;
  min: number;
  max?: number;
  step: number;
  integer: boolean;
  nullable?: boolean;
}

/** v4 `NUMBER_FIELDS` (`:60-68`), transcribed in order. */
const NUMBER_FIELDS: ReadonlyArray<NumberFieldSpec> = [
  { key: 'clusterThreshold', label: 'Cluster threshold', help: 'How alike two notes must be (0 to 1, by cosine) to be folded together.', min: 0, max: 1, step: 0.01, integer: false },
  { key: 'minClusterSize', label: 'Smallest cluster', help: 'Fewest notes worth a digest. Falls to two when one of them is quite old.', min: 2, step: 1, integer: true },
  { key: 'maxClusterSize', label: 'Largest cluster', help: 'Most notes put before the model in one sitting.', min: 2, step: 1, integer: true },
  { key: 'matureAfterDays', label: 'Mature after (days)', help: 'Notes younger than this are left to settle.', min: 0, step: 1, integer: false },
  { key: 'maxClustersPerRun', label: 'Clusters per run', help: 'Bounds the cost of a run; a backlog drains over several.', min: 1, step: 1, integer: true },
  { key: 'watermark', label: 'Watermark', help: 'Unconsidered active notes at which a character is queued automatically.', min: 1, step: 1, integer: true },
  { key: 'coldRetentionDays', label: 'Archive retention (days)', help: 'Delete superseded archived notes older than this. Leave blank to keep them forever.', min: 1, step: 1, integer: true, nullable: true },
];

/**
 * The Consolidation card (v4 `components/tools/memory-consolidation-card.tsx`
 * at `01a83539d`, new in `d58548051`): folds clusters of active memories into
 * digests and sends the originals to the archive. Instance-wide settings
 * (`memoryConsolidationConfigGet/Set`), plus "Consolidate now" for one
 * character (a dry run answered in-process with a report, a real run queued).
 *
 * Ported faithfully, and three places where Angular differs from React:
 * - **Single-field saves (P4.D276 R-F).** Every save posts exactly the one
 *   changed key, and the card then shows the server's echo merged over
 *   `DEFAULT_CONFIG` — never a client merge with the previous values. v4's
 *   server fills every `.default()` field of the partial, so one save resets
 *   the others (P4.D271's `PARTIAL_DEFAULTS_CLOBBER`); the card shows that, as
 *   v4's does. Never "fix" it here.
 * - **Controlled controls on a FAILED save (R-E).** v4's checkbox and profile
 *   select are React-controlled, so a failed save snaps them back to the
 *   stored value. An Angular `[checked]` / `[selected]` binding whose value did
 *   not change does not re-render, so each change handler takes its element
 *   and re-syncs it from the stored config once the save settles. The
 *   checkbox is NOT optimistic (v4 `:200`).
 * - **No realtime subscription (R-J).** v4 relies on the global topic map; v5's
 *   narrows the unscoped `memories` hint (R-A, recorded), so after a real run
 *   finishes the roster's counts stay stale until the tab remounts.
 */
@Component({
  selector: 'qt-memory-consolidation-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MemoryConsolidationReportDialog],
  template: `
    @if (configQuery.isLoading()) {
      <p class="qt-text-small qt-text-muted">Loading consolidation settings…</p>
    } @else {
      <div class="space-y-4">
        <p class="qt-text-small qt-text-muted">
          Consolidation folds clusters of near-duplicate memories into a single, denser digest, and
          sends the originals to the archive, where they keep for reference and are labelled as
          superseded. Recall then reads the digests rather than the pile. It runs once a day and
          whenever a character’s unconsidered notes pass the watermark — once switched on. A capable
          model does this better than the cheap one. Off by default.
        </p>

        <label class="flex items-center gap-3 qt-body">
          <input
            type="checkbox"
            class="qt-checkbox"
            [checked]="config().enabled"
            [disabled]="busy()"
            (change)="toggleEnabled($any($event.target))"
          />
          <span>Enable automatic consolidation</span>
        </label>

        <div class="space-y-1">
          <label for="consolidation-profile" class="block qt-text-label">Connection profile</label>
          <select
            id="consolidation-profile"
            class="qt-select"
            [disabled]="busy()"
            (change)="chooseProfile($any($event.target))"
          >
            <option value="" [selected]="config().connectionProfileId === null">The cheap LLM (default)</option>
            @if (selectedProfileMissing()) {
              <option [value]="config().connectionProfileId ?? ''" [selected]="true">(a profile that has since left the building)</option>
            }
            @for (p of profiles(); track p.id) {
              <option [value]="p.id" [selected]="config().connectionProfileId === p.id">{{ profileLabel(p) }}</option>
            }
          </select>
          <p class="qt-text-xs qt-text-muted">A capable model is recommended for the first, large backfill.</p>
        </div>

        <div class="grid gap-3 sm:grid-cols-2">
          @for (spec of fields; track spec.key) {
            <div class="space-y-1">
              <label [for]="'consolidation-' + spec.key" class="block qt-text-label">{{ spec.label }}</label>
              <input
                [id]="'consolidation-' + spec.key"
                type="number"
                class="qt-input"
                [attr.min]="spec.min"
                [attr.max]="spec.max ?? null"
                [attr.step]="spec.step"
                [attr.placeholder]="spec.nullable ? 'never' : null"
                [value]="fieldValue(spec)"
                [disabled]="busy()"
                (input)="setDraft(spec.key, $any($event.target).value)"
                (blur)="commitNumber(spec)"
              />
              <p class="qt-text-xs qt-text-muted">{{ spec.help }}</p>
            </div>
          }
        </div>

        <div class="space-y-2 pt-2 border-t qt-border-default">
          <p class="qt-text-label">Consolidate now</p>
          <p class="qt-text-small qt-text-muted">
            Choose a character. A dry run rehearses a handful of clusters and shows the proposed
            digests without writing a thing; the real run is queued in the background and ignores the
            master switch above.
          </p>
          <div class="flex flex-wrap items-center gap-3">
            <select
              aria-label="Character to consolidate"
              class="qt-select w-auto"
              [disabled]="busy()"
              (change)="characterId.set($any($event.target).value)"
            >
              <option value="" [selected]="characterId() === ''">Choose a character…</option>
              @for (c of characters(); track c.id) {
                <option [value]="c.id" [selected]="characterId() === c.id">{{ characterLabel(c) }}</option>
              }
            </select>
            <button
              type="button"
              class="qt-button qt-button-secondary"
              [disabled]="busy() || !characterId()"
              (click)="consolidate(true)"
            >
              {{ running() === 'dry' ? 'Rehearsing…' : 'Dry run' }}
            </button>
            <button
              type="button"
              class="qt-button qt-button-primary"
              [disabled]="busy() || !characterId()"
              (click)="consolidate(false)"
            >
              {{ running() === 'real' ? 'Queuing…' : 'Consolidate for real' }}
            </button>
          </div>
        </div>

        @if (error(); as msg) {
          <p class="qt-text-small qt-text-destructive">{{ msg }}</p>
        }

        @if (report(); as r) {
          <qt-memory-consolidation-report-dialog [report]="r" (close)="report.set(null)" />
        }
      </div>
    }
  `,
})
export class MemoryConsolidationCard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  protected readonly fields = NUMBER_FIELDS;

  protected readonly config = signal<MemoryConsolidationConfig>(DEFAULT_CONFIG);
  private readonly drafts = signal<Partial<Record<NumericKey, string>>>({});
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly characterId = signal('');
  protected readonly running = signal<'dry' | 'real' | null>(null);
  protected readonly report = signal<ConsolidationReport | null>(null);

  protected readonly configQuery = injectQuery(() => ({
    queryKey: memoryKeys.consolidationConfig(),
    queryFn: () => fetchConsolidationConfig(this.core),
  }));

  /** Shared with the housekeeping card (v4 shares `characterMemoryCounts`). */
  private readonly countsQuery = injectQuery(() => ({
    queryKey: memoryKeys.characterCounts(),
    queryFn: (): Promise<CharacterMemoryCount[]> => fetchCharacterMemoryCounts(this.core),
  }));

  /** v4 `useConnectionProfiles()` — the server's order, unsorted. */
  private readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connection-profiles'],
    queryFn: async (): Promise<ConnectionProfileDto[]> => {
      const resp = await this.core.dispatchExpect(
        { type: 'connectionProfileList' },
        'connectionProfiles',
      );
      return resp.data.profiles;
    },
  }));

  protected readonly characters = computed(() => this.countsQuery.data() ?? []);
  protected readonly profiles = computed(() => this.profilesQuery.data() ?? []);
  protected readonly busy = computed(() => this.saving() || this.running() !== null);

  /** v4 `:186-187` — shows while the profiles are still loading too (v4's own flash). */
  protected readonly selectedProfileMissing = computed(() => {
    const id = this.config().connectionProfileId;
    return id !== null && !this.profiles().some((p) => p.id === id);
  });

  constructor() {
    // v4 `:100-103` — seed the form from the query, only when `settings` came back.
    effect(() => {
      const loaded = this.configQuery.data();
      if (loaded) {
        this.config.set({ ...DEFAULT_CONFIG, ...loaded });
      }
    });
    // v4 `:105-108` — surface a read failure in the shared error line.
    effect(() => {
      const loadError = this.configQuery.error();
      if (loadError) {
        this.error.set(readErrorText(loadError, 'Failed to load consolidation settings'));
      }
    });
  }

  /** v4 `:221-222` — the JSX line break between the two expressions is dropped. */
  protected profileLabel(p: ConnectionProfileDto): string {
    return `${p.name || ''} (${p.provider || ''}${p.modelName ? ` • ${p.modelName}` : ''})`;
  }

  /** v4 `:273`. */
  protected characterLabel(c: CharacterMemoryCount): string {
    return `${c.name} (${c.memoryCount.toLocaleString()} memories)`;
  }

  /** v4 `:232` — the draft, else the stored value (`''` for a null). */
  protected fieldValue(spec: NumberFieldSpec): string {
    const draft = this.drafts()[spec.key];
    if (draft !== undefined) return draft;
    const stored = this.config()[spec.key];
    return stored === null ? '' : String(stored);
  }

  protected setDraft(key: NumericKey, value: string): void {
    this.drafts.update((d) => ({ ...d, [key]: value }));
  }

  private clearDraft(key: NumericKey): void {
    this.drafts.update((d) => {
      const next = { ...d };
      delete next[key];
      return next;
    });
  }

  /** v4 `commitNumber` (`:131-155`), on blur. */
  protected commitNumber(spec: NumberFieldSpec): void {
    const raw = this.drafts()[spec.key];
    if (raw === undefined) return;
    const trimmed = raw.trim();
    if (trimmed === '') {
      this.clearDraft(spec.key);
      if (spec.nullable && this.config()[spec.key] !== null) {
        void this.saveConfig({ [spec.key]: null });
      }
      return;
    }
    const value = Number(trimmed);
    const outOfRange =
      !Number.isFinite(value) || value < spec.min || (spec.max !== undefined && value > spec.max);
    if (outOfRange || (spec.integer && !Number.isInteger(value))) {
      // v4: no toast, the draft KEPT, no save.
      this.error.set(
        `${spec.label} must be ${spec.integer ? 'a whole number' : 'a number'} ${spec.max !== undefined ? `between ${spec.min} and ${spec.max}` : `of at least ${spec.min}`}`,
      );
      return;
    }
    this.clearDraft(spec.key);
    if (value === this.config()[spec.key]) return;
    void this.saveConfig({ [spec.key]: value });
  }

  /** v4 `:200` — NOT optimistic: `config` moves only on success (R-E). */
  protected async toggleEnabled(el: HTMLInputElement): Promise<void> {
    await this.saveConfig({ enabled: !this.config().enabled });
    el.checked = this.config().enabled;
  }

  /** v4 `:215` — `value || null` (R-E: the select snaps back to the stored id). */
  protected async chooseProfile(el: HTMLSelectElement): Promise<void> {
    await this.saveConfig({ connectionProfileId: el.value || null });
    el.value = this.config().connectionProfileId ?? '';
  }

  /** v4 `saveConfig` (`:110-129`) — the ONE key, the echo over `DEFAULT_CONFIG` (R-F). */
  private async saveConfig(patch: Partial<MemoryConsolidationConfig>): Promise<void> {
    this.saving.set(true);
    this.error.set(null);
    try {
      const settings = await setConsolidationConfig(this.core, patch);
      this.config.set({ ...DEFAULT_CONFIG, ...settings });
      void this.queryClient.invalidateQueries({ queryKey: memoryKeys.consolidationConfig() });
      this.toasts.showSuccess('Consolidation settings saved');
    } catch (err) {
      const msg = writeErrorText(err, 'Failed to save consolidation settings');
      this.error.set(msg);
      this.toasts.showError(msg);
    } finally {
      this.saving.set(false);
    }
  }

  /**
   * v4 `consolidate` (`:157-179`). A dry run WITH a report opens the dialog;
   * anything else — a real run, OR a dry reply without `report` (v4's quirk,
   * ported) — toasts the queued sentence.
   */
  protected async consolidate(dryRun: boolean): Promise<void> {
    const characterId = this.characterId();
    if (!characterId) return;
    this.running.set(dryRun ? 'dry' : 'real');
    this.error.set(null);
    try {
      const data = await consolidateMemories(this.core, { characterId, dryRun });
      if (dryRun && data.report) {
        this.report.set(data.report);
      } else {
        this.toasts.showSuccess(
          'The consolidation has been sent below stairs — it will run in the background',
        );
      }
    } catch (err) {
      const msg = writeErrorText(err, dryRun ? 'The rehearsal failed' : 'Failed to queue consolidation');
      this.error.set(msg);
      this.toasts.showError(msg);
    } finally {
      this.running.set(null);
    }
  }
}
