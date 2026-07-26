import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import {
  CONTAINMENT_COMPARATORS,
  ORDERING_COMPARATORS,
  type ComparatorKey,
  type DraftGateCondition,
  type DraftIssue,
  type GateMode,
  type ToolDraft,
} from '../../pascal/tool-draft';
import { Icon } from '../../ui/icon';
import { COMPARATOR_LABELS } from './comparator-labels';

/**
 * GateSection — who may reach for this tool at all (v4
 * `components/custom-tools/GateSection.tsx`, 328).
 *
 * The outcome table decides what happens once a tool is run; this decides
 * whether a character is offered it in the first place. Three modes, exclusive
 * by construction (the format admits at most one clause): anyone may reach for
 * it, only those whose fact sheet passes, or everyone except those whose fact
 * sheet passes.
 *
 * The chips are deliberately thinner than the outcome table's. A gate is
 * answered before the deal, so there are no parameters to compare against and
 * no roll to test — every subject is a metadata key and every operand is a
 * literal, and the widget offers exactly that and nothing more.
 */

const MODE_OPTIONS: Array<{ mode: GateMode; label: string; title: string }> = [
  { mode: 'none', label: 'Anyone', title: 'Every character in the scene is offered this tool' },
  {
    mode: 'available',
    label: 'Only show if…',
    title: 'Offer it only to a character whose fact sheet passes every test below',
  },
  {
    mode: 'withheld',
    label: 'Do not show if…',
    title: 'Withhold it from a character whose fact sheet passes every test below',
  },
];

/** Every comparator is on offer: a metadata key's stored type is unknowable here. */
const COMPARATORS: ComparatorKey[] = [
  'gt',
  'gte',
  'lt',
  'lte',
  'eq',
  'neq',
  'contains',
  'ncontains',
];

let gateIdCounter = 0;

// ---------------------------------------------------------------------------
// Gate chip
// ---------------------------------------------------------------------------

@Component({
  selector: 'qt-gate-chip',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div
      class="flex items-center gap-1 flex-wrap rounded border px-2 py-1"
      [class.qt-input-error]="hasError()"
    >
      <input
        type="text"
        [value]="condition().key"
        (input)="onKey(inputValue($event))"
        placeholder="key on the character's fact sheet"
        [disabled]="disabled()"
        class="qt-input w-44 text-sm"
        [class.qt-input-error]="condition().key.trim() === ''"
        aria-label="Metadata key"
        title="The invoking character's fact sheet — a key the character lacks never matches."
      />

      <!--
        selected-per-option, never a value binding on the select (the standing
        dogfood-#6 rule): the option list is @for-driven, and a value set before
        its option exists silently falls back to the first one. The qt-select
        class is w-full, so the width is pinned here — otherwise an eight-item
        comparator dropdown stretches across the whole chip and shoves the
        operand onto a line of its own. NB no backticks in a template comment -
        they end the literal.
      -->
      <select
        (change)="onComparator(selectValue($event))"
        [disabled]="disabled()"
        class="qt-select qt-select-sm w-32 shrink-0"
        aria-label="Comparator"
      >
        @for (key of COMPARATORS; track key) {
          <option [value]="key" [selected]="condition().comparator === key">
            {{ COMPARATOR_LABELS[key] }}
          </option>
        }
      </select>

      <!-- eq/neq alone need a type picker: with the stored type unknowable, only
           the author can say whether they mean 3, "3", or true. -->
      @if (!ordering() && !containment()) {
        <select
          (change)="onTypePick(selectValue($event))"
          [disabled]="disabled()"
          class="qt-select qt-select-sm w-24 shrink-0"
          aria-label="Literal type"
        >
          <option value="number" [selected]="operand().kind === 'number'">number</option>
          <option value="string" [selected]="operand().kind === 'string'">text</option>
          <option value="boolean" [selected]="operand().kind === 'boolean'">true/false</option>
        </select>
      }

      @switch (operand().kind) {
        @case ('boolean') {
          <select
            (change)="setOperand({ kind: 'boolean', value: selectValue($event) === 'true' })"
            [disabled]="disabled()"
            class="qt-select qt-select-sm w-20"
            aria-label="Operand value"
          >
            <option value="true" [selected]="booleanValue()">true</option>
            <option value="false" [selected]="!booleanValue()">false</option>
          </select>
        }
        @case ('string') {
          <input
            type="text"
            [value]="operandText()"
            (input)="setOperand({ kind: 'string', text: inputValue($event) })"
            [disabled]="disabled()"
            class="qt-input w-32 text-sm"
            aria-label="Operand text"
          />
        }
        @default {
          <input
            type="number"
            step="any"
            [value]="operandText()"
            (input)="setOperand({ kind: 'number', text: inputValue($event) })"
            [disabled]="disabled()"
            class="qt-input w-24 text-sm"
            aria-label="Operand number"
          />
        }
      }

      @if (ordering() || containment()) {
        <span class="text-xs qt-text-secondary" [title]="hintTitle()">ⓘ</span>
      }

      <button
        type="button"
        class="qt-button qt-button-ghost qt-button-sm"
        (click)="remove.emit()"
        [disabled]="disabled()"
        title="Remove this condition"
        aria-label="Remove this condition"
      >
        <qt-icon name="close" class="w-3 h-3" />
      </button>
    </div>
  `,
})
export class GateChip {
  readonly condition = input.required<DraftGateCondition>();
  readonly hasError = input(false);
  readonly disabled = input(false);

  readonly conditionChange = output<DraftGateCondition>();
  readonly remove = output<void>();

  protected readonly COMPARATORS = COMPARATORS;
  protected readonly COMPARATOR_LABELS = COMPARATOR_LABELS;

  readonly operand = computed(() => this.condition().operand);
  readonly ordering = computed(() => ORDERING_COMPARATORS.has(this.condition().comparator));
  readonly containment = computed(() => CONTAINMENT_COMPARATORS.has(this.condition().comparator));

  protected operandText(): string {
    const o = this.operand();
    return o.kind === 'boolean' ? '' : o.text;
  }

  protected booleanValue(): boolean {
    const o = this.operand();
    return o.kind === 'boolean' ? o.value : false;
  }

  protected hintTitle(): string {
    return this.ordering()
      ? 'Matches only when the stored value is a number — anything else fails the test, fail-soft, never an error.'
      : 'Matches only when the stored value is text — anything else fails the test, fail-soft, never an error.';
  }

  protected inputValue(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }

  protected selectValue(event: Event): string {
    return (event.target as HTMLSelectElement).value;
  }

  protected onKey(key: string): void {
    this.conditionChange.emit({ ...this.condition(), key });
  }

  protected setOperand(operand: DraftGateCondition['operand']): void {
    this.conditionChange.emit({ ...this.condition(), operand });
  }

  protected onComparator(raw: string): void {
    const comparator = raw as ComparatorKey;
    // Re-seat the operand in the widget the new comparator can actually use.
    const operand = this.operand();
    let next = operand;
    if (ORDERING_COMPARATORS.has(comparator) && operand.kind !== 'number') {
      next = { kind: 'number', text: operand.kind === 'string' ? operand.text : '' };
    } else if (CONTAINMENT_COMPARATORS.has(comparator) && operand.kind !== 'string') {
      next = { kind: 'string', text: operand.kind === 'number' ? operand.text : '' };
    }
    this.conditionChange.emit({ ...this.condition(), comparator, operand: next });
  }

  protected onTypePick(raw: string): void {
    const kind = raw as 'number' | 'string' | 'boolean';
    const operand = this.operand();
    if (kind === 'number') {
      this.setOperand({ kind: 'number', text: operand.kind === 'string' ? operand.text : '' });
    } else if (kind === 'string') {
      this.setOperand({ kind: 'string', text: operand.kind === 'number' ? operand.text : '' });
    } else {
      this.setOperand({ kind: 'boolean', value: true });
    }
  }
}

// ---------------------------------------------------------------------------
// The section
// ---------------------------------------------------------------------------

@Component({
  selector: 'qt-gate-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, GateChip],
  template: `
    <section class="qt-card p-4 space-y-3">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <div>
          <h2 class="qt-card-title text-sm">Who may reach for it</h2>
          <p class="qt-hint">
            Tested before the deal, against the invoking character&rsquo;s
            <code>metadata.json</code> — the only thing known before a roll exists.
          </p>
        </div>
        <div class="flex rounded overflow-hidden border" role="radiogroup" aria-label="Availability">
          @for (option of MODE_OPTIONS; track option.mode) {
            <button
              type="button"
              role="radio"
              [attr.aria-checked]="draft().gateMode === option.mode"
              (click)="setMode(option.mode)"
              [disabled]="disabled()"
              [title]="option.title"
              [class]="
                draft().gateMode === option.mode
                  ? 'px-3 py-1 text-sm qt-button qt-button-primary'
                  : 'px-3 py-1 text-sm qt-button qt-button-ghost'
              "
            >
              {{ option.label }}
            </button>
          }
        </div>
      </div>

      @if (draft().gateMode === 'none') {
        <p class="text-xs qt-text-secondary">
          Every character in the scene is offered this tool. Set a condition and it becomes theirs
          alone — the model is never told a withheld tool exists, and the composer popup does not
          list it.
        </p>
      } @else {
        <div class="space-y-1">
          <p class="text-xs qt-text-secondary">
            {{
              draft().gateMode === 'available'
                ? 'Offered only to a character whose sheet passes every test:'
                : 'Withheld from a character whose sheet passes every test:'
            }}
          </p>

          @for (condition of draft().gateConditions; track condition.id; let i = $index) {
            <div class="flex items-center gap-2 flex-wrap">
              @if (i > 0) {
                <span class="text-xs qt-text-secondary font-mono">AND</span>
              }
              <qt-gate-chip
                [condition]="condition"
                [hasError]="conditionErrorIds().has(condition.id)"
                [disabled]="disabled()"
                (conditionChange)="updateCondition(condition.id, $event)"
                (remove)="deleteCondition(condition.id)"
              />
            </div>
          }

          @if (duplicateNotice(); as notice) {
            <p class="text-xs qt-text-destructive">{{ notice }}</p>
          }
          @for (message of sectionErrors(); track message) {
            <p class="text-xs qt-text-destructive">{{ message }}</p>
          }

          <button
            type="button"
            class="qt-button qt-button-ghost qt-button-sm"
            (click)="addCondition()"
            [disabled]="disabled()"
          >
            <qt-icon name="plus" class="w-3 h-3" />
            add condition
          </button>

          <p class="qt-hint">
            A key the character lacks never matches. So an &ldquo;only show if&rdquo; withholds the
            tool from a character with no sheet at all, while a &ldquo;do not show if&rdquo; still
            offers it to them.
          </p>
        </div>
      }
    </section>
  `,
})
export class GateSection {
  readonly draft = input.required<ToolDraft>();
  readonly issues = input.required<DraftIssue[]>();
  readonly disabled = input(false);

  readonly draftChange = output<ToolDraft>();

  protected readonly MODE_OPTIONS = MODE_OPTIONS;

  readonly duplicateNotice = signal<string | null>(null);

  private readonly gateIssues = computed(() =>
    this.issues().filter((issue) => issue.severity === 'error' && issue.where.section === 'gate'),
  );

  /** Section-level errors — the ones with no chip to sit beside. */
  readonly sectionErrors = computed(() =>
    this.gateIssues()
      .filter((issue) => issue.where.section === 'gate' && issue.where.conditionId === undefined)
      .map((issue) => issue.message),
  );

  readonly conditionErrorIds = computed(
    () =>
      new Set(
        this.gateIssues()
          .map((issue) => (issue.where.section === 'gate' ? issue.where.conditionId : undefined))
          .filter((id): id is string => id !== undefined),
      ),
  );

  private setConditions(gateConditions: DraftGateCondition[]): void {
    this.draftChange.emit({ ...this.draft(), gateConditions });
  }

  protected setMode(gateMode: GateMode): void {
    this.duplicateNotice.set(null);
    // Conditions survive a mode change, exactly as the roll fields survive a
    // form change: flipping "only if" to "not if" is the commonest edit there
    // is, and retyping the tests to make it would be a poor joke.
    this.draftChange.emit({ ...this.draft(), gateMode });
  }

  protected addCondition(): void {
    gateIdCounter += 1;
    this.duplicateNotice.set(null);
    this.setConditions([
      ...this.draft().gateConditions,
      {
        id: `new-gate-${gateIdCounter}`,
        key: '',
        comparator: 'eq',
        operand: { kind: 'boolean', value: true },
      },
    ]);
  }

  protected updateCondition(id: string, next: DraftGateCondition): void {
    // One comparator object carries each key once, so a duplicate key+comparator
    // pair is blocked at the door rather than silently overwriting its twin.
    const slot = `${next.key}:${next.comparator}`;
    const collision = this.draft().gateConditions.some(
      (c) => c.id !== id && `${c.key}:${c.comparator}` === slot,
    );
    if (collision) {
      this.duplicateNotice.set(
        `The gate can test “${next.key}” with ${COMPARATOR_LABELS[next.comparator]} only once.`,
      );
      return;
    }
    this.duplicateNotice.set(null);
    this.setConditions(this.draft().gateConditions.map((c) => (c.id === id ? next : c)));
  }

  protected deleteCondition(id: string): void {
    this.duplicateNotice.set(null);
    this.setConditions(this.draft().gateConditions.filter((c) => c.id !== id));
  }
}
