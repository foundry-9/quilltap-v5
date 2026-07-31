import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';

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
} from '../../core/core-contract';
import { displayTitle, isStateRef } from '../../pascal/custom-tool-types';
import { definitionFromDraft, gateFromConditions, type ToolDraft } from '../../pascal/tool-draft';
import { evaluateToolGate, type ToolGateVerdict } from '../../pascal/tool-gate';
import { Icon } from '../../ui/icon';
import * as api from './workbench.api';

/**
 * ProvingBench — the Workbench's right-hand panel: single dry-run rolls, the
 * fact sheet, the Monte Carlo table audit, and the live JSON preview (v4
 * `components/custom-tools/ProvingBench.tsx`, 397).
 *
 * Rolls and audits execute SERVER-SIDE through the same `executeCustomTool`
 * core a live chat uses, so the bench can never drift from what the table would
 * actually deal. The bench posts nothing and writes nothing.
 */

/** A character eligible to lend the bench its fact sheet. */
interface BenchCharacter {
  id: string;
  name: string;
}

/** The fact sheet, in one of its two modes (§4.5 card 2). */
type FactSheet = { mode: 'character'; characterId: string } | { mode: 'manual'; text: string };

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
  imports: [Icon, CustomToolParamsForm],
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
            (change)="onParamChange($event.param, $event.value)"
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
            <p>🎲 <strong>{{ bubbleTitle() }}</strong> — {{ entry.message }}</p>
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
          </div>
        }
      </section>

      <!-- Card 2 — The fact sheet -->
      <section class="qt-card p-3 space-y-2">
        <h3 class="qt-card-title text-sm">The fact sheet</h3>
        <p class="qt-hint">
          Metadata tests read the invoking character&rsquo;s <code>metadata.json</code>. Lend the
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
            [value]="characterId()"
            (change)="sheet.set({ mode: 'character', characterId: selectValue($event) })"
            class="qt-select qt-select-sm w-full"
            aria-label="Character whose fact sheet to use"
          >
            <option value="">— nobody in particular —</option>
            @for (character of characters(); track character.id) {
              <option [value]="character.id">{{ character.name }}</option>
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
                <div class="h-1.5 rounded qt-bg-muted overflow-hidden">
                  <div
                    class="h-full rounded"
                    [style.width.%]="barWidth(entry.share, entry.hits)"
                    [style.background-color]="barColor(entry.index)"
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

  readonly values = signal<ParameterFormValues>({});
  readonly isPrivate = signal(false);
  readonly sheet = signal<FactSheet>({ mode: 'manual', text: '{}' });
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

  readonly characterId = computed(() => {
    const s = this.sheet();
    return s.mode === 'character' ? s.characterId : '';
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
      /\{\{\s*state\./.test(d.outcomes.map((o) => o.message).join('\n'))
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
      return evaluateToolGate(gate, this.benchMetadata() as Record<string, unknown>);
    }
    return this.rolls()[0]?.gate ?? null;
  });

  /** The hint that fires when a tool tests metadata but no sheet was lent. */
  readonly noSheetHint = computed(() => {
    if (!this.testsMetadata()) return false;
    const s = this.sheet();
    if (s.mode === 'manual') return s.text.trim().replace(/\s/g, '') === '{}';
    return !s.characterId;
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
    this.sheet.set({ mode: 'character', characterId: '' });
    if (this._characters().length > 0) return;
    try {
      const destinations = await api.fetchDestinations(this.core);
      this._characters.set(
        (destinations.characters ?? []).map((c) => ({ id: c.characterId, name: c.characterName })),
      );
    } catch {
      // A failed roster read leaves "nobody in particular" as the only option;
      // it is not an error the bench needs to shout about.
      this._characters.set([]);
    }
  }

  /**
   * The bench sends either fact-sheet form and NEVER resolves the sheet itself
   * (§W1): a plain object passes through verbatim, `{characterId}` hydrates
   * server-side.
   */
  benchMetadata(): CustomToolMetadataInput | undefined {
    const s = this.sheet();
    if (s.mode === 'character') return s.characterId ? { characterId: s.characterId } : undefined;
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
      this.rolls.update((prev) => [result, ...prev].slice(0, 10));
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
   */
  protected barColor(index: number): string {
    return `var(--qt-alert-${stateToken(this.draft().outcomes[index]?.state)}-border)`;
  }

  protected sheetSuffix(roll: CustomToolRunResult): string {
    if (!roll.metadataTested) return '';
    const pairs = Object.entries(roll.metadataTested)
      .map(([key, value]) => `${key}=${JSON.stringify(value)}`)
      .join(', ');
    return ` · sheet: ${pairs}`;
  }
}
