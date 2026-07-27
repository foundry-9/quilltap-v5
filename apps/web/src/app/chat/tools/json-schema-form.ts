import { ChangeDetectionStrategy, Component, computed, effect, input, output, signal } from '@angular/core';

/**
 * The JSON-Schema subset v4 renders (`components/chat/JsonSchemaForm.tsx:6-18`).
 */
export interface SchemaProperty {
  type?: string;
  description?: string;
  enum?: (string | number)[];
  default?: unknown;
  minimum?: number;
  maximum?: number;
  minLength?: number;
  maxLength?: number;
  oneOf?: SchemaProperty[];
}

export interface ParametersSchema {
  type?: string;
  properties: Record<string, SchemaProperty>;
  required?: string[];
}

export type FormValues = Record<string, unknown>;

/** v4 `validateAll` (`JsonSchemaForm.tsx:54-73`). */
export function validateValues(
  schema: ParametersSchema,
  values: FormValues,
  includedOptionals: ReadonlySet<string>,
): boolean {
  const required = new Set(schema.required ?? []);
  for (const key of required) {
    const val = values[key];
    if (val === undefined || val === null || val === '') return false;
  }
  for (const [key, prop] of Object.entries(schema.properties)) {
    if (!required.has(key) && !includedOptionals.has(key)) continue;
    const val = values[key];
    if ((prop.type === 'number' || prop.type === 'integer') && val !== undefined && val !== '') {
      const num = Number(val);
      if (Number.isNaN(num)) return false;
      if (prop.minimum !== undefined && num < prop.minimum) return false;
      if (prop.maximum !== undefined && num > prop.maximum) return false;
    }
  }
  return true;
}

/** v4's optional pre-inclusion rule (`:41-51`). */
export function initialIncludedOptionals(
  schema: ParametersSchema,
  values: FormValues,
): Set<string> {
  const included = new Set<string>();
  const required = new Set(schema.required ?? []);
  for (const [key, prop] of Object.entries(schema.properties)) {
    if (!required.has(key) && (prop.default !== undefined || values[key] !== undefined)) {
      included.add(key);
    }
  }
  return included;
}

/** v4's long-text heuristic (`:236-239`). */
export function isLongText(prop: SchemaProperty): boolean {
  const d = prop.description?.toLowerCase() ?? '';
  return (
    d.includes('description') ||
    d.includes('content') ||
    d.includes('body') ||
    (prop.maxLength !== undefined && prop.maxLength > 500)
  );
}

/**
 * A form generated from a tool's `parameters` JSON Schema (v4
 * `components/chat/JsonSchemaForm.tsx`, 382 LOC).
 *
 * **This is not the Workbench's form.** `chat/custom-tool-params-form.ts` renders
 * Pascal's OWN parameter format (`CustomToolParameterSpec`), a different, much
 * narrower declaration language; nothing in v5 rendered JSON Schema before this.
 * (The work order's "reuse the Workbench schema-form family" reads on the
 * Workbench's `builder-form.ts`, which is Pascal's format editor rather than a
 * schema renderer — recorded in the lane record.)
 *
 * Ported: required fields sorted first (`:263-267`); optionals gated by their own
 * checkbox, pre-included when they carry a default or an existing value
 * (`:41-51`), and REMOVED from the values when unticked (`:105-110`); the input
 * chosen by type — enum → select, boolean → checkbox, number/integer → number
 * with its bounds and step, `oneOf` → a radio group that clears the value when
 * the variant changes (`:281-296`), object/array → a JSON textarea that keeps the
 * raw string while it does not parse (`:207-227`), and string → text or textarea
 * by v4's description heuristic.
 */
@Component({
  selector: 'qt-json-schema-form',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="space-y-1">
      @for (key of sortedKeys(); track key) {
        @let prop = schema().properties[key];
        @let req = isRequired(key);
        <div class="mb-3">
          <div class="flex items-center gap-2 mb-1">
            @if (!req) {
              <input
                type="checkbox"
                class="qt-checkbox"
                [id]="'include-' + key"
                [checked]="included().has(key)"
                (change)="toggleOptional(key, $any($event.target).checked)"
              />
            }
            <label [attr.for]="req ? 'field-' + key : 'include-' + key" class="text-sm font-medium qt-text">
              {{ key }}@if (req) {<span class="qt-text-destructive ml-0.5">*</span>}
            </label>
          </div>

          @if (prop.description) {
            <p class="text-xs qt-text-secondary mb-1.5">{{ prop.description }}</p>
          }

          @if (req || included().has(key)) {
            @if (prop.type === 'string' && prop.enum) {
              <select
                [id]="'field-' + key"
                class="qt-select w-full text-sm"
                [value]="text(key, prop)"
                (change)="set(key, $any($event.target).value)"
              >
                <option value="">Select...</option>
                @for (opt of prop.enum; track opt) {
                  <option [value]="opt" [selected]="text(key, prop) === str(opt)">{{ opt }}</option>
                }
              </select>
            } @else if (prop.type === 'boolean') {
              <label class="flex items-center gap-2 text-sm qt-text">
                <input
                  type="checkbox"
                  class="qt-checkbox"
                  [attr.aria-label]="key"
                  [checked]="bool(key, prop)"
                  (change)="set(key, $any($event.target).checked)"
                />
                <span>Enabled</span>
              </label>
            } @else if (prop.type === 'number' || prop.type === 'integer') {
              <input
                [id]="'field-' + key"
                type="number"
                class="qt-input w-full text-sm"
                [value]="text(key, prop)"
                [attr.min]="prop.minimum"
                [attr.max]="prop.maximum"
                [attr.step]="prop.type === 'integer' ? 1 : 'any'"
                [attr.placeholder]="prop.default !== undefined ? 'Default: ' + prop.default : null"
                (input)="setNumber(key, prop, $any($event.target).value)"
              />
            } @else if (prop.oneOf && prop.oneOf.length > 0) {
              <!-- v4 OneOfField (:277-382): a radio per variant (only when
                   there is more than one), and the ACTIVE variant's own input.
                   Switching variant clears the value. -->
              <div class="space-y-2">
                @if (prop.oneOf.length > 1) {
                  <div class="flex gap-3 flex-wrap">
                    @for (variant of prop.oneOf; track $index) {
                      <label class="flex items-center gap-1.5 text-sm qt-text cursor-pointer">
                        <input
                          type="radio"
                          class="qt-radio"
                          [name]="'oneOf-' + key"
                          [checked]="variantIndex(key, prop) === $index"
                          (change)="chooseVariant(key, $index)"
                        />
                        {{ variantLabel(variant, $index) }}
                      </label>
                    }
                  </div>
                }
                @let active = prop.oneOf[variantIndex(key, prop)];
                @if (active.enum) {
                  <select
                    class="qt-select w-full text-sm"
                    [attr.aria-label]="key"
                    [value]="rawText(key)"
                    (change)="set(key, $any($event.target).value)"
                  >
                    <option value="">Select...</option>
                    @for (opt of active.enum; track opt) {
                      <option [value]="opt" [selected]="rawText(key) === str(opt)">
                        {{ opt }}
                      </option>
                    }
                  </select>
                } @else if (active.type === 'integer' || active.type === 'number') {
                  <input
                    type="number"
                    class="qt-input w-full text-sm"
                    [attr.aria-label]="key"
                    [value]="rawText(key)"
                    [attr.min]="active.minimum"
                    [attr.max]="active.maximum"
                    [attr.step]="active.type === 'integer' ? 1 : 'any'"
                    (input)="setNumber(key, active, $any($event.target).value)"
                  />
                } @else {
                  <input
                    type="text"
                    class="qt-input w-full text-sm"
                    [attr.aria-label]="key"
                    [value]="rawText(key)"
                    (input)="set(key, $any($event.target).value)"
                  />
                }
              </div>
            } @else if (prop.type === 'object' || prop.type === 'array') {
              <textarea
                [id]="'field-' + key"
                class="qt-input w-full text-sm font-mono"
                rows="4"
                placeholder="Enter valid JSON..."
                [value]="jsonText(key)"
                (input)="setJson(key, $any($event.target).value)"
              ></textarea>
            } @else if (longText(prop)) {
              <textarea
                [id]="'field-' + key"
                class="qt-input w-full text-sm"
                rows="3"
                [attr.maxlength]="prop.maxLength"
                [attr.placeholder]="prop.default !== undefined ? 'Default: ' + prop.default : null"
                [value]="text(key, prop)"
                (input)="set(key, $any($event.target).value)"
              ></textarea>
            } @else {
              <input
                [id]="'field-' + key"
                type="text"
                class="qt-input w-full text-sm"
                [attr.maxlength]="prop.maxLength"
                [attr.placeholder]="prop.default !== undefined ? 'Default: ' + prop.default : null"
                [value]="text(key, prop)"
                (input)="set(key, $any($event.target).value)"
              />
            }
          }
        </div>
      }
    </div>
  `,
})
export class JsonSchemaForm {
  readonly schema = input.required<ParametersSchema>();
  readonly values = input.required<FormValues>();

  readonly valuesChange = output<FormValues>();
  readonly validChange = output<boolean>();

  protected readonly included = signal<ReadonlySet<string>>(new Set());
  private seeded = false;

  constructor() {
    effect(() => {
      const schema = this.schema();
      if (!this.seeded) {
        this.seeded = true;
        this.included.set(initialIncludedOptionals(schema, this.values()));
      }
      this.validChange.emit(validateValues(schema, this.values(), this.included()));
    });
  }

  protected readonly sortedKeys = computed(() => {
    const required = new Set(this.schema().required ?? []);
    return Object.keys(this.schema().properties).sort(
      (a, b) => (required.has(a) ? 0 : 1) - (required.has(b) ? 0 : 1),
    );
  });

  protected str(value: string | number): string {
    return String(value);
  }

  protected isRequired(key: string): boolean {
    return (this.schema().required ?? []).includes(key);
  }

  protected longText(prop: SchemaProperty): boolean {
    return isLongText(prop);
  }

  protected text(key: string, prop: SchemaProperty): string {
    const v = this.values()[key];
    return String(v ?? prop.default ?? '');
  }

  protected bool(key: string, prop: SchemaProperty): boolean {
    return Boolean(this.values()[key] ?? prop.default ?? false);
  }

  /** The value as typed, with no default fallback (v4's `oneOf` inputs). */
  protected rawText(key: string): string {
    const v = this.values()[key];
    return v === undefined ? '' : String(v);
  }

  private readonly variantChoice = signal<Record<string, number>>({});

  /** v4 `OneOfField`'s initial variant (`:290-300`) — matched by the value's type. */
  protected variantIndex(key: string, prop: SchemaProperty): number {
    const chosen = this.variantChoice()[key];
    if (chosen !== undefined) return chosen;
    const value = this.values()[key];
    const variants = prop.oneOf ?? [];
    if (value === undefined) return 0;
    for (let i = 0; i < variants.length; i++) {
      const v = variants[i];
      if (v.type === 'string' && typeof value === 'string') return i;
      if ((v.type === 'integer' || v.type === 'number') && typeof value === 'number') return i;
    }
    return 0;
  }

  /** v4 resets the value on a variant switch (`:315`). */
  protected chooseVariant(key: string, index: number): void {
    this.variantChoice.update((prev) => ({ ...prev, [key]: index }));
    this.set(key, undefined);
  }

  /** v4 `getVariantLabel` (`:304-309`). */
  protected variantLabel(variant: SchemaProperty, index: number): string {
    if (variant.enum) return variant.enum.join(' / ');
    if (variant.type === 'integer' || variant.type === 'number') {
      return variant.minimum !== undefined
        ? `Number (${variant.minimum}-${variant.maximum ?? '...'})`
        : 'Number';
    }
    if (variant.type === 'string') return 'Text';
    return `Option ${index + 1}`;
  }

  protected jsonText(key: string): string {
    const v = this.values()[key];
    if (v === undefined) return '';
    return typeof v === 'string' ? v : JSON.stringify(v, null, 2);
  }

  protected set(key: string, value: unknown): void {
    this.valuesChange.emit({ ...this.values(), [key]: value });
  }

  /** v4 parses on every keystroke and drops the key entirely when blank (`:187-200`). */
  protected setNumber(key: string, prop: SchemaProperty, raw: string): void {
    if (raw === '') {
      this.set(key, undefined);
      return;
    }
    this.set(key, prop.type === 'integer' ? parseInt(raw, 10) : parseFloat(raw));
  }

  /** v4 keeps the raw string while it does not parse, so typing is not fought (`:212-221`). */
  protected setJson(key: string, raw: string): void {
    try {
      this.set(key, JSON.parse(raw));
    } catch {
      this.set(key, raw);
    }
  }

  /** v4 `handleToggleOptional` (`:89-111`). */
  protected toggleOptional(key: string, include: boolean): void {
    const next = new Set(this.included());
    if (include) {
      next.add(key);
      this.included.set(next);
      const prop = this.schema().properties[key];
      if (prop?.default !== undefined && this.values()[key] === undefined) {
        this.set(key, prop.default);
      }
    } else {
      next.delete(key);
      this.included.set(next);
      const values = { ...this.values() };
      delete values[key];
      this.valuesChange.emit(values);
    }
  }
}
