import {
  ChangeDetectionStrategy,
  Component,
  computed,
  DestroyRef,
  effect,
  inject,
  input,
  linkedSignal,
  output,
  signal,
  untracked,
} from '@angular/core';

import {
  CustomToolParamsForm,
  coerceParamValues,
  initialParamValues,
  type ParameterFormValues,
} from '../../chat/custom-tool-params-form';
import { CoreClient, coreErrorMessage } from '../../core/core-client';
import type {
  CustomToolAuditResult,
  CustomToolMetadataInput,
  CustomToolParameterSpec,
  CustomToolRunResult,
  PreviewWardrobeResolution,
} from '../../core/core-contract';
import { displayTitle, isStateRef } from '../../pascal/custom-tool-types';
import { scanPlaceholders } from '../../pascal/placeholders';
import {
  definitionFromDraft,
  gateFromConditions,
  type ToolDraft,
  type WardrobeVantage,
} from '../../pascal/tool-draft';
import { flattenProgressions, type ProgressPrimitive } from '../../progressions/engine';
import { evaluateToolGate, type ToolGateVerdict } from '../../pascal/tool-gate';
import { Icon } from '../../ui/icon';
import { formatSlotLabels } from '../../wardrobe/slot-meta';
import { wardrobeOriginLabel } from '../../wardrobe/wardrobe-container';
import { WardrobeItemThumbnail } from '../../wardrobe/wardrobe-item-thumbnail';
import * as api from './workbench.api';
import {
  isApplicableWardrobeEffect,
  isApplicableWriteEffect,
  type BenchRoll,
  type DryEffect,
  type ResolvedWardrobeGesture,
} from './workbench.api';

/**
 * ProvingBench — the Workbench's right-hand panel: single dry-run rolls, the
 * fact sheet, the Monte Carlo table audit, and the live JSON preview (v4
 * `components/custom-tools/ProvingBench.tsx`, 397).
 *
 * Rolls and audits execute SERVER-SIDE through the same `executeCustomTool`
 * core a live chat uses, so the bench can never drift from what the table would
 * actually deal. The bench posts nothing and writes nothing.
 *
 * The fact sheet's *Pick a character* mode reads and writes the editor's
 * shared wardrobe vantage (v4 `5cb0e0190`): a character chosen here is the one
 * the side-effects card browses, and a character vantage chosen there seats
 * them here. Without a bound vantage the bench holds its own.
 */

/** A character eligible to lend the bench its fact sheet. */
interface BenchCharacter {
  id: string;
  name: string;
}

/**
 * The fact sheet, in one of its two modes (§4.5 card 2). The character mode's
 * character is the shared wardrobe vantage's, not the sheet's own (v4
 * `5cb0e0190`).
 */
type FactSheet = { mode: 'character' } | { mode: 'manual'; text: string };

/** One wardrobe line under a bench roll, as the template draws it. */
export type BenchWardrobeLine =
  | { kind: 'attempt'; text: string }
  | { kind: 'apply'; verb: string; imageFileId: string | null; text: string }
  | { kind: 'skip'; text: string };

/**
 * The bench's oracle: script an answer, script a silence, or — for single rolls
 * only — spend a real consult. The audit never goes live: ten thousand hands
 * must not mean ten thousand LLM calls.
 */
type BenchOracle = { mode: 'scripted'; answer: string } | { mode: 'fail' } | { mode: 'live' };

export function formatShort(value: number): string {
  if (Number.isInteger(value)) return String(value);
  return String(Number(value.toPrecision(4)));
}

/** Map an outcome state to its `--qt-alert-*` token family. */
export function stateToken(state: string | undefined): 'success' | 'warning' | 'error' | 'info' {
  switch (state) {
    case 'success':
      return 'success';
    case 'partial':
      return 'warning';
    case 'failure':
      return 'error';
    default:
      return 'info';
  }
}

/**
 * The bench renders the SERVER's error string verbatim. A definition the schema
 * rejects comes back as v4's `formatDefinitionIssues` sentence; a missing
 * character as the 404's; a broken vault or a run failure as the 422's. Nothing
 * here re-derives any of them.
 */
export function extractErrorMessage(err: unknown): string {
  return coreErrorMessage(err, 'The bench could not oblige.');
}

@Component({
  selector: 'qt-proving-bench',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, CustomToolParamsForm, WardrobeItemThumbnail],
  template: `
    <div class="space-y-3">
      <!-- Card 1 — Test roll -->
      <section class="qt-card p-3 space-y-2">
        <h3 class="qt-card-title text-sm">The proving bench</h3>
        <div class="space-y-2">
          <qt-custom-tool-params-form
            [parameters]="paramSpecs()"
            [values]="effectiveValues()"
            [disabled]="rolling()"
            idPrefix="bench"
            (paramChange)="onParamChange($event.param, $event.value)"
          />
          <label class="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              class="qt-checkbox"
              [checked]="isPrivate()"
              (change)="isPrivate.set(checkedValue($event))"
            />
            Roll privately
          </label>
          <button
            type="button"
            class="qt-button qt-button-primary qt-button-sm w-full"
            (click)="roll()"
            [disabled]="benchDisabled() || rolling()"
            [attr.title]="
              benchDisabled() ? 'The draft must be valid before the bench will deal' : null
            "
          >
            <qt-icon name="play" class="w-3.5 h-3.5" />
            {{ rolling() ? 'Rolling…' : 'Roll' }}
          </button>
          @if (rollError(); as message) {
            <p class="text-xs qt-text-destructive">{{ message }}</p>
          }
        </div>

        @for (entry of rolls(); track $index; let i = $index) {
          <div
            class="qt-pascal-result text-sm py-1"
            [class]="'qt-pascal-result--' + entry.state"
            [class.opacity-60]="i > 0"
            [class.qt-chat-message-whisper]="entry.visibility === 'whisper'"
          >
            <p>🎲 <strong>{{ bubbleHeading(entry) }}</strong></p>
            <p>{{ entry.message }}</p>
            @if (entry.diceBreakdown) {
              <p class="text-xs qt-text-secondary font-mono">{{ entry.diceBreakdown }}</p>
            }
            @if (entry.visibility === 'whisper') {
              <p class="text-xs qt-text-secondary italic">whispered</p>
            }
            <!-- The debug line the real Salon bubble never shows. -->
            <p class="text-xs qt-text-secondary font-mono">
              raw {{ short(entry.raw) }} → value {{ short(entry.value) }} · matched
              {{ rowLabel(entry.outcomeIndex) }} ({{ entry.state }}){{ sheetSuffix(entry) }}
            </p>
            @if (entry.llm; as llm) {
              <p class="text-xs qt-text-secondary font-mono">
                consult {{ llm.ok ? 'answered' : failedSuffix(llm) }}: {{ jsonString(llm.output) }}
              </p>
            }
            @if (entry.effects && entry.effects.length > 0) {
              <div class="text-xs qt-text-secondary font-mono">
                @for (effect of entry.effects; track effect.index) {
                  @if (isWrite(effect)) {
                    <p>
                      → {{ effectTarget(effect) }} = {{ effectValue(effect) }}
                      <em>(would write)</em>
                    </p>
                  } @else if (isWardrobe(effect)) {
                    @for (line of wardrobeLines(entry, effect); track $index) {
                      @switch (line.kind) {
                        @case ('apply') {
                          <p>
                            → wardrobe: {{ line.verb }} @if (applyImage(line); as fileId) {<span
                                class="inline-block align-middle mr-1"
                                ><qt-wardrobe-item-thumbnail [fileId]="fileId" [size]="28"
                              /></span>}{{ line.text }} <em>(would apply)</em>
                          </p>
                        }
                        @case ('skip') {
                          <p>→ wardrobe: {{ line.text }} <em>(would skip)</em></p>
                        }
                        @default {
                          <p>
                            → wardrobe: {{ line.text }}
                            <em>(would attempt)</em>
                          </p>
                        }
                      }
                    }
                  } @else {
                    <p>· effect {{ effect.index + 1 }} skipped: {{ effectSkipped(effect) }}</p>
                  }
                }
                <p class="italic">The bench computes effects; it never applies them.</p>
              </div>
            }
          </div>
        }
      </section>

      <!-- Card 2 — The fact sheet -->
      <section class="qt-card p-3 space-y-2">
        <h3 class="qt-card-title text-sm">The fact sheet</h3>
        <p class="qt-hint">
          Metadata tests read the invoking character&rsquo;s <code>metadata.json</code>, and
          <code>progress</code> tests read the <code>progressions</code> key inside it. Lend the
          bench a sheet, or it rolls as nobody in particular.
        </p>
        <div
          class="flex rounded overflow-hidden border w-fit"
          role="radiogroup"
          aria-label="Fact sheet mode"
        >
          <button
            type="button"
            role="radio"
            [attr.aria-checked]="sheet().mode === 'character'"
            [class]="
              sheet().mode === 'character'
                ? 'px-2 py-1 text-xs qt-button qt-button-primary'
                : 'px-2 py-1 text-xs qt-button qt-button-ghost'
            "
            (click)="pickCharacterMode()"
          >
            Pick a character
          </button>
          <button
            type="button"
            role="radio"
            [attr.aria-checked]="sheet().mode === 'manual'"
            [class]="
              sheet().mode === 'manual'
                ? 'px-2 py-1 text-xs qt-button qt-button-primary'
                : 'px-2 py-1 text-xs qt-button qt-button-ghost'
            "
            (click)="sheet.set({ mode: 'manual', text: '{}' })"
          >
            Hand-typed sheet
          </button>
        </div>

        @if (sheet().mode === 'character') {
          <select
            [value]="chairCharacterId()"
            (change)="onChairChange(selectValue($event))"
            class="qt-select qt-select-sm w-full"
            aria-label="Character whose fact sheet to use"
          >
            <option value="" [selected]="chairCharacterId() === ''">— nobody in particular —</option>
            @for (character of characters(); track character.id) {
              <option [value]="character.id" [selected]="character.id === chairCharacterId()">{{
                character.name
              }}</option>
            }
          </select>
        } @else {
          <div>
            <textarea
              [value]="manualText()"
              (input)="sheet.set({ mode: 'manual', text: inputValue($event) })"
              rows="3"
              class="qt-textarea w-full font-mono text-xs"
              [class.qt-input-error]="manualSheetError() !== null"
              aria-label="Hand-typed fact sheet (JSON object)"
              spellcheck="false"
            ></textarea>
            @if (manualSheetError(); as message) {
              <p class="text-xs qt-text-destructive">{{ message }}</p>
            }
          </div>
        }

        @if (noSheetHint()) {
          <p class="text-xs qt-text-secondary">
            No fact sheet supplied — metadata tests will all decline, exactly as for an unattributed
            manual roll.
          </p>
        }

        @if (derivedProgress(); as derived) {
          <div class="space-y-1">
            <p class="text-xs qt-text-secondary">
              Progressions derived from this sheet, as of now — what a <code>progress</code> test or
              <code>{{ PROGRESS_PLACEHOLDER }}</code> would read this second:
            </p>
            <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 font-mono text-xs">
              @for (row of derived; track row.key) {
                <dt class="qt-text-secondary">{{ row.key }}</dt>
                <dd class="qt-text truncate">{{ row.value }}</dd>
              }
            </dl>
          </div>
        }

        @if (draftGate()) {
          @if (gateVerdict(); as verdict) {
            @if (verdict.available) {
              <p class="text-xs qt-text-secondary">✓ This sheet would be offered the tool.</p>
            } @else {
              <p class="text-xs qt-text-destructive">
                ✕ This sheet would never be offered the tool —
                {{
                  verdict.withheldBy === 'availableWhen'
                    ? 'it does not pass every “only show if” test.'
                    : 'it passes the “do not show if” test.'
                }}
                The roll below is the bench indulging you.
              </p>
            }
          } @else {
            <p class="text-xs qt-text-secondary">
              This tool is gated. Roll once to learn whether this character would be offered it.
            </p>
          }
        }
      </section>

      <!-- Card 2¾ — Mock state, for $state references -->
      <section class="qt-card p-3 space-y-2">
        <h3 class="qt-card-title text-sm">Mock state</h3>
        <p class="qt-hint">
          The merged state (chat → project → group → general) a roll resolves
          <code>$state</code> references against. A path that is absent or the wrong type falls back
          to the reference&rsquo;s own fallback.
        </p>
        <textarea
          [value]="mockStateText()"
          (input)="mockStateText.set(inputValue($event))"
          rows="3"
          class="qt-textarea w-full font-mono text-xs"
          [class.qt-input-error]="mockStateError() !== null"
          aria-label="Mock merged state (JSON object)"
          spellcheck="false"
        ></textarea>
        @if (mockStateError(); as message) {
          <p class="text-xs qt-text-destructive">{{ message }}</p>
        }
        @if (noMockStateHint()) {
          <p class="text-xs qt-text-secondary">
            No mock state supplied — every <code>$state</code> reference will use its fallback.
          </p>
        }
      </section>

      <!-- Card 2½ — The oracle (only when the tool consults one) -->
      @if (draft().llmEnabled) {
        <section class="qt-card p-3 space-y-2">
          <h3 class="qt-card-title text-sm">The oracle</h3>
          <p class="qt-hint">
            This tool consults a model mid-run. Script its answer here, or spend a real consult on a
            single roll.
          </p>
          <div
            class="flex rounded overflow-hidden border w-fit"
            role="radiogroup"
            aria-label="Oracle mode"
          >
            <button
              type="button"
              role="radio"
              [attr.aria-checked]="oracle().mode === 'scripted'"
              [class]="
                oracle().mode === 'scripted'
                  ? 'px-2 py-1 text-xs qt-button qt-button-primary'
                  : 'px-2 py-1 text-xs qt-button qt-button-ghost'
              "
              (click)="pickScripted()"
            >
              Scripted answer
            </button>
            <button
              type="button"
              role="radio"
              [attr.aria-checked]="oracle().mode === 'fail'"
              [class]="
                oracle().mode === 'fail'
                  ? 'px-2 py-1 text-xs qt-button qt-button-primary'
                  : 'px-2 py-1 text-xs qt-button qt-button-ghost'
              "
              (click)="oracle.set({ mode: 'fail' })"
              title="The consult fails; the run shows your error line"
            >
              Silence
            </button>
            <button
              type="button"
              role="radio"
              [attr.aria-checked]="oracle().mode === 'live'"
              [class]="
                oracle().mode === 'live'
                  ? 'px-2 py-1 text-xs qt-button qt-button-primary'
                  : 'px-2 py-1 text-xs qt-button qt-button-ghost'
              "
              (click)="oracle.set({ mode: 'live' })"
              title="A single roll asks the real model — one paid consult per roll"
            >
              Ask it live
            </button>
          </div>

          @if (oracle().mode === 'scripted') {
            <div>
              <textarea
                [value]="scriptedAnswer()"
                (input)="oracle.set({ mode: 'scripted', answer: inputValue($event) })"
                rows="2"
                class="qt-textarea w-full text-xs"
                placeholder="What shall the oracle be made to say?"
                aria-label="Scripted oracle answer"
              ></textarea>
              @if (scriptedAnswer().trim() === '') {
                <p class="text-xs qt-text-secondary">
                  Empty — the bench treats this as silence, and the run shows your error line.
                </p>
              }
            </div>
          }
          @if (oracle().mode === 'live') {
            <p class="text-xs qt-text-secondary">
              Each roll pays for one real consult through the cheap-model machinery. The audit never
              goes live — it deals its hands against the scripted answer, or silence.
            </p>
          }
        </section>
      }

      <!-- Card 3 — Table audit -->
      <section class="qt-card p-3 space-y-2">
        <h3 class="qt-card-title text-sm">The audit</h3>
        <button
          type="button"
          class="qt-button qt-button-secondary qt-button-sm w-full"
          (click)="runAudit()"
          [disabled]="benchDisabled() || auditing()"
          [attr.title]="
            benchDisabled() ? 'The draft must be valid before the house will audit it' : null
          "
        >
          {{ auditing() ? 'Dealing…' : 'Deal a thousand hands' }}
        </button>
        @if (auditError(); as message) {
          <p class="text-xs qt-text-destructive">{{ message }}</p>
        }

        @if (audit(); as result) {
          <div class="space-y-1">
            <p class="text-xs qt-text-secondary">
              {{ result.runs.toLocaleString() }} draws · values {{ short(result.valueMin) }}–{{
                short(result.valueMax)
              }}
              · mean {{ short(result.valueMean) }}
            </p>
            @for (entry of result.outcomes; track entry.index) {
              <div class="space-y-0.5">
                <div class="flex justify-between text-xs">
                  <span>
                    {{ auditLabel(entry.index) }}
                    <span class="qt-text-secondary">({{ outcomeState(entry.index) }})</span>
                  </span>
                  <span>{{ (entry.share * 100).toFixed(1) }}%</span>
                </div>
                <div class="qt-progress qt-progress-sm">
                  <div
                    class="qt-progress-fill"
                    [style.width.%]="barWidth(entry.share, entry.hits)"
                    [style.--qt-progress-indicator]="barColor(entry.index)"
                  ></div>
                </div>
                @if (entry.hits === 0 && !isCatchAll(entry.index)) {
                  <p class="text-xs qt-text-secondary">
                    This outcome never fired in {{ result.runs.toLocaleString() }} draws
                    <em>with these parameters and this fact sheet</em> — it may be unreachable,
                    reachable only with other parameter values, or gated on metadata this sheet
                    doesn&rsquo;t carry.
                  </p>
                }
              </div>
            }
          </div>
        }
      </section>

      <!-- Card 4 — JSON preview -->
      <section class="qt-card p-3 space-y-2">
        <h3 class="qt-card-title text-sm">The exact bytes</h3>
        <p class="qt-hint">What Save would write — the form&rsquo;s teaching surface.</p>
        <pre
          class="text-xs font-mono qt-bg-muted rounded p-2 overflow-x-auto max-h-64 overflow-y-auto whitespace-pre"
          >{{ jsonPreview() }}</pre
        >
      </section>
    </div>
  `,
})
export class ProvingBench {
  private readonly core = inject(CoreClient);

  readonly draft = input.required<ToolDraft>();
  /** False while the draft has blocking errors — Roll/Audit disable with a hint. */
  readonly valid = input.required<boolean>();

  /** Reports which outcome row a bench roll landed on, for the form's flash. */
  readonly matched = output<string | null>();
  /** The editor's shared wardrobe vantage; the bench holds its own when unbound. */
  readonly vantage = input<WardrobeVantage | undefined>(undefined);
  readonly vantageChange = output<WardrobeVantage>();

  private readonly localVantage = signal<WardrobeVantage>({ kind: 'none' });
  private readonly effectiveVantage = computed(() => this.vantage() ?? this.localVantage());
  /** The character in the chair — the vantage's, or nobody. */
  readonly chairCharacterId = computed(() => {
    const vantage = this.effectiveVantage();
    return vantage.kind === 'character' ? vantage.characterId : '';
  });

  readonly values = signal<ParameterFormValues>({});
  readonly isPrivate = signal(false);
  /**
   * The sheet opens in character mode when a chair exists, and a character
   * vantage chosen elsewhere (the side-effects card, or the editor's opening
   * default) seats that character here — latched on the character id, as v4
   * derives it during render. A `linkedSignal`, not an effect: an effect's write
   * would land a render late under zoneless change detection (P4.D320 item 12).
   */
  readonly sheet = linkedSignal<string, FactSheet>({
    source: this.chairCharacterId,
    computation: (chairId, previous) => {
      if (!previous) return chairId ? { mode: 'character' } : { mode: 'manual', text: '{}' };
      return chairId && previous.value.mode !== 'character' ? { mode: 'character' } : previous.value;
    },
  });
  readonly mockStateText = signal('{}');
  readonly oracle = signal<BenchOracle>({ mode: 'scripted', answer: '' });
  readonly rolls = signal<api.BenchRoll[]>([]);
  readonly audit = signal<CustomToolAuditResult | null>(null);

  readonly rolling = signal(false);
  readonly auditing = signal(false);
  readonly rollError = signal<string | null>(null);
  readonly auditError = signal<string | null>(null);
  private readonly _characters = signal<BenchCharacter[]>([]);

  readonly characters = this._characters.asReadonly();

  /**
   * The clock the derived-progressions panel reads (v4's own comment):
   *
   * > Held in state and advanced by an interval rather than read during
   * > render: `Date.now()` in a render body is impure […]. The interval runs
   * > only while a hand-typed sheet is in play, so a bench with no
   * > progressions on it costs nothing.
   *
   * v4 gates it in an effect keyed on `sheet.mode`; the same gate here is an
   * Angular `effect` that starts and stops the timer as the mode flips, and
   * `DestroyRef` clears whatever is running when the panel goes away.
   */
  private readonly benchNowMs = signal(Date.now());

  /** The placeholder the derived panel names. A constant, not template text:
   * Angular interpolates a literal `{{…}}` on sight. */
  protected readonly PROGRESS_PLACEHOLDER = '{{progress.…}}';

  constructor() {
    let timer: ReturnType<typeof setInterval> | null = null;
    const stop = () => {
      if (timer !== null) clearInterval(timer);
      timer = null;
    };
    effect(() => {
      stop();
      if (this.sheet().mode !== 'manual') return;
      // The interval alone, with no priming call: the first tick lands within a
      // second, and setting a signal straight from an effect body is the render
      // loop nobody wants.
      timer = setInterval(() => this.benchNowMs.set(Date.now()), 1000);
    });
    inject(DestroyRef).onDestroy(stop);

    // The roster the character select offers, read once the mode is in play —
    // including a sheet that OPENED in character mode on a seated chair.
    effect(() => {
      if (this.sheet().mode !== 'character') return;
      untracked(() => void this.loadRoster());
    });
  }

  protected readonly short = formatShort;

  readonly paramSpecs = computed(() => {
    const specs: Record<string, CustomToolParameterSpec> = {};
    for (const param of this.draft().parameters) {
      if (!param.name) continue;
      const numeric = param.type === 'number' || param.type === 'integer';
      specs[param.name] = {
        type: param.type,
        default:
          param.type === 'boolean'
            ? Boolean(param.defaultValue)
            : numeric
              ? Number(param.defaultValue) || 0
              : String(param.defaultValue),
        description: param.description || undefined,
        min: numeric && param.min.trim() !== '' ? Number(param.min) : undefined,
        max: numeric && param.max.trim() !== '' ? Number(param.max) : undefined,
      };
    }
    return specs;
  });

  /**
   * Merge declared defaults UNDER whatever the user has touched, so a fresh
   * parameter shows up pre-filled without wiping edits to the others.
   */
  readonly effectiveValues = computed(() => ({
    ...initialParamValues(this.paramSpecs()),
    ...this.values(),
  }));

  readonly testsMetadata = computed(() =>
    this.draft().outcomes.some((o) => o.conditions.some((c) => c.subject.kind === 'metadata')),
  );

  readonly manualText = computed(() => {
    const s = this.sheet();
    return s.mode === 'manual' ? s.text : '';
  });


  readonly manualSheetError = computed(() => {
    const s = this.sheet();
    if (s.mode !== 'manual') return null;
    try {
      const parsed: unknown = JSON.parse(s.text);
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        return 'The fact sheet must be a single JSON object.';
      }
      return null;
    } catch {
      return 'The fact sheet is not valid JSON.';
    }
  });

  /** Whether the draft references `$state` anywhere (v4 `ProvingBench` :137). */
  readonly testsState = computed(() => {
    const d = this.draft();
    return (
      d.outcomes.some((o) => o.conditions.some((c) => c.operand.kind === 'state')) ||
      (d.rollForm === 'range' &&
        (['min', 'max', 'multiplier', 'offset'] as const).some(
          (f) => d.rollRange[f].kind === 'state',
        )) ||
      d.parameters.some((p) => isStateRef(p.defaultValue)) ||
      // v4 `0506517d3` (ProvingBench.tsx:168): the classifier, not a regex probe —
      // a bare `{{state.}}` or an unterminated `{{state.foo` no longer counts.
      d.outcomes.some((o) => scanPlaceholders(o.message).some((p) => p.ref.kind === 'state'))
    );
  });

  readonly mockStateError = computed(() => {
    try {
      const parsed: unknown = JSON.parse(this.mockStateText());
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        return 'Mock state must be a single JSON object.';
      }
      return null;
    } catch {
      return 'Mock state is not valid JSON.';
    }
  });

  /** The note that fires when a tool tests `$state` but the mock is empty. */
  readonly noMockStateHint = computed(
    () => this.testsState() && this.mockStateText().trim().replace(/\s/g, '') === '{}',
  );

  readonly benchDisabled = computed(
    () =>
      !this.valid() ||
      (this.sheet().mode === 'manual' && this.manualSheetError() !== null) ||
      this.mockStateError() !== null,
  );

  readonly jsonPreview = computed(
    () => `${JSON.stringify(definitionFromDraft(this.draft()), null, 2)}\n`,
  );

  readonly bubbleTitle = computed(() => {
    const d = this.draft();
    return displayTitle({ name: d.name || 'contrivance', title: d.title || undefined });
  });

  /**
   * The draft's availability gate, in the shape {@link evaluateToolGate} takes.
   * Null while the tool is ungated or the gate is still half-typed — the form
   * is already complaining about the latter, and the bench need not join in.
   */
  readonly draftGate = computed(() => {
    const d = this.draft();
    if (d.gateMode === 'none') return null;
    const gate = gateFromConditions(d.gateConditions);
    if (!gate) return null;
    return d.gateMode === 'available' ? { availableWhen: gate } : { withheldWhen: gate };
  });

  /**
   * Whether this sheet would be dealt the tool at all — the one thing a roll can
   * never show, since the bench deals to whoever asks.
   *
   * A hand-typed sheet is right here, so the verdict is live. A character's real
   * sheet lives on the server, so that one comes back with a roll — the bench
   * never guesses at a vault.
   */
  readonly gateVerdict = computed<ToolGateVerdict | null>(() => {
    const gate = this.draftGate();
    if (!gate) return null;
    if (this.sheet().mode === 'manual' && this.manualSheetError() === null) {
      return evaluateToolGate(
        gate,
        this.benchMetadata() as Record<string, unknown>,
        this.progressSheet(),
      );
    }
    return this.rolls()[0]?.gate ?? null;
  });

  /**
   * The progress sheet a hand-typed fact sheet derives to, at THIS instant —
   * v4's own comment:
   *
   * > Shown read-only under the sheet so the author can see `cannon.percent`
   * > before rolling, rather than working it out from two ISO timestamps in
   * > their head. A character-backed sheet lives on the server, so the bench
   * > declines to guess at it — the same line the gate verdict already draws.
   */
  private readonly progressSheet = computed<Record<string, ProgressPrimitive> | null>(() => {
    const s = this.sheet();
    if (s.mode !== 'manual') return null;
    try {
      const parsed: unknown = JSON.parse(s.text);
      const flattened = flattenProgressions(parsed, this.benchNowMs());
      return Object.keys(flattened).length > 0 ? flattened : null;
    } catch {
      return null;
    }
  });

  /** The same sheet as `[{key, value}]`, for the read-only list. */
  readonly derivedProgress = computed(() => {
    const sheet = this.progressSheet();
    if (!sheet) return null;
    return Object.entries(sheet).map(([key, value]) => ({ key, value: String(value) }));
  });

  /** The hint that fires when a tool tests metadata but no sheet was lent. */
  readonly noSheetHint = computed(() => {
    if (!this.testsMetadata()) return false;
    const s = this.sheet();
    if (s.mode === 'manual') return s.text.trim().replace(/\s/g, '') === '{}';
    return !this.chairCharacterId();
  });

  protected checkedValue(event: Event): boolean {
    return (event.target as HTMLInputElement).checked;
  }

  protected selectValue(event: Event): string {
    return (event.target as HTMLSelectElement).value;
  }

  protected inputValue(event: Event): string {
    return (event.target as HTMLTextAreaElement).value;
  }

  protected onParamChange(name: string, value: string | boolean): void {
    this.values.update((prev) => ({ ...prev, [name]: value }));
  }

  /**
   * The character list comes from the DESTINATIONS response — no separate roster
   * call, exactly as v4 (`ProvingBench:105-114`). Loaded only when the mode is
   * actually picked.
   */
  async pickCharacterMode(): Promise<void> {
    this.sheet.set({ mode: 'character' });
    await this.loadRoster();
  }

  private rosterRequested = false;

  private async loadRoster(): Promise<void> {
    if (this.rosterRequested || this._characters().length > 0) return;
    this.rosterRequested = true;
    try {
      const destinations = await api.fetchDestinations(this.core);
      this._characters.set(
        (destinations.characters ?? []).map((c) => ({ id: c.characterId, name: c.characterName })),
      );
    } catch {
      // A failed roster read leaves "nobody in particular" as the only option;
      // it is not an error the bench needs to shout about.
      this._characters.set([]);
      this.rosterRequested = false;
    }
  }

  /**
   * The select writes the VANTAGE, not the sheet: a character seats them (and
   * the side-effects card browses them); "nobody in particular" clears a
   * character vantage and leaves a shared one alone (v4 `:405-420`).
   */
  protected onChairChange(characterId: string): void {
    if (characterId) this.setVantage({ kind: 'character', characterId });
    else if (this.effectiveVantage().kind === 'character') this.setVantage({ kind: 'none' });
  }

  private setVantage(next: WardrobeVantage): void {
    if (this.vantage() !== undefined) this.vantageChange.emit(next);
    else this.localVantage.set(next);
  }

  /**
   * The bench sends either fact-sheet form and NEVER resolves the sheet itself
   * (§W1): a plain object passes through verbatim, `{characterId}` hydrates
   * server-side.
   */
  benchMetadata(): CustomToolMetadataInput | undefined {
    const s = this.sheet();
    if (s.mode === 'character') {
      const chair = this.chairCharacterId();
      return chair ? { characterId: chair } : undefined;
    }
    try {
      const parsed: unknown = JSON.parse(s.text);
      return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
        ? (parsed as Record<string, unknown>)
        : undefined;
    } catch {
      return undefined;
    }
  }

  /** The mock merged state a bench roll or audit resolves `$state` against. */
  private benchState(): Record<string, unknown> | undefined {
    try {
      const parsed: unknown = JSON.parse(this.mockStateText());
      return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
        ? (parsed as Record<string, unknown>)
        : undefined;
    } catch {
      return undefined;
    }
  }

  /** The scripted answer's text, or '' in the other two modes. */
  readonly scriptedAnswer = computed(() => {
    const o = this.oracle();
    return o.mode === 'scripted' ? o.answer : '';
  });

  /** Switching TO scripted keeps an answer already typed, exactly as v4 does. */
  protected pickScripted(): void {
    const o = this.oracle();
    this.oracle.set({ mode: 'scripted', answer: o.mode === 'scripted' ? o.answer : '' });
  }

  /** The oracle input for a single roll; undefined when the tool has no consult. */
  private previewOracle(): api.PreviewOracle | undefined {
    if (!this.draft().llmEnabled) return undefined;
    const o = this.oracle();
    if (o.mode === 'live') return { live: true };
    if (o.mode === 'scripted' && o.answer.trim() !== '') return { output: o.answer };
    return { fail: true };
  }

  /**
   * The audit's oracle: the scripted answer, else silence. NEVER live — the
   * return type has no such arm, which is the guard.
   */
  private auditOracle(): api.AuditOracle | undefined {
    if (!this.draft().llmEnabled) return undefined;
    const o = this.oracle();
    if (o.mode === 'scripted' && o.answer.trim() !== '') return { output: o.answer };
    return { fail: true };
  }

  async roll(): Promise<void> {
    this.rolling.set(true);
    this.rollError.set(null);
    try {
      const result = await api.previewCustomTool(this.core, {
        definition: definitionFromDraft(this.draft()),
        params: coerceParamValues(this.paramSpecs(), this.effectiveValues()),
        private: this.isPrivate(),
        metadata: this.benchMetadata(),
        state: this.benchState(),
        llm: this.previewOracle(),
      });
      // The chair's name as the roll was dealt, for "not in Imogen’s reach" —
      // from the bench's own roster (P4.D320 R-11). SPA-local, never on the wire.
      const chairName =
        this.sheet().mode === 'character'
          ? (this._characters().find((c) => c.id === this.chairCharacterId())?.name ?? null)
          : null;
      this.rolls.update((prev) => [{ ...result, chairName }, ...prev].slice(0, 10));
      this.matched.emit(this.draft().outcomes[result.outcomeIndex]?.id ?? null);
    } catch (err) {
      this.rollError.set(extractErrorMessage(err));
    } finally {
      this.rolling.set(false);
    }
  }

  async runAudit(): Promise<void> {
    this.auditing.set(true);
    this.auditError.set(null);
    try {
      this.audit.set(
        await api.auditCustomTool(this.core, {
          definition: definitionFromDraft(this.draft()),
          params: coerceParamValues(this.paramSpecs(), this.effectiveValues()),
          metadata: this.benchMetadata(),
          state: this.benchState(),
          llm: this.auditOracle(),
        }),
      );
    } catch (err) {
      this.auditError.set(extractErrorMessage(err));
    } finally {
      this.auditing.set(false);
    }
  }

  // -- Row labelling --------------------------------------------------------

  /** v4 `MiniPascalBubble` — the failure half of the consult debug line. */
  protected failedSuffix(llm: { reason?: string }): string {
    return `failed (${llm.reason ?? 'no reason recorded'})`;
  }

  /** The consult's answer, quoted the way v4's `JSON.stringify(output)` does. */
  protected jsonString(value: string): string {
    return JSON.stringify(value);
  }

  protected rowLabel(index: number): string {
    return this.draft().outcomes[index]?.catchAll ? 'the catch-all' : `row ${index + 1}`;
  }

  protected auditLabel(index: number): string {
    return this.draft().outcomes[index]?.catchAll ? 'otherwise' : `row ${index + 1}`;
  }

  protected outcomeState(index: number): string {
    return this.draft().outcomes[index]?.state ?? '?';
  }

  protected isCatchAll(index: number): boolean {
    return this.draft().outcomes[index]?.catchAll ?? false;
  }

  /** A bar that fired at all is never invisible — v4 floors it at 1%. */
  protected barWidth(share: number, hits: number): number {
    return Math.max(share * 100, hits > 0 ? 1 : 0);
  }

  /**
   * The same token family the Pascal bubble accents use, so the bars read in
   * every bundled theme.
   *
   * Since P4.38 this is bound to the bar family's own `--qt-progress-indicator`
   * rather than to `background-color` (v4 `ProvingBench.tsx:485-489`), so a theme
   * restyling `qt-progress` restyles these meters too. The value is unchanged.
   */
  protected barColor(index: number): string {
    return `var(--qt-alert-${stateToken(this.draft().outcomes[index]?.state)}-border)`;
  }

  /**
   * The bubble's heading: the run's rendered chip label when the definition
   * declares one, else the display title. v4 `MiniPascalBubble` — and the
   * heading now stands as its own paragraph over the message's own block, so
   * an outcome opening with a list, a heading, a quote, or a fence renders
   * correctly instead of being welded onto the end of a sentence.
   */
  protected bubbleHeading(roll: BenchRoll): string {
    return roll.chipLabel?.trim() || this.bubbleTitle();
  }

  protected isWrite(effect: DryEffect): boolean {
    return isApplicableWriteEffect(effect);
  }

  protected isWardrobe(effect: DryEffect): boolean {
    return isApplicableWardrobeEffect(effect);
  }

  /** The target as the author wrote it — the raw text, not the parsed shape. */
  protected effectTarget(effect: DryEffect): string {
    return isApplicableWriteEffect(effect) ? effect.target.raw : '';
  }

  /** v4 renders the resolved value through `JSON.stringify`, quotes and all. */
  protected effectValue(effect: DryEffect): string {
    return isApplicableWriteEffect(effect) ? JSON.stringify(effect.value) : '';
  }

  /** The wardrobe lines one dry gesture prints under a roll (v4 `BenchWardrobeLines`). */
  protected wardrobeLines(roll: BenchRoll, effect: DryEffect): BenchWardrobeLine[] {
    if (!isApplicableWardrobeEffect(effect)) return [];
    return benchWardrobeLines(
      effect.wardrobe,
      roll.wardrobeResolution?.find((entry) => entry.index === effect.index),
      roll.chairName ?? null,
    );
  }

  protected applyImage(line: BenchWardrobeLine): string | null {
    return line.kind === 'apply' ? line.imageFileId : null;
  }

  protected effectSkipped(effect: DryEffect): string {
    return 'skipped' in effect ? effect.skipped : '';
  }

  protected sheetSuffix(roll: CustomToolRunResult): string {
    if (!roll.metadataTested) return '';
    const pairs = Object.entries(roll.metadataTested)
      .map(([key, value]) => `${key}=${JSON.stringify(value)}`)
      .join(', ');
    return ` · sheet: ${pairs}`;
  }
}

/**
 * The gesture's verb and mode, in the bench's words: "wear (add to slot top)",
 * "take off … from top" (v4 `5cb0e0190`, `ProvingBench.tsx:750-765`).
 */
export function gestureVerb(gesture: ResolvedWardrobeGesture): { verb: string; suffix: string } {
  switch (gesture.action) {
    case 'wear':
      return {
        verb: 'wear',
        suffix:
          gesture.mode && gesture.mode !== 'wear'
            ? ` (${gesture.mode.replace(/_/g, ' ')}${gesture.slot ? ` ${gesture.slot}` : ''})`
            : '',
      };
    case 'take_off':
      return { verb: 'take off', suffix: gesture.slot ? ` from ${gesture.slot}` : '' };
    case 'clear_slot':
      return { verb: 'clear', suffix: '' };
  }
}

/**
 * A dry-run gesture, in the bench's words. "Would attempt" when nobody sits in
 * the chair — the bench cannot know a garment exists without a wardrobe to
 * look in. With a character in the chair the server resolves each reference
 * through the applier's own lookup, so each item says what it would have
 * found ("would apply") or why it would be skipped ("would skip"). The item
 * list is built HERE, curly quotes (U+201C / U+201D) and all, never
 * interpolated in a template.
 */
export function describeBenchGesture(gesture: ResolvedWardrobeGesture): string {
  const items = gesture.items.map((item) => `\u201c${item}\u201d`).join(', ');
  const { verb, suffix } = gestureVerb(gesture);
  if (gesture.action === 'clear_slot') return `clear ${gesture.slot ?? 'a slot'}`;
  return `${verb} ${items}${suffix}`;
}

/**
 * v4 `BenchWardrobeLines` (`ProvingBench.tsx:781-823`): no resolution for the
 * gesture (or a `clear_slot`) → the one "would attempt" line; else one line per
 * item — found: `<verb> <thumb><title> (<slots · origin>)<suffix>` "would
 * apply"; missed: `<verb> “<ref>”<suffix> — <why>` "would skip", where a
 * not-found reads "not in <chair>’s reach" and the other reasons print the
 * server's message verbatim.
 */
export function benchWardrobeLines(
  gesture: ResolvedWardrobeGesture,
  resolution: PreviewWardrobeResolution | undefined,
  chairName: string | null,
): BenchWardrobeLine[] {
  if (!resolution || gesture.action === 'clear_slot') {
    return [{ kind: 'attempt', text: describeBenchGesture(gesture) }];
  }
  const { verb, suffix } = gestureVerb(gesture);
  return resolution.items.map((entry): BenchWardrobeLine => {
    if (entry.found) {
      const origin = wardrobeOriginLabel(entry.found.origin);
      const slots = formatSlotLabels(entry.found.types);
      const detail = [slots, origin].filter(Boolean).join(' · ');
      return {
        kind: 'apply',
        verb,
        imageFileId: entry.found.imageFileId,
        text: `${entry.found.title}${detail ? ` (${detail})` : ''}${suffix}`,
      };
    }
    const why =
      entry.reason === 'not_found'
        ? `not in ${chairName ? `${chairName}\u2019s` : 'the chair\u2019s'} reach`
        : entry.message;
    return { kind: 'skip', text: `${verb} \u201c${entry.ref}\u201d${suffix} — ${why}` };
  });
}
