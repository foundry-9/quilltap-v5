import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
  viewChild,
} from '@angular/core';

import { CoreClient } from '../../core/core-client';
import { folderCrumb, isSameFolder } from '../folders';
import { encodeWardrobeContainer, type WardrobeContainer } from '../wardrobe-container';
import { createWardrobeFoldersStore } from '../wardrobe-folders.api';

/** v4 `WardrobeFolderField.tsx:17` — the select value that reveals the input. */
const NEW_FOLDER = '\u0000new';

/**
 * The item editor's Folder field — a port of v4
 * `components/wardrobe/wardrobe-item-editor/WardrobeFolderField.tsx` (NEW in
 * `4a38222e6`, read at that pin).
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
  template: `
    <div>
      <label for="wardrobe-folder" class="qt-label mb-1">Folder</label>
      @if (typing()) {
        <div class="flex items-center gap-2">
          <input
            #folderInput
            id="wardrobe-folder"
            type="text"
            [value]="value()"
            (input)="valueChange.emit($any($event.target).value)"
            (keydown)="onInputKeydown($event)"
            placeholder="e.g., Formal/Evening"
            class="qt-input"
          />
          <button type="button" class="qt-button-ghost qt-button-sm" (click)="typing.set(false)">
            Pick
          </button>
        </div>
      } @else {
        <select id="wardrobe-folder" class="qt-select" (change)="onSelect($any($event.target).value)">
          <option value="" [selected]="selected() === ''">Wardrobe (top level)</option>
          @for (path of options(); track path) {
            <option [value]="path" [selected]="selected() === path">{{ crumb(path) }}</option>
          }
          <option [value]="newFolder">New folder…</option>
        </select>
      }
      <p class="mt-1 text-xs qt-text-small">
        Where it is filed for your browsing. Folders never change what anyone can wear.
      </p>
    </div>
  `,
})
export class WardrobeFolderField {
  private readonly core = inject(CoreClient);

  /** The wardrobe the item is (or will be) filed in; null while unknown. */
  readonly container = input<WardrobeContainer | null>(null);
  readonly value = input.required<string>();
  readonly valueChange = output<string>();

  protected readonly newFolder = NEW_FOLDER;
  protected readonly typing = signal(false);
  private readonly store = createWardrobeFoldersStore(this.core);
  private readonly folderInput = viewChild<ElementRef<HTMLInputElement>>('folderInput');
  /** `container` keyed by its tier key — a fresh object for the same wardrobe
   *  is not a new destination (v4's query key is the tier key). */
  private readonly destination = computed(() => this.container(), {
    equal: (a, b) =>
      a === b || (a !== null && b !== null && encodeWardrobeContainer(a) === encodeWardrobeContainer(b)),
  });

  /**
   * A folder the tree doesn't list yet (a new one, or one in another wardrobe
   * after the destination changed) is still offered as chosen (v4 `:30-36`).
   */
  protected readonly options = computed(() => {
    const value = this.value();
    const paths = this.store.folders().map((f) => f.path);
    if (value && !paths.some((p) => isSameFolder(p, value))) paths.push(value);
    return paths.sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }));
  });

  /** v4 `:38`. */
  protected readonly selected = computed(
    () => this.options().find((p) => isSameFolder(p, this.value())) ?? '',
  );

  protected readonly crumb = folderCrumb;

  constructor() {
    // The read, once per destination (the store drops a late answer).
    effect(() => {
      const container = this.destination();
      untracked(() => void this.store.load(container));
    });

    // v4 `autoFocus` on the revealed input.
    effect(() => {
      const el = this.folderInput();
      if (el) el.nativeElement.focus();
    });
  }

  /** v4 `:69-75` — New folder… reveals the input without changing the value. */
  protected onSelect(value: string): void {
    if (value === NEW_FOLDER) {
      this.typing.set(true);
      return;
    }
    this.valueChange.emit(value);
  }

  /** v4 `:53-58` — Escape returns to the select; the stop keeps the editor open. */
  protected onInputKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.stopPropagation();
      this.typing.set(false);
    }
  }
}
