import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient, coreErrorMessage } from '../core/core-client';
import type { CustomToolParameterSpec } from '../core/core-contract';
import { RealtimeService } from '../core/realtime.service';
import type { OutcomeState, When, WhenObject } from '../pascal/custom-tool-types';
import { COMPARATOR_LABELS } from '../screens/custom-tools/comparator-labels';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';
import {
  CustomToolParamsForm,
  coerceParamValues,
  type ParamChange,
  type ParameterFormValues,
} from './custom-tool-params-form';
import {
  fetchInterventions,
  previewIntervention,
  reaskIntervention,
  resolveIntervention,
  type EffectPin,
  type InterventionDecision,
  type InterventionEffectView,
  type InterventionHoldView,
  type InterventionOverrides,
  type InterventionPreview,
} from './custom-tool-interventions.api';
import { customToolsKeys } from './custom-tools.api';
import { describeSpan } from './intervention-spans';

/**
 * CustomToolInterventionDialog — Pascal holds a roll for the operator (v4
 * `components/chat/CustomToolInterventionDialog.tsx` at `236c04e35`:
 * `839575d89` + `8ae8a127c`'s span hints + `b02d902f3`'s compact oracle
 * section).
 *
 * When a model's `run_custom` or a trigger phrase reaches a custom tool marked
 * intervenable, the server deals the run and then HOLDS it inside the turn
 * until the operator answers here: deal it (perhaps changed), let it stand as
 * rolled, or set the table aside (a veto). The model is never told.
 *
 * The hold lives on the server, not in the stream, so this dialog reads the
 * chat's pending holds through TanStack Query, refreshed by the
 * `customToolInterventions` realtime topic (the topic map routes the hint to
 * `customToolsKeys.interventions(chatId)`, exactly as v4's does) with a poll
 * only as the offline fallback. A reload or a second tab therefore reopens it.
 * Several holds are shown one at a time, oldest first.
 *
 * The client never computes a run itself: every edit is a set of PINS
 * (`InterventionOverrides`) sent to the preview verb, debounced, and the
 * server's recompute is what is shown — and what is finally dealt. Fields the
 * author hid are not rendered at all (the server would refuse them).

 */

const PREVIEW_DEBOUNCE_MS = 300;
const OUTCOME_STATES: OutcomeState[] = ['success', 'partial', 'failure', 'info'];
/** The offline fallback poll (v4 `useRealtimeRefetchInterval(5_000)`). */
const FALLBACK_POLL_MS = 5_000;

/** Format a number the way Pascal does: integers plain, floats to 4 significant digits. */
export function formatNumber(n: number): string {
  return Number.isInteger(n) ? String(n) : String(Number(n.toPrecision(4)));
}

/** A row's `when`, summarised for the outcome list. */
export function summarizeWhen(when: When): string {
  if (when === true) return 'otherwise';
  const groups = Array.isArray(when) ? when : [when];
  return groups.map(summarizeGroup).join(' or ');
}

export function summarizeGroup(group: WhenObject): string {
  const parts: string[] = [];
  const comparators = (subject: string, comparator: Record<string, unknown> | undefined) => {
    if (!comparator) return;
    for (const [key, label] of Object.entries(COMPARATOR_LABELS)) {
      const operand = comparator[key];
      if (operand === undefined) continue;
      const shown =
        typeof operand === 'object' && operand !== null && '$param' in operand
          ? `params.${(operand as { $param: string }).$param}`
          : typeof operand === 'number'
            ? formatNumber(operand)
            : JSON.stringify(operand);
      parts.push(`${subject} ${label} ${shown}`);
    }
  };
  const g = group as unknown as Record<string, unknown>;
  comparators('value', g);
  comparators('roll', g['roll'] as Record<string, unknown> | undefined);
  for (const [name, comparator] of Object.entries((g['params'] ?? {}) as Record<string, Record<string, unknown>>)) {
    comparators(`params.${name}`, comparator);
  }
  for (const [key, comparator] of Object.entries((g['metadata'] ?? {}) as Record<string, Record<string, unknown>>)) {
    comparators(`metadata.${key}`, comparator);
  }
  const llm = g['llm'] as (Record<string, unknown> & { ok?: boolean }) | undefined;
  if (llm) {
    if (llm.ok !== undefined) parts.push(llm.ok ? 'consult answered' : 'consult failed');
    comparators('answer', llm);
  }
  const other = ['state', 'progress', 'inventory', 'inventoryCount', 'inventoryAggregate'].filter(
    (key) => g[key] !== undefined,
  );
  if (other.length > 0) parts.push(`${other.join(', ')} tests`);
  return parts.length > 1 ? `(${parts.join(' and ')})` : (parts[0] ?? 'always');
}

/** An edited effect value is a literal: a boolean, a number, or the text as typed. */
export function literalFrom(text: string): number | string | boolean {
  if (text === 'true') return true;
  if (text === 'false') return false;
  const n = Number(text);
  return text.trim() !== '' && Number.isFinite(n) ? n : text;
}

/** Drop a key from the overrides — the reset control. */
export function unpin<K extends keyof InterventionOverrides>(
  overrides: InterventionOverrides,
  key: K,
): InterventionOverrides {
  const { [key]: _gone, ...rest } = overrides;
  return rest;
}

function inputText(event: Event): string {
  return (event.target as HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement).value;
}

// ---------------------------------------------------------------------------
// Pieces
// ---------------------------------------------------------------------------

/** v4 `Section`: a titled block, with "dealt: … · reset" once it is pinned. */
@Component({
  selector: 'qt-intervention-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { style: 'display: block' },
  template: `
    <section class="qt-intervention-section" [class.qt-intervention-section--pinned]="pinned()">
      <div class="flex items-baseline justify-between gap-2 mb-1">
        <h3 class="text-sm font-semibold">{{ title() }}</h3>
        @if (pinned() && resettable()) {
          <span class="text-xs qt-text-secondary"
            >{{ dealt() !== undefined ? 'dealt: ' + dealt() + ' · ' : ''
            }}<button type="button" class="qt-intervention-reset" (click)="reset.emit()">reset</button></span
          >
        }
      </div>
      <ng-content />
    </section>
  `,
})
export class InterventionSection {
  readonly title = input.required<string>();
  /** Absent for a section with nothing to pin. */
  readonly pinned = input(false);
  /** v4's `onReset` present — the compact oracle section has none. */
  readonly resettable = input(true);
  /** The value as first dealt, shown once the field has been changed. */
  readonly dealt = input<string | undefined>(undefined);
  readonly reset = output<void>();
}

/** v4 `NumberField`: the operator's draft while typing, the server's figure otherwise. */
@Component({
  selector: 'qt-intervention-number-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { style: 'display: block' },
  template: `
    <label class="flex items-center gap-2 text-sm">
      <span class="text-xs qt-text-secondary">{{ label() }}</span>
      <input
        type="number"
        step="any"
        class="qt-input w-32"
        [value]="text()"
        (input)="onInput(inputText($event))"
        (blur)="draft.set(null)"
      />
    </label>
  `,
})
export class InterventionNumberField {
  readonly label = input.required<string>();
  readonly value = input.required<number>();
  readonly valueChange = output<number>();

  /** What the operator is typing, while they type. */
  protected readonly draft = signal<string | null>(null);
  protected readonly text = computed(() => this.draft() ?? formatNumber(this.value()));
  protected readonly inputText = inputText;

  protected onInput(text: string): void {
    this.draft.set(text);
    const n = Number(text);
    if (text.trim() !== '' && Number.isFinite(n)) this.valueChange.emit(n);
  }
}

// ---------------------------------------------------------------------------
// One held run
// ---------------------------------------------------------------------------

/**
 * v4 `HeldRun`. Re-created per hold id by its host, so a fresh hold starts with
 * a clean slate of pins.
 */
@Component({
  selector: 'qt-held-run',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CustomToolParamsForm, InterventionNumberField, InterventionSection, Modal],
  template: `
    <qt-modal
      [title]="'Pascal holds: ' + dealt().title"
      maxWidth="2xl"
      [closeOnBackdrop]="false"
      [closeOnEscape]="false"
      (close)="noop()"
    >
      <div class="qt-intervention space-y-4" data-testid="custom-tool-intervention">
        <p class="text-sm qt-text-secondary">
          {{ reached() }}. The cards are dealt but not yet shown — adjust what you like, then deal it. No one
          at the table will know you touched the deck.@if (waiting() > 0) {<span class="qt-intervention-queue"
              >&ngsp;{{ waiting() }} more waiting.</span
            >}
        </p>

        @if (hold().eligible.params.length > 0) {
          <qt-intervention-section
            title="Parameters"
            [pinned]="overrides().params !== undefined"
            [dealt]="paramsDealt()"
            (reset)="reset('params')"
          >
            <qt-custom-tool-params-form
              [parameters]="paramSpecs()"
              [values]="paramValues()"
              [idPrefix]="'intervention-' + hold().id"
              layout="stacked"
              (paramChange)="onParamChange($event)"
            />
            @if (preview().clampedParams.length > 0) {
              <p class="qt-intervention-note">
                Held to the declared bounds: {{ preview().clampedParams.join(', ') }}.
              </p>
            }
          </qt-intervention-section>
        }

        @if (shown().has('roll')) {
          <qt-intervention-section
            title="The roll"
            [pinned]="overrides().roll !== undefined || overrides().dice !== undefined"
            [dealt]="dealt().rollForm === 'dice' ? dealt().diceBreakdown : formatNumber(dealt().raw)"
            (reset)="resetRoll()"
          >
            @if (run().rollForm === 'dice') {
              <div class="flex flex-wrap gap-2 items-center">
                @for (face of run().diceRolls ?? []; track $index) {
                  <input
                    type="number"
                    [attr.aria-label]="'Die ' + ($index + 1)"
                    class="qt-input w-16"
                    [attr.min]="1"
                    [attr.max]="hold().definition.diceSides ?? null"
                    [value]="face"
                    (input)="onDieChange($index, inputText($event))"
                  />
                }
                <span class="text-xs qt-text-secondary">{{ run().diceBreakdown }}</span>
              </div>
            } @else {
              <qt-intervention-number-field
                label="Raw draw"
                [value]="overrides().roll ?? run().raw"
                (valueChange)="pin({ roll: $event })"
              />
            }
            <p class="text-xs qt-text-secondary" data-testid="intervention-roll-span">{{ rollSpanText() }}</p>
          </qt-intervention-section>
        }

        @if (shown().has('value') && run().rollForm === 'range') {
          <qt-intervention-section
            title="The value"
            [pinned]="overrides().value !== undefined"
            [dealt]="formatNumber(dealt().value)"
            (reset)="reset('value')"
          >
            <qt-intervention-number-field
              label="Value after modifiers"
              [value]="overrides().value ?? run().value"
              (valueChange)="pin({ value: $event })"
            />
            <p class="text-xs qt-text-secondary" data-testid="intervention-value-span">
              After the modifiers, it comes to {{ describeSpan(run().valueSpan) }}.
            </p>
          </qt-intervention-section>
        }

        @if (hold().definition.hasLlm && consultShown()) {
          <qt-intervention-section
            title="The consult"
            [pinned]="overrides().llmPrompt !== undefined || overrides().llmAnswer !== undefined"
            [dealt]="consults().length > 1 ? 'asked ' + consults().length + ' times' : undefined"
            (reset)="resetConsult()"
          >
            @if (shown().has('llmPrompt')) {
              <label class="block">
                <span class="text-xs qt-text-secondary">Prompt</span>
                <textarea
                  class="qt-textarea w-full font-mono text-xs"
                  rows="4"
                  [value]="overrides().llmPrompt ?? preview().promptNow ?? ''"
                  (input)="onPromptChange(inputText($event))"
                ></textarea>
              </label>
            }
            @if (shown().has('llmAnswer')) {
              <div class="space-y-1">
                <label class="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    [checked]="answer()?.ok ?? false"
                    (change)="onAnsweredChange($any($event.target).checked)"
                  />
                  The oracle answered
                </label>
                <textarea
                  aria-label="Answer"
                  class="qt-textarea w-full text-sm"
                  rows="3"
                  [value]="answer()?.output ?? ''"
                  (input)="onAnswerChange(inputText($event))"
                ></textarea>
              </div>
            }
            <div class="flex items-center gap-2">
              <button
                type="button"
                class="qt-button qt-button-secondary"
                (click)="reask()"
                [disabled]="reasking() || busy()"
              >
                {{ reasking() ? 'Asking…' : 'Re-ask' }}
              </button>
              @if (preview().staleAnswer) {
                <span class="qt-intervention-note">This answer was given to a different question.</span>
              }
            </div>
          </qt-intervention-section>
        }

        @if (shown().has('outcome')) {
          <qt-intervention-section
            title="The outcome"
            [pinned]="overrides().outcomeIndex !== undefined"
            [dealt]="'row ' + (dealt().outcomeIndex + 1)"
            (reset)="reset('outcomeIndex')"
          >
            <ol class="space-y-1">
              @for (outcome of hold().definition.outcomes; track $index) {
                <li>
                  <button
                    type="button"
                    [attr.aria-pressed]="$index === run().outcomeIndex"
                    (click)="pin({ outcomeIndex: $index })"
                    [class]="
                      'qt-intervention-row qt-pascal-result qt-pascal-result--' +
                      outcome.state +
                      ($index === run().outcomeIndex ? ' qt-intervention-row--winning' : '')
                    "
                  >
                    <span class="font-mono text-xs">{{ $index + 1 }}.</span>{{ ' '
                    }}<span class="text-xs">{{ summarizeWhen(outcome.when) }}</span>{{ ' '
                    }}<span class="text-xs qt-text-secondary">→ {{ outcome.state }}</span>
                  </button>
                </li>
              }
            </ol>
          </qt-intervention-section>
        }

        @if (hold().definition.hasLlm && !consultShown()) {
          <!-- The author hid the consult itself, but a change upstream can still
               alter what it would be asked; say so, and let the operator re-ask. -->
          <qt-intervention-section title="The oracle" [resettable]="false">
            <div class="flex flex-wrap items-center gap-2" data-testid="intervention-hidden-consult">
              <button
                type="button"
                class="qt-button qt-button-secondary"
                (click)="reask()"
                [disabled]="reasking() || busy()"
              >
                {{ reasking() ? 'Asking…' : 'Re-ask' }}
              </button>
              @if (preview().staleAnswer) {
                <span class="qt-intervention-note">
                  Your changes have altered what the oracle would be asked; its answer was given to a different
                  question.
                </span>
              } @else {
                <span class="text-xs qt-text-secondary">The oracle’s answer still fits the question.</span>
              }
              @if (consults().length > 1) {
                <span class="text-xs qt-text-secondary">Asked {{ consults().length }} times.</span>
              }
            </div>
          </qt-intervention-section>
        }

        @if (shown().has('message')) {
          <qt-intervention-section
            title="The message"
            [pinned]="overrides().message !== undefined"
            (reset)="reset('message')"
          >
            <textarea
              aria-label="Message"
              class="qt-textarea w-full text-sm"
              rows="3"
              [value]="overrides().message ?? run().message"
              (input)="onTextPin('message', inputText($event))"
            ></textarea>
          </qt-intervention-section>
        }

        @if (shown().has('state')) {
          <qt-intervention-section
            title="The verdict"
            [pinned]="overrides().state !== undefined"
            [dealt]="dealt().state"
            (reset)="reset('state')"
          >
            <select aria-label="State" class="qt-select" (change)="onStateChange(inputText($event))">
              @for (state of OUTCOME_STATES; track state) {
                <option [value]="state" [selected]="state === (overrides().state ?? run().state)">
                  {{ state }}
                </option>
              }
            </select>
          </qt-intervention-section>
        }

        @if (hold().eligible.effects.length > 0) {
          <qt-intervention-section
            title="Consequences"
            [pinned]="overrides().effects !== undefined"
            (reset)="reset('effects')"
          >
            <ul class="space-y-2">
              @for (effect of eligibleEffects(); track effect.index) {
                @let pinned = effectPin(effect.index);
                <li class="qt-intervention-effect">
                  <div class="flex flex-wrap items-center gap-2 text-sm">
                    <span class="font-mono text-xs">{{ effect.label }}</span>
                    <span class="text-xs qt-text-secondary">{{ effectFires(effect) }}</span>
                    <select
                      [attr.aria-label]="'Effect ' + (effect.index + 1) + ' mode'"
                      class="qt-select ml-auto"
                      (change)="onEffectModeChange(effect.index, inputText($event))"
                    >
                      <option value="auto" [selected]="(pinned?.mode ?? 'auto') === 'auto'">Auto</option>
                      <option value="on" [selected]="pinned?.mode === 'on'">On</option>
                      <option value="off" [selected]="pinned?.mode === 'off'">Off</option>
                    </select>
                  </div>
                  @if (effect.fired && effect.kind === 'write') {
                    <input
                      [attr.aria-label]="'Effect ' + (effect.index + 1) + ' value'"
                      class="qt-input w-full mt-1 font-mono text-xs"
                      [value]="effectValueText(effect)"
                      (input)="patchEffect(effect.index, { value: literalFrom(inputText($event)) })"
                    />
                  }
                  @if (effect.fired && (effect.kind === 'wardrobe' || effect.kind === 'inventory')) {
                    <textarea
                      [attr.aria-label]="'Effect ' + (effect.index + 1) + ' items'"
                      class="qt-textarea w-full mt-1 text-xs"
                      rows="2"
                      [value]="(pinned?.items ?? effect.items ?? []).join('\\n')"
                      (input)="patchEffect(effect.index, { items: inputText($event).split('\\n') })"
                    ></textarea>
                  }
                  @if (effect.fired && effect.kind === 'log') {
                    <textarea
                      [attr.aria-label]="'Effect ' + (effect.index + 1) + ' log line'"
                      class="qt-textarea w-full mt-1 text-xs"
                      rows="2"
                      [value]="pinned?.log ?? effect.log ?? ''"
                      (input)="patchEffect(effect.index, { log: inputText($event) })"
                    ></textarea>
                  }
                </li>
              }
            </ul>
          </qt-intervention-section>
        }

        @if (presentationShown()) {
          <qt-intervention-section
            title="Presentation"
            [pinned]="presentationPinned()"
            (reset)="resetPresentation()"
          >
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
              @if (shown().has('visibility')) {
                <label class="text-xs">
                  <span class="qt-text-secondary">Heard by</span>
                  <select class="qt-select w-full" (change)="onVisibilityChange(inputText($event))">
                    <option value="public" [selected]="(overrides().visibility ?? run().visibility) === 'public'">
                      The whole table
                    </option>
                    <option value="whisper" [selected]="(overrides().visibility ?? run().visibility) === 'whisper'">
                      A whisper
                    </option>
                  </select>
                </label>
              }
              @if (shown().has('announcer') && hold().hasCharacter) {
                <label class="text-xs">
                  <span class="qt-text-secondary">Announced by</span>
                  <select class="qt-select w-full" (change)="onAnnouncerChange(inputText($event))">
                    <option value="pascal" [selected]="(overrides().announcer ?? run().announcer) === 'pascal'">
                      Pascal
                    </option>
                    <option
                      value="character"
                      [selected]="(overrides().announcer ?? run().announcer) === 'character'"
                    >
                      {{ hold().characterName ?? 'The character' }}
                    </option>
                  </select>
                </label>
              }
              @if (shown().has('title')) {
                <label class="text-xs">
                  <span class="qt-text-secondary">Title</span>
                  <input
                    class="qt-input w-full"
                    [value]="overrides().title ?? run().title"
                    (input)="onTextPin('title', inputText($event))"
                  />
                </label>
              }
              @if (shown().has('chipLabel')) {
                <label class="text-xs">
                  <span class="qt-text-secondary">Chip label</span>
                  <input
                    class="qt-input w-full"
                    [value]="overrides().chipLabel ?? run().chipLabel ?? ''"
                    (input)="onTextPin('chipLabel', inputText($event))"
                  />
                </label>
              }
            </div>
          </qt-intervention-section>
        }

        @if (preview().warnings.length > 0) {
          <ul class="qt-intervention-note space-y-1">
            @for (warning of preview().warnings; track warning) {
              <li>{{ warning }}</li>
            }
          </ul>
        }

        <div>
          <h3 class="text-xs uppercase tracking-wide qt-text-secondary mb-1">
            As it will be announced{{ pinnedSuffix() }}
          </h3>
          <div [class]="'qt-pascal-result qt-pascal-result--' + run().state" data-testid="intervention-preview">
            <div class="text-xs font-semibold">{{ announcedLine() }}</div>
            <div class="text-sm whitespace-pre-wrap">{{ run().message }}</div>
          </div>
        </div>
      </div>

      <div qt-modal-footer class="flex flex-wrap items-center justify-end gap-2 w-full">
        @if (confirmingVeto()) {
          <span class="text-sm qt-text-secondary mr-auto"
            >Sweep the cards away? Nothing is announced and nothing changes.</span
          >
          <button
            type="button"
            class="qt-button qt-button-secondary"
            (click)="confirmingVeto.set(false)"
            [disabled]="busy()"
          >
            Keep the table
          </button>
          <button
            type="button"
            class="qt-button qt-button-destructive"
            (click)="resolve({ kind: 'veto' })"
            [disabled]="busy()"
          >
            Set it aside
          </button>
        } @else {
          <button
            type="button"
            class="qt-button qt-button-secondary mr-auto"
            (click)="confirmingVeto.set(true)"
            [disabled]="busy()"
          >
            Set the table aside
          </button>
          <button
            type="button"
            class="qt-button qt-button-secondary"
            (click)="resolve({ kind: 'as-rolled' })"
            [disabled]="busy()"
          >
            As rolled
          </button>
          <button
            type="button"
            class="qt-button qt-button-primary"
            (click)="resolve({ kind: 'deal', overrides: overrides() })"
            [disabled]="busy() || previewing()"
          >
            Deal it
          </button>
        }
      </div>
    </qt-modal>
  `,
})
export class HeldRun {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly chatId = input.required<string>();
  readonly hold = input.required<InterventionHoldView>();
  /** How many more holds wait behind this one. */
  readonly waiting = input(0);
  readonly settled = output<void>();

  protected readonly OUTCOME_STATES = OUTCOME_STATES;
  protected readonly formatNumber = formatNumber;
  protected readonly summarizeWhen = summarizeWhen;
  protected readonly literalFrom = literalFrom;
  protected readonly describeSpan = describeSpan;
  protected readonly inputText = inputText;

  /** The pins (v4 `overrides`). */
  readonly overrides = signal<InterventionOverrides>({});
  private readonly serverPreview = signal<InterventionPreview | null>(null);
  /** Every consult so far; seeded from the hold until a Re-ask replaces it (v4 `useState(hold.consults)`). */
  private readonly reasked = signal<InterventionHoldView['consults'] | null>(null);
  protected readonly confirmingVeto = signal(false);
  /** v4 `previewMutation.isPending` / `reaskMutation.isPending` / `resolveMutation.isPending`. */
  protected readonly previewing = signal(false);
  protected readonly reasking = signal(false);
  protected readonly busy = signal(false);

  protected readonly consults = computed(() => this.reasked() ?? this.hold().consults);
  protected readonly shown = computed(() => new Set<string>(this.hold().eligible.fields));
  protected readonly pinnedCount = computed(() => Object.keys(this.overrides()).length);
  /** With nothing pinned the run is simply the run as dealt. */
  protected readonly preview = computed(() =>
    this.pinnedCount() === 0 ? this.hold().baseline : (this.serverPreview() ?? this.hold().baseline),
  );
  protected readonly run = computed(() => this.preview().run);
  protected readonly dealt = computed(() => this.hold().baseline.run);
  protected readonly consultShown = computed(() => this.shown().has('llmPrompt') || this.shown().has('llmAnswer'));
  protected readonly answer = computed(() => this.overrides().llmAnswer ?? this.run().llm);

  protected readonly reached = computed(() => {
    const hold = this.hold();
    if (hold.entrance === 'trigger') return `overheard: “${hold.triggerPhrase ?? ''}”`;
    return hold.characterName ? `called by ${hold.characterName}` : 'called by a character';
  });

  protected readonly rollSpanText = computed(() => {
    const run = this.run();
    const sides = this.hold().definition.diceSides;
    return run.rollForm === 'dice'
      ? `${run.notation ?? 'The dice'} can come to ${describeSpan(run.rollSpan)}${sides ? `, each die 1 to ${sides}` : ''}.`
      : `The table draws ${describeSpan(run.rollSpan)}.`;
  });

  protected readonly pinnedSuffix = computed(() => {
    const n = this.pinnedCount();
    return n > 0 ? ` — ${n} change${n === 1 ? '' : 's'} pinned` : '';
  });

  protected readonly announcedLine = computed(() => {
    const run = this.run();
    const name = this.hold().characterName;
    return (
      (run.chipLabel ?? run.title) +
      (run.visibility === 'whisper' ? ' · whispered' : '') +
      (run.announcer === 'character' && name ? ` · in ${name}'s voice` : '')
    );
  });

  // -- Parameters (v4 `ParamsSection`) -------------------------------------

  /** The form wants literal defaults; the run's resolved values are exactly that. */
  protected readonly paramSpecs = computed(() => {
    const hold = this.hold();
    const dealtParams = this.dealt().params;
    const specs: Record<string, CustomToolParameterSpec> = {};
    for (const name of hold.eligible.params) {
      const declared = hold.definition.parameters[name];
      if (!declared) continue;
      specs[name] = { ...declared, default: dealtParams[name] ?? '' } as CustomToolParameterSpec;
    }
    return specs;
  });

  protected readonly paramValues = computed(() => {
    const values: ParameterFormValues = {};
    const pinned = this.overrides().params;
    const params = this.run().params;
    for (const name of Object.keys(this.paramSpecs())) {
      const current = pinned?.[name] ?? params[name];
      values[name] = typeof current === 'boolean' ? current : String(current ?? '');
    }
    return values;
  });

  protected readonly paramsDealt = computed(() => {
    const specs = this.paramSpecs();
    return Object.entries(this.dealt().params)
      .filter(([n]) => n in specs)
      .map(([n, v]) => `${n} ${String(v)}`)
      .join(', ');
  });

  // -- Effects (v4 `EffectsSection`) -----------------------------------------

  protected readonly eligibleEffects = computed(() => {
    const eligible = this.hold().eligible.effects;
    return this.run().effects.filter((e) => eligible.includes(e.index));
  });

  // -- Presentation (v4 `PresentationSection`) -------------------------------

  private static readonly PRESENTATION = ['visibility', 'announcer', 'title', 'chipLabel'] as const;
  protected readonly presentationShown = computed(() =>
    HeldRun.PRESENTATION.some((key) => this.shown().has(key)),
  );
  protected readonly presentationPinned = computed(() =>
    HeldRun.PRESENTATION.some((key) => this.overrides()[key] !== undefined),
  );

  constructor() {
    // Every edit asks the server; nothing is computed here. A burst of edits
    // collapses into one request (v4 `:181-186`).
    effect((onCleanup) => {
      const overrides = this.overrides();
      if (Object.keys(overrides).length === 0) return;
      const timer = setTimeout(() => void this.runPreview(overrides), PREVIEW_DEBOUNCE_MS);
      onCleanup(() => clearTimeout(timer));
    });
  }

  protected noop(): void {
    // A held roll waits for an answer; there is no dismissing it unanswered.
  }

  pin(patch: InterventionOverrides): void {
    this.overrides.update((prev) => ({ ...prev, ...patch }));
  }

  reset(key: keyof InterventionOverrides): void {
    this.overrides.update((prev) => unpin(prev, key));
  }

  protected resetRoll(): void {
    this.overrides.update((prev) => unpin(unpin(prev, 'roll'), 'dice'));
  }

  protected resetConsult(): void {
    this.overrides.update((prev) => unpin(unpin(prev, 'llmPrompt'), 'llmAnswer'));
  }

  protected resetPresentation(): void {
    this.overrides.update((prev) =>
      unpin(unpin(unpin(unpin(prev, 'visibility'), 'announcer'), 'title'), 'chipLabel'),
    );
  }

  protected onParamChange(change: ParamChange): void {
    const specs = this.paramSpecs();
    const coerced = coerceParamValues(specs, { ...this.paramValues(), [change.param]: change.value });
    const pinned = this.overrides().params ?? {};
    this.pin({ params: { ...pinned, [change.param]: coerced[change.param]! } });
  }

  protected onDieChange(index: number, text: string): void {
    const faces = [...(this.overrides().dice ?? this.run().diceRolls ?? [])];
    faces[index] = Number(text);
    if (faces.every((f) => Number.isFinite(f))) this.pin({ dice: faces });
  }

  /** A non-empty edit pins; an emptied field resets (v4's message / title / chip label / prompt). */
  protected onTextPin(key: 'message' | 'title' | 'chipLabel', text: string): void {
    if (text) this.pin({ [key]: text });
    else this.reset(key);
  }

  protected onPromptChange(text: string): void {
    if (text) this.pin({ llmPrompt: text });
    else this.reset('llmPrompt');
  }

  protected onAnsweredChange(ok: boolean): void {
    const output = this.answer()?.output ?? '';
    this.pin({ llmAnswer: { ok, output } });
  }

  protected onAnswerChange(output: string): void {
    const ok = this.answer()?.ok ?? true;
    this.pin({ llmAnswer: { ok, output } });
  }

  protected onStateChange(value: string): void {
    this.pin({ state: value as OutcomeState });
  }

  protected onVisibilityChange(value: string): void {
    this.pin({ visibility: value as 'public' | 'whisper' });
  }

  protected onAnnouncerChange(value: string): void {
    this.pin({ announcer: value as 'pascal' | 'character' });
  }

  protected effectPin(index: number): EffectPin | undefined {
    return this.overrides().effects?.[String(index)];
  }

  protected effectFires(effect: InterventionEffectView): string {
    return effect.fired ? 'fires' : `does not fire${effect.skipped ? ` (${effect.skipped})` : ''}`;
  }

  protected effectValueText(effect: InterventionEffectView): string {
    return String(this.effectPin(effect.index)?.value ?? effect.value ?? '');
  }

  /** v4 `setPin`: keys are the index as a string; an empty map drops the `effects` key. */
  private setEffectPin(index: number, next: EffectPin | null): void {
    this.overrides.update((prev) => {
      const all = { ...(prev.effects ?? {}) };
      if (next) all[String(index)] = next;
      else delete all[String(index)];
      const { effects: _old, ...rest } = prev;
      return Object.keys(all).length > 0 ? { ...rest, effects: all } : rest;
    });
  }

  protected patchEffect(index: number, fields: Partial<EffectPin>): void {
    const pinned = this.effectPin(index);
    const mode = pinned?.mode ?? 'auto';
    this.setEffectPin(index, { mode, ...pinned, ...fields });
  }

  protected onEffectModeChange(index: number, value: string): void {
    const next = value as EffectPin['mode'];
    const pinned = this.effectPin(index);
    if (next === 'auto' && pinned && Object.keys(pinned).length === 1) this.setEffectPin(index, null);
    else this.patchEffect(index, { mode: next });
  }

  // -- The three verbs -------------------------------------------------------

  private async runPreview(overrides: InterventionOverrides): Promise<void> {
    this.previewing.set(true);
    try {
      const data = await previewIntervention(this.core, this.chatId(), this.hold().id, overrides);
      this.serverPreview.set(data.preview);
    } catch (err) {
      this.toasts.showError(coreErrorMessage(err, 'Pascal could not reckon that change'));
    } finally {
      this.previewing.set(false);
    }
  }

  protected async reask(): Promise<void> {
    const overrides = this.overrides();
    this.reasking.set(true);
    try {
      const data = await reaskIntervention(this.core, this.chatId(), this.hold().id, overrides);
      // Re-ask replaces a pinned answer with a fresh one.
      this.overrides.update((prev) => unpin(prev, 'llmAnswer'));
      this.reasked.set(data.consults);
      this.serverPreview.set(data.preview);
    } catch (err) {
      this.toasts.showError(coreErrorMessage(err, 'The oracle would not answer again'));
    } finally {
      this.reasking.set(false);
    }
  }

  protected async resolve(decision: InterventionDecision): Promise<void> {
    this.busy.set(true);
    try {
      await resolveIntervention(this.core, this.chatId(), this.hold().id, decision);
      this.settled.emit();
    } catch (err) {
      this.toasts.showError(coreErrorMessage(err, 'Pascal could not settle that roll'));
      // A 409 (settled elsewhere) moves the dialog on too.
      this.settled.emit();
    } finally {
      this.busy.set(false);
    }
  }
}

// ---------------------------------------------------------------------------
// The host: the chat's held runs, oldest first
// ---------------------------------------------------------------------------

@Component({
  selector: 'qt-custom-tool-intervention-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [HeldRun],
  template: `
    @if (hold(); as current) {
      <!-- Keyed by hold id: a fresh hold starts with a clean slate of pins. -->
      @for (one of [current]; track one.id) {
        <qt-held-run [chatId]="chatId()" [hold]="one" [waiting]="waiting()" (settled)="onSettled()" />
      }
    }
  `,
})
export class CustomToolInterventionDialog {
  private readonly core = inject(CoreClient);
  private readonly realtime = inject(RealtimeService);
  private readonly queryClient = injectQueryClient();

  readonly chatId = input.required<string>();

  private readonly holdsQuery = injectQuery(() => ({
    queryKey: customToolsKeys.interventions(this.chatId()),
    queryFn: async (): Promise<{ holds: InterventionHoldView[] }> => {
      return await fetchInterventions(this.core, this.chatId());
    },
    refetchInterval: this.realtime.refetchInterval(FALLBACK_POLL_MS),
  }));

  private readonly holds = computed(() => this.holdsQuery.data()?.holds ?? []);
  protected readonly hold = computed(() => this.holds()[0] ?? null);
  protected readonly waiting = computed(() => this.holds().length - 1);

  protected onSettled(): void {
    void this.queryClient.invalidateQueries({ queryKey: customToolsKeys.interventions(this.chatId()) });
  }
}
