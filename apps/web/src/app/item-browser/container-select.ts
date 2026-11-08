import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { GENERAL_CONTAINER, encodeWardrobeContainer, type WardrobeContainer } from '../wardrobe/wardrobe-container';
import type { CatalogueEntry } from './container-catalogue';

interface OptionGroup {
  label: string;
  options: Array<{ key: string; name: string }>;
}

/**
 * The container `<select>` of a tiered browser (the wardrobe's, the
 * Inventory's) and of its Move / Copy to another tier dialogs — a port of v4
 * `components/item-browser/ContainerSelect.tsx` at the pin `624d0d25c` (NEW
 * at `b2b783b47`): Characters, General, Projects, Groups, each an
 * `<optgroup>`, every option's value the container's encoded key (`scope:id`).
 * Containers named in `exclude` (an item's own home, in a transfer) are left
 * out; an empty group is dropped; a disabled placeholder shows only while
 * nothing is chosen.
 *
 * The caller names the select (v4: a `<label htmlFor={id}>`; here also
 * `labelledBy`, which the a11y lint can see).
 *
 * `catalogue` takes plain arrays (v4's `Pick<ContainerCatalogue, …>`): bind a
 * {@link injectContainerCatalogue} catalogue's signals' values.
 */
@Component({
  selector: 'qt-container-select',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <select
      [id]="id()"
      [attr.aria-labelledby]="labelledBy() ?? null"
      [attr.class]="selectClass()"
      [disabled]="disabled()"
      (change)="valueChange.emit($any($event.target).value)"
    >
      @if (!value()) {
        <option value="" disabled [selected]="true">{{ placeholder() }}</option>
      }
      @for (g of groups(); track g.label) {
        <optgroup [attr.label]="g.label">
          @for (o of g.options; track o.key) {
            <option [value]="o.key" [selected]="o.key === value()">{{ o.name }}</option>
          }
        </optgroup>
      }
    </select>
  `,
})
export class ContainerSelect {
  readonly id = input.required<string>();
  /** The encoded key of the chosen container, or `''`. */
  readonly value = input('');
  readonly valueChange = output<string>();
  readonly catalogue = input.required<{
    characters: readonly CatalogueEntry[];
    projects: readonly CatalogueEntry[];
    groups: readonly CatalogueEntry[];
  }>();
  /** The General option's name, e.g. `Quilltap General`. */
  readonly generalName = input.required<string>();
  /** Shown (disabled) while nothing is chosen. */
  readonly placeholder = input.required<string>();
  /** Encoded keys to leave out. */
  readonly exclude = input<readonly string[]>([]);
  /** v4's `className`. */
  readonly selectClass = input('qt-select w-full');
  readonly disabled = input(false);
  /**
   * The id of the caller's label, when it names the select (an adaptation:
   * v4's callers name it with a `<label htmlFor={id}>` in THEIR template,
   * which `check-a11y-names.mjs` cannot see across templates; unset, the
   * attribute is omitted and the DOM is v4's).
   */
  readonly labelledBy = input<string | undefined>(undefined);

  /** v4 `:54-64`. */
  protected readonly groups = computed<OptionGroup[]>(() => {
    const exclude = this.exclude();
    const catalogue = this.catalogue();
    const options = (scope: WardrobeContainer['scope'], entries: readonly CatalogueEntry[]) =>
      entries
        .map((e) => ({ key: encodeWardrobeContainer({ scope, id: e.id }), name: e.name }))
        .filter((o) => !exclude.includes(o.key));
    return [
      { label: 'Characters', options: options('character', catalogue.characters) },
      {
        label: 'General',
        options: [{ key: encodeWardrobeContainer(GENERAL_CONTAINER), name: this.generalName() }].filter(
          (o) => !exclude.includes(o.key),
        ),
      },
      { label: 'Projects', options: options('project', catalogue.projects) },
      { label: 'Groups', options: options('group', catalogue.groups) },
    ].filter((g) => g.options.length > 0);
  });
}
