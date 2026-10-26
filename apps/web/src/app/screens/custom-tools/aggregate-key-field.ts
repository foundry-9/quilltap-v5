import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { AGGREGATE_FOLDS, AGGREGATE_SCOPES } from '../../pascal/inventory-aggregate';

/**
 * AggregateKeyField — the key box of an `inventoryAggregate` chip, with a small
 * Insert-key helper that fills in a `<scope>.<field>` skeleton (v4
 * `components/custom-tools/AggregateKeyField.tsx`, `a53024d3c`).
 *
 * Shared by the gate and the outcome table: the key is literal in both, so the
 * widget is the same.
 */

/** Every skeleton the helper offers, in reading order (24 of them). */
export const AGGREGATE_SKELETONS: string[] = AGGREGATE_SCOPES.flatMap((scope) => [
  `${scope}.count`,
  `${scope}.names`,
  `${scope}.titles`,
  `${scope}.metadata.`,
  ...AGGREGATE_FOLDS.map((fold) => `${scope}.metadata.<key>.${fold}`),
]);

/** The key input's title — four sentences, with v4's typographic apostrophe. */
export const AGGREGATE_KEY_TITLE =
  'One attribute across everything in a scope. Scopes: equipped (in this chat), owned (in their own vault, ' +
  'equipped or not), reachable (everything they can see). Fields: count, names, titles, metadata.<key> (a list ' +
  'of the objects’ values), or metadata.<key>.min / .max / .sum / .count (a number). A list matches = when any ' +
  'entry does; ordering needs a fold.';

/**
 * The metadata key already typed into the box, if any — kept when a fold
 * skeleton is chosen (v4's regex, verbatim).
 */
const TYPED_KEY = /^[a-z]+\.metadata\.(.+?)(?:\.(?:min|max|sum|count))?$/;

@Component({
  selector: 'qt-aggregate-key-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <input
      type="text"
      [value]="value()"
      (input)="valueChange.emit(inputValue($event))"
      placeholder="equipped.metadata.toolVersion.max"
      [disabled]="disabled()"
      [attr.class]="'qt-input w-56 text-sm ' + (value().trim() === '' ? 'qt-input-error' : '')"
      aria-label="Inventory aggregate key"
      [attr.title]="TITLE"
    />
    <select
      (change)="onInsert($event)"
      [disabled]="disabled()"
      class="qt-select qt-select-sm w-24 shrink-0"
      aria-label="Insert an aggregate key"
      title="Fill the box with a scope and a field; type the metadata key yourself."
    >
      <option value="" selected>Insert key…</option>
      @for (skeleton of SKELETONS; track skeleton) {
        <option [value]="skeleton">{{ skeleton }}</option>
      }
    </select>
  `,
})
export class AggregateKeyField {
  readonly value = input.required<string>();
  readonly disabled = input(false);

  readonly valueChange = output<string>();

  protected readonly SKELETONS = AGGREGATE_SKELETONS;
  protected readonly TITLE = AGGREGATE_KEY_TITLE;

  protected readonly typedKey = computed(() => TYPED_KEY.exec(this.value().trim())?.[1]);

  protected inputValue(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }

  /**
   * A fold skeleton keeps whatever metadata key is already typed, if any. The
   * select is stateless in v4 (`value=""`), so it is put back on its prompt
   * after every choice.
   */
  protected onInsert(event: Event): void {
    const select = event.target as HTMLSelectElement;
    const skeleton = select.value;
    select.value = '';
    if (!skeleton) return;
    this.valueChange.emit(skeleton.replace('<key>', this.typedKey() ?? 'key'));
  }
}
