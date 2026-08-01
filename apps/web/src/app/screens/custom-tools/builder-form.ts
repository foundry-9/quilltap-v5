import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import {
  IDENTIFIER_PATTERN,
  MAX_CHIP_LABEL_LENGTH,
  MAX_DESCRIPTION_LENGTH,
  MAX_LLM_OUTPUT_CEILING,
  MAX_LLM_OUTPUT_LENGTH,
  MAX_LLM_PROMPT_LENGTH,
  MAX_MESSAGE_LENGTH,
  MAX_PARAMETERS,
  MAX_TITLE_LENGTH,
  displayTitle,
  isStateRef,
} from '../../pascal/custom-tool-types';
import {
  MAX_DICE_COUNT,
  MAX_DIE_SIDES,
  MIN_DIE_SIDES,
  parseDiceNotation,
} from '../../pascal/dice-notation';
import {
  findParameterReferences,
  numericParamNames,
  parseNumberText,
  renameParameterEverywhere,
  slugFromTitle,
  type DraftIssue,
  type DraftParameter,
  type NumberOrParamValue,
  type ToolDraft,
} from '../../pascal/tool-draft';
import { Icon } from '../../ui/icon';
import { GateSection } from './gate-section';
import { NumberOrParamField } from './number-or-param-field';

/**
 * BuilderForm — identity, options, parameters, and the roll, i.e. everything in
 * the Workbench's form column except the outcome cascade (v4
 * `components/custom-tools/BuilderForm.tsx`, 531).
 *
 * Validity is by construction wherever the format allows: the name field
 * coerces to the identifier grammar as you type, min/max HIDE (not merely
 * disable) on non-numeric parameters, and roll `$param` pickers only ever list
 * numeric parameters. What cannot be prevented structurally arrives as issues
 * from `validateDraft` and renders as inline error state.
 */

/** Strip a typed name down to the identifier grammar, live. */
export function coerceIdentifier(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9_-]/g, '')
    .replace(/^[^a-z]+/, '')
    .slice(0, 64);
}

/**
 * The consulted-oracle card's prose (v4 `BuilderForm.tsx:536-640`). Held as
 * module constants because each one embeds `{{…}}` placeholder text, which an
 * Angular interpolation would truncate at the first inner `}}`.
 */
const PROMPT_PLACEHOLDER =
  'The roll came up {{value}}. In one word, YES or NO: does the mechanism yield?';

const PROMPT_HINT =
  'Posed to the instance’s cheap utility model after the roll, before the outcome table. ' +
  'Takes {{value}}, {{roll}}, {{dice}}, {{params.name}}, and {{metadata.key}}. ' +
  'Ask for the answer shape the table means to test — a bare word, a number, a sentence.';

const ERROR_MESSAGE_HINT =
  'Stands in as the answer when the consult fails — your words, never the provider’s. ' +
  'The table can test for it with a “Consult succeeded” condition, and {{llm}} renders it.';

const ANSWER_CAP_HINT =
  `Characters kept of the answer. Blank means ${MAX_LLM_OUTPUT_LENGTH.toLocaleString()}; ` +
  `up to ${MAX_LLM_OUTPUT_CEILING.toLocaleString()}. Keep it short for a verdict, ` +
  'or let a long-winded oracle run on — the call’s token budget follows suit.';

/** The chip-label field's placeholder and hint — same `{{…}}` reason as above. */
const CHIP_LABEL_PLACEHOLDER = 'Agent lambda — {{params.label}}';

const CHIP_LABEL_HINT =
  'Optional. Names each RUN on the chip and the announcement heading, rendered after the deal — ' +
  'takes {{value}}, {{roll}}, {{dice}}, {{params.name}}, {{metadata.key}}, {{state.path}}, and {{llm}}. ' +
  'Blank, and the title labels the chip.';

const CONSULT_OFF_HINT =
  'Off. Enable it and every run asks a model your question — the answer becomes {{llm}} in ' +
  'messages and a testable subject in the outcome table, with your own error line when no ' +
  'answer comes.';

let paramIdCounter = 0;

/** The four range fields, with their label and placeholder-default. */
const RANGE_FIELDS = [
  { field: 'min', label: 'Min', fallback: '0' },
  { field: 'max', label: 'Max', fallback: '1' },
  { field: 'multiplier', label: 'Multiplier', fallback: '×1' },
  { field: 'offset', label: 'Offset', fallback: '+0' },
] as const;

type RangeField = (typeof RANGE_FIELDS)[number]['field'];

@Component({
  selector: 'qt-builder-form',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [GateSection, Icon, NumberOrParamField],
  template: `
    <div class="space-y-4">
      <!-- Identity -->
      <section class="qt-card p-4 space-y-3">
        <h2 class="qt-card-title text-sm">The contrivance itself</h2>

        <div>
          <label for="wb-title" class="qt-label">Title</label>
          <input
            id="wb-title"
            type="text"
            [value]="draft().title"
            [maxLength]="MAX_TITLE_LENGTH"
            (input)="onTitleChange(inputValue($event))"
            [placeholder]="titlePlaceholder()"
            [disabled]="disabled()"
            class="qt-input w-full"
          />
          <p class="qt-hint">
            Optional. Leave it blank and the table announces {{ titleAnnouncement() }}.
          </p>
        </div>

        <div>
          <label for="wb-chip-label" class="qt-label">Chip label</label>
          <input
            id="wb-chip-label"
            type="text"
            [value]="draft().chipLabel"
            [maxLength]="MAX_CHIP_LABEL_LENGTH"
            (input)="update({ chipLabel: inputValue($event) })"
            [placeholder]="CHIP_LABEL_PLACEHOLDER"
            [disabled]="disabled()"
            class="qt-input w-full"
            [class.qt-input-error]="chipLabelError() !== null"
          />
          <p [class]="chipLabelError() ? 'text-xs qt-text-destructive mt-1' : 'qt-hint'">
            {{ chipLabelError() ?? CHIP_LABEL_HINT }}
          </p>
          @for (message of chipLabelWarnings(); track message) {
            <p class="text-xs qt-text-secondary">⚠ {{ message }}</p>
          }
        </div>

        <div>
          <label for="wb-name" class="qt-label">Name</label>
          <input
            id="wb-name"
            type="text"
            [value]="draft().name"
            (input)="onNameChange(inputValue($event))"
            placeholder="force_the_lock"
            [disabled]="disabled()"
            class="qt-input w-full font-mono"
            [class.qt-input-error]="nameError() !== null"
            [attr.pattern]="IDENTIFIER_SOURCE"
          />
          <p [class]="nameError() ? 'text-xs qt-text-destructive mt-1' : 'qt-hint'">
            {{
              nameError() ??
                'The tool’s identity: lowercase, starts with a letter, 1–64 characters.'
            }}
          </p>
        </div>

        <div>
          <label for="wb-description" class="qt-label">Description</label>
          <textarea
            id="wb-description"
            [value]="draft().description"
            [maxLength]="MAX_DESCRIPTION_LENGTH"
            (input)="update({ description: inputValue($event) })"
            [disabled]="disabled()"
            class="qt-textarea w-full"
            [class.qt-input-error]="descriptionError() !== null"
            rows="2"
          ></textarea>
          <p class="qt-hint flex justify-between">
            <span
              >What the tool does <em>in the fiction</em> — this is how a model decides to reach for
              it.</span
            >
            <span>{{ draft().description.length }}/{{ MAX_DESCRIPTION_LENGTH }}</span>
          </p>
        </div>

        <div class="flex flex-wrap gap-4 text-sm">
          <label
            class="flex items-center gap-2"
            title="Tombstone: suppresses this name at this tier and every farther one"
          >
            <input
              type="checkbox"
              class="qt-checkbox"
              [checked]="draft().disabled"
              (change)="update({ disabled: checkedValue($event) })"
              [disabled]="disabled()"
            />
            Disabled (tombstone)
          </label>
          <label
            class="flex items-center gap-2"
            title="Off: the house does not show the odds — models see only name, description, and parameters"
          >
            <input
              type="checkbox"
              class="qt-checkbox"
              [checked]="draft().revealOdds"
              (change)="update({ revealOdds: checkedValue($event) })"
              [disabled]="disabled()"
            />
            Reveal the odds
          </label>
          <label class="flex items-center gap-2">
            Results
            <select
              [value]="draft().defaultVisibility"
              (change)="onVisibilityChange(selectValue($event))"
              [disabled]="disabled()"
              class="qt-select qt-select-sm"
              aria-label="Default visibility"
            >
              <option value="public">announced publicly</option>
              <option value="whisper">whispered</option>
            </select>
          </label>
        </div>
      </section>

      <!-- Availability gate -->
      <qt-gate-section
        [draft]="draft()"
        [issues]="issues()"
        [disabled]="disabled()"
        (draftChange)="draftChange.emit($event)"
      />

      <!-- Parameters -->
      <section class="qt-card p-4 space-y-3">
        <div class="flex items-center justify-between">
          <h2 class="qt-card-title text-sm">Parameters</h2>
          <button
            type="button"
            class="qt-button qt-button-secondary qt-button-sm"
            (click)="addParam()"
            [disabled]="disabled() || draft().parameters.length >= MAX_PARAMETERS"
            [attr.title]="
              draft().parameters.length >= MAX_PARAMETERS
                ? 'At most ' + MAX_PARAMETERS + ' parameters'
                : null
            "
          >
            <qt-icon name="plus" class="w-3.5 h-3.5" />
            Add parameter ({{ draft().parameters.length }}/{{ MAX_PARAMETERS }})
          </button>
        </div>

        @if (draft().parameters.length === 0) {
          <p class="text-xs qt-text-secondary">
            None declared. Parameters let the caller weight the roll — a bonus, a difficulty, a
            material.
          </p>
        }

        @for (param of draft().parameters; track param.id) {
          <div class="qt-card p-3 space-y-2">
            <div class="flex items-center gap-2 flex-wrap">
              <input
                type="text"
                [value]="param.name"
                (input)="onParamNameChange(param, inputValue($event))"
                (blur)="commitParamRename()"
                placeholder="bonus"
                [disabled]="disabled()"
                class="qt-input w-36 font-mono"
                [class.qt-input-error]="paramError(param.id, 'name') !== null"
                aria-label="Parameter name"
              />
              <select
                [value]="param.type"
                (change)="onParamTypeChange(param, selectValue($event))"
                [disabled]="disabled()"
                class="qt-select qt-select-sm"
                aria-label="Parameter type"
              >
                <option value="number">number</option>
                <option value="integer">integer</option>
                <option value="string">string</option>
                <option value="boolean">boolean</option>
              </select>
              <button
                type="button"
                class="qt-button qt-button-ghost qt-button-sm ml-auto"
                (click)="deleteParam(param)"
                [disabled]="disabled()"
                title="Delete this parameter"
              >
                <qt-icon name="trash" class="w-4 h-4" />
              </button>
            </div>

            <div class="flex items-center gap-3 flex-wrap text-sm">
              <label class="flex items-center gap-2">
                Default
                @if (isStateRef(param.defaultValue)) {
                  <span
                    class="qt-text-xs qt-text-secondary font-mono rounded qt-bg-muted px-2 py-1"
                    title="This default draws from persistent state. Edit $state references in the raw JSON."
                  >
                    {{ stateDefaultText(param.defaultValue) }}
                  </span>
                } @else if (param.type === 'boolean') {
                  <input
                    type="checkbox"
                    class="qt-checkbox"
                    [checked]="!!param.defaultValue"
                    (change)="updateParam(param.id, { defaultValue: checkedValue($event) })"
                    [disabled]="disabled()"
                  />
                } @else {
                  <input
                    [type]="param.type === 'string' ? 'text' : 'number'"
                    [attr.step]="param.type === 'integer' ? 1 : 'any'"
                    [value]="String(param.defaultValue)"
                    (input)="updateParam(param.id, { defaultValue: inputValue($event) })"
                    [disabled]="disabled()"
                    class="qt-input w-28"
                    [class.qt-input-error]="paramError(param.id, 'default') !== null"
                  />
                }
              </label>
              <!--
                min/max HIDE rather than disable on non-numeric types: a bound
                that is merely greyed out still reads as a bound in force.
              -->
              @if (param.type === 'number' || param.type === 'integer') {
                <label class="flex items-center gap-2">
                  Min
                  <input
                    type="number"
                    step="any"
                    [value]="param.min"
                    (input)="updateParam(param.id, { min: inputValue($event) })"
                    [disabled]="disabled()"
                    class="qt-input w-24"
                    [class.qt-input-error]="paramError(param.id, 'min') !== null"
                  />
                </label>
                <label class="flex items-center gap-2">
                  Max
                  <input
                    type="number"
                    step="any"
                    [value]="param.max"
                    (input)="updateParam(param.id, { max: inputValue($event) })"
                    [disabled]="disabled()"
                    class="qt-input w-24"
                    [class.qt-input-error]="paramError(param.id, 'max') !== null"
                  />
                </label>
              }
            </div>
            @if (firstParamError(param.id); as message) {
              <p class="text-xs qt-text-destructive">{{ message }}</p>
            }

            <input
              type="text"
              [value]="param.description"
              (input)="updateParam(param.id, { description: inputValue($event) })"
              placeholder="What this parameter means, in the fiction (optional)"
              [disabled]="disabled()"
              class="qt-input w-full text-sm"
              aria-label="Parameter description"
            />
          </div>
        }
        <p class="qt-hint">
          Every parameter needs a default, so a bare invocation can still roll.
        </p>
      </section>

      <!-- Roll -->
      <section class="qt-card p-4 space-y-3">
        <div class="flex items-center justify-between">
          <h2 class="qt-card-title text-sm">The roll</h2>
          <div class="flex rounded overflow-hidden border" role="radiogroup" aria-label="Roll form">
            @for (form of ROLL_FORMS; track form) {
              <button
                type="button"
                role="radio"
                [attr.aria-checked]="draft().rollForm === form"
                (click)="update({ rollForm: form })"
                [disabled]="disabled()"
                class="px-3 py-1 text-sm"
                [class]="
                  draft().rollForm === form
                    ? 'qt-button-primary qt-button'
                    : 'qt-button qt-button-ghost'
                "
              >
                {{ form === 'range' ? 'Range' : 'Dice' }}
              </button>
            }
          </div>
        </div>

        @if (draft().rollForm === 'dice') {
          <div class="space-y-1">
            <input
              type="text"
              [value]="draft().rollDice"
              (input)="update({ rollDice: inputValue($event) })"
              placeholder="3d6+2"
              [disabled]="disabled()"
              class="qt-input w-40 font-mono"
              [class.qt-input-error]="diceError() !== null"
              aria-label="Dice notation"
            />
            @if (parsedDice(); as dice) {
              <p class="qt-hint">
                {{ dice.count }} {{ dice.count === 1 ? 'die' : 'dice' }}, {{ dice.sides }} sides{{
                  diceModifierText()
                }}
                — totals {{ dice.count + dice.modifier }}–{{
                  dice.count * dice.sides + dice.modifier
                }}
              </p>
            } @else {
              <p class="text-xs qt-text-destructive">
                Dice notation like &ldquo;3d6+2&rdquo; or &ldquo;1d20&rdquo; ({{ MIN_DIE_SIDES }}–{{
                  MAX_DIE_SIDES
                }}
                sides, 1–{{ MAX_DICE_COUNT }} dice).
              </p>
            }
            <p class="qt-hint">Dice carry their own modifier; the range transform does not apply.</p>
          </div>
        } @else {
          <div class="space-y-2">
            <div class="grid grid-cols-2 gap-3">
              @for (spec of RANGE_FIELDS; track spec.field) {
                <label class="flex items-center justify-between gap-2 text-sm">
                  {{ spec.label }}
                  <qt-number-or-param-field
                    [value]="rangeValue(spec.field)"
                    [paramNames]="numericNames()"
                    [placeholder]="spec.fallback"
                    [hasError]="rollFieldError(spec.field) !== null"
                    [disabled]="disabled()"
                    [label]="spec.label"
                    (valueChange)="onRangeChange(spec.field, $event)"
                  />
                </label>
              }
            </div>
            <label class="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                class="qt-checkbox"
                [checked]="draft().rollRange.round"
                (change)="onRoundChange(checkedValue($event))"
                [disabled]="disabled()"
              />
              Round the final value
            </label>
            @if (firstRollFieldError(); as message) {
              <p class="text-xs qt-text-destructive">{{ message }}</p>
            }
            <p class="qt-hint">{{ rangeReadout() }}</p>
          </div>
        }
      </section>

      <!-- LLM consult -->
      <section class="qt-card p-4 space-y-3">
        <div class="flex items-center justify-between">
          <h2 class="qt-card-title text-sm">The consulted oracle</h2>
          <label class="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              class="qt-checkbox"
              [checked]="draft().llmEnabled"
              (change)="update({ llmEnabled: checkedValue($event) })"
              [disabled]="disabled()"
            />
            Consult an LLM
          </label>
        </div>

        @if (draft().llmEnabled) {
          <div class="space-y-3">
            <div>
              <label for="wb-llm-prompt" class="qt-label">The question</label>
              <textarea
                id="wb-llm-prompt"
                [value]="draft().llmPrompt"
                [maxLength]="MAX_LLM_PROMPT_LENGTH"
                (input)="update({ llmPrompt: inputValue($event) })"
                [disabled]="disabled()"
                rows="3"
                class="qt-textarea w-full text-sm"
                [class.qt-input-error]="llmError('prompt') !== null"
                [placeholder]="PROMPT_PLACEHOLDER"
              ></textarea>
              <p [class]="llmError('prompt') ? 'text-xs qt-text-destructive mt-1' : 'qt-hint'">
                {{ llmError('prompt') ?? PROMPT_HINT }}
              </p>
              <p class="qt-hint text-right">
                {{ draft().llmPrompt.length }}/{{ MAX_LLM_PROMPT_LENGTH }}
              </p>
            </div>

            <div>
              <label for="wb-llm-error" class="qt-label">When the oracle is silent</label>
              <input
                id="wb-llm-error"
                type="text"
                [value]="draft().llmErrorMessage"
                [maxLength]="MAX_MESSAGE_LENGTH"
                (input)="update({ llmErrorMessage: inputValue($event) })"
                [disabled]="disabled()"
                class="qt-input w-full text-sm"
                [class.qt-input-error]="llmError('errorMessage') !== null"
                placeholder="The wire crackles, and no answer comes."
              />
              <p [class]="llmError('errorMessage') ? 'text-xs qt-text-destructive mt-1' : 'qt-hint'">
                {{ llmError('errorMessage') ?? ERROR_MESSAGE_HINT }}
              </p>
            </div>

            <div>
              <label for="wb-llm-max-output" class="qt-label">Answer cap</label>
              <input
                id="wb-llm-max-output"
                type="number"
                [min]="1"
                [max]="MAX_LLM_OUTPUT_CEILING"
                step="1"
                [value]="draft().llmMaxOutput"
                (input)="update({ llmMaxOutput: inputValue($event) })"
                [disabled]="disabled()"
                [placeholder]="String(MAX_LLM_OUTPUT_LENGTH)"
                class="qt-input w-36 text-sm"
                [class.qt-input-error]="llmError('maxOutput') !== null"
              />
              <p [class]="llmError('maxOutput') ? 'text-xs qt-text-destructive mt-1' : 'qt-hint'">
                {{ llmError('maxOutput') ?? ANSWER_CAP_HINT }}
              </p>
            </div>

            @for (message of llmWarnings(); track message) {
              <p class="text-xs qt-text-secondary">⚠ {{ message }}</p>
            }
          </div>
        } @else {
          <p class="text-xs qt-text-secondary">
            {{ CONSULT_OFF_HINT }}
          </p>
        }
      </section>
    </div>
  `,
})
export class BuilderForm {
  readonly draft = input.required<ToolDraft>();
  readonly issues = input.required<DraftIssue[]>();
  readonly disabled = input(false);

  readonly draftChange = output<ToolDraft>();

  /**
   * While true, editing the title keeps regenerating the name slug (§4.1).
   *
   * v4 seeds this from `draft.name === ''` at mount (`useState`). An Angular
   * `input()` is not readable in a field initializer, so the seed is taken
   * lazily on first use — same one-shot semantics, same source of truth. Plain
   * state rather than a signal: it steers behavior, never the template.
   */
  private nameTracksTitle: boolean | null = null;

  private readonly pendingRename = signal<{ id: string; from: string; to: string } | null>(null);

  protected readonly MAX_TITLE_LENGTH = MAX_TITLE_LENGTH;
  protected readonly MAX_CHIP_LABEL_LENGTH = MAX_CHIP_LABEL_LENGTH;
  protected readonly MAX_DESCRIPTION_LENGTH = MAX_DESCRIPTION_LENGTH;
  protected readonly MAX_PARAMETERS = MAX_PARAMETERS;
  protected readonly MAX_MESSAGE_LENGTH = MAX_MESSAGE_LENGTH;
  protected readonly MAX_LLM_PROMPT_LENGTH = MAX_LLM_PROMPT_LENGTH;
  protected readonly MAX_LLM_OUTPUT_CEILING = MAX_LLM_OUTPUT_CEILING;
  protected readonly MAX_LLM_OUTPUT_LENGTH = MAX_LLM_OUTPUT_LENGTH;
  /**
   * The consult hints live in module constants, not inline in the template:
   * every one of them CONTAINS `{{…}}` placeholder text, and Angular's parser
   * would close the interpolation at the first inner `}}`. (The same trap the
   * P4.6bb lane hit.) The bytes are v4's.
   */
  protected readonly PROMPT_PLACEHOLDER = PROMPT_PLACEHOLDER;
  protected readonly PROMPT_HINT = PROMPT_HINT;
  protected readonly CHIP_LABEL_PLACEHOLDER = CHIP_LABEL_PLACEHOLDER;
  protected readonly CHIP_LABEL_HINT = CHIP_LABEL_HINT;
  protected readonly ERROR_MESSAGE_HINT = ERROR_MESSAGE_HINT;
  protected readonly ANSWER_CAP_HINT = ANSWER_CAP_HINT;
  protected readonly CONSULT_OFF_HINT = CONSULT_OFF_HINT;
  protected readonly MIN_DIE_SIDES = MIN_DIE_SIDES;
  protected readonly MAX_DIE_SIDES = MAX_DIE_SIDES;
  protected readonly MAX_DICE_COUNT = MAX_DICE_COUNT;
  protected readonly IDENTIFIER_SOURCE = IDENTIFIER_PATTERN.source;
  protected readonly RANGE_FIELDS = RANGE_FIELDS;
  protected readonly ROLL_FORMS = ['range', 'dice'] as const;
  protected readonly String = String;
  protected readonly isStateRef = isStateRef;

  /** The read-only pill text for a `$state` parameter default (v4's markup). */
  protected stateDefaultText(value: DraftParameter['defaultValue']): string {
    return isStateRef(value) ? `$state: ${value.$state} → ${String(value.fallback)}` : '';
  }

  readonly numericNames = computed(() => numericParamNames(this.draft()));

  private fieldError(predicate: (issue: DraftIssue) => boolean): string | null {
    return (
      this.issues().find((issue) => issue.severity === 'error' && predicate(issue))?.message ?? null
    );
  }

  readonly nameError = computed(() =>
    this.fieldError((i) => i.where.section === 'identity' && i.where.field === 'name'),
  );
  readonly chipLabelError = computed(() =>
    this.fieldError((i) => i.where.section === 'identity' && i.where.field === 'chipLabel'),
  );
  /** The label's advisory placeholder warnings, rendered under the field. */
  readonly chipLabelWarnings = computed(() =>
    this.issues()
      .filter(
        (i) =>
          i.severity === 'warning' && i.where.section === 'identity' && i.where.field === 'chipLabel',
      )
      .map((i) => i.message),
  );
  readonly descriptionError = computed(() =>
    this.fieldError((i) => i.where.section === 'identity' && i.where.field === 'description'),
  );
  /** The blocking error on one consult field, if any. */
  llmError(field: 'prompt' | 'errorMessage' | 'maxOutput'): string | null {
    return this.fieldError((i) => i.where.section === 'llm' && i.where.field === field);
  }

  /** The consult's advisory placeholder warnings, rendered under the fields. */
  readonly llmWarnings = computed(() =>
    this.issues()
      .filter((i) => i.severity === 'warning' && i.where.section === 'llm')
      .map((i) => i.message),
  );

  readonly diceError = computed(() =>
    this.fieldError((i) => i.where.section === 'roll' && i.where.field === 'dice'),
  );

  rollFieldError(field: RangeField): string | null {
    return this.fieldError((i) => i.where.section === 'roll' && i.where.field === field);
  }

  /** v4 renders ONE line for the whole range block, first error wins. */
  firstRollFieldError(): string | null {
    return (
      this.rollFieldError('min') ??
      this.rollFieldError('max') ??
      this.rollFieldError('multiplier') ??
      this.rollFieldError('offset')
    );
  }

  paramError(id: string, field: 'name' | 'default' | 'min' | 'max'): string | null {
    return this.fieldError(
      (i) => i.where.section === 'parameter' && i.where.id === id && i.where.field === field,
    );
  }

  /** v4's `name ?? default ?? min` line — max is deliberately not in it. */
  firstParamError(id: string): string | null {
    return (
      this.paramError(id, 'name') ?? this.paramError(id, 'default') ?? this.paramError(id, 'min')
    );
  }

  readonly titlePlaceholder = computed(() => {
    const name = this.draft().name;
    return name ? displayTitle({ name }) : 'Force the Lock';
  });

  readonly titleAnnouncement = computed(() => {
    const name = this.draft().name;
    return name ? `“${displayTitle({ name })}”` : 'a title-cased name';
  });

  readonly parsedDice = computed(() => parseDiceNotation(this.draft().rollDice.trim()));

  readonly diceModifierText = computed(() => {
    const dice = this.parsedDice();
    if (!dice || dice.modifier === 0) return '';
    return `, ${dice.modifier > 0 ? '+' : '−'}${Math.abs(dice.modifier)}`;
  });

  protected inputValue(event: Event): string {
    return (event.target as HTMLInputElement | HTMLTextAreaElement).value;
  }

  protected selectValue(event: Event): string {
    return (event.target as HTMLSelectElement).value;
  }

  protected checkedValue(event: Event): boolean {
    return (event.target as HTMLInputElement).checked;
  }

  rangeValue(field: RangeField): NumberOrParamValue {
    return this.draft().rollRange[field];
  }

  update(partial: Partial<ToolDraft>): void {
    this.draftChange.emit({ ...this.draft(), ...partial });
  }

  private tracksTitle(): boolean {
    this.nameTracksTitle ??= this.draft().name === '';
    return this.nameTracksTitle;
  }

  onTitleChange(title: string): void {
    if (this.tracksTitle()) {
      this.update({ title, name: slugFromTitle(title) });
    } else {
      this.update({ title });
    }
  }

  /** Typing a name by hand breaks the title→slug link, permanently. */
  onNameChange(raw: string): void {
    this.nameTracksTitle = false;
    this.update({ name: coerceIdentifier(raw) });
  }

  onVisibilityChange(raw: string): void {
    this.update({ defaultVisibility: raw as 'public' | 'whisper' });
  }

  onRangeChange(field: RangeField, value: NumberOrParamValue): void {
    this.update({ rollRange: { ...this.draft().rollRange, [field]: value } });
  }

  onRoundChange(round: boolean): void {
    this.update({ rollRange: { ...this.draft().rollRange, round } });
  }

  // -- Parameters -----------------------------------------------------------

  updateParam(id: string, partial: Partial<DraftParameter>): void {
    this.update({
      parameters: this.draft().parameters.map((p) => (p.id === id ? { ...p, ...partial } : p)),
    });
  }

  addParam(): void {
    paramIdCounter += 1;
    this.update({
      parameters: [
        ...this.draft().parameters,
        {
          id: `new-param-${paramIdCounter}`,
          name: '',
          type: 'number',
          defaultValue: '0',
          description: '',
          min: '',
          max: '',
        },
      ],
    });
  }

  onParamTypeChange(param: DraftParameter, raw: string): void {
    const type = raw as DraftParameter['type'];
    this.updateParam(param.id, {
      type,
      // A type change re-seats the default in the new type's widget.
      defaultValue: type === 'boolean' ? false : String(param.defaultValue ?? ''),
      ...(type === 'string' || type === 'boolean' ? { min: '', max: '' } : {}),
    });
  }

  onParamNameChange(param: DraftParameter, raw: string): void {
    const next = coerceIdentifier(raw);
    const prev = this.pendingRename();
    this.pendingRename.set(
      prev && prev.id === param.id
        ? { ...prev, to: next }
        : { id: param.id, from: param.name, to: next },
    );
    this.updateParam(param.id, { name: next });
  }

  /**
   * Renaming offers "rename everywhere" (safe, atomic). The commit happens on
   * BLUR so half-typed names don't thrash every reference.
   */
  commitParamRename(): void {
    const pending = this.pendingRename();
    if (!pending) return;
    const { from, to } = pending;
    this.pendingRename.set(null);
    if (from === '' || from === to) return;
    this.draftChange.emit(renameParameterEverywhere(this.draft(), from, to));
  }

  /** Deleting a referenced parameter breaks LOUDLY, never rewrites (§4.2). */
  deleteParam(param: DraftParameter): void {
    const references = param.name ? findParameterReferences(this.draft(), param.name) : [];
    if (references.length > 0) {
      const listing = references.map((site) => `  • ${site}`).join('\n');
      const confirmed = window.confirm(
        `"${param.name}" is referenced here:\n${listing}\n\n` +
          'Deleting it will NOT rewrite those references — each flips to an error for you to resolve. Delete anyway?',
      );
      if (!confirmed) return;
    }
    this.update({ parameters: this.draft().parameters.filter((p) => p.id !== param.id) });
  }

  // -- The range readout ----------------------------------------------------

  readonly rangeReadout = computed(() => {
    const r = this.draft().rollRange;
    const part = (value: NumberOrParamValue, fallback: string) =>
      value.kind === 'param'
        ? `“${value.name}”`
        : value.kind === 'state'
          ? `state:${value.path}`
          : value.text.trim() === ''
            ? fallback
            : value.text.trim();

    const min = part(r.min, '0');
    const max = part(r.max, '1');
    const pieces = [`Draws uniformly in [${min}, ${max})`];
    const multiplier = part(r.multiplier, '1');
    if (multiplier !== '1') pieces.push(`then ×${multiplier}`);
    const offset = part(r.offset, '0');
    if (offset !== '0') pieces.push(`then +${offset}`);
    if (r.round) pieces.push('then rounds');

    const sentence = `${pieces.join(', ')}.`;

    // When fully literal, also show the resulting value bounds.
    const allLiteral = [r.min, r.max, r.multiplier, r.offset].every((v) => v.kind === 'literal');
    if (!allLiteral) return sentence;
    const lit = (v: NumberOrParamValue, fallback: number) =>
      v.kind === 'literal' && v.text.trim() !== '' ? parseNumberText(v.text) : fallback;
    const minN = lit(r.min, 0);
    const maxN = lit(r.max, 1);
    const multiplierN = lit(r.multiplier, 1);
    const offsetN = lit(r.offset, 0);
    if (![minN, maxN, multiplierN, offsetN].every(Number.isFinite)) return sentence;
    const lowRaw = minN * multiplierN + offsetN;
    const highRaw = maxN * multiplierN + offsetN;
    let low = Math.min(lowRaw, highRaw);
    let high = Math.max(lowRaw, highRaw);
    if (r.round) {
      low = Math.round(low);
      high = Math.round(high);
    }
    return `${sentence} Values land in [${low}, ${high}).`;
  });
}
