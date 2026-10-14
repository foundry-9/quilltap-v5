import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ConnectionProfileDto, MemoryConsolidationConfig } from '../../../core/core-contract';
import type { MemorySettingsDocument } from './memory-settings-document';

type NumericKey =
  | 'clusterThreshold'
  | 'tfidfClusterThreshold'
  | 'minClusterSize'
  | 'maxClusterSize'
  | 'matureAfterDays'
  | 'maxClustersPerRun'
  | 'watermark';

interface NumberFieldSpec {
  key: NumericKey;
  label: string;
  help: string;
  min: number;
  max?: number;
  step: number;
  integer: boolean;
}

/**
 * v4 `NUMBER_FIELDS` (`memory-consolidation-particulars.tsx:26-34`), in order:
 * `tfidfClusterThreshold` IN (`4ae0c4ca6`), `coldRetentionDays` OUT (it moved
 * to the phase switches as "Archive retention").
 */
const NUMBER_FIELDS: ReadonlyArray<NumberFieldSpec> = [
  { key: 'clusterThreshold', label: 'Cluster threshold', help: 'How alike two notes must be (0 to 1, by cosine) to be folded together.', min: 0, max: 1, step: 0.01, integer: false },
  { key: 'tfidfClusterThreshold', label: 'Cluster threshold (Built-in)', help: 'The same measure when the default embedding profile is the Built-in TF-IDF one, which reckons likeness far more sparingly.', min: 0, max: 1, step: 0.01, integer: false },
  { key: 'minClusterSize', label: 'Smallest cluster', help: 'Fewest notes worth a digest. Falls to two when one of them is quite old.', min: 2, step: 1, integer: true },
  { key: 'maxClusterSize', label: 'Largest cluster', help: 'Most notes put before the model in one sitting.', min: 2, step: 1, integer: true },
  { key: 'matureAfterDays', label: 'Mature after (days)', help: 'Notes younger than this are left to settle.', min: 0, step: 1, integer: false },
  { key: 'maxClustersPerRun', label: 'Clusters per run', help: 'Bounds the cost of a run; a backlog drains over several.', min: 1, step: 1, integer: true },
  { key: 'watermark', label: 'Watermark', help: 'Unconsidered active notes at which a character is queued automatically.', min: 1, step: 1, integer: true },
];

/**
 * The consolidation phase's particulars on the Maintenance card — v4
 * `components/tools/memory-consolidation-particulars.tsx` (`b045dd986`, at
 * `83334b9a5`): which model does the folding and how alike notes must be to
 * share a digest. Its on/off switch and the archive retention sit with the
 * other phase switches. A collapsed disclosure; saves go through the card's
 * consolidation document (one key per save — L-2).
 *
 * The profile select is v4-controlled, so a failed save snaps it back to the
 * stored id (P4.D276 R-E, L-3). A number field clears its draft FIRST, then
 * validates (v4 `:42-57`): a refused value shows the stored one again.
 */
@Component({
  selector: 'qt-memory-consolidation-particulars',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="space-y-3">
      <button
        type="button"
        class="qt-text-label underline-offset-2 hover:underline"
        [attr.aria-expanded]="open()"
        (click)="open.set(!open())"
      >
        {{ open() ? '▾' : '▸' }} Consolidation particulars
      </button>

      @if (open()) {
        <div class="space-y-4">
          <div class="space-y-1">
            <label for="consolidation-profile" class="block qt-text-label">Connection profile</label>
            <select
              id="consolidation-profile"
              class="qt-select"
              [disabled]="doc().saving()"
              (change)="chooseProfile($any($event.target))"
            >
              <option value="" [selected]="doc().config().connectionProfileId === null">The cheap LLM (default)</option>
              @if (selectedProfileMissing()) {
                <option [value]="doc().config().connectionProfileId ?? ''" [selected]="true">(a profile that has since left the building)</option>
              }
              @for (p of profiles(); track p.id) {
                <option [value]="p.id" [selected]="doc().config().connectionProfileId === p.id">{{ profileLabel(p) }}</option>
              }
            </select>
            <p class="qt-text-xs qt-text-muted">
              A capable model does this better than the cheap one, and is recommended for the first,
              large backfill.
            </p>
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
                  [value]="fieldValue(spec)"
                  [disabled]="doc().saving()"
                  (input)="setDraft(spec.key, $any($event.target).value)"
                  (blur)="commitNumber(spec)"
                />
                <p class="qt-text-xs qt-text-muted">{{ spec.help }}</p>
              </div>
            }
          </div>
        </div>
      }
    </div>
  `,
})
export class MemoryConsolidationParticulars {
  private readonly core = inject(CoreClient);

  /** The Maintenance card's consolidation document (L-1). */
  readonly doc = input.required<MemorySettingsDocument<MemoryConsolidationConfig>>();

  protected readonly fields = NUMBER_FIELDS;
  protected readonly open = signal(false);
  private readonly drafts = signal<Partial<Record<NumericKey, string>>>({});

  /** v4 `useConnectionProfiles()` — the server's order, unsorted. */
  private readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connection-profiles'],
    queryFn: async (): Promise<ConnectionProfileDto[]> => {
      const resp = await this.core.dispatchExpect({ type: 'connectionProfileList' }, 'connectionProfiles');
      return resp.data.profiles;
    },
  }));

  protected readonly profiles = computed(() => this.profilesQuery.data() ?? []);

  /** v4 `:59-60` — shows while the profiles are still loading too (v4's own flash). */
  protected readonly selectedProfileMissing = computed(() => {
    const id = this.doc().config().connectionProfileId;
    return id !== null && !this.profiles().some((p) => p.id === id);
  });

  /** v4 `:89-92` — the JSX line break between the two expressions is dropped. */
  protected profileLabel(p: ConnectionProfileDto): string {
    return `${p.name || ''} (${p.provider || ''}${p.modelName ? ` • ${p.modelName}` : ''})`;
  }

  protected fieldValue(spec: NumberFieldSpec): string {
    return this.drafts()[spec.key] ?? String(this.doc().config()[spec.key]);
  }

  protected setDraft(key: NumericKey, value: string): void {
    this.drafts.update((d) => ({ ...d, [key]: value }));
  }

  /** v4 `commitNumber` (`:42-57`): the draft is cleared FIRST, then the value is judged. */
  protected commitNumber(spec: NumberFieldSpec): void {
    const raw = this.drafts()[spec.key];
    if (raw === undefined) return;
    this.drafts.update((d) => {
      const next = { ...d };
      delete next[spec.key];
      return next;
    });
    const value = Number(raw.trim());
    const outOfRange =
      raw.trim() === '' ||
      !Number.isFinite(value) ||
      value < spec.min ||
      (spec.max !== undefined && value > spec.max);
    if (outOfRange || (spec.integer && !Number.isInteger(value))) {
      this.doc().setError(
        `${spec.label} must be ${spec.integer ? 'a whole number' : 'a number'} ${spec.max !== undefined ? `between ${spec.min} and ${spec.max}` : `of at least ${spec.min}`}`,
      );
      return;
    }
    if (value !== this.doc().config()[spec.key]) void this.doc().save({ [spec.key]: value });
  }

  /** v4 `:84` — `value || null`; the select re-syncs to the stored id once the save settles (R-E). */
  protected async chooseProfile(el: HTMLSelectElement): Promise<void> {
    const doc = this.doc();
    await doc.save({ connectionProfileId: el.value || null });
    el.value = doc.config().connectionProfileId ?? '';
  }
}
