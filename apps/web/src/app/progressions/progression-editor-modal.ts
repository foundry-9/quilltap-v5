import {
  ChangeDetectionStrategy,
  Component,
  computed,
  DestroyRef,
  inject,
  input,
  OnInit,
  output,
  signal,
} from '@angular/core';

import { Icon } from '../ui/icon';
import { deriveProgression, renderProgressionReport, UNIT_MS } from './engine';
import { idFromName } from './character-progressions.api';
import {
  MAX_PROGRESSION_DESCRIPTION_LENGTH,
  MAX_PROGRESSION_NAME_LENGTH,
  MAX_REPORT_TEMPLATE_LENGTH,
  PROGRESSION_ID_PATTERN,
  safeParseProgression,
  TIME_INCREMENTS,
  type OnComplete,
  type Progression,
  type TimeIncrement,
} from './schema';

/** The entry being edited, with its id. Null = creating a fresh one. */
export interface ProgressionEdit {
  id: string;
  progression: Progression;
}

/** How a cadence is chosen in the form; `<n><unit>` splits into its two parts. */
type CadenceMode = 'turn' | 'increment' | 'period';

const PERIOD_UNITS: Array<{ value: string; label: string }> = [
  { value: 's', label: 'seconds' },
  { value: 'm', label: 'minutes' },
  { value: 'h', label: 'hours' },
  { value: 'd', label: 'days' },
  { value: 'w', label: 'weeks' },
];

const INCREMENTS = TIME_INCREMENTS;

/**
 * v4's `PLACEHOLDER_LEGEND`. A TS constant rather than template markup, and it
 * has to be: every token is a `{{…}}` pair, which Angular's interpolation would
 * eat on sight (the `PROMPT_FIELD_HINTS` precedent).
 */
const PLACEHOLDER_LEGEND: Array<[string, string]> = [
  ['{{name}}', 'the progression’s name'],
  ['{{description}}', 'your sentence above, or nothing'],
  ['{{elapsed}}', '“20 weeks, 3 days”'],
  ['{{elapsedWhole}}', '“20 weeks”'],
  ['{{remaining}}', 'the same, counting down'],
  ['{{remainingWhole}}', 'whole units only'],
  ['{{percent}}', 'a whole number, 0–100'],
  ['{{quantity}}', '“0.3/1.0 MJ”, or nothing'],
  ['{{start}}', 'when it began (wall clock)'],
  ['{{end}}', 'when it ends (wall clock)'],
  ['{{increment}}', 'the unit word — “week”'],
];

/** v4's duration shortcuts, in order. */
const DURATIONS: Array<[number, TimeIncrement]> = [
  [10, 'minute'],
  [1, 'hour'],
  [1, 'day'],
  [1, 'week'],
  [9, 'month'],
];

/** The form's own state — every field a string, as an input gives it. */
interface FormState {
  id: string;
  name: string;
  description: string;
  start: string;
  end: string;
  increment: TimeIncrement;
  percentageReport: boolean;
  cadenceMode: CadenceMode;
  periodCount: string;
  periodUnit: string;
  hasQuantity: boolean;
  quantityTotal: string;
  quantityUnit: string;
  quantityPrecision: string;
  reportTemplate: string;
  onComplete: OnComplete;
}

/** An ISO instant as a `datetime-local` value, in the BROWSER's zone. */
export function toLocalInput(iso: string): string {
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return '';
  const local = new Date(ms - new Date(ms).getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 16);
}

/** The reverse: a `datetime-local` reading as a real instant with an offset. */
export function fromLocalInput(value: string): string | null {
  const ms = Date.parse(value);
  if (Number.isNaN(ms)) return null;
  return new Date(ms).toISOString();
}

function initialForm(editing: ProgressionEdit | null): FormState {
  if (!editing) {
    const now = Date.now();
    return {
      id: '',
      name: '',
      description: '',
      start: toLocalInput(new Date(now).toISOString()),
      end: toLocalInput(new Date(now + UNIT_MS.hour).toISOString()),
      increment: 'minute',
      percentageReport: true,
      cadenceMode: 'turn',
      periodCount: '1',
      periodUnit: 'h',
      hasQuantity: false,
      quantityTotal: '1',
      quantityUnit: '',
      quantityPrecision: '1',
      reportTemplate: '',
      onComplete: 'keep',
    };
  }

  const p = editing.progression;
  const period = /^([1-9]\d{0,4})([smhdw])$/.exec(p.reportFrequency);

  return {
    id: editing.id,
    name: p.name,
    description: p.description ?? '',
    start: toLocalInput(p.startTime),
    end: toLocalInput(p.endTime),
    increment: p.timeIncrement,
    percentageReport: p.percentageReport,
    cadenceMode:
      p.reportFrequency === 'turn'
        ? 'turn'
        : p.reportFrequency === 'increment'
          ? 'increment'
          : 'period',
    periodCount: period?.[1] ?? '1',
    periodUnit: period?.[2] ?? 'h',
    hasQuantity: p.quantity !== undefined,
    quantityTotal: String(p.quantity?.total ?? 1),
    quantityUnit: p.quantity?.unit ?? '',
    quantityPrecision: String(p.quantity?.precision ?? 1),
    reportTemplate: p.reportTemplate ?? '',
    onComplete: p.onComplete,
  };
}

/** The form as a progression, or the first thing wrong with it. */
export function toProgression(
  form: FormState,
): { ok: true; value: Progression } | { ok: false; reason: string } {
  const startTime = fromLocalInput(form.start);
  const endTime = fromLocalInput(form.end);
  if (!startTime) return { ok: false, reason: 'The start needs a date and a time.' };
  if (!endTime) return { ok: false, reason: 'The end needs a date and a time.' };

  const candidate: Record<string, unknown> = {
    name: form.name.trim(),
    startTime,
    endTime,
    timeIncrement: form.increment,
    percentageReport: form.percentageReport,
    reportFrequency:
      form.cadenceMode === 'period' ? `${form.periodCount}${form.periodUnit}` : form.cadenceMode,
    onComplete: form.onComplete,
  };
  if (form.description.trim() !== '') candidate['description'] = form.description.trim();
  if (form.reportTemplate.trim() !== '') candidate['reportTemplate'] = form.reportTemplate.trim();
  if (form.hasQuantity) {
    candidate['quantity'] = {
      total: Number(form.quantityTotal),
      unit: form.quantityUnit.trim(),
      precision: Number(form.quantityPrecision),
    };
  }

  const parsed = safeParseProgression(candidate);
  if (!parsed.success) {
    const issue = parsed.issues[0];
    return { ok: false, reason: `${issue.path.join('.') || 'This entry'}: ${issue.message}` };
  }
  return { ok: true, value: parsed.data };
}

/**
 * `qt-progression-editor-modal` — one modal, one entry (v4
 * `components/characters/progressions/ProgressionEditorModal.tsx` at
 * `25f534c0b`), whose header rides across:
 *
 * > The whole reason this ships in v1 rather than leaving people to the vault
 * > file: hand-typing ISO timestamps into JSON is the wrong surface for "you
 * > became pregnant on 1 August". So the times are `datetime-local` inputs in
 * > the browser's own zone (converted to a real instant on save), the span has
 * > a duration shortcut, and the template carries a live preview rendered by
 * > the same client-safe engine the server prompts with — the author sees the
 * > exact sentence their character will read.
 * >
 * > The id is coerced from the name on create and immutable afterwards: a
 * > Pascal tool file addresses `progress.cannon.complete` and would be broken
 * > by a rename, which is the whole point of separating id from display name.
 * >
 * > The form seeds itself from `editing` ONCE, at mount. […] the parent keys
 * > this component on the entry being edited, so opening a different one mounts
 * > a fresh form and half-typed edits never bleed between entries.
 *
 * Two recorded divergences from the subprompts trio's shape, both because v4
 * makes them: this dialog does NOT use the shared `qt-modal` (v4 hand-rolls its
 * overlay here, where its own `SubpromptEditorModal` uses v4's `Modal`), and
 * the seed is `ngOnInit` rather than an effect, for the reason the header
 * gives.
 */
@Component({
  selector: 'qt-progression-editor-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  // v4's outer element is a fixed overlay; an unstyled Angular custom element
  // is `display: inline`, which would take the overlay out of the flow
  // (dogfood #97 / #107's class).
  host: { class: 'block' },
  template: `
    <div class="fixed inset-0 z-50 flex items-center justify-center qt-bg-overlay p-4">
      <div class="qt-card qt-bg-card w-full max-w-2xl max-h-[90vh] overflow-y-auto p-6 space-y-4">
        <div class="flex items-start justify-between">
          <h3 class="qt-heading-4">{{ heading() }}</h3>
          <button
            type="button"
            class="qt-button-icon qt-button-ghost"
            aria-label="Close"
            (click)="close.emit()"
          >
            <qt-icon name="close" class="w-4 h-4" />
          </button>
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <label class="space-y-1">
            <span class="qt-text-small">Name</span>
            <input
              type="text"
              class="qt-input w-full"
              placeholder="Cannon recharge"
              [attr.maxlength]="maxName"
              [value]="form().name"
              (input)="set('name', $any($event.target).value)"
            />
          </label>

          <label class="space-y-1">
            <span class="qt-text-small">
              Identifier
              <span class="qt-text-secondary">{{ identifierHint() }}</span>
            </span>
            <input
              type="text"
              class="qt-input w-full font-mono"
              placeholder="cannon"
              [value]="effectiveId()"
              [disabled]="!isCreate()"
              (input)="editId($any($event.target).value)"
            />
          </label>
        </div>

        <label class="space-y-1 block">
          <span class="qt-text-small">
            Description <span class="qt-text-secondary">— one sentence, in the second person</span>
          </span>
          <textarea
            class="qt-textarea w-full"
            rows="2"
            placeholder="You are carrying a child."
            [attr.maxlength]="maxDescription"
            [value]="form().description"
            (input)="set('description', $any($event.target).value)"
          ></textarea>
        </label>

        <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <label class="space-y-1">
            <span class="qt-text-small">Begins</span>
            <input
              type="datetime-local"
              class="qt-input w-full"
              [value]="form().start"
              (input)="set('start', $any($event.target).value)"
            />
          </label>
          <label class="space-y-1">
            <span class="qt-text-small">Ends</span>
            <input
              type="datetime-local"
              class="qt-input w-full"
              [value]="form().end"
              (input)="set('end', $any($event.target).value)"
            />
          </label>
        </div>

        <div class="flex items-center gap-2 flex-wrap">
          <span class="qt-text-small qt-text-secondary">Or ends</span>
          @for (d of durations; track d[0] + d[1]) {
            <button
              type="button"
              class="qt-button qt-button-ghost qt-button-sm"
              (click)="applyDuration(d[0], d[1])"
            >
              +{{ d[0] }} {{ d[1] }}{{ d[0] === 1 ? '' : 's' }}
            </button>
          }
          <span class="qt-text-small qt-text-secondary">after it begins.</span>
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <label class="space-y-1">
            <span class="qt-text-small">
              Spoken in <span class="qt-text-secondary">— the unit the report uses</span>
            </span>
            <select
              class="qt-select w-full"
              [value]="form().increment"
              (change)="set('increment', $any($event.target).value)"
            >
              @for (unit of increments; track unit) {
                <option [value]="unit">{{ unit }}s</option>
              }
            </select>
          </label>

          <label class="space-y-1">
            <span class="qt-text-small">Once it finishes</span>
            <select
              class="qt-select w-full"
              [value]="form().onComplete"
              (change)="set('onComplete', $any($event.target).value)"
            >
              <option value="keep">keep reporting it</option>
              <option value="once">say so once, then go quiet</option>
            </select>
          </label>
        </div>

        <fieldset class="space-y-2">
          <legend class="qt-text-small">Mentioned</legend>
          @for (mode of cadenceModes(); track mode[0]) {
            <label class="flex items-center gap-2 qt-text-small">
              <input
                type="radio"
                name="cadence"
                [checked]="form().cadenceMode === mode[0]"
                (change)="set('cadenceMode', mode[0])"
              />
              {{ mode[1] }}
              @if (mode[0] === 'period') {
                <span class="flex items-center gap-1">
                  <input
                    type="number"
                    min="1"
                    max="99999"
                    class="qt-input qt-input-sm w-20"
                    aria-label="Cadence count"
                    [value]="form().periodCount"
                    [disabled]="form().cadenceMode !== 'period'"
                    (input)="set('periodCount', $any($event.target).value)"
                  />
                  <select
                    class="qt-select qt-select-sm w-28"
                    aria-label="Cadence unit"
                    [value]="form().periodUnit"
                    [disabled]="form().cadenceMode !== 'period'"
                    (change)="set('periodUnit', $any($event.target).value)"
                  >
                    @for (u of periodUnits; track u.value) {
                      <option [value]="u.value">{{ u.label }}</option>
                    }
                  </select>
                </span>
              }
            </label>
          }
          <p class="qt-hint">
            A change you make here, or one a tool makes, is always announced on the very next turn —
            the cadence only governs the quiet stretches in between.
          </p>
        </fieldset>

        <label class="flex items-center gap-2 qt-text-small">
          <input
            type="checkbox"
            [checked]="form().percentageReport"
            (change)="set('percentageReport', $any($event.target).checked)"
          />
          Include the percentage in the default wording
        </label>

        <div class="space-y-2">
          <label class="flex items-center gap-2 qt-text-small">
            <input
              type="checkbox"
              [checked]="form().hasQuantity"
              (change)="set('hasQuantity', $any($event.target).checked)"
            />
            It fills a measurable amount (megajoules, litres, rounds)
          </label>
          @if (form().hasQuantity) {
            <div class="grid grid-cols-3 gap-2">
              <label class="space-y-1">
                <span class="qt-text-small">Full amount</span>
                <input
                  type="number"
                  step="any"
                  class="qt-input w-full"
                  [value]="form().quantityTotal"
                  (input)="set('quantityTotal', $any($event.target).value)"
                />
              </label>
              <label class="space-y-1">
                <span class="qt-text-small">Unit</span>
                <input
                  type="text"
                  maxlength="16"
                  class="qt-input w-full"
                  placeholder="MJ"
                  [value]="form().quantityUnit"
                  (input)="set('quantityUnit', $any($event.target).value)"
                />
              </label>
              <label class="space-y-1">
                <span class="qt-text-small">Decimals</span>
                <input
                  type="number"
                  min="0"
                  max="6"
                  class="qt-input w-full"
                  [value]="form().quantityPrecision"
                  (input)="set('quantityPrecision', $any($event.target).value)"
                />
              </label>
            </div>
          }
        </div>

        <div class="space-y-2">
          <label class="space-y-1 block">
            <span class="qt-text-small">
              How it&rsquo;s told
              <span class="qt-text-secondary">— leave blank for the default wording</span>
            </span>
            <textarea
              class="qt-textarea w-full"
              rows="2"
              [attr.maxlength]="maxTemplate"
              [placeholder]="templatePlaceholder"
              [value]="form().reportTemplate"
              (input)="set('reportTemplate', $any($event.target).value)"
            ></textarea>
          </label>
          <details class="qt-text-small">
            <summary class="cursor-pointer qt-text-secondary">What you may write in it</summary>
            <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 mt-2">
              @for (pair of legend; track pair[0]) {
                <dt class="font-mono text-xs qt-text">{{ pair[0] }}</dt>
                <dd class="text-xs qt-text-secondary">{{ pair[1] }}</dd>
              }
            </dl>
            <p class="qt-hint mt-2">
              This wording covers the stretch while it is running. Before it starts and after it
              finishes the report says so in its own words, which no template overrides.
            </p>
          </details>
        </div>

        @if (preview(); as line) {
          <div class="qt-card p-3">
            <p class="qt-text-small qt-text-secondary mb-1">
              As {{ form().name || 'this character' }} would read it now:
            </p>
            <p class="qt-text-small italic">{{ line }}</p>
          </div>
        }

        @if (error(); as message) {
          <p class="qt-text-small qt-text-destructive">{{ message }}</p>
        }

        <div class="flex justify-end gap-2 pt-2">
          <button
            type="button"
            class="qt-button-secondary"
            [disabled]="saving()"
            (click)="close.emit()"
          >
            Cancel
          </button>
          <button
            type="button"
            class="qt-button-primary"
            [disabled]="saving()"
            (click)="handleSave()"
          >
            {{ saveLabel() }}
          </button>
        </div>
      </div>
    </div>
  `,
})
export class ProgressionEditorModal implements OnInit {
  /** The entry being edited, with its id. Null = creating a fresh one. */
  readonly editing = input<ProgressionEdit | null>(null);
  /** Ids already in use, so a create cannot silently overwrite one. */
  readonly existingIds = input.required<string[]>();
  readonly saving = input.required<boolean>();
  readonly close = output<void>();
  readonly save = output<{ id: string; progression: Progression }>();

  protected readonly legend = PLACEHOLDER_LEGEND;
  protected readonly periodUnits = PERIOD_UNITS;
  protected readonly increments = INCREMENTS;
  protected readonly durations = DURATIONS;
  protected readonly maxName = MAX_PROGRESSION_NAME_LENGTH;
  protected readonly maxDescription = MAX_PROGRESSION_DESCRIPTION_LENGTH;
  protected readonly maxTemplate = MAX_REPORT_TEMPLATE_LENGTH;
  /**
   * v4's placeholder attribute. Bound rather than literal for the same reason
   * {@link PLACEHOLDER_LEGEND} is a constant: Angular interpolates `{{…}}` in
   * template text AND in a plain attribute value.
   */
  protected readonly templatePlaceholder =
    '{{description}} You are {{elapsedWhole}} along; due in {{remaining}}.';

  protected readonly form = signal<FormState>(initialForm(null));
  private readonly idTouched = signal(false);
  protected readonly error = signal<string | null>(null);

  /**
   * The clock the live preview reads. Held in state and advanced by an interval
   * rather than read during render: a preview frozen beside an advancing
   * recharge would be a worse lie than none.
   */
  private readonly nowMs = signal(Date.now());

  constructor() {
    const timer = setInterval(() => this.nowMs.set(Date.now()), 1000);
    inject(DestroyRef).onDestroy(() => clearInterval(timer));
  }

  /**
   * The seed, ONCE — v4's `useState(() => initialForm(editing))`. Deliberately
   * `ngOnInit` and not an effect: the parent keys this component on the entry
   * being edited, so a fresh instance is mounted per opening and there is
   * nothing to re-seat.
   */
  ngOnInit(): void {
    this.form.set(initialForm(this.editing()));
  }

  protected readonly isCreate = computed(() => this.editing() === null);

  protected readonly identifierHint = computed(() =>
    this.isCreate() ? '(how a tool addresses it)' : '(fixed — tools address it)',
  );

  /**
   * On create the id follows the name until the author edits it themselves —
   * the subprompts precedent. On edit it never moves at all: a Pascal tool file
   * addresses the id, and a rename would break it silently.
   */
  protected readonly effectiveId = computed(() =>
    this.isCreate() && !this.idTouched() ? idFromName(this.form().name) : this.form().id,
  );

  protected readonly heading = computed(() =>
    this.isCreate() ? 'New progression' : `Edit “${this.form().name || this.effectiveId()}”`,
  );

  protected readonly saveLabel = computed(() =>
    this.saving() ? 'Saving…' : this.isCreate() ? 'Add progression' : 'Save changes',
  );

  /** The three cadence radios; the middle label names the current increment. */
  protected readonly cadenceModes = computed<Array<[CadenceMode, string]>>(() => [
    ['turn', 'every turn'],
    ['increment', `whenever the ${this.form().increment} count changes`],
    ['period', 'at most once every'],
  ]);

  /** The line this progression would produce right now, for the live preview. */
  protected readonly preview = computed(() => {
    const built = toProgression(this.form());
    if (!built.ok) return null;
    return renderProgressionReport(
      built.value,
      deriveProgression(this.effectiveId() || 'preview', built.value, this.nowMs()),
    );
  });

  protected set<K extends keyof FormState>(key: K, value: FormState[K]): void {
    this.form.update((prev) => ({ ...prev, [key]: value }));
  }

  protected editId(value: string): void {
    this.idTouched.set(true);
    this.set('id', value);
  }

  protected handleSave(): void {
    const id = this.effectiveId().trim();
    if (!PROGRESSION_ID_PATTERN.test(id)) {
      this.error.set(
        'The id must be lowercase, start with a letter, and hold only letters, digits, _ and -.',
      );
      return;
    }
    if (this.isCreate() && this.existingIds().includes(id)) {
      this.error.set(`This character already carries a progression called “${id}”.`);
      return;
    }
    const built = toProgression(this.form());
    if (!built.ok) {
      this.error.set(built.reason);
      return;
    }
    this.error.set(null);
    this.save.emit({ id, progression: built.value });
  }

  /** "Ends N units after the start" — the shortcut nobody wants to do by hand. */
  protected applyDuration(count: number, unit: TimeIncrement): void {
    const startMs = Date.parse(this.form().start);
    if (Number.isNaN(startMs)) return;
    this.set('end', toLocalInput(new Date(startMs + count * UNIT_MS[unit]).toISOString()));
  }
}
