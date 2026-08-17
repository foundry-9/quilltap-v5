import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  afterRenderEffect,
  computed,
  effect,
  input,
  output,
  signal,
  untracked,
  viewChildren,
} from '@angular/core';

import type {
  ProviderOptionEnumValue,
  ProviderOptionField,
  ProviderOptionsSchema,
} from './provider-options-schema';

/**
 * A stored value (or a schema default) as the string an `<input>` displays
 * (v4 `ProviderOptionsPanel.tsx` `toInputString`, `:56-60`).
 */
export function toInputString(value: unknown): string {
  if (typeof value === 'number') return String(value);
  if (typeof value === 'string') return value;
  return '';
}

/**
 * One `number` row (v4's `NumberField`, `:288-352`).
 *
 * A separate component because the box owns its own string while it is being
 * edited, rather than reading straight off the value input. A half-typed number
 * is not a value the bag can hold — `1.` and `-` both arrive as `''`, and `00`
 * round-trips as `0` — and a control that re-derives its display from what the
 * host stored will fight the person typing (v4 Bug 72).
 *
 * `syncedFrom` is the input value the draft was last reconciled against.
 * Writing through sets it to the value the host will hand back, so our own echo
 * is never mistaken for the parameter changing underneath us. Anything else
 * moving the input (a different profile, a schema swap) re-seeds both.
 *
 * ⚠ The naive spelling — reconciling against the DRAFT, or a `linkedSignal`
 * keyed on the incoming string — reintroduces the bug: every echo that
 * normalizes differently from what was typed (`007` → `7`) is rewritten under
 * the caret. The spec pins that with a leading-zero entry.
 */
@Component({
  selector: 'qt-provider-number-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  styles: [':host { display: block; }'],
  template: `
    <label [for]="fieldId()" class="qt-text-label-xs">{{ field().label }}</label>
    <input
      [id]="fieldId()"
      type="number"
      class="qt-input text-sm"
      [value]="draft()"
      [attr.placeholder]="placeholder()"
      (input)="onInput($any($event.target).value)"
    />
    @if (field().helpText) {
      <p class="qt-text-xs mt-1">{{ field().helpText }}</p>
    }
  `,
})
export class ProviderNumberField {
  readonly field = input.required<ProviderOptionField>();
  /** The host's value for this key — `undefined` when the bag holds none. */
  readonly value = input<unknown>(undefined);
  /** One write back into the host's bag; `undefined` DELETES the key. */
  readonly valueChange = output<unknown>();

  protected readonly draft = signal('');
  private readonly syncedFrom = signal('');

  protected readonly fieldId = computed(() => `pof-${this.field().key}`);

  /**
   * An empty box means "unset", so the schema default shows through as the
   * PLACEHOLDER — the only way the user can see they have reached the state the
   * help text calls "leave blank for the default" (v4 `:331-334`).
   */
  protected readonly placeholder = computed(() => {
    const fallback = this.field().default;
    return fallback === undefined ? null : toInputString(fallback);
  });

  private readonly incoming = computed(() => toInputString(this.value()));

  constructor() {
    // v4 reconciles during render (`:308-312`); the signal equivalent is an
    // effect reading `incoming` alone — `syncedFrom` is read untracked so that
    // writing it here cannot re-trigger this same effect.
    effect(() => {
      const incoming = this.incoming();
      if (incoming !== untracked(this.syncedFrom)) {
        this.syncedFrom.set(incoming);
        this.draft.set(incoming);
      }
    });
  }

  /** v4's `NumberField` onChange (`:335-345`). */
  protected onInput(raw: string): void {
    this.draft.set(raw);
    if (raw === '') {
      this.write(undefined);
      return;
    }
    const parsed = Number(raw);
    this.write(Number.isNaN(parsed) ? raw : parsed);
  }

  /** v4's `emit` (`:314-317`) — remember what the host will hand back. */
  private write(next: unknown): void {
    this.syncedFrom.set(toInputString(next));
    this.valueChange.emit(next);
  }
}

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
 * - `default` is that display fallback alone (`:43-53`); neither app seeds the
 *   bag on a new profile. A `number` field is the one exception: its default
 *   renders as the input's PLACEHOLDER, never as a value, so an unset option
 *   reads as unset (v4 Bug 72 — see {@link ProviderNumberField}).
 * - A `boolean` is checked on `value === true` (`:149`) — a hand-edited
 *   `'true'` STRING shows OFF, and shows off in v4 too. (The P4.D81 hardcoded
 *   Enable Thinking row tolerated the string; that divergence retires with the
 *   row. Untouched, the string still rides the bag to the wire, where
 *   `build_ollama_body` does accept it — nothing is silently "fixed".)
 * - A cleared `number` emits `undefined`, which DELETES the key (`:338-339`);
 *   an unparseable one stores the raw string (`:343`).
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
  imports: [ProviderNumberField],
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
                    <qt-provider-number-field
                      [field]="row.field"
                      [value]="row.value"
                      (valueChange)="emit(row.field, $event)"
                    />
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
    // v4 `fieldValue` (`:43-53`): stored wins, `default` is the display
    // fallback — EXCEPT for a number field, which renders its default as a
    // placeholder rather than a value, so an unset option reads as unset.
    // Without that, "absent" and "explicitly the default" look identical and
    // *"leave blank for the default"* is a state the user can never see
    // themselves reach (Bug 72). Every other control needs the fallback as a
    // real value: the enum row relies on it to preselect.
    const stored = parameters[field.key];
    const value =
      stored !== undefined ? stored : field.type === 'number' ? undefined : field.default;
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

  protected emit(field: ProviderOptionField, value: unknown): void {
    this.setParameter.emit({ key: field.key, value });
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
