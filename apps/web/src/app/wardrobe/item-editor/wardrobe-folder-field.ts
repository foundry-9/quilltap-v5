import { ChangeDetectionStrategy, Component, computed, effect, inject, input, output, untracked } from '@angular/core';

import { CoreClient } from '../../core/core-client';
import { ItemFolderField } from '../../item-browser/item-folder-field';
import { encodeWardrobeContainer, type WardrobeContainer } from '../wardrobe-container';
import { createWardrobeFoldersStore } from '../wardrobe-folders.api';

/**
 * The item editor's Folder field — a port of v4
 * `components/wardrobe/wardrobe-item-editor/WardrobeFolderField.tsx` at the
 * pin `624d0d25c` (NEW in `4a38222e6`; `b2b783b47` shrank it to a binding of
 * the shared `qt-item-folder-field` over the destination wardrobe's folders).
 *
 * v4's module doc, carried: the root, every folder in the destination
 * wardrobe, and "New folder…", which reveals an input for a path to file the
 * item under (created on save). Filing only — a folder says nothing about
 * when the garment is worn.
 *
 * "Created on save" is the SERVER's: the typed path rides the create / update
 * body as `folder` and the routes resolve it against the tier's folders,
 * creating the missing parts. The field itself creates nothing, and the
 * editor's `payloadForEdit` is what keeps an unchanged folder off an edit.
 *
 * v5 shape: `value` + `valueChange` are v4's `value` / `onChange`. The tree is
 * v4's `useWardrobeFolders(container)` — here the shared
 * {@link createWardrobeFoldersStore}, re-loaded whenever the destination
 * changes (v4's query key is the container's tier key; null skips the read,
 * v4's `enabled: container !== null`). A failed (or absent-verb) read answers
 * no folders with v4's warn line, so the field still offers the top level and
 * **New folder…**; a late answer for an earlier destination is dropped.
 *
 * @module wardrobe/item-editor/wardrobe-folder-field
 */
@Component({
  selector: 'qt-wardrobe-folder-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ItemFolderField],
  template: `
    <qt-item-folder-field
      id="wardrobe-folder"
      [folders]="store.folders()"
      [value]="value()"
      (valueChange)="valueChange.emit($event)"
      rootLabel="Wardrobe (top level)"
      placeholder="e.g., Formal/Evening"
      hint="Where it is filed for your browsing. Folders never change what anyone can wear."
    />
  `,
})
export class WardrobeFolderField {
  private readonly core = inject(CoreClient);

  /** The wardrobe the item is (or will be) filed in; null while unknown. */
  readonly container = input<WardrobeContainer | null>(null);
  readonly value = input.required<string>();
  readonly valueChange = output<string>();

  protected readonly store = createWardrobeFoldersStore(this.core);
  /** `container` keyed by its tier key — a fresh object for the same wardrobe
   *  is not a new destination (v4's query key is the tier key). */
  private readonly destination = computed(() => this.container(), {
    equal: (a, b) =>
      a === b || (a !== null && b !== null && encodeWardrobeContainer(a) === encodeWardrobeContainer(b)),
  });

  constructor() {
    // The read, once per destination (the store drops a late answer).
    effect(() => {
      const container = this.destination();
      untracked(() => void this.store.load(container));
    });
  }
}
