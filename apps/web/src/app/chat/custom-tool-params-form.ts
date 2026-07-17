import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { CustomToolParameterSpec } from '../core/core-contract';

/**
 * The parameter form generated from a custom tool's `parameters` declarations —
 * a port of v4 `components/custom-tools/CustomToolParamsForm.tsx`.
 *
 * Extracted from the composer popup so the popup and (someday) Pascal's
 * Workbench proving bench render the very same form from the very same
 * declarations — a third copy would drift (v4's own reason for the split).
 *
 * Values are held loosely (text inputs yield strings) and coerced back to the
 * declared types on use, via {@link coerceParamValues}.
 */

/** Form values are held loosely (text inputs yield strings) and coerced on use. */
export type ParameterFormValues = Record<string, string | boolean>;

/** Seed a form from the declared defaults (v4 `initialParamValues`). */
export function initialParamValues(
  parameters: Record<string, CustomToolParameterSpec>,
): ParameterFormValues {
  const values: ParameterFormValues = {};
  for (const [name, param] of Object.entries(parameters)) {
    values[name] = param.type === 'boolean' ? Boolean(param.default) : String(param.default);
  }
  return values;
}

/**
 * Coerce the form's loose values back to the declared types (v4
 * `coerceParamValues`). Blank or unparseable numbers fall back to the declared
 * default rather than sending NaN — the server validates properly; this only
 * keeps the payload well-typed.
 */
export function coerceParamValues(
  parameters: Record<string, CustomToolParameterSpec>,
  values: ParameterFormValues,
): Record<string, number | string | boolean> {
  const out: Record<string, number | string | boolean> = {};
  for (const [name, param] of Object.entries(parameters)) {
    const raw = values[name];
    if (param.type === 'boolean') {
      out[name] = Boolean(raw);
      continue;
    }
    if (param.type === 'string') {
      out[name] = typeof raw === 'string' ? raw : String(param.default);
      continue;
    }
    const parsed = param.type === 'integer' ? parseInt(String(raw), 10) : parseFloat(String(raw));
    out[name] = Number.isFinite(parsed) ? parsed : Number(param.default);
  }
  return out;
}

/** One field's change (v4 `onChange(param, value)`). */
export interface ParamChange {
  param: string;
  value: string | boolean;
}

@Component({
  selector: 'qt-custom-tool-params-form',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @for (entry of fields(); track entry.name) {
      @if (entry.param.type === 'boolean') {
        <label class="flex items-center gap-2 text-sm">
          <input
            [id]="entry.inputId"
            type="checkbox"
            [checked]="boolValue(entry.name)"
            [disabled]="disabled()"
            (change)="change.emit({ param: entry.name, value: $any($event.target).checked })"
          />
          <span [title]="entry.param.description">{{ entry.name }}</span>
        </label>
      } @else {
        <div class="flex items-center justify-between gap-2">
          <label [attr.for]="entry.inputId" class="text-sm" [title]="entry.param.description">
            {{ entry.name }}
          </label>
          <input
            [id]="entry.inputId"
            [type]="entry.param.type === 'string' ? 'text' : 'number'"
            [value]="textValue(entry.name)"
            [attr.min]="entry.param.type === 'string' ? null : entry.param.min"
            [attr.max]="entry.param.type === 'string' ? null : entry.param.max"
            [disabled]="disabled()"
            [class]="entry.param.type === 'string' ? 'qt-input w-40' : 'qt-input w-20'"
            (input)="change.emit({ param: entry.name, value: $any($event.target).value })"
          />
        </div>
      }
    }
  `,
})
export class CustomToolParamsForm {
  readonly parameters = input.required<Record<string, CustomToolParameterSpec>>();
  readonly values = input.required<ParameterFormValues>();
  readonly disabled = input(false);
  /** Unique prefix for input ids, so two forms on one page never collide. */
  readonly idPrefix = input.required<string>();

  readonly change = output<ParamChange>();

  protected readonly fields = computed(() =>
    Object.entries(this.parameters()).map(([name, param]) => ({
      name,
      param,
      inputId: `${this.idPrefix()}-${name}`,
    })),
  );

  protected boolValue(name: string): boolean {
    return Boolean(this.values()[name]);
  }

  protected textValue(name: string): string {
    return String(this.values()[name] ?? '');
  }
}
