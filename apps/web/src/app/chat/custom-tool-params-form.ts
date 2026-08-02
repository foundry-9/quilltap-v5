import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  afterRenderEffect,
  computed,
  input,
  output,
  viewChild,
} from '@angular/core';

import type { CustomToolParameterSpec } from '../core/core-contract';

/**
 * The parameter form generated from a custom tool's `parameters` declarations —
 * a port of v4 `components/custom-tools/CustomToolParamsForm.tsx`.
 *
 * Extracted from the composer popup so the popup and Pascal's Workbench proving
 * bench render the very same form from the very same declarations — a third copy
 * would drift (v4's own reason for the split).
 *
 * Two layouts, one form (v4 `faab6881`). `inline` is the original cramped row —
 * label left, small input right — and remains the DEFAULT so the proving bench
 * is untouched. `stacked` is for a full dialog, where there is room to put the
 * label on its own line, show the parameter's description instead of hiding it
 * in a tooltip, and state the declared bounds rather than only enforcing them.
 *
 * Stacked string parameters get an auto-growing textarea rather than a single
 * line. Nothing in the format declares how long a string is expected to be —
 * `llm_request` and `colour` are the same declaration — so rather than guess
 * from the default's length, the field simply fits itself to whatever is in it,
 * from one line up to a cap, and scrolls past that.
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

/** Floor and ceiling for an auto-growing string field, in pixels. */
const TEXTAREA_MIN_HEIGHT = 40;
const TEXTAREA_MAX_HEIGHT = 224;

/**
 * Describe a numeric parameter's declared bounds, or nothing when it has none
 * (v4 `boundsHint`). Exported so the spec can pin the four arms directly.
 */
export function boundsHint(param: CustomToolParameterSpec): string | null {
  if (param.type === 'string' || param.type === 'boolean') return null;
  if (param.min !== undefined && param.max !== undefined) return `${param.min}–${param.max}`;
  if (param.min !== undefined) return `${param.min} or more`;
  if (param.max !== undefined) return `${param.max} or less`;
  return null;
}

/**
 * A textarea that fits itself to its content, up to {@link TEXTAREA_MAX_HEIGHT}
 * and then scrolls (v4 `AutoGrowTextarea`). The measurement runs in an
 * `afterRenderEffect` reading `value()` — v5's analog of v4's
 * `useEffect(…, [value])`: it fires once on mount and again on every value
 * change, after the DOM binding has been applied.
 */
@Component({
  selector: 'qt-auto-grow-textarea',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <textarea
      #ta
      [id]="id()"
      rows="1"
      [value]="value()"
      [disabled]="disabled()"
      class="qt-textarea w-full"
      [style.min-height.px]="minHeight"
      [style.max-height.px]="maxHeight"
      style="overflow-y: auto"
      (input)="valueChange.emit($any($event.target).value)"
    ></textarea>
  `,
})
export class AutoGrowTextarea {
  readonly id = input.required<string>();
  readonly value = input.required<string>();
  readonly disabled = input(false);
  readonly valueChange = output<string>();

  protected readonly minHeight = TEXTAREA_MIN_HEIGHT;
  protected readonly maxHeight = TEXTAREA_MAX_HEIGHT;

  private readonly ta = viewChild.required<ElementRef<HTMLTextAreaElement>>('ta');

  constructor() {
    afterRenderEffect(() => {
      // Touch the value so the measurement re-runs whenever it changes.
      this.value();
      const el = this.ta().nativeElement;
      // Collapse first, or scrollHeight reports the height it already has.
      el.style.height = 'auto';
      el.style.height = `${Math.min(Math.max(el.scrollHeight, TEXTAREA_MIN_HEIGHT), TEXTAREA_MAX_HEIGHT)}px`;
    });
  }
}

@Component({
  selector: 'qt-custom-tool-params-form',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [AutoGrowTextarea],
  template: `
    @for (entry of fields(); track entry.name) {
      @if (entry.param.type === 'boolean') {
        @if (!stacked()) {
          <label class="flex items-center gap-2 text-sm">
            <input
              [id]="entry.inputId"
              type="checkbox"
              [checked]="boolValue(entry.name)"
              [disabled]="disabled()"
              (change)="paramChange.emit({ param: entry.name, value: $any($event.target).checked })"
            />
            <span [title]="entry.param.description">{{ entry.name }}</span>
          </label>
        } @else {
          <label class="flex items-start gap-2 cursor-pointer">
            <input
              [id]="entry.inputId"
              type="checkbox"
              class="mt-1"
              [checked]="boolValue(entry.name)"
              [disabled]="disabled()"
              (change)="paramChange.emit({ param: entry.name, value: $any($event.target).checked })"
            />
            <span class="min-w-0">
              <span class="block text-sm font-medium font-mono">{{ entry.name }}</span>
              @if (entry.param.description) {
                <span class="block text-xs qt-text-secondary">{{ entry.param.description }}</span>
              }
            </span>
          </label>
        }
      } @else if (!stacked()) {
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
            (input)="paramChange.emit({ param: entry.name, value: $any($event.target).value })"
          />
        </div>
      } @else {
        <div>
          <label [attr.for]="entry.inputId" class="flex items-baseline gap-2 mb-1.5">
            <span class="text-sm font-medium font-mono">{{ entry.name }}</span>
            <span class="text-xs qt-text-secondary">
              {{ entry.param.type }}{{ entry.bounds ? ' · ' + entry.bounds : '' }}
            </span>
          </label>
          @if (entry.param.description) {
            <p class="text-xs qt-text-secondary mb-2">{{ entry.param.description }}</p>
          }
          @if (entry.param.type === 'string') {
            <qt-auto-grow-textarea
              [id]="entry.inputId"
              [value]="textValue(entry.name)"
              [disabled]="disabled()"
              (valueChange)="paramChange.emit({ param: entry.name, value: $event })"
            />
          } @else {
            <input
              [id]="entry.inputId"
              type="number"
              [value]="textValue(entry.name)"
              [attr.min]="entry.param.min"
              [attr.max]="entry.param.max"
              [disabled]="disabled()"
              class="qt-input w-32"
              (input)="paramChange.emit({ param: entry.name, value: $any($event.target).value })"
            />
          }
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
  /** `inline` (default) is the compact row; `stacked` is the roomy dialog form. */
  readonly layout = input<'inline' | 'stacked'>('inline');

  readonly paramChange = output<ParamChange>();

  protected readonly stacked = computed(() => this.layout() === 'stacked');

  protected readonly fields = computed(() =>
    Object.entries(this.parameters()).map(([name, param]) => ({
      name,
      param,
      inputId: `${this.idPrefix()}-${name}`,
      bounds: boundsHint(param),
    })),
  );

  protected boolValue(name: string): boolean {
    return Boolean(this.values()[name]);
  }

  protected textValue(name: string): string {
    return String(this.values()[name] ?? '');
  }
}
