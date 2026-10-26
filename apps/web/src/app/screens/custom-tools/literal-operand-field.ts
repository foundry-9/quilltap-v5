import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { LiteralOperand } from '../../pascal/tool-draft';

/**
 * LiteralOperandField — the literal right-hand side of a condition chip, as
 * both the outcome table and the availability gate render it (v4
 * `components/custom-tools/LiteralOperandField.tsx`): an optional number / text
 * / true-false picker, then the input that kind takes.
 *
 * The picker exists for eq/neq against a subject whose stored type is
 * unknowable at authoring time (a metadata key, the consult's answer): only the
 * author can say whether they mean 3, "3", or true. Switching kinds keeps typed
 * text between number and text and starts a boolean at `true`.
 *
 * An Inventory state test passes `stateOptions`: its operand is always text,
 * drawn from the four states, so it renders as a select of them instead.
 *
 * v5 inlined this twice (the gate chip and the outcome table's operand field);
 * P4.D334 extracted it (R-4) — the markup moved byte for byte and the
 * `stateOptions` arm is the pin's addition.
 *
 * Every select here is selected-per-option rather than a value binding (the
 * standing dogfood-#6 rule).
 */

/** The operand a picker choice produces from the one being replaced. */
export function literalOfKind(kind: LiteralOperand['kind'], current: LiteralOperand): LiteralOperand {
  if (kind === 'number') return { kind: 'number', text: current.kind === 'string' ? current.text : '' };
  if (kind === 'string') return { kind: 'string', text: current.kind === 'number' ? current.text : '' };
  return { kind: 'boolean', value: true };
}

@Component({
  selector: 'qt-literal-operand-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (stateOptions(); as states) {
      <select
        (change)="operandChange.emit({ kind: 'string', text: selectValue($event) })"
        [disabled]="disabled()"
        class="qt-select qt-select-sm w-28"
        aria-label="Inventory state"
      >
        @if (!states.includes(stateText())) {
          <option value="" selected>—</option>
        }
        @for (state of states; track state) {
          <option [value]="state" [selected]="stateText() === state">{{ state }}</option>
        }
      </select>
    } @else {
      @if (showTypePicker()) {
        <select
          (change)="operandChange.emit(literalOfKind(selectValue($event), operand()))"
          [disabled]="disabled()"
          [attr.class]="pickerClasses()"
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
            (change)="operandChange.emit({ kind: 'boolean', value: selectValue($event) === 'true' })"
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
            (input)="operandChange.emit({ kind: 'string', text: inputValue($event) })"
            [disabled]="disabled()"
            [attr.class]="'qt-input ' + textClassName() + ' text-sm'"
            aria-label="Operand text"
          />
        }
        @default {
          <input
            type="number"
            step="any"
            [value]="operandText()"
            (input)="operandChange.emit({ kind: 'number', text: inputValue($event) })"
            [disabled]="disabled()"
            class="qt-input w-24 text-sm"
            aria-label="Operand number"
          />
        }
      }
    }
  `,
})
export class LiteralOperandField {
  readonly operand = input.required<LiteralOperand>();
  readonly disabled = input(false);
  /** Show the number / text / true-false picker before the input. */
  readonly showTypePicker = input(false);
  /** Extra classes on the picker — the gate pins its width so the chip stays on one line. */
  readonly pickerClassName = input('');
  /** Extra classes on the text input; the two chips size it differently. */
  readonly textClassName = input('w-28');
  /** A closed vocabulary for a text operand (the Inventory states), rendered as a select. */
  readonly stateOptions = input<readonly string[] | undefined>(undefined);

  readonly operandChange = output<LiteralOperand>();

  protected readonly literalOfKind = (raw: string, current: LiteralOperand) =>
    literalOfKind(raw as LiteralOperand['kind'], current);

  protected readonly pickerClasses = computed(() =>
    `qt-select qt-select-sm ${this.pickerClassName()}`.trim(),
  );

  protected readonly stateText = computed(() => {
    const o = this.operand();
    return o.kind === 'string' ? o.text : '';
  });

  protected operandText(): string {
    const o = this.operand();
    return o.kind === 'boolean' ? '' : o.text;
  }

  protected booleanValue(): boolean {
    const o = this.operand();
    return o.kind === 'boolean' ? o.value : false;
  }

  protected inputValue(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }

  protected selectValue(event: Event): string {
    return (event.target as HTMLSelectElement).value;
  }
}
