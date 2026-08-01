import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import {
  MAX_EFFECTS,
  MAX_EFFECT_TARGET_LENGTH,
  type OutcomeState,
} from '../../pascal/custom-tool-types';
import type { DraftEffect, DraftIssue, ToolDraft } from '../../pascal/tool-draft';
import { Icon } from '../../ui/icon';

/**
 * SideEffectsSection — the Workbench's "Side effects" card, between the
 * builder form and the outcome table: what a run writes back once the deal is
 * done (v4 `components/custom-tools/SideEffectsSection.tsx`, 252).
 *
 * The form authors the common shapes only: a When of "Always" or "on a given
 * outcome state", a target, and a value that is a literal number, a literal
 * boolean, or an expression. A hand-written richer condition (comparators,
 * metadata, the oracle) is carried verbatim behind a read-only badge — the
 * `$state`-default precedent: the builder round-trips what it cannot author.
 * Everything else arrives as issues from `validateDraft` and renders inline.
 */

/** The hint's `{{…}}` would close an Angular interpolation early — hence a constant. */
const EFFECT_VALUE_PLACEHOLDER = '{{state.encounter.count}} + 1';

const EFFECTS_HINT =
  'Effects apply in order, after the outcome is chosen. A JSON string value is always an ' +
  'expression — quote literal prose:';

const EFFECTS_HINT_TAIL =
  ', never bare words. Expressions take + − × ÷, parentheses, and the same {{…}} references as a ' +
  'message.';

let effectIdCounter = 0;

const OUTCOME_STATES: readonly OutcomeState[] = ['success', 'partial', 'failure', 'info'] as const;

/** The When select's flat values; 'verbatim' never appears as an option. */
type WhenChoice = 'always' | OutcomeState;

export function whenChoiceOf(effect: DraftEffect): WhenChoice | 'verbatim' {
  if (effect.when.kind === 'always') return 'always';
  if (effect.when.kind === 'outcome-state') return effect.when.state;
  return 'verbatim';
}

@Component({
  selector: 'qt-side-effects-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <section class="qt-card p-4 space-y-3">
      <div class="flex items-center justify-between">
        <h2 class="qt-card-title text-sm">Side effects</h2>
        <button
          type="button"
          class="qt-button qt-button-secondary qt-button-sm"
          (click)="addEffect()"
          [disabled]="disabled() || effects().length >= MAX_EFFECTS"
          [attr.title]="effects().length >= MAX_EFFECTS ? atMostTitle : null"
        >
          <qt-icon name="plus" class="w-3.5 h-3.5" />
          Add effect ({{ effects().length }}/{{ MAX_EFFECTS }})
        </button>
      </div>

      @if (effects().length === 0) {
        <p class="text-xs qt-text-secondary">
          None declared. An effect lets a roll record its consequences &mdash; tick a counter in the
          story&rsquo;s state, note a fact on the rolling character&rsquo;s sheet &mdash; applied
          server-side as part of the deal, beyond any model&rsquo;s power to fudge.
        </p>
      }

      @for (effect of effects(); track effect.id) {
        <div class="qt-card p-3 space-y-2">
          <div class="flex items-center gap-2 flex-wrap text-sm">
            <label class="flex items-center gap-2">
              When
              @if (whenChoice(effect) === 'verbatim') {
                <span
                  class="text-xs qt-text-secondary font-mono rounded qt-bg-muted px-2 py-1"
                  title="This condition is richer than the form authors. Edit it in the raw JSON."
                  >{{ verbatimWhen(effect) }}</span
                >
              } @else {
                <select
                  [value]="whenChoice(effect)"
                  (change)="onWhenChange(effect.id, selectValue($event))"
                  [disabled]="disabled()"
                  class="qt-select qt-select-sm"
                  aria-label="When this effect applies"
                >
                  <option value="always">every run</option>
                  @for (state of OUTCOME_STATES; track state) {
                    <option [value]="state">outcome is {{ state }}</option>
                  }
                </select>
              }
            </label>
            <button
              type="button"
              class="qt-button qt-button-ghost qt-button-sm ml-auto"
              (click)="removeEffect(effect.id)"
              [disabled]="disabled()"
              title="Delete this effect"
            >
              <qt-icon name="trash" class="w-4 h-4" />
            </button>
          </div>

          <div class="flex items-center gap-2 flex-wrap text-sm">
            <label class="flex items-center gap-2 flex-1 min-w-56">
              Target
              <input
                type="text"
                [value]="effect.target"
                [maxLength]="MAX_EFFECT_TARGET_LENGTH"
                (input)="updateEffect(effect.id, { target: inputValue($event) })"
                placeholder="state.encounter.count"
                [disabled]="disabled()"
                class="qt-input flex-1 font-mono"
                [class.qt-input-error]="effectErrors(effect.id).length > 0"
                aria-label="Effect target"
              />
            </label>
            @if (effect.target.trim() === '') {
              <span class="flex gap-1">
                <button
                  type="button"
                  class="qt-button qt-button-ghost qt-button-sm font-mono"
                  (click)="updateEffect(effect.id, { target: 'state.' })"
                  [disabled]="disabled()"
                  title="Write into tiered persistent state, at the tier where the key already lives"
                >
                  state.
                </button>
                <button
                  type="button"
                  class="qt-button qt-button-ghost qt-button-sm font-mono"
                  (click)="updateEffect(effect.id, { target: 'metadata.' })"
                  [disabled]="disabled()"
                  title="Write onto the rolling character's own fact sheet (metadata.json)"
                >
                  metadata.
                </button>
              </span>
            }
          </div>

          <div class="flex items-center gap-2 flex-wrap text-sm">
            <label class="flex items-center gap-2">
              Value
              <select
                [value]="effect.valueKind"
                (change)="onValueKindChange(effect, selectValue($event))"
                [disabled]="disabled()"
                class="qt-select qt-select-sm"
                aria-label="Value kind"
              >
                <option value="expression">expression</option>
                <option value="literal-number">number</option>
                <option value="literal-boolean">true/false</option>
              </select>
            </label>
            @if (effect.valueKind === 'literal-boolean') {
              <select
                [value]="effect.value === 'true' ? 'true' : 'false'"
                (change)="updateEffect(effect.id, { value: selectValue($event) })"
                [disabled]="disabled()"
                class="qt-select qt-select-sm"
                aria-label="Boolean value"
              >
                <option value="true">true</option>
                <option value="false">false</option>
              </select>
            } @else {
              <input
                [type]="effect.valueKind === 'literal-number' ? 'number' : 'text'"
                step="any"
                [value]="effect.value"
                (input)="updateEffect(effect.id, { value: inputValue($event) })"
                [placeholder]="
                  effect.valueKind === 'literal-number' ? '1' : EFFECT_VALUE_PLACEHOLDER
                "
                [disabled]="disabled()"
                class="qt-input flex-1 min-w-48 font-mono"
                [class.qt-input-error]="effectErrors(effect.id).length > 0"
                aria-label="Effect value"
              />
            }
          </div>

          @for (message of effectErrors(effect.id); track message) {
            <p class="text-xs qt-text-destructive">{{ message }}</p>
          }
          @for (message of effectWarnings(effect.id); track message) {
            <p class="text-xs qt-text-secondary">⚠ {{ message }}</p>
          }
        </div>
      }

      <p class="qt-hint">
        {{ EFFECTS_HINT }} <code class="font-mono">'broken pick'</code>{{ EFFECTS_HINT_TAIL }}
      </p>
    </section>
  `,
})
export class SideEffectsSection {
  readonly draft = input.required<ToolDraft>();
  readonly issues = input.required<DraftIssue[]>();
  readonly disabled = input(false);

  readonly draftChange = output<ToolDraft>();

  protected readonly MAX_EFFECTS = MAX_EFFECTS;
  protected readonly MAX_EFFECT_TARGET_LENGTH = MAX_EFFECT_TARGET_LENGTH;
  protected readonly OUTCOME_STATES = OUTCOME_STATES;
  protected readonly EFFECT_VALUE_PLACEHOLDER = EFFECT_VALUE_PLACEHOLDER;
  protected readonly EFFECTS_HINT = EFFECTS_HINT;
  protected readonly EFFECTS_HINT_TAIL = EFFECTS_HINT_TAIL;
  protected readonly atMostTitle = `At most ${MAX_EFFECTS} effects`;

  readonly effects = computed(() => this.draft().effects);

  protected whenChoice(effect: DraftEffect): WhenChoice | 'verbatim' {
    return whenChoiceOf(effect);
  }

  /** The verbatim condition's JSON, for the read-only badge. */
  protected verbatimWhen(effect: DraftEffect): string {
    return effect.when.kind === 'verbatim' ? JSON.stringify(effect.when.when) : '';
  }

  private setEffects(next: DraftEffect[]): void {
    this.draftChange.emit({ ...this.draft(), effects: next });
  }

  updateEffect(id: string, partial: Partial<DraftEffect>): void {
    this.setEffects(
      this.effects().map((effect) => (effect.id === id ? { ...effect, ...partial } : effect)),
    );
  }

  protected onWhenChange(id: string, value: string): void {
    const choice = value as WhenChoice;
    this.updateEffect(id, {
      when: choice === 'always' ? { kind: 'always' } : { kind: 'outcome-state', state: choice },
    });
  }

  protected onValueKindChange(effect: DraftEffect, raw: string): void {
    const valueKind = raw as DraftEffect['valueKind'];
    this.updateEffect(effect.id, {
      valueKind,
      value:
        valueKind === 'literal-boolean'
          ? 'true'
          : valueKind === 'literal-number'
            ? ''
            : effect.value,
    });
  }

  addEffect(): void {
    effectIdCounter += 1;
    this.setEffects([
      ...this.effects(),
      {
        id: `new-effect-${effectIdCounter}`,
        when: { kind: 'always' },
        target: '',
        valueKind: 'expression',
        value: '',
      },
    ]);
  }

  removeEffect(id: string): void {
    this.setEffects(this.effects().filter((effect) => effect.id !== id));
  }

  effectErrors(id: string): string[] {
    return this.issues()
      .filter(
        (issue) =>
          issue.severity === 'error' && issue.where.section === 'effect' && issue.where.id === id,
      )
      .map((issue) => issue.message);
  }

  effectWarnings(id: string): string[] {
    return this.issues()
      .filter(
        (issue) =>
          issue.severity === 'warning' && issue.where.section === 'effect' && issue.where.id === id,
      )
      .map((issue) => issue.message);
  }

  protected inputValue(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }

  protected selectValue(event: Event): string {
    return (event.target as HTMLSelectElement).value;
  }
}
