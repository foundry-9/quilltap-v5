import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  afterRenderEffect,
  computed,
  input,
  output,
  viewChildren,
} from '@angular/core';

import type {
  ProviderOptionEnumValue,
  ProviderOptionField,
  ProviderOptionsSchema,
} from './provider-options-schema';

/** One rendered choice inside a `multi-enum` field. */
interface PreparedChoice {
  value: string;
  label: string;
  selected: boolean;
  disabled: boolean;
}

/** A field with everything the template needs already resolved. */
interface PreparedField {
  field: ProviderOptionField;
  /** The stored value, falling back to the schema `default` for display. */
  value: unknown;
  /** `multi-enum` only: the resolved choice list; empty for every other type. */
  choices: PreparedChoice[];
  /** `multi-enum` only: the currently selected values. */
  selected: string[];
}

interface PreparedGroup {
  title?: string;
  helpText?: string;
  fields: PreparedField[];
}

/**
 * Generic renderer for the provider-options schema each LLM plugin declares
 * (v4 `components/settings/connection-profiles/ProviderOptionsPanel.tsx`,
 * ported against v4's CLIENT as the oracle).
 *
 * Reads values from the flat `parameters` record — the same JSON blob stored on
 * a ConnectionProfile and handed back to the plugin as
 * `profileParameters` — and writes back one key at a time through
 * `setParameter`. The host merges that write into the PRESERVED bag; the panel
 * never rebuilds it, so keys nothing renders a control for survive a save.
 *
 * Semantics carried verbatim from v4:
 * - Nothing renders at all when the schema is absent or has no groups
 *   (`:57`).
 * - `showIf` compares the RAW stored value (`:39-41`) — a sibling whose value
 *   is only a schema `default` does not satisfy the guard, because the default
 *   is a display fallback and was never written into the bag.
 * - `default` is that display fallback alone (`:43-47`); neither app seeds the
 *   bag on a new profile.
 * - A `boolean` is checked on `value === true` (`:149`) — a hand-edited
 *   `'true'` STRING shows OFF, and shows off in v4 too. (The P4.D81 hardcoded
 *   Enable Thinking row tolerated the string; that divergence retires with the
 *   row. Untouched, the string still rides the bag to the wire, where
 *   `build_ollama_body` does accept it — nothing is silently "fixed".)
 * - A cleared `number` emits `undefined`, which DELETES the key (`:302-303`);
 *   an unparseable one stores the raw string (`:306`).
 * - `multi-enum` with zero resolvable choices renders nothing (`:232`), and
 *   `fetchedModels` choices exclude the profile's own model and cap at 50
 *   (`:220-224`).
 *
 * v4's panel also takes an `onDirective` callback for fields tagged
 * `affects` — deliberately NOT ported, because v4's only caller never passes
 * one: `ProfileModal` derives the model-input swap straight from
 * `parameters.useCustomModel === true` (`ProfileModal.tsx:198-203`, and its
 * comment says why). v5's modal derives it the same way, so an output nothing
 * subscribes to would be pure cruft.
 */
@Component({
  selector: 'qt-provider-options-panel',
  changeDetection: ChangeDetectionStrategy.OnPush,
  styles: [':host { display: block; }'],
  template: `
    @if (groups().length > 0) {
      <div class="space-y-4">
        @for (group of groups(); track $index) {
          <div class="qt-settings-shell">
            @if (group.title) {
              <h4 class="qt-settings-section-heading mb-3">{{ group.title }}</h4>
            }
            <div class="space-y-3">
              @for (row of group.fields; track row.field.key) {
                @switch (row.field.type) {
                  @case ('boolean') {
                    <div class="flex items-start gap-2">
                      <input
                        type="checkbox"
                        [id]="fieldId(row.field)"
                        class="qt-checkbox mt-0.5"
                        [checked]="row.value === true"
                        (change)="emit(row.field, $any($event.target).checked)"
                      />
                      <div class="flex flex-col gap-1">
                        <label [for]="fieldId(row.field)" class="text-sm">{{
                          row.field.label
                        }}</label>
                        @if (row.field.helpText) {
                          <p class="qt-text-xs">{{ row.field.helpText }}</p>
                        }
                      </div>
                    </div>
                  }
                  @case ('enum') {
                    <div>
                      <label [for]="fieldId(row.field)" class="qt-text-label-xs">{{
                        row.field.label
                      }}</label>
                      <select
                        #enumSelect
                        [id]="fieldId(row.field)"
                        [attr.data-qt-value]="asString(row.value)"
                        class="qt-select text-sm"
                        (change)="emit(row.field, $any($event.target).value)"
                      >
                        @for (option of enumValues(row.field); track option.value) {
                          <option [value]="option.value">{{ option.label }}</option>
                        }
                      </select>
                      @if (row.field.helpText) {
                        <p class="qt-text-xs mt-1">{{ row.field.helpText }}</p>
                      }
                    </div>
                  }
                  @case ('multi-enum') {
                    <div>
                      <label class="block qt-text-label">{{ row.field.label }}</label>
                      <div
                        class="space-y-1 max-h-32 overflow-y-auto border qt-border-default rounded p-2 bg-background mt-1"
                      >
                        @for (choice of row.choices; track choice.value) {
                          <label
                            [class]="
                              choice.disabled
                                ? 'flex items-center gap-2 p-1 rounded cursor-not-allowed opacity-50'
                                : 'flex items-center gap-2 p-1 rounded cursor-pointer hover:qt-bg-muted'
                            "
                          >
                            <input
                              type="checkbox"
                              class="qt-checkbox"
                              [checked]="choice.selected"
                              [disabled]="choice.disabled"
                              (change)="toggleChoice(row, choice, $any($event.target).checked)"
                            />
                            <span class="qt-text-xs text-foreground truncate">{{
                              choice.label
                            }}</span>
                          </label>
                        }
                      </div>
                      @if (row.field.helpText) {
                        <p class="qt-text-xs mt-1">{{ row.field.helpText }}</p>
                      }
                    </div>
                  }
                  @case ('number') {
                    <div>
                      <label [for]="fieldId(row.field)" class="qt-text-label-xs">{{
                        row.field.label
                      }}</label>
                      <input
                        [id]="fieldId(row.field)"
                        type="number"
                        class="qt-input text-sm"
                        [value]="asNumericText(row.value)"
                        (input)="emitNumber(row.field, $any($event.target).value)"
                      />
                      @if (row.field.helpText) {
                        <p class="qt-text-xs mt-1">{{ row.field.helpText }}</p>
                      }
                    </div>
                  }
                  @case ('string') {
                    <div>
                      <label [for]="fieldId(row.field)" class="qt-text-label-xs">{{
                        row.field.label
                      }}</label>
                      <input
                        [id]="fieldId(row.field)"
                        type="text"
                        class="qt-input text-sm"
                        [value]="asString(row.value)"
                        (input)="emit(row.field, $any($event.target).value)"
                      />
                      @if (row.field.helpText) {
                        <p class="qt-text-xs mt-1">{{ row.field.helpText }}</p>
                      }
                    </div>
                  }
                }
              }
            </div>
            @if (group.helpText) {
              <p class="qt-text-xs mt-3">{{ group.helpText }}</p>
            }
          </div>
        }
      </div>
    }
  `,
})
export class ProviderOptionsPanel {
  readonly schema = input<ProviderOptionsSchema | null>(null);
  readonly parameters = input<Record<string, unknown>>({});
  /** Model ids the host has fetched, for `multiEnumSource: 'fetchedModels'`. */
  readonly fetchedModels = input<string[]>([]);
  /** The model the profile is bound to — excluded from fetched-model choices. */
  readonly modelName = input<string>('');

  /** One key/value write into the host's `parameters` bag (v4 `onSetParameter`). */
  readonly setParameter = output<{ key: string; value: unknown }>();

  private readonly enumSelects = viewChildren<ElementRef<HTMLSelectElement>>('enumSelect');

  constructor() {
    // v4 writes `<select value={stringValue}>` and React assigns that value
    // AFTER the options mount, so a stored value outside the option list
    // leaves `selectedIndex` at -1 and the control blank rather than snapping
    // to the first row. An Angular `[value]` binding on the select runs before
    // the `@for` fills it, so the assignment is made here instead — reading
    // `groups()` so it re-runs on every schema or bag change.
    afterRenderEffect(() => {
      this.groups();
      for (const ref of this.enumSelects()) {
        const select = ref.nativeElement;
        select.value = select.dataset['qtValue'] ?? '';
      }
    });
  }

  protected readonly groups = computed<PreparedGroup[]>(() => {
    const schema = this.schema();
    if (!schema || schema.groups.length === 0) return [];
    const parameters = this.parameters();
    return schema.groups.map((group) => ({
      title: group.title,
      helpText: group.helpText,
      fields: group.fields
        .filter((field) => this.shouldRender(field, parameters))
        .map((field) => this.prepare(field, parameters))
        // v4's `MultiEnumField` bails to null when nothing is selectable
        // (`ProviderOptionsPanel.tsx:232`), which draws no row.
        .filter((row) => row.field.type !== 'multi-enum' || row.choices.length > 0),
    }));
  });

  /** v4 `shouldRenderField` (`:35-41`) — the RAW bag value, defaults excluded. */
  private shouldRender(field: ProviderOptionField, parameters: Record<string, unknown>): boolean {
    if (!field.showIf) return true;
    return parameters[field.showIf.field] === field.showIf.equals;
  }

  private prepare(field: ProviderOptionField, parameters: Record<string, unknown>): PreparedField {
    // v4 `fieldValue` (`:43-47`): stored wins, `default` is the display fallback.
    const stored = parameters[field.key];
    const value = stored !== undefined ? stored : field.default;
    if (field.type !== 'multi-enum') {
      return { field, value, choices: [], selected: [] };
    }

    const selected = Array.isArray(value)
      ? value.filter((v): v is string => typeof v === 'string')
      : [];
    const max = field.max ?? Number.POSITIVE_INFINITY;
    const source =
      field.multiEnumSource === 'fetchedModels'
        ? this.fetchedModels()
            .filter((m) => m !== this.modelName())
            .slice(0, 50)
            .map((m) => ({ value: m, label: m }))
        : (field.enumValues ?? []).map((option) => ({
            value: option.value,
            label: option.label,
          }));

    const choices = source.map((choice) => {
      const isSelected = selected.includes(choice.value);
      return {
        value: choice.value,
        label: choice.label,
        selected: isSelected,
        disabled: !isSelected && selected.length >= max,
      };
    });
    return { field, value, choices, selected };
  }

  protected fieldId(field: ProviderOptionField): string {
    return `pof-${field.key}`;
  }

  protected enumValues(field: ProviderOptionField): ProviderOptionEnumValue[] {
    return field.enumValues ?? [];
  }

  /** v4's `EnumField`/`StringField` coercion (`:180`, `:326`). */
  protected asString(value: unknown): string {
    return typeof value === 'string' ? value : '';
  }

  /** v4's `NumberField` coercion (`:285-290`) — a stored string shows verbatim. */
  protected asNumericText(value: unknown): string {
    if (typeof value === 'number') return String(value);
    if (typeof value === 'string') return value;
    return '';
  }

  protected emit(field: ProviderOptionField, value: unknown): void {
    this.setParameter.emit({ key: field.key, value });
  }

  /** v4's `NumberField` onChange (`:300-307`). */
  protected emitNumber(field: ProviderOptionField, raw: string): void {
    if (raw === '') {
      this.emit(field, undefined);
      return;
    }
    const parsed = Number(raw);
    this.emit(field, Number.isNaN(parsed) ? raw : parsed);
  }

  /** v4's `MultiEnumField` onChange (`:254-260`). */
  protected toggleChoice(row: PreparedField, choice: PreparedChoice, checked: boolean): void {
    const max = row.field.max ?? Number.POSITIVE_INFINITY;
    if (checked && row.selected.length < max) {
      this.emit(row.field, [...row.selected, choice.value]);
    } else if (!checked) {
      this.emit(
        row.field,
        row.selected.filter((v) => v !== choice.value),
      );
    }
  }
}
