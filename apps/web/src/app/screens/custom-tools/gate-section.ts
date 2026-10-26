import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import {
  CONTAINMENT_COMPARATORS,
  ORDERING_COMPARATORS,
  gateConditionSlotKey,
  groupsOf,
  reseatOperandForComparator,
  type ComparatorKey,
  type DraftGateCondition,
  type DraftIssue,
  type GateMode,
  type ToolDraft,
} from '../../pascal/tool-draft';
import { INVENTORY_STATE_VALUES } from '../../pascal/inventory-subjects';
import { Icon } from '../../ui/icon';
import { AggregateKeyField } from './aggregate-key-field';
import { COMPARATOR_LABELS } from './comparator-labels';
import { ConditionGroups } from './condition-groups';
import { LiteralOperandField } from './literal-operand-field';

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
 * no roll to test — every subject is a metadata key, a progress field, an
 * Inventory object named literally, or an aggregate over the bag, and every
 * operand is a literal, and the widget offers exactly that and nothing more.
 *
 * Chips on one line AND together; an OR toggle starts a new line, and any line
 * that holds decides the gate.
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

/**
 * What each subject may be asked: a state is = / ≠, a count is a number. An
 * aggregate offers everything; whether its key is a list or a number is the
 * validator's to flag, beside the chip.
 */
export function comparatorsFor(subject: DraftGateCondition['subject']): ComparatorKey[] {
  if (subject === 'inventory') return ['eq', 'neq'];
  if (subject === 'inventoryCount') return ['gt', 'gte', 'lt', 'lte', 'eq', 'neq'];
  return COMPARATORS;
}

/** The same chip on a new subject, its comparator and operand made legal there. */
export function withSubject(
  condition: DraftGateCondition,
  subject: DraftGateCondition['subject'],
): DraftGateCondition {
  if (subject === 'inventory') {
    const comparator = condition.comparator === 'neq' ? 'neq' : 'eq';
    const keep =
      condition.operand.kind === 'string' &&
      (INVENTORY_STATE_VALUES as readonly string[]).includes(condition.operand.text);
    return {
      ...condition,
      subject,
      comparator,
      operand: keep ? condition.operand : { kind: 'string', text: 'equipped' },
    };
  }
  if (subject === 'inventoryCount') {
    const comparator = CONTAINMENT_COMPARATORS.has(condition.comparator) ? 'gte' : condition.comparator;
    const operand =
      condition.operand.kind === 'number' ? condition.operand : { kind: 'number' as const, text: '1' };
    return { ...condition, subject, comparator, operand };
  }
  return { ...condition, subject };
}

let gateIdCounter = 0;

// ---------------------------------------------------------------------------
// Gate chip
// ---------------------------------------------------------------------------

@Component({
  selector: 'qt-gate-chip',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, AggregateKeyField, LiteralOperandField],
  template: `
    <div
      class="flex items-center gap-1 flex-wrap rounded border px-2 py-1"
      [class.qt-input-error]="hasError()"
    >
      <!--
        The chip's leading subject, v4 0587d1e96. selected-per-option rather
        than a value binding, for the reason the comparator select below spells
        out. NB no backticks in a template comment - they end the literal.
      -->
      <select
        (change)="onSubject(selectValue($event))"
        [disabled]="disabled()"
        class="qt-select qt-select-sm w-28 shrink-0"
        aria-label="Gate subject"
        title="Which sheet this test reads: the character's own metadata.json, their derived progressions, or what they carry."
      >
        <option value="metadata" [selected]="condition().subject === 'metadata'">metadata</option>
        <option value="progress" [selected]="condition().subject === 'progress'">progress</option>
        <option value="inventory" [selected]="condition().subject === 'inventory'">inventory state</option>
        <option value="inventoryCount" [selected]="condition().subject === 'inventoryCount'">
          inventory count
        </option>
        <option value="inventoryAggregate" [selected]="condition().subject === 'inventoryAggregate'">
          inventory aggregate
        </option>
      </select>

      @if (aggregate()) {
        <qt-aggregate-key-field
          [value]="condition().key"
          [disabled]="disabled()"
          (valueChange)="onKey($event)"
        />
      } @else {
        <input
          type="text"
          [value]="condition().key"
          (input)="onKey(inputValue($event))"
          [placeholder]="keyPlaceholder()"
          [disabled]="disabled()"
          class="qt-input w-44 text-sm"
          [class.qt-input-error]="condition().key.trim() === ''"
          [attr.aria-label]="keyLabel()"
          [attr.title]="keyTitle()"
        />
      }

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
        @for (key of comparators(); track key) {
          <option [value]="key" [selected]="condition().comparator === key">
            {{ COMPARATOR_LABELS[key] }}
          </option>
        }
      </select>

      <!-- eq/neq alone need a type picker: with the stored type unknowable, only
           the author can say whether they mean 3, "3", or true. -->
      <qt-literal-operand-field
        [operand]="operand()"
        [disabled]="disabled()"
        [showTypePicker]="!ordering() && !containment() && !inventorySubject()"
        pickerClassName="w-24 shrink-0"
        textClassName="w-32"
        [stateOptions]="condition().subject === 'inventory' ? INVENTORY_STATES : undefined"
        (operandChange)="setOperand($event)"
      />

      @if ((ordering() || containment()) && !inventorySubject() && !aggregate()) {
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

  protected readonly INVENTORY_STATES = INVENTORY_STATE_VALUES;
  protected readonly COMPARATOR_LABELS = COMPARATOR_LABELS;

  readonly operand = computed(() => this.condition().operand);
  protected readonly comparators = computed(() => comparatorsFor(this.condition().subject));
  protected readonly inventorySubject = computed(
    () => this.condition().subject === 'inventory' || this.condition().subject === 'inventoryCount',
  );
  protected readonly aggregate = computed(() => this.condition().subject === 'inventoryAggregate');
  readonly ordering = computed(() => ORDERING_COMPARATORS.has(this.condition().comparator));
  readonly containment = computed(() => CONTAINMENT_COMPARATORS.has(this.condition().comparator));

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

  /** v4's per-subject placeholder, aria-label and title (`GateSection.tsx`). */
  protected readonly isProgress = computed(() => this.condition().subject === 'progress');

  protected readonly keyPlaceholder = computed(() =>
    this.isProgress()
      ? 'cannon.complete'
      : this.inventorySubject()
        ? 'Excalibur'
        : "key on the character's fact sheet",
  );

  protected readonly keyLabel = computed(() =>
    this.isProgress()
      ? 'Progress key'
      : this.inventorySubject()
        ? 'Inventory object reference'
        : 'Metadata key',
  );

  protected readonly keyTitle = computed(() =>
    this.isProgress()
      ? 'A progression of the invoking character, keyed "<id>.<field>" — percent, complete, remainingMs, state, started, elapsedMs, startTime, endTime, name, elapsed, remaining, quantity. A progression they do not carry never matches.'
      : this.inventorySubject()
        ? 'An object by id, title or name, written literally — a gate has no parameters to fill a placeholder. With no character or chat, the test never matches.'
        : "The invoking character's fact sheet — a key the character lacks never matches.",
  );

  protected onSubject(raw: string): void {
    this.conditionChange.emit(withSubject(this.condition(), raw as DraftGateCondition['subject']));
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
    this.conditionChange.emit({
      ...this.condition(),
      comparator,
      operand: reseatOperandForComparator(this.operand(), comparator),
    });
  }
}

// ---------------------------------------------------------------------------
// The section
// ---------------------------------------------------------------------------

@Component({
  selector: 'qt-gate-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, GateChip, ConditionGroups],
  template: `
    <section class="qt-card p-4 space-y-3">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <div>
          <h2 class="qt-card-title text-sm">Who may reach for it</h2>
          <p class="qt-hint">
            Tested before the deal, against the invoking character&rsquo;s
            <code>metadata.json</code>, their timed progressions, or what they carry in this chat —
            the only things known before a roll exists.
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
                ? 'Offered only to a character whose sheet passes the tests below:'
                : 'Withheld from a character whose sheet passes the tests below:'
            }}
          </p>

          <qt-condition-groups
            [conditions]="draft().gateConditions"
            [disabled]="disabled()"
            [chip]="chipTemplate"
            (joinerChange)="onJoinerChange($event.id, $event.joiner)"
          />
          <ng-template #chipTemplate let-condition>
            <qt-gate-chip
              [condition]="condition"
              [hasError]="conditionErrorIds().has(condition.id)"
              [disabled]="disabled()"
              (conditionChange)="updateCondition(condition.id, $event)"
              (remove)="deleteCondition(condition.id)"
            />
          </ng-template>

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
            Tests on one line must all hold; any line that holds opens the door.
          </p>
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
        subject: 'metadata',
        key: '',
        comparator: 'eq',
        operand: { kind: 'boolean', value: true },
      },
    ]);
  }

  protected updateCondition(id: string, next: DraftGateCondition): void {
    // One comparator object carries each key once, so a duplicate key+comparator
    // pair on one line is blocked at the door rather than silently overwriting
    // its twin. Another line is another object, and may repeat it.
    const slot = gateConditionSlotKey(next);
    const line = groupsOf(this.draft().gateConditions).find((group) => group.some((c) => c.id === id)) ?? [];
    const collision = line.some(
      (c) => c.id !== id && c.subject === next.subject && gateConditionSlotKey(c) === slot,
    );
    if (collision) {
      this.duplicateNotice.set(
        `A line of the gate can test “${next.key}” with ${COMPARATOR_LABELS[next.comparator]} only once.`,
      );
      return;
    }
    this.duplicateNotice.set(null);
    this.setConditions(this.draft().gateConditions.map((c) => (c.id === id ? next : c)));
  }

  /** The joiner toggle: clears the notice and sets the chip's `joiner`. */
  protected onJoinerChange(id: string, joiner: 'and' | 'or'): void {
    this.duplicateNotice.set(null);
    this.setConditions(this.draft().gateConditions.map((c) => (c.id === id ? { ...c, joiner } : c)));
  }

  protected deleteCondition(id: string): void {
    this.duplicateNotice.set(null);
    this.setConditions(this.draft().gateConditions.filter((c) => c.id !== id));
  }
}
