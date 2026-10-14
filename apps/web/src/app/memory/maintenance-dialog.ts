import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  afterNextRender,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import {
  MAINTENANCE_PHASES,
  MEMORY_CAP_BOUNDS,
  type CharacterMemoryCount,
  type MaintenancePhase,
  type MaintenanceReport,
  type MemoryMaintainOptions,
} from '../core/core-contract';
import { writeErrorText } from '../screens/settings/memory/api-error-text';
import { Icon } from '../ui/icon';
import { ToastService } from '../ui/toast.service';
import { MaintenanceReportView, MaintenanceReportsTable, PHASE_LABELS } from './maintenance-report';
import { fetchCharacterMemoryCounts, memoryKeys, runMaintenance } from './memory.api';

const ALL = '__all__';

type PickablePhase = Exclude<MaintenancePhase, 'mirror'>;

/** v4 `PICKABLE_PHASES` (`:27`) — the phases a person picks; the mirror follows consolidation on its own. */
const PICKABLE_PHASES = MAINTENANCE_PHASES.filter((p): p is PickablePhase => p !== 'mirror');

/** v4 `PHASE_HINTS` (`:29-34`). */
const PHASE_HINTS: Record<PickablePhase, string> = {
  'fold-near-duplicates':
    'Fold notes that say the same thing about the same occasion into the sturdiest of them. No model is consulted.',
  consolidate:
    'Ask a model to fold clusters of related notes into digests; the originals go to the archive. Costs model calls.',
  demote: 'Send the least protected notes to the archive while a character holds more than the cap.',
  retire: 'Let go of superseded archive notes older than the retention window (when one is set).',
};

type Result = { kind: 'one'; report: MaintenanceReport } | { kind: 'many'; reports: MaintenanceReport[] };

/**
 * "Run maintenance now" — v4 `components/memory/maintenance-dialog.tsx`
 * (`b045dd986`, at `83334b9a5`): choose a character (or, from the settings
 * card, every one), choose the phases, rehearse with a dry run or queue the
 * real thing. One dialog for the Maintenance card (the picker variant) and the
 * memory list (fixed to one character: `characterId` hides the picker and the
 * counts read never runs).
 *
 * **Transcribed as v4 writes it (P4.D300 L-4, the P4.D276 R-H idiom):** hand
 * markup — no `role="dialog"`, no Escape, no backdrop close; the header's close
 * icon and the footer's Close are the only exits. The host reparents to
 * `document.body` after the first render (v4's `createPortal`; the
 * `images/image-detail-modal.ts` idiom). Every control is disabled while a
 * run is in flight.
 *
 * The request body is v4's EXACT object, key order included (`:89-94`): the
 * character first (absent for everyone), the phases in phase order, `dryRun`,
 * then the cap for this run — only when demotion is asked for.
 */
@Component({
  selector: 'qt-maintenance-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, MaintenanceReportView, MaintenanceReportsTable],
  template: `
    <div class="qt-dialog-overlay p-4 z-[100]">
      <div class="qt-dialog max-w-3xl max-h-[90vh] overflow-hidden flex flex-col">
        <div class="qt-dialog-header">
          <div class="flex items-center justify-between">
            <h2 class="qt-dialog-title">{{ title() }}</h2>
            <button
              type="button"
              class="qt-text-secondary hover:text-foreground"
              aria-label="Close"
              (click)="close.emit()"
            >
              <qt-icon name="close" class="w-6 h-6" />
            </button>
          </div>
          <p class="qt-dialog-description">
            Rehearse first, if you please: a dry run shows what each phase would do and writes not a
            word. The real run is queued in the background and goes ahead whatever the switches on the
            settings card say.
          </p>
        </div>

        <div class="qt-dialog-body border-b qt-border-default space-y-4">
          @if (!characterId()) {
            <div class="space-y-1">
              <label for="maintenance-target" class="block qt-text-label">Character</label>
              <select
                id="maintenance-target"
                class="qt-select w-auto"
                [disabled]="running() !== null"
                (change)="chooseTarget($any($event.target).value)"
              >
                <option value="" [selected]="target() === ''">Choose a character…</option>
                <option [value]="all" [selected]="target() === all">Every character</option>
                @for (c of characters(); track c.id) {
                  <option [value]="c.id" [selected]="target() === c.id">{{ c.label }}</option>
                }
              </select>
            </div>
          }

          <fieldset class="space-y-2">
            <legend class="qt-text-label mb-1">Phases</legend>
            @for (p of pickable; track p.phase) {
              <label class="flex items-start gap-3 qt-body">
                <input
                  type="checkbox"
                  class="qt-checkbox mt-1"
                  [checked]="phases().has(p.phase)"
                  [disabled]="running() !== null"
                  (change)="togglePhase(p.phase)"
                />
                <span>
                  <span>{{ p.label }}</span>
                  <span class="block qt-text-xs qt-text-muted">{{ p.hint }}</span>
                </span>
              </label>
            }
          </fieldset>

          @if (phases().has('demote')) {
            <div class="space-y-1">
              <label for="maintenance-cap" class="block qt-text-label">Cap for this run</label>
              <input
                id="maintenance-cap"
                type="number"
                [attr.min]="bounds.min"
                [attr.max]="bounds.max"
                step="100"
                placeholder="the configured cap"
                class="qt-input w-40"
                [value]="capDraft()"
                [disabled]="running() !== null"
                (input)="setCapDraft($any($event.target).value)"
              />
              <p class="qt-text-xs qt-text-muted">{{ capHint() }}</p>
            </div>
          }

          @if (dryRunBlocked()) {
            <p class="qt-text-xs qt-text-warning">
              Consolidation calls a model, so it can be rehearsed for one character at a time.
            </p>
          }
        </div>

        <div class="flex-1 overflow-y-auto p-6">
          @if (error(); as msg) {
            <p class="qt-text-small qt-text-destructive mb-4">{{ msg }}</p>
          }
          @if (running() === 'dry') {
            <p class="qt-text-small qt-text-muted">Rehearsing…</p>
          }
          @if (running() === null) {
            @if (result(); as r) {
              @if (r.kind === 'one') {
                <qt-maintenance-report-view [report]="r.report" />
              } @else {
                <qt-maintenance-reports-table [reports]="r.reports" />
              }
            } @else if (!error()) {
              <p class="qt-text-small qt-text-muted">
                Choose the phases, then rehearse or send for the staff.
              </p>
            }
          }
        </div>

        <div class="qt-dialog-footer flex justify-end gap-2">
          <button type="button" class="qt-button qt-button-secondary" (click)="close.emit()">
            Close
          </button>
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="!ready() || dryRunBlocked()"
            (click)="run(true)"
          >
            {{ running() === 'dry' ? 'Rehearsing…' : 'Dry run' }}
          </button>
          <button
            type="button"
            class="qt-button qt-button-primary"
            [disabled]="!ready()"
            (click)="run(false)"
          >
            {{ running() === 'real' ? 'Queuing…' : 'Run for real' }}
          </button>
        </div>
      </div>
    </div>
  `,
})
export class MaintenanceDialog {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  /** Fixed to one character (the memory list); omitted, the dialog offers a picker with "every character". */
  readonly characterId = input<string | undefined>(undefined);
  readonly characterName = input<string | undefined>(undefined);

  readonly close = output<void>();
  /** After a real run is queued. */
  readonly queued = output<void>();

  protected readonly all = ALL;
  protected readonly bounds = MEMORY_CAP_BOUNDS;
  protected readonly pickable = PICKABLE_PHASES.map((phase) => ({
    phase,
    label: PHASE_LABELS[phase],
    hint: PHASE_HINTS[phase],
  }));

  /** v4 `useState(characterId ?? '')` — seeded once, on first read. */
  private readonly targetDraft = signal<string | null>(null);
  protected readonly target = computed(() => this.targetDraft() ?? this.characterId() ?? '');
  protected readonly phases = signal<ReadonlySet<MaintenancePhase>>(new Set(PICKABLE_PHASES));
  protected readonly capDraft = signal('');
  protected readonly running = signal<'dry' | 'real' | null>(null);
  protected readonly result = signal<Result | null>(null);
  protected readonly error = signal<string | null>(null);

  /** v4 `:66-69` — the counts read runs only for the picker variant. */
  private readonly countsQuery = injectQuery(() => ({
    queryKey: memoryKeys.characterCounts(),
    queryFn: (): Promise<CharacterMemoryCount[]> => fetchCharacterMemoryCounts(this.core),
    enabled: !this.characterId(),
  }));

  /** v4 `:261-263` — the reply's order (P4.D298 sorts it, L-12). */
  protected readonly characters = computed(() =>
    (this.countsQuery.data() ?? []).map((c) => ({
      id: c.id,
      label: `${c.name} (${(c.demotable ?? c.memoryCount ?? 0).toLocaleString()} counted)`,
    })),
  );

  protected readonly title = computed(() => {
    const name = this.characterName();
    return `Memory Maintenance${name ? ` — ${name}` : ''}`;
  });

  private readonly everyone = computed(() => this.target() === ALL);
  /** v4 `chosen` — the ticked phases, in phase order. */
  private readonly chosen = computed(() => PICKABLE_PHASES.filter((p) => this.phases().has(p)));
  /** A dry run that would call the model names one character (the API refuses otherwise). */
  protected readonly dryRunBlocked = computed(() => this.everyone() && this.phases().has('consolidate'));
  private readonly cap = computed(() =>
    this.capDraft().trim() === '' ? undefined : Math.floor(Number(this.capDraft())),
  );
  private readonly capInvalid = computed(() => {
    const cap = this.cap();
    return cap !== undefined && (!Number.isFinite(cap) || cap < MEMORY_CAP_BOUNDS.min || cap > MEMORY_CAP_BOUNDS.max);
  });
  protected readonly ready = computed(
    () => this.target() !== '' && this.chosen().length > 0 && !this.capInvalid() && this.running() === null,
  );
  protected readonly capHint = computed(() =>
    this.capInvalid()
      ? `Between ${MEMORY_CAP_BOUNDS.min.toLocaleString()} and ${MEMORY_CAP_BOUNDS.max.toLocaleString()}, if you please.`
      : 'Leave blank to use each character’s own cap (its override, else the global one).',
  );

  constructor() {
    const host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
    inject(DestroyRef).onDestroy(() => host.remove());
    afterNextRender(() => {
      if (typeof document !== 'undefined') {
        document.body.appendChild(host);
      }
    });
  }

  protected chooseTarget(value: string): void {
    this.targetDraft.set(value);
    this.result.set(null);
  }

  protected togglePhase(phase: MaintenancePhase): void {
    this.result.set(null);
    this.phases.update((current) => {
      const next = new Set(current);
      if (next.has(phase)) next.delete(phase);
      else next.add(phase);
      return next;
    });
  }

  protected setCapDraft(value: string): void {
    this.capDraft.set(value);
    this.result.set(null);
  }

  /** v4 `body` (`:89-94`) — the exact object, key order included. */
  private body(dryRun: boolean): MemoryMaintainOptions {
    const cap = this.cap();
    return {
      ...(this.everyone() ? {} : { characterId: this.target() }),
      phases: this.chosen(),
      dryRun,
      ...(cap !== undefined && this.phases().has('demote') ? { settings: { cap } } : {}),
    };
  }

  /** v4 `run` (`:96-126`). Every value the handler reads is captured before a `.set()`. */
  protected async run(dryRun: boolean): Promise<void> {
    if (!this.ready() || (dryRun && this.dryRunBlocked())) return;
    const everyone = this.everyone();
    const body = this.body(dryRun);
    this.running.set(dryRun ? 'dry' : 'real');
    this.error.set(null);
    try {
      const data = (await runMaintenance(this.core, body)) as {
        report?: MaintenanceReport;
        reports?: MaintenanceReport[];
        jobIds?: string[];
      };
      if (dryRun) {
        if (data.report) this.result.set({ kind: 'one', report: data.report });
        else this.result.set({ kind: 'many', reports: data.reports ?? [] });
        return;
      }
      const queued = data.jobIds?.length ?? 0;
      this.toasts.showSuccess(
        everyone
          ? `The staff have been sent round to ${queued} character${queued === 1 ? '' : 's'}; the work proceeds below stairs.`
          : 'The staff have been sent round; the work proceeds below stairs.',
      );
      void this.queryClient.invalidateQueries({ queryKey: memoryKeys.maintenanceStatus() });
      this.queued.emit();
      this.close.emit();
    } catch (err) {
      const msg = writeErrorText(err, dryRun ? 'The rehearsal failed' : 'Failed to queue maintenance');
      this.error.set(msg);
      this.toasts.showError(msg);
    } finally {
      this.running.set(null);
    }
  }
}
