import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import {
  MEMORY_CAP_BOUNDS,
  type CharacterMemoryCount,
  type MemoryConsolidationConfig,
  type MemoryMaintenanceConfig,
  type MemoryMaintenancePhaseToggles,
} from '../../../core/core-contract';
import { MaintenanceDialog } from '../../../memory/maintenance-dialog';
import {
  fetchCharacterMemoryCounts,
  fetchConsolidationConfig,
  fetchMaintenanceConfig,
  memoryKeys,
  setConsolidationConfig,
  setMaintenanceConfig,
} from '../../../memory/memory.api';
import { MemoryConsolidationParticulars } from './memory-consolidation-particulars';
import { MemoryMaintenanceLedger } from './memory-maintenance-ledger';
import { MEMORY_SETTING_DEFAULTS } from './memory-setting-defaults';
import { injectMemorySettingsDocument } from './memory-settings-document';

const CAP_MIN = MEMORY_CAP_BOUNDS.min;
const CAP_MAX = MEMORY_CAP_BOUNDS.max;

type PhaseKey = keyof MemoryMaintenancePhaseToggles;

/** v4 `CharacterSummary` (`memory-maintenance-card.tsx:29-36`). */
interface CharacterSummary {
  id: string;
  name: string;
  /** Hot AUTO rows — the only number the cap is measured against (bug 210). */
  demotable: number;
  /** Hot MANUAL rows and digests — never demoted, never counted against the cap. */
  protected: number;
}

/** v4's three phase switches under "Enable automatic maintenance", in v4's order around Consolidate. */
const PHASE_SWITCHES: Record<PhaseKey, { label: string; hint: string }> = {
  foldNearDuplicates: {
    label: 'Fold near-duplicates',
    hint: 'Notes that say the same thing about the same occasion are folded into the sturdiest of them, stray details kept as footnotes. No model is consulted.',
  },
  demote: {
    label: 'Demote over the cap',
    hint: 'Past the cap, the least protected notes — old, unreinforced, unlinked, unconsulted — go to the archive first. Nothing is destroyed.',
  },
  retire: {
    label: 'Retire old archive notes',
    hint: 'Superseded archive notes older than the retention window below are let go for good.',
  },
};

/**
 * The Maintenance card — v4 `components/tools/memory-maintenance-card.tsx`
 * (`b045dd986`, relabelled `48790fb73`, at `83334b9a5`; memory refactor §3.11,
 * decision 11): one card for the one maintenance job, replacing the
 * Housekeeping, Consolidation and Near-Duplicate Folding cards. Per-phase
 * switches for automatic runs, the cap and its per-character overrides, the
 * consolidation particulars, "Run now" (with a dry run) and the ledger.
 *
 * Two settings documents (L-1): the maintenance document and the
 * consolidation document — Consolidate's switch and the archive retention save
 * through the latter. `busy` is either document saving; the three phase
 * switches are ALSO disabled while automatic maintenance is off, Consolidate
 * is not (it has its own switch, as it costs model calls).
 *
 * Every save posts exactly v4's patch — one key, the WHOLE three-key `phases`
 * object, the WHOLE overrides map — and shows the server's echo (L-2). A
 * failed save snaps a switch back (L-3 / P4.D276 R-E): each change handler
 * takes its element and re-syncs it from the stored config once the save
 * settles. Number inputs follow v4 exactly: the cap and the retention clear
 * their draft BEFORE judging it (a refused value shows the stored one again);
 * an override keeps its draft on a refusal and clears it only on a save.
 */
@Component({
  selector: 'qt-memory-maintenance-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MaintenanceDialog, MemoryConsolidationParticulars, MemoryMaintenanceLedger],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading maintenance settings…</p>
    } @else {
      <div class="space-y-6">
        <p class="qt-text-small qt-text-muted">
          One round of the household staff tends each character’s memories, in a fixed order:
          repetitions are folded together, related notes are consolidated into digests, anything over
          the cap goes to the archive, and superseded archive notes past their retention are let go.
          Archived notes are not lost: they stay on the shelf, still guard against repeats, and return
          to active duty if the world asks for them again.
        </p>

        <section class="space-y-3">
          <h3 class="qt-text-label">Automatic rounds</h3>
          <div class="space-y-3">
            <label class="flex items-start gap-3 qt-body">
              <input
                type="checkbox"
                class="qt-checkbox mt-1"
                [checked]="m().enabled"
                [disabled]="busy()"
                (change)="toggleEnabled($any($event.target))"
              />
              <span>
                <span>Enable automatic maintenance</span>
                <span class="block qt-text-xs qt-text-muted">A daily round, and a visit whenever a character nears the cap. Off by default — read the limits below first.</span>
              </span>
            </label>
            <div class="pl-7 space-y-3">
              <label class="flex items-start gap-3 qt-body">
                <input
                  type="checkbox"
                  class="qt-checkbox mt-1"
                  [checked]="m().phases.foldNearDuplicates"
                  [disabled]="busy() || !m().enabled"
                  (change)="togglePhase('foldNearDuplicates', $any($event.target))"
                />
                <span>
                  <span>{{ switches.foldNearDuplicates.label }}</span>
                  <span class="block qt-text-xs qt-text-muted">{{ switches.foldNearDuplicates.hint }}</span>
                </span>
              </label>
              <label class="flex items-start gap-3 qt-body">
                <input
                  type="checkbox"
                  class="qt-checkbox mt-1"
                  [checked]="c().enabled"
                  [disabled]="busy()"
                  (change)="toggleConsolidate($any($event.target))"
                />
                <span>
                  <span>Consolidate</span>
                  <span class="block qt-text-xs qt-text-muted">Once a day, and whenever a character’s unconsidered notes pass the watermark, a model folds clusters of related notes into digests and the originals go to the archive. Has its own switch, as it costs model calls.</span>
                </span>
              </label>
              @for (key of lowerPhases; track key) {
                <label class="flex items-start gap-3 qt-body">
                  <input
                    type="checkbox"
                    class="qt-checkbox mt-1"
                    [checked]="m().phases[key]"
                    [disabled]="busy() || !m().enabled"
                    (change)="togglePhase(key, $any($event.target))"
                  />
                  <span>
                    <span>{{ switches[key].label }}</span>
                    <span class="block qt-text-xs qt-text-muted">{{ switches[key].hint }}</span>
                  </span>
                </label>
              }
              <div class="flex flex-wrap items-center gap-3 pl-7">
                <label class="qt-text-small qt-text-muted" for="maintenance-retention">Archive retention</label>
                <input
                  id="maintenance-retention"
                  type="number"
                  min="1"
                  step="1"
                  placeholder="never"
                  class="qt-input w-28"
                  [value]="retentionValue()"
                  [disabled]="busy()"
                  (input)="retentionDraft.set($any($event.target).value)"
                  (blur)="commitRetention()"
                />
                <span class="qt-text-small qt-text-muted">days (blank keeps them forever)</span>
              </div>
              <p class="qt-text-xs qt-text-muted">
                The Commonplace files in each vault are refreshed after every consolidation.
                Hand-written notes and digests are never folded, demoted or let go by any of this.
              </p>
            </div>
          </div>
        </section>

        <section class="space-y-3 pt-4 border-t qt-border-default">
          <h3 class="qt-text-label">The cap</h3>
          <p class="qt-text-small qt-text-muted">
            Only extracted notes in active service count. Maintenance calls when a character reaches
            nine-tenths of it.
          </p>
          <div class="space-y-3">
            <div class="flex flex-wrap items-center gap-3">
              <label class="qt-text-small qt-text-muted" for="perCharacterCap">Per-character cap</label>
              <input
                id="perCharacterCap"
                type="number"
                [attr.min]="capMin"
                [attr.max]="capMax"
                step="100"
                class="qt-input w-32"
                [value]="capValue()"
                [disabled]="maintenance.saving()"
                (input)="capDraft.set($any($event.target).value)"
                (blur)="commitCap()"
              />
              <span class="qt-text-small qt-text-muted">extracted notes per character</span>
            </div>

            @if (characters().length > 0) {
              <div>
                <button
                  type="button"
                  class="qt-text-small qt-text-muted underline-offset-2 hover:underline"
                  (click)="showOverrides.set(!showOverrides())"
                >
                  {{ overridesToggle() }}
                </button>

                @if (showOverrides()) {
                  <div class="mt-3 space-y-2">
                    <p class="qt-text-small qt-text-muted">{{ overridesHelp() }}</p>
                    <div class="space-y-1">
                      @for (row of overrideRows(); track row.id) {
                        <div class="flex items-center gap-3">
                          <div class="flex-1 qt-body">
                            <span>{{ row.name }}</span>{{ ' ' }}<span [attr.class]="row.countClass">{{ row.counts }}</span>
                          </div>
                          <input
                            type="number"
                            [attr.min]="capMin"
                            [attr.max]="capMax"
                            step="100"
                            class="qt-input w-28"
                            [placeholder]="'' + m().perCharacterCap"
                            [value]="row.draft"
                            [disabled]="maintenance.saving()"
                            [attr.aria-label]="'Cap override for ' + row.name"
                            (input)="setOverrideDraft(row.id, $any($event.target).value)"
                            (blur)="commitOverride(row.id, $any($event.target).value)"
                          />
                        </div>
                      }
                    </div>
                  </div>
                }
              </div>
            }
          </div>
        </section>

        <section class="space-y-3 pt-4 border-t qt-border-default">
          <qt-memory-consolidation-particulars [doc]="consolidation" />
        </section>

        <section class="space-y-3 pt-4 border-t qt-border-default">
          <h3 class="qt-text-label">Run now</h3>
          <div class="flex flex-wrap items-center gap-3">
            <button type="button" class="qt-button qt-button-secondary" (click)="showDialog.set(true)">
              Run maintenance…
            </button>
            <span class="qt-text-small qt-text-muted">One character or every one, any phases, with a dry run first.</span>
          </div>
        </section>

        <section class="space-y-3 pt-4 border-t qt-border-default">
          <h3 class="qt-text-label">The ledger</h3>
          <qt-memory-maintenance-ledger [characters]="characters()" />
        </section>

        @if (error(); as msg) {
          <p class="qt-text-small qt-text-destructive">{{ msg }}</p>
        }

        @if (showDialog()) {
          <qt-maintenance-dialog (close)="showDialog.set(false)" />
        }
      </div>
    }
  `,
})
export class MemoryMaintenanceCard {
  private readonly core = inject(CoreClient);

  protected readonly maintenance = injectMemorySettingsDocument<MemoryMaintenanceConfig>({
    queryKey: memoryKeys.maintenanceConfig(),
    read: fetchMaintenanceConfig,
    write: setMaintenanceConfig,
    defaults: MEMORY_SETTING_DEFAULTS.maintenance,
    noun: 'maintenance settings',
  });
  protected readonly consolidation = injectMemorySettingsDocument<MemoryConsolidationConfig>({
    queryKey: memoryKeys.consolidationConfig(),
    read: fetchConsolidationConfig,
    write: setConsolidationConfig,
    defaults: MEMORY_SETTING_DEFAULTS.consolidation,
    noun: 'consolidation settings',
  });

  /**
   * v4 `useCharacterMemoryCounts` (`:46-61`) — decoration for the overrides list
   * and the ledger's names: a failed read leaves them EMPTY rather than failing
   * the card (no error line). Rows in the reply's order (L-12).
   */
  private readonly countsQuery = injectQuery(() => ({
    queryKey: memoryKeys.characterCounts(),
    queryFn: (): Promise<CharacterMemoryCount[]> => fetchCharacterMemoryCounts(this.core),
  }));

  protected readonly characters = computed<CharacterSummary[]>(() =>
    (this.countsQuery.data() ?? []).map((c) => ({
      id: c.id,
      name: c.name,
      demotable: c.demotable ?? c.memoryCount ?? 0,
      protected: c.protected ?? 0,
    })),
  );

  protected readonly switches = PHASE_SWITCHES;
  protected readonly lowerPhases: PhaseKey[] = ['demote', 'retire'];
  protected readonly capMin = CAP_MIN;
  protected readonly capMax = CAP_MAX;

  protected readonly m = this.maintenance.config;
  protected readonly c = this.consolidation.config;
  protected readonly busy = computed(() => this.maintenance.saving() || this.consolidation.saving());
  protected readonly loading = computed(
    () => this.maintenance.loading() || this.consolidation.loading() || this.countsQuery.isLoading(),
  );
  /** v4 `:332` — ONE error line, whichever document has one. */
  protected readonly error = computed(() => this.maintenance.error() ?? this.consolidation.error());

  protected readonly showDialog = signal(false);
  protected readonly showOverrides = signal(false);

  // The typed values live in drafts until blur, so the documents still hold the
  // saved values to compare against (v4's PR-98 review fix, `:192-194`).
  protected readonly retentionDraft = signal<string | null>(null);
  protected readonly capDraft = signal<string | null>(null);
  private readonly overrideDrafts = signal<Record<string, string>>({});

  protected readonly retentionValue = computed(() => {
    const stored = this.c().coldRetentionDays;
    return this.retentionDraft() ?? (stored === null ? '' : String(stored));
  });
  protected readonly capValue = computed(() => this.capDraft() ?? String(this.m().perCharacterCap));

  protected readonly overridesToggle = computed(
    () =>
      `${this.showOverrides() ? '▾' : '▸'} Per-character overrides (${Object.keys(this.m().perCharacterCapOverrides).length} active)`,
  );
  protected readonly overridesHelp = computed(
    () =>
      `Set a different cap for individual characters. Leave blank to fall back to the global cap (${this.m().perCharacterCap.toLocaleString()}).`,
  );

  /** v4 `:268-301` — each row's counts, its colour against its effective cap, its draft. */
  protected readonly overrideRows = computed(() => {
    const config = this.m();
    const drafts = this.overrideDrafts();
    return this.characters().map((character) => {
      const override = config.perCharacterCapOverrides[character.id];
      const effectiveCap = override ?? config.perCharacterCap;
      const overCap = character.demotable > effectiveCap;
      return {
        id: character.id,
        name: character.name,
        counts:
          `(${character.demotable.toLocaleString()} counted` +
          (character.protected > 0 ? ` · ${character.protected.toLocaleString()} exempt` : '') +
          ')',
        countClass: overCap ? 'qt-text-destructive qt-text-small' : 'qt-text-muted qt-text-small',
        draft: character.id in drafts ? drafts[character.id] : override === undefined ? '' : String(override),
      };
    });
  });

  /** v4 `:139` — not optimistic; the box re-syncs to the stored value once the save settles (L-3). */
  protected async toggleEnabled(el: HTMLInputElement): Promise<void> {
    const enabled = this.m().enabled;
    await this.maintenance.save({ enabled: !enabled });
    el.checked = this.m().enabled;
  }

  /** v4 `togglePhase` (`:99-100`) — the WHOLE three-key `phases` object. */
  protected async togglePhase(key: PhaseKey, el: HTMLInputElement): Promise<void> {
    const phases = this.m().phases;
    await this.maintenance.save({ phases: { ...phases, [key]: !phases[key] } });
    el.checked = this.m().phases[key];
  }

  /** v4 `:153` — the CONSOLIDATION document's own switch. */
  protected async toggleConsolidate(el: HTMLInputElement): Promise<void> {
    const enabled = this.c().enabled;
    await this.consolidation.save({ enabled: !enabled });
    el.checked = this.c().enabled;
  }

  /** v4 `commitRetention` (`:102-116`), on blur — the draft cleared first. */
  protected commitRetention(): void {
    const draft = this.retentionDraft();
    if (draft === null) return;
    const stored = this.c().coldRetentionDays;
    const trimmed = draft.trim();
    this.retentionDraft.set(null);
    if (trimmed === '') {
      if (stored !== null) void this.consolidation.save({ coldRetentionDays: null });
      return;
    }
    const value = Number(trimmed);
    if (!Number.isInteger(value) || value < 1) {
      this.consolidation.setError('Archive retention must be a whole number of days, or blank for never');
      return;
    }
    if (value !== stored) void this.consolidation.save({ coldRetentionDays: value });
  }

  /** v4 `handleCapBlur` (`:198-208`) — the draft cleared first. */
  protected commitCap(): void {
    const draft = this.capDraft();
    if (draft === null) return;
    const stored = this.m().perCharacterCap;
    const value = Math.floor(Number(draft.trim()));
    this.capDraft.set(null);
    if (draft.trim() === '' || !Number.isFinite(value) || value < CAP_MIN || value > CAP_MAX) {
      this.maintenance.setError(
        `The per-character cap must be between ${CAP_MIN.toLocaleString()} and ${CAP_MAX.toLocaleString()}`,
      );
      return;
    }
    if (value === stored) return;
    void this.maintenance.save({ perCharacterCap: value });
  }

  protected setOverrideDraft(characterId: string, value: string): void {
    this.overrideDrafts.update((d) => ({ ...d, [characterId]: value }));
  }

  /** v4 `handleOverrideBlur` (`:210-234`) — the WHOLE map; the draft kept on a refusal. */
  protected commitOverride(characterId: string, rawValue: string): void {
    const trimmed = rawValue.trim();
    const nextOverrides: Record<string, number> = { ...this.m().perCharacterCapOverrides };

    if (trimmed === '') {
      // Empty input removes any override for this character.
      if (!(characterId in nextOverrides)) return;
      delete nextOverrides[characterId];
    } else {
      const parsed = Math.floor(Number(trimmed));
      if (!Number.isFinite(parsed) || parsed < CAP_MIN || parsed > CAP_MAX) {
        this.maintenance.setError(
          `Override for character ${characterId.slice(0, 8)} must be between ${CAP_MIN.toLocaleString()} and ${CAP_MAX.toLocaleString()}`,
        );
        return;
      }
      if (nextOverrides[characterId] === parsed) return;
      nextOverrides[characterId] = parsed;
    }

    this.overrideDrafts.update((d) => {
      const next = { ...d };
      delete next[characterId];
      return next;
    });
    void this.maintenance.save({ perCharacterCapOverrides: nextOverrides });
  }
}
