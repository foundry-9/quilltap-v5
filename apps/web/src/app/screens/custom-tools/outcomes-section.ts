import {
  ChangeDetectionStrategy,
  Component,
  computed,
  ElementRef,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';

import { MAX_MESSAGE_LENGTH, MAX_OUTCOMES, type OutcomeState } from '../../pascal/custom-tool-types';
import { PLACEHOLDER_PATTERN } from '../../pascal/placeholders';
import {
  CONTAINMENT_COMPARATORS,
  ORDERING_COMPARATORS,
  conditionSlotKey,
  type ComparatorKey,
  type ConditionOperand,
  type ConditionSubject,
  type DraftCondition,
  type DraftIssue,
  type DraftOutcome,
  type DraftParameter,
  type ToolDraft,
} from '../../pascal/tool-draft';
import { Icon } from '../../ui/icon';
import { COMPARATOR_LABELS } from './comparator-labels';

/**
 * OutcomesSection — the cascading outcome table (v4
 * `components/custom-tools/OutcomesSection.tsx`, 828).
 *
 * An ordered list, checked top to bottom, first row whose every condition holds
 * wins. The final row is the pinned catch-all: not movable, not deletable, its
 * condition reading "otherwise" — which makes the loader's ordering rule
 * unviolatable BY CONSTRUCTION. Conditions are chips (subject + comparator +
 * operand) joined by AND; a duplicate subject+comparator pair is blocked at the
 * door because one comparator object can carry each key once.
 */

/**
 * Re-exported for the callers that learned this name here, before the gate
 * needed the same vocabulary and it moved to its own file (v4 does the same
 * extraction in `6864bf0e`).
 */
export { COMPARATOR_LABELS };

const STATE_OPTIONS: Array<{ state: OutcomeState; label: string; badge: string }> = [
  { state: 'success', label: 'success', badge: 'qt-badge qt-badge-success' },
  { state: 'partial', label: 'partial', badge: 'qt-badge qt-badge-warning' },
  { state: 'failure', label: 'failure', badge: 'qt-badge qt-badge-destructive' },
  { state: 'info', label: 'info', badge: 'qt-badge qt-badge-info' },
];

let conditionIdCounter = 0;
let outcomeIdCounter = 0;

/** Serialize a subject select value; `param:` and `metadata` carry extra state. */
export function subjectSelectValue(subject: ConditionSubject): string {
  switch (subject.kind) {
    case 'value':
      return 'value';
    case 'roll':
      return 'roll';
    case 'param':
      return `param:${subject.name}`;
    case 'metadata':
      return 'metadata';
    case 'llm':
      return 'llm';
    case 'llm-ok':
      return 'llm-ok';
  }
}

/** The sentence a blocked duplicate slot reads. */
export function describeSlot(condition: Pick<DraftCondition, 'subject' | 'comparator'>): string {
  const label = COMPARATOR_LABELS[condition.comparator];
  switch (condition.subject.kind) {
    case 'value':
      return `A row can test ${label} on the value only once.`;
    case 'roll':
      return `A row can test ${label} on the raw roll only once.`;
    case 'param':
      return `A row can test ${label} on "${condition.subject.name}" only once.`;
    case 'metadata':
      return `A row can test ${label} on metadata "${condition.subject.key}" only once.`;
    case 'llm':
      return `A row can test ${label} on the consult's answer only once.`;
    case 'llm-ok':
      return 'A row can test whether the consult succeeded only once.';
  }
}

/**
 * The value type a subject carries, or null where it is unknowable at authoring
 * time — a metadata key's stored value, or whatever the consulted model chooses
 * to answer.
 */
function subjectValueType(
  subject: ConditionSubject,
  paramByName: Map<string, DraftParameter>,
): 'number' | 'string' | 'boolean' | null {
  if (subject.kind === 'value' || subject.kind === 'roll') return 'number';
  if (subject.kind === 'param') {
    const p = paramByName.get(subject.name);
    return p ? (p.type === 'integer' ? 'number' : p.type) : null;
  }
  if (subject.kind === 'llm-ok') return 'boolean';
  // 'metadata' | 'llm'
  return null;
}

// ---------------------------------------------------------------------------
// Operand field
// ---------------------------------------------------------------------------

@Component({
  selector: 'qt-operand-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="flex items-center gap-1">
      <!--
        Metadata eq/neq: no declared type steers the widget, so a literal-type
        picker chooses between number, text, and true/false (§4.4.1).
      -->
      @if (showTypePicker()) {
        <select
          [value]="operand().kind"
          (change)="onTypePick($event)"
          [disabled]="disabled()"
          class="qt-select qt-select-sm"
          aria-label="Literal type"
        >
          <option value="number">number</option>
          <option value="string">text</option>
          <option value="boolean">true/false</option>
        </select>
      }

      @switch (widget()) {
        @case ('state') {
          <span
            class="qt-text-xs qt-text-secondary font-mono rounded qt-bg-muted px-2 py-1"
            title="This operand draws from persistent state. Edit $state references in the raw JSON."
          >
            {{ statePillText() }}
          </span>
        }
        @case ('param') {
          <select
            [value]="operandName()"
            (change)="setOperand({ kind: 'param', name: selectValue($event) })"
            [disabled]="disabled()"
            class="qt-select qt-select-sm w-28"
            aria-label="Operand parameter"
          >
            @if (!eligibleParams().includes(operandName())) {
              <option [value]="operandName()">{{ operandName() || '—' }}</option>
            }
            @for (name of eligibleParams(); track name) {
              <option [value]="name">{{ name }}</option>
            }
          </select>
        }
        @case ('boolean') {
          <select
            [value]="booleanValue() ? 'true' : 'false'"
            (change)="setOperand({ kind: 'boolean', value: selectValue($event) === 'true' })"
            [disabled]="disabled()"
            class="qt-select qt-select-sm w-20"
            aria-label="Operand value"
          >
            <option value="true">true</option>
            <option value="false">false</option>
          </select>
        }
        @case ('string') {
          <input
            type="text"
            [value]="operandText()"
            (input)="setOperand({ kind: 'string', text: inputValue($event) })"
            [disabled]="disabled()"
            class="qt-input w-28 text-sm"
            aria-label="Operand text"
          />
        }
        @default {
          <input
            type="number"
            step="any"
            [value]="numberText()"
            (input)="setOperand({ kind: 'number', text: inputValue($event) })"
            [disabled]="disabled()"
            class="qt-input w-24 text-sm"
            aria-label="Operand number"
          />
        }
      }

      <button
        type="button"
        (click)="toggle()"
        [disabled]="disabled() || (operand().kind !== 'param' && eligibleParams().length === 0)"
        class="qt-button qt-button-ghost qt-button-sm"
        [title]="toggleTitle()"
        aria-label="Toggle operand between a literal and a parameter reference"
      >
        <qt-icon name="swap" class="w-3 h-3" />
      </button>
    </div>
  `,
})
export class OperandField {
  readonly condition = input.required<DraftCondition>();
  readonly subjectType = input.required<'number' | 'string' | 'boolean' | null>();
  readonly eligibleParams = input.required<string[]>();
  readonly disabled = input(false);

  readonly conditionChange = output<DraftCondition>();

  readonly operand = computed(() => this.condition().operand);
  private readonly ordering = computed(() => ORDERING_COMPARATORS.has(this.condition().comparator));
  private readonly containment = computed(() =>
    CONTAINMENT_COMPARATORS.has(this.condition().comparator),
  );
  /** Metadata and the consult's answer both have an unknowable stored type. */
  private readonly typeUnknowable = computed(() => {
    const kind = this.condition().subject.kind;
    return kind === 'metadata' || kind === 'llm';
  });

  // Containment needs no picker — the substring is always text. A `$state`
  // operand is a read-only pill, so it takes no picker either.
  readonly showTypePicker = computed(
    () =>
      this.typeUnknowable() &&
      !this.ordering() &&
      !this.containment() &&
      this.operand().kind !== 'param' &&
      this.operand().kind !== 'state',
  );

  /** Which widget the operand renders as — v4's nested ternary, named. */
  readonly widget = computed<'state' | 'param' | 'boolean' | 'string' | 'number'>(() => {
    const o = this.operand();
    if (o.kind === 'state') return 'state';
    if (o.kind === 'param') return 'param';
    if (o.kind === 'boolean') return 'boolean';
    if (o.kind === 'string' && (this.containment() || this.subjectType() !== 'number')) {
      return 'string';
    }
    return 'number';
  });

  /** The read-only pill for a carried `$state` operand (v4's markup). */
  protected statePillText(): string {
    const o = this.operand();
    return o.kind === 'state' ? `$state: ${o.path} → ${String(o.fallback)}` : '';
  }

  protected operandName(): string {
    const o = this.operand();
    return o.kind === 'param' ? o.name : '';
  }

  protected operandText(): string {
    const o = this.operand();
    return o.kind === 'string' ? o.text : '';
  }

  protected numberText(): string {
    const o = this.operand();
    return o.kind === 'number' ? o.text : '';
  }

  protected booleanValue(): boolean {
    const o = this.operand();
    return o.kind === 'boolean' ? o.value : false;
  }

  protected selectValue(event: Event): string {
    return (event.target as HTMLSelectElement).value;
  }

  protected inputValue(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }

  protected setOperand(next: ConditionOperand): void {
    this.conditionChange.emit({ ...this.condition(), operand: next });
  }

  protected onTypePick(event: Event): void {
    const kind = this.selectValue(event) as 'number' | 'string' | 'boolean';
    const o = this.operand();
    if (kind === 'number') {
      this.setOperand({ kind: 'number', text: o.kind === 'string' ? o.text : '' });
    } else if (kind === 'string') {
      this.setOperand({ kind: 'string', text: o.kind === 'number' ? o.text : '' });
    } else {
      this.setOperand({ kind: 'boolean', value: true });
    }
  }

  protected toggleTitle(): string {
    if (this.operand().kind === 'param') return 'Use a literal instead';
    return this.eligibleParams().length === 0
      ? 'Declare a compatible parameter first'
      : 'Compare against a parameter';
  }

  protected toggle(): void {
    if (this.operand().kind === 'param') {
      const t = this.subjectType();
      this.setOperand(
        this.containment() || t === 'string'
          ? { kind: 'string', text: '' }
          : t === 'boolean'
            ? { kind: 'boolean', value: true }
            : { kind: 'number', text: '' },
      );
      return;
    }
    this.setOperand({ kind: 'param', name: this.eligibleParams()[0] ?? '' });
  }
}

// ---------------------------------------------------------------------------
// Condition chip
// ---------------------------------------------------------------------------

@Component({
  selector: 'qt-condition-chip',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, OperandField],
  template: `
    <div
      class="flex items-center gap-1 flex-wrap rounded border px-2 py-1"
      [class.qt-input-error]="hasError()"
    >
      <!--
        selected-per-option, never a value binding on the select: two of the
        options below sit behind an @if, and a value set before the matching
        option exists silently falls back to the first one (the standing
        dogfood-#6 rule; the P4.6j memory). NB no backticks in a template
        comment - they end the literal.
      -->
      <select
        (change)="onSubjectChange(selectValue($event))"
        [disabled]="disabled()"
        class="qt-select qt-select-sm"
        aria-label="Condition subject"
      >
        <option value="value" [selected]="subjectValue() === 'value'">Value</option>
        <option
          value="roll"
          title="The raw pre-transform draw"
          [selected]="subjectValue() === 'roll'"
        >
          Raw roll
        </option>
        @for (p of namedParameters(); track p.id) {
          <option [value]="'param:' + p.name" [selected]="subjectValue() === 'param:' + p.name">
            Parameter: {{ p.name }}
          </option>
        }
        <option value="metadata" [selected]="subjectValue() === 'metadata'">Metadata…</option>
        @if (draft().llmEnabled) {
          <option
            value="llm"
            title="What the consulted model answered"
            [selected]="subjectValue() === 'llm'"
          >
            Consult answer
          </option>
          <option
            value="llm-ok"
            title="Whether the consult produced an answer at all"
            [selected]="subjectValue() === 'llm-ok'"
          >
            Consult succeeded
          </option>
        }
      </select>

      @if (metadataKey() !== null) {
        <input
          type="text"
          [value]="metadataKey()"
          (input)="onMetadataKey(inputValue($event))"
          placeholder="key on the character's fact sheet"
          [disabled]="disabled()"
          class="qt-input w-44 text-sm"
          [class.qt-input-error]="metadataKey()!.trim() === ''"
          aria-label="Metadata key"
          title="The invoking character's fact sheet — a key the character lacks simply doesn't match."
        />
      }

      <!-- Same rule: the option list is @for-driven and its membership changes
           with the subject's type. -->
      <select
        (change)="onComparatorChange(selectValue($event))"
        [disabled]="disabled()"
        class="qt-select qt-select-sm min-w-14"
        aria-label="Comparator"
      >
        @for (key of comparators(); track key) {
          <option [value]="key" [selected]="condition().comparator === key">
            {{ COMPARATOR_LABELS[key] }}
          </option>
        }
      </select>

      <qt-operand-field
        [condition]="condition()"
        [subjectType]="subjectType()"
        [eligibleParams]="eligibleOperandParams()"
        [disabled]="disabled()"
        (conditionChange)="conditionChange.emit($event)"
      />

      @if (metadataKey() !== null && (ordering() || containment())) {
        <span class="text-xs qt-text-secondary" [title]="metadataHintTitle()">ⓘ</span>
      }

      @if (subjectKind() === 'llm' && ordering()) {
        <span
          class="text-xs qt-text-secondary"
          title="Matches only when the answer reads as a number — anything else declines the row at run time, fail-soft, never an error."
          >ⓘ</span
        >
      }

      @if (subjectKind() === 'llm' && containment()) {
        <span
          class="text-xs qt-text-secondary"
          title="Searches the answer without regard to case — 'west door' finds 'the West Door'."
          >ⓘ</span
        >
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
export class ConditionChip {
  readonly condition = input.required<DraftCondition>();
  readonly draft = input.required<ToolDraft>();
  readonly hasError = input(false);
  readonly disabled = input(false);

  readonly conditionChange = output<DraftCondition>();
  readonly remove = output<void>();

  protected readonly COMPARATOR_LABELS = COMPARATOR_LABELS;

  private readonly paramByName = computed(
    () => new Map(this.draft().parameters.map((p) => [p.name, p] as const)),
  );

  readonly namedParameters = computed(() => this.draft().parameters.filter((p) => p.name));

  readonly subjectValue = computed(() => subjectSelectValue(this.condition().subject));

  /** The metadata key, or null when the subject is not metadata. */
  readonly metadataKey = computed(() => {
    const s = this.condition().subject;
    return s.kind === 'metadata' ? s.key : null;
  });

  readonly subjectType = computed(() =>
    subjectValueType(this.condition().subject, this.paramByName()),
  );

  readonly ordering = computed(() => ORDERING_COMPARATORS.has(this.condition().comparator));
  readonly containment = computed(() =>
    CONTAINMENT_COMPARATORS.has(this.condition().comparator),
  );

  readonly subjectKind = computed(() => this.condition().subject.kind);

  /** The metadata ⓘ carries a different sentence for ordering and containment. */
  protected metadataHintTitle(): string {
    return this.ordering()
      ? 'Matches only when the stored value is a number — anything else declines the row at run time, fail-soft, never an error.'
      : 'Matches only when the stored value is text — anything else declines the row at run time, fail-soft, never an error.';
  }

  /**
   * Metadata and the consult's answer offer every comparator — the stored type
   * is unknowable at authoring time (§4.4.1). String subjects offer equality
   * and containment; boolean subjects only = and ≠; numbers cannot contain.
   */
  readonly comparators = computed<ComparatorKey[]>(() => {
    const t = this.subjectType();
    if (t === 'string') return ['eq', 'neq', 'contains', 'ncontains'];
    if (t === 'boolean') return ['eq', 'neq'];
    if (t === null) return ['gt', 'gte', 'lt', 'lte', 'eq', 'neq', 'contains', 'ncontains'];
    return ['gt', 'gte', 'lt', 'lte', 'eq', 'neq'];
  });

  /** Parameters an operand reference may name, mirroring `validateComparator`. */
  readonly eligibleOperandParams = computed(() => {
    const subject = this.condition().subject;
    const ordering = this.ordering();
    const containment = this.containment();
    const subjectType = this.subjectType();
    return this.draft()
      .parameters.filter((p) => {
        if (!p.name) return false;
        // A substring must be text whatever the subject turns out to hold.
        if (containment) return p.type === 'string';
        if (subject.kind === 'metadata' || subject.kind === 'llm') {
          // With the subject's type unknown, no reference can be ruled
          // incompatible — ordering still demands a number, though.
          return !ordering || p.type === 'number' || p.type === 'integer';
        }
        const paramType = p.type === 'integer' ? 'number' : p.type;
        if (ordering) return paramType === 'number';
        return subjectType === null || paramType === subjectType;
      })
      .map((p) => p.name);
  });

  protected selectValue(event: Event): string {
    return (event.target as HTMLSelectElement).value;
  }

  protected inputValue(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }

  protected onMetadataKey(key: string): void {
    this.conditionChange.emit({ ...this.condition(), subject: { kind: 'metadata', key } });
  }

  protected onSubjectChange(value: string): void {
    const condition = this.condition();
    const subject = condition.subject;

    let nextSubject: ConditionSubject;
    if (value === 'value') nextSubject = { kind: 'value' };
    else if (value === 'roll') nextSubject = { kind: 'roll' };
    else if (value === 'metadata') {
      nextSubject = { kind: 'metadata', key: subject.kind === 'metadata' ? subject.key : '' };
    } else if (value === 'llm') nextSubject = { kind: 'llm' };
    else if (value === 'llm-ok') nextSubject = { kind: 'llm-ok' };
    else nextSubject = { kind: 'param', name: value.slice('param:'.length) };

    // Re-legalize the comparator and operand for the new subject's type.
    let nextComparator = condition.comparator;
    const nextType = subjectValueType(nextSubject, this.paramByName());
    if ((nextType === 'string' || nextType === 'boolean') && ORDERING_COMPARATORS.has(nextComparator)) {
      nextComparator = 'eq';
    }
    if ((nextType === 'number' || nextType === 'boolean') && CONTAINMENT_COMPARATORS.has(nextComparator)) {
      nextComparator = 'eq';
    }
    let nextOperand: ConditionOperand = condition.operand;
    if (nextType === 'boolean' && nextOperand.kind !== 'param') {
      nextOperand = { kind: 'boolean', value: true };
    } else if (nextType === 'string' && nextOperand.kind === 'number') {
      nextOperand = { kind: 'string', text: nextOperand.text };
    } else if (
      nextType === 'number' &&
      (nextOperand.kind === 'string' || nextOperand.kind === 'boolean')
    ) {
      nextOperand = {
        kind: 'number',
        text: nextOperand.kind === 'string' ? nextOperand.text : '',
      };
    }

    this.conditionChange.emit({
      ...condition,
      subject: nextSubject,
      comparator: nextComparator,
      operand: nextOperand,
    });
  }

  protected onComparatorChange(raw: string): void {
    const comparator = raw as ComparatorKey;
    const condition = this.condition();
    let nextOperand = condition.operand;
    if (
      ORDERING_COMPARATORS.has(comparator) &&
      (nextOperand.kind === 'string' || nextOperand.kind === 'boolean')
    ) {
      // Ordering takes a number (or a numeric-param reference) everywhere.
      nextOperand = {
        kind: 'number',
        text: nextOperand.kind === 'string' ? nextOperand.text : '',
      };
    }
    if (
      CONTAINMENT_COMPARATORS.has(comparator) &&
      (nextOperand.kind === 'number' || nextOperand.kind === 'boolean')
    ) {
      // Containment looks for text (or a string-param reference) everywhere.
      nextOperand = {
        kind: 'string',
        text: nextOperand.kind === 'number' ? nextOperand.text : '',
      };
    }
    this.conditionChange.emit({ ...condition, comparator, operand: nextOperand });
  }
}

// ---------------------------------------------------------------------------
// Condition list
// ---------------------------------------------------------------------------

@Component({
  selector: 'qt-condition-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, ConditionChip],
  template: `
    <div class="space-y-1 pl-6">
      @for (condition of outcome().conditions; track condition.id; let i = $index) {
        <div class="flex items-center gap-2 flex-wrap">
          @if (i > 0) {
            <span class="text-xs qt-text-secondary font-mono">AND</span>
          }
          <qt-condition-chip
            [condition]="condition"
            [draft]="draft()"
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
      <button
        type="button"
        class="qt-button qt-button-ghost qt-button-sm"
        (click)="addCondition()"
        [disabled]="disabled()"
      >
        <qt-icon name="plus" class="w-3 h-3" />
        add condition
      </button>
    </div>
  `,
})
export class ConditionList {
  readonly outcome = input.required<DraftOutcome>();
  readonly draft = input.required<ToolDraft>();
  readonly conditionErrorIds = input.required<Set<string>>();
  readonly disabled = input(false);

  readonly conditionsChange = output<DraftCondition[]>();

  readonly duplicateNotice = signal<string | null>(null);

  protected updateCondition(id: string, next: DraftCondition): void {
    // Block a change that would collide with an existing subject+comparator
    // slot — one comparator object carries each key once (§4.4.1).
    const slot = conditionSlotKey(next);
    const collision = this.outcome().conditions.some(
      (c) => c.id !== id && conditionSlotKey(c) === slot,
    );
    if (collision) {
      this.duplicateNotice.set(describeSlot(next));
      return;
    }
    this.duplicateNotice.set(null);
    this.conditionsChange.emit(this.outcome().conditions.map((c) => (c.id === id ? next : c)));
  }

  protected addCondition(): void {
    conditionIdCounter += 1;
    const conditions = this.outcome().conditions;
    const base: DraftCondition = {
      id: `new-cond-${conditionIdCounter}`,
      subject: { kind: 'value' },
      comparator: 'gte',
      operand: { kind: 'number', text: '' },
    };
    // Find a free slot so the fresh chip never lands on a duplicate.
    const taken = new Set(conditions.map(conditionSlotKey));
    const comparators: ComparatorKey[] = ['gte', 'gt', 'lte', 'lt', 'eq', 'neq'];
    for (const subject of [{ kind: 'value' } as const, { kind: 'roll' } as const]) {
      for (const comparator of comparators) {
        if (!taken.has(conditionSlotKey({ subject, comparator }))) {
          this.conditionsChange.emit([...conditions, { ...base, subject, comparator }]);
          return;
        }
      }
    }
    // Every value/roll slot is taken; fall back to a metadata chip.
    this.conditionsChange.emit([
      ...conditions,
      { ...base, subject: { kind: 'metadata', key: '' }, operand: { kind: 'number', text: '' } },
    ]);
  }

  protected deleteCondition(id: string): void {
    this.duplicateNotice.set(null);
    this.conditionsChange.emit(this.outcome().conditions.filter((c) => c.id !== id));
  }
}

// ---------------------------------------------------------------------------
// Message editor
// ---------------------------------------------------------------------------

@Component({
  selector: 'qt-outcome-message-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="space-y-1">
      <div class="relative">
        <textarea
          #box
          [value]="outcome().message"
          [maxLength]="MAX_MESSAGE_LENGTH"
          (input)="onInput($event)"
          [disabled]="disabled()"
          rows="2"
          class="qt-textarea w-full text-sm"
          placeholder="What Pascal announces when this row wins…"
          aria-label="Outcome message"
        ></textarea>
        <div class="absolute top-1 right-1">
          <button
            type="button"
            class="qt-button qt-button-ghost qt-button-sm"
            (click)="menuOpen.set(!menuOpen())"
            [disabled]="disabled()"
            [attr.aria-expanded]="menuOpen()"
            aria-haspopup="menu"
          >
            Insert value ▾
          </button>
          @if (menuOpen()) {
            <div
              class="absolute right-0 mt-1 qt-card qt-shadow-lg rounded border z-10 py-1 w-52"
              role="menu"
            >
              <button
                type="button"
                role="menuitem"
                class="block w-full text-left px-3 py-1 text-sm"
                (click)="insertValue()"
              >
                Value
              </button>
              <button
                type="button"
                role="menuitem"
                class="block w-full text-left px-3 py-1 text-sm"
                (click)="insertRoll()"
              >
                Raw roll
              </button>
              @if (draft().rollForm === 'dice') {
                <button
                  type="button"
                  role="menuitem"
                  class="block w-full text-left px-3 py-1 text-sm"
                  (click)="insertDice()"
                >
                  Dice breakdown
                </button>
              }
              @for (p of namedParameters(); track p.id) {
                <button
                  type="button"
                  role="menuitem"
                  class="block w-full text-left px-3 py-1 text-sm"
                  (click)="insertParam(p.name)"
                >
                  Parameter: {{ p.name }}
                </button>
              }
              <button
                type="button"
                role="menuitem"
                class="block w-full text-left px-3 py-1 text-sm"
                (click)="insertMetadataKey()"
              >
                Metadata key…
              </button>
              @if (draft().llmEnabled) {
                <button
                  type="button"
                  role="menuitem"
                  class="block w-full text-left px-3 py-1 text-sm"
                  (click)="insertConsultAnswer()"
                >
                  Consult answer
                </button>
              }
            </div>
          }
        </div>
      </div>
      <div class="flex items-center gap-1 flex-wrap">
        @for (placeholder of placeholders(); track $index) {
          <code class="qt-badge qt-badge-outline font-mono">{{ placeholder }}</code>
        }
        <span class="text-xs qt-text-secondary ml-auto"
          >{{ outcome().message.length }}/{{ MAX_MESSAGE_LENGTH }}</span
        >
      </div>
    </div>
  `,
})
export class OutcomeMessageEditor {
  readonly outcome = input.required<DraftOutcome>();
  readonly draft = input.required<ToolDraft>();
  readonly disabled = input(false);

  readonly messageChange = output<string>();

  private readonly box = viewChild<ElementRef<HTMLTextAreaElement>>('box');
  readonly menuOpen = signal(false);

  protected readonly MAX_MESSAGE_LENGTH = MAX_MESSAGE_LENGTH;

  readonly namedParameters = computed(() => this.draft().parameters.filter((p) => p.name));

  /** Placeholders present in the text, for the visibility strip. */
  readonly placeholders = computed(() =>
    // v4 `0506517d3` (OutcomesSection.tsx:771): the shared pattern, not an inline copy.
    [...this.outcome().message.matchAll(PLACEHOLDER_PATTERN)].map((m) => m[0]),
  );

  protected onInput(event: Event): void {
    this.messageChange.emit((event.target as HTMLTextAreaElement).value);
  }

  // The placeholder literals live HERE, not in the template: `{{` opens an
  // interpolation in an Angular binding expression, so they cannot be written
  // inline the way v4's JSX writes them.
  protected insertValue(): void {
    this.insertAtCursor('{{value}}');
  }

  protected insertRoll(): void {
    this.insertAtCursor('{{roll}}');
  }

  protected insertDice(): void {
    this.insertAtCursor('{{dice}}');
  }

  protected insertParam(name: string): void {
    this.insertAtCursor(`{{params.${name}}}`);
  }

  protected insertConsultAnswer(): void {
    this.insertAtCursor('{{llm}}');
  }

  insertAtCursor(placeholder: string): void {
    const textarea = this.box()?.nativeElement;
    this.menuOpen.set(false);
    const message = this.outcome().message;
    if (!textarea) {
      this.messageChange.emit(message + placeholder);
      return;
    }
    const start = textarea.selectionStart ?? message.length;
    const end = textarea.selectionEnd ?? start;
    this.messageChange.emit(message.slice(0, start) + placeholder + message.slice(end));
    requestAnimationFrame(() => {
      textarea.focus();
      const cursor = start + placeholder.length;
      textarea.setSelectionRange(cursor, cursor);
    });
  }

  protected insertMetadataKey(): void {
    // Suggest keys the author demonstrably cares about: those already tested in
    // this tool's `when` objects (§4.4.2).
    const testedKeys = new Set<string>();
    for (const o of this.draft().outcomes) {
      for (const condition of o.conditions) {
        if (condition.subject.kind === 'metadata' && condition.subject.key.trim() !== '') {
          testedKeys.add(condition.subject.key);
        }
      }
    }
    const suggestion = [...testedKeys][0] ?? '';
    const key = window.prompt('Metadata key to render (any non-empty string):', suggestion);
    if (key && key.trim() !== '') this.insertAtCursor(`{{metadata.${key.trim()}}}`);
    else this.menuOpen.set(false);
  }
}

// ---------------------------------------------------------------------------
// One outcome row
// ---------------------------------------------------------------------------

@Component({
  selector: 'qt-outcome-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, ConditionList, OutcomeMessageEditor],
  template: `
    <div
      class="qt-card p-3 space-y-2"
      [class]="accent()"
      [class.qt-input-error]="errors().length > 0"
      [class.border]="errors().length > 0"
      [class.qt-highlight]="flash()"
    >
      <div class="flex items-center gap-2">
        <span class="text-xs qt-text-secondary font-mono w-6">{{
          isTail() ? '∞' : index() + 1
        }}</span>

        @if (isTail()) {
          <span class="text-sm italic" title="The mandatory catch-all: every roll lands somewhere"
            >otherwise</span
          >
        } @else {
          <span class="text-sm">when…</span>
        }

        <div class="ml-auto flex items-center gap-1">
          @if (!isTail()) {
            <button
              type="button"
              class="qt-button qt-button-ghost qt-button-sm"
              (click)="moveUp.emit()"
              [disabled]="disabled() || !canMoveUp()"
              title="Move up"
              [attr.aria-label]="'Move outcome ' + (index() + 1) + ' up'"
            >
              <qt-icon name="chevron-down" class="w-3.5 h-3.5 rotate-180" />
            </button>
            <button
              type="button"
              class="qt-button qt-button-ghost qt-button-sm"
              (click)="moveDown.emit()"
              [disabled]="disabled() || !canMoveDown()"
              title="Move down"
              [attr.aria-label]="'Move outcome ' + (index() + 1) + ' down'"
            >
              <qt-icon name="chevron-down" class="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              class="qt-button qt-button-ghost qt-button-sm"
              (click)="remove.emit()"
              [disabled]="disabled()"
              title="Delete this outcome"
              [attr.aria-label]="'Delete outcome ' + (index() + 1)"
            >
              <qt-icon name="trash" class="w-3.5 h-3.5" />
            </button>
          }
        </div>
      </div>

      @if (!isTail()) {
        <qt-condition-list
          [outcome]="outcome()"
          [draft]="draft()"
          [conditionErrorIds]="conditionErrorIds()"
          [disabled]="disabled()"
          (conditionsChange)="update.emit({ conditions: $event })"
        />
      }

      <div class="flex items-center gap-2 flex-wrap">
        <div class="flex gap-1" role="radiogroup" aria-label="Outcome state">
          @for (option of STATE_OPTIONS; track option.state) {
            <button
              type="button"
              role="radio"
              [attr.aria-checked]="outcome().state === option.state"
              (click)="update.emit({ state: option.state })"
              [disabled]="disabled()"
              [class]="option.badge"
              [class.opacity-40]="outcome().state !== option.state"
            >
              {{ option.label }}
            </button>
          }
        </div>
      </div>

      <qt-outcome-message-editor
        [outcome]="outcome()"
        [draft]="draft()"
        [disabled]="disabled()"
        (messageChange)="update.emit({ message: $event })"
      />

      @for (message of errors(); track message) {
        <p class="text-xs qt-text-destructive">{{ message }}</p>
      }
      @for (message of warnings(); track message) {
        <p class="text-xs qt-text-secondary">⚠ {{ message }}</p>
      }
    </div>
  `,
})
export class OutcomeRow {
  readonly outcome = input.required<DraftOutcome>();
  readonly index = input.required<number>();
  readonly isTail = input.required<boolean>();
  readonly draft = input.required<ToolDraft>();
  readonly errors = input.required<string[]>();
  readonly conditionErrorIds = input.required<Set<string>>();
  readonly warnings = input.required<string[]>();
  readonly flash = input(false);
  readonly disabled = input(false);
  readonly canMoveUp = input(false);
  readonly canMoveDown = input(false);

  readonly update = output<Partial<DraftOutcome>>();
  readonly remove = output<void>();
  readonly moveUp = output<void>();
  readonly moveDown = output<void>();

  protected readonly STATE_OPTIONS = STATE_OPTIONS;

  readonly accent = computed(() => `qt-pascal-result qt-pascal-result--${this.outcome().state}`);
}

// ---------------------------------------------------------------------------
// The section
// ---------------------------------------------------------------------------

@Component({
  selector: 'qt-outcomes-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, OutcomeRow],
  template: `
    <section class="qt-card p-4 space-y-3">
      <div class="flex items-center justify-between">
        <div>
          <h2 class="qt-card-title text-sm">The outcome table</h2>
          <p class="qt-hint">
            Checked top to bottom — the first row whose every condition holds wins.
          </p>
        </div>
        <button
          type="button"
          class="qt-button qt-button-secondary qt-button-sm"
          (click)="addOutcome()"
          [disabled]="disabled() || !canAdd()"
          [title]="canAdd() ? 'Insert a row above the catch-all' : 'At most ' + MAX_OUTCOMES + ' outcomes'"
        >
          <qt-icon name="plus" class="w-3.5 h-3.5" />
          Add outcome ({{ outcomes().length }}/{{ MAX_OUTCOMES }})
        </button>
      </div>

      @for (outcome of outcomes(); track outcome.id; let i = $index) {
        <qt-outcome-row
          [outcome]="outcome"
          [index]="i"
          [isTail]="i === outcomes().length - 1"
          [draft]="draft()"
          [errors]="outcomeErrors(outcome.id)"
          [conditionErrorIds]="conditionErrorIds(outcome.id)"
          [warnings]="messageWarnings(outcome.id)"
          [flash]="flashOutcomeId() === outcome.id"
          [disabled]="disabled()"
          [canMoveUp]="i > 0 && i < nonTailCount()"
          [canMoveDown]="i < nonTailCount() - 1"
          (update)="updateOutcome(outcome.id, $event)"
          (remove)="deleteOutcome(outcome.id)"
          (moveUp)="move(i, -1)"
          (moveDown)="move(i, 1)"
        />
      }
    </section>
  `,
})
export class OutcomesSection {
  readonly draft = input.required<ToolDraft>();
  readonly issues = input.required<DraftIssue[]>();
  /** Outcome id to flash after a bench roll lands on it. */
  readonly flashOutcomeId = input<string | null>(null);
  readonly disabled = input(false);

  readonly draftChange = output<ToolDraft>();

  protected readonly MAX_OUTCOMES = MAX_OUTCOMES;

  readonly outcomes = computed(() => this.draft().outcomes);
  readonly nonTailCount = computed(() => this.outcomes().length - 1);
  readonly canAdd = computed(() => this.outcomes().length < MAX_OUTCOMES);

  outcomeErrors(id: string): string[] {
    return this.issues()
      .filter(
        (issue) =>
          issue.severity === 'error' &&
          (issue.where.section === 'outcome' || issue.where.section === 'message') &&
          issue.where.id === id,
      )
      .map((issue) => issue.message);
  }

  conditionErrorIds(id: string): Set<string> {
    return new Set(
      this.issues()
        .filter(
          (i) =>
            i.severity === 'error' &&
            i.where.section === 'outcome' &&
            i.where.id === id &&
            i.where.conditionId !== undefined,
        )
        .map((i) => (i.where.section === 'outcome' ? (i.where.conditionId ?? '') : '')),
    );
  }

  messageWarnings(id: string): string[] {
    return this.issues()
      .filter(
        (issue) =>
          issue.severity === 'warning' &&
          issue.where.section === 'message' &&
          issue.where.id === id,
      )
      .map((issue) => issue.message);
  }

  private setOutcomes(next: DraftOutcome[]): void {
    this.draftChange.emit({ ...this.draft(), outcomes: next });
  }

  updateOutcome(id: string, partial: Partial<DraftOutcome>): void {
    this.setOutcomes(this.outcomes().map((o) => (o.id === id ? { ...o, ...partial } : o)));
  }

  addOutcome(): void {
    outcomeIdCounter += 1;
    const outcomes = this.outcomes();
    const fresh: DraftOutcome = {
      id: `new-outcome-${outcomeIdCounter}`,
      catchAll: false,
      conditions: [],
      state: 'success',
      message: '',
    };
    // Insert above the pinned catch-all.
    this.setOutcomes([...outcomes.slice(0, -1), fresh, outcomes[outcomes.length - 1]]);
  }

  deleteOutcome(id: string): void {
    this.setOutcomes(this.outcomes().filter((o) => o.id !== id));
  }

  move(index: number, delta: -1 | 1): void {
    const target = index + delta;
    if (target < 0 || target >= this.nonTailCount()) return;
    const next = [...this.outcomes()];
    [next[index], next[target]] = [next[target], next[index]];
    this.setOutcomes(next);
  }
}
