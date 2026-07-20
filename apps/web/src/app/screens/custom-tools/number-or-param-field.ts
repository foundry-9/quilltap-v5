import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { NumberOrParamValue } from '../../pascal/tool-draft';
import { Icon } from '../../ui/icon';

/**
 * NumberOrParamField — the reusable control at the heart of the Workbench (v4
 * `components/custom-tools/NumberOrParamField.tsx`, 94).
 *
 * Every numeric field that accepts a `{ "$param": … }` reference (roll fields,
 * comparator operands) renders as one of these: a literal number input with a
 * toggle to parameter mode, where a select lists only the type-compatible
 * declared parameters. When no eligible parameter exists the toggle is DISABLED
 * with a hint — the schema's rules made visible, never violated.
 */
@Component({
  selector: 'qt-number-or-param-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="flex items-center gap-1">
      @if (value().kind === 'literal') {
        <input
          type="number"
          [value]="literalText()"
          (input)="onLiteralInput($event)"
          [placeholder]="placeholder() ?? ''"
          [disabled]="disabled()"
          class="qt-input w-24"
          [class.qt-input-error]="hasError()"
          [attr.aria-label]="label()"
          step="any"
        />
      } @else if (value().kind === 'state') {
        <span
          class="qt-text-xs qt-text-secondary font-mono rounded qt-bg-muted px-2 py-1"
          [class.qt-input-error]="hasError()"
          title="This field draws from persistent state. Edit $state references in the raw JSON, or swap to a literal."
        >
          {{ statePillText() }}
        </span>
      } @else {
        <select
          [value]="paramName()"
          (change)="onParamSelect($event)"
          [disabled]="disabled()"
          class="qt-select qt-select-sm w-28"
          [class.qt-input-error]="hasError()"
          [attr.aria-label]="label() + ' parameter'"
        >
          <!--
            The held name is offered even when it is NOT eligible, so a
            reference that a type change invalidated stays visible (and stays an
            error) instead of silently re-pointing at whatever sorts first.
          -->
          @if (!paramNames().includes(paramName())) {
            <option [value]="paramName()">{{ paramName() || '—' }}</option>
          }
          @for (name of paramNames(); track name) {
            <option [value]="name">{{ name }}</option>
          }
        </select>
      }
      <button
        type="button"
        (click)="toggle()"
        [disabled]="disabled() || (value().kind === 'literal' && noParams())"
        class="qt-button qt-button-ghost qt-button-sm"
        [title]="toggleTitle()"
        [attr.aria-label]="'Toggle ' + label() + ' between a literal and a parameter reference'"
      >
        <qt-icon name="swap" class="w-3.5 h-3.5" />
      </button>
    </div>
  `,
})
export class NumberOrParamField {
  readonly value = input.required<NumberOrParamValue>();
  /** Parameters eligible for reference (already type-filtered by the caller). */
  readonly paramNames = input.required<string[]>();
  /** Placeholder shown while a literal is blank (e.g. the field's default). */
  readonly placeholder = input<string>();
  readonly hasError = input(false);
  readonly disabled = input(false);
  readonly label = input.required<string>();

  readonly valueChange = output<NumberOrParamValue>();

  readonly noParams = computed(() => this.paramNames().length === 0);

  protected literalText(): string {
    const v = this.value();
    return v.kind === 'literal' ? v.text : '';
  }

  protected paramName(): string {
    const v = this.value();
    return v.kind === 'param' ? v.name : '';
  }

  /** The read-only pill for a carried `$state` roll field (v4's markup). */
  protected statePillText(): string {
    const v = this.value();
    return v.kind === 'state' ? `$state: ${v.path} → ${v.fallback}` : '';
  }

  protected toggleTitle(): string {
    const v = this.value();
    if (v.kind === 'literal') {
      return this.noParams()
        ? 'Declare a numeric parameter first'
        : 'Use a parameter instead of a number';
    }
    return v.kind === 'state'
      ? 'Replace this $state reference with a literal number'
      : 'Use a literal number instead';
  }

  protected onLiteralInput(event: Event): void {
    this.valueChange.emit({ kind: 'literal', text: (event.target as HTMLInputElement).value });
  }

  protected onParamSelect(event: Event): void {
    this.valueChange.emit({ kind: 'param', name: (event.target as HTMLSelectElement).value });
  }

  protected toggle(): void {
    const v = this.value();
    this.valueChange.emit(
      v.kind === 'literal'
        ? { kind: 'param', name: this.paramNames()[0] ?? '' }
        : { kind: 'literal', text: '' },
    );
  }
}
