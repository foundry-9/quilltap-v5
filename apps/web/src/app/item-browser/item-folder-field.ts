import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  effect,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';

import { folderCrumb, isSameFolder } from './item-folders';

/** v4 `NEW_FOLDER` — the select value that reveals the input. */
const NEW_FOLDER = '\u0000new';

/**
 * An item editor's Folder field (the wardrobe's, the Inventory's) — a port of
 * v4 `components/item-browser/ItemFolderField.tsx` at the pin `624d0d25c`
 * (NEW at `b2b783b47`; v4's `WardrobeFolderField.tsx` shrank onto it).
 *
 * v4's module doc, carried: the root, every folder in the destination tier,
 * and "New folder…", which reveals an input for a path to file the item under
 * (created on save). Filing only — a folder never changes what the item does.
 *
 * v5 shape: `value` + `valueChange` are v4's `value` / `onChange`; the tree is
 * the caller's (each kind binds its own folder store).
 */
@Component({
  selector: 'qt-item-folder-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div>
      <label [attr.for]="id()" [attr.class]="labelClassName()">Folder</label>
      @if (typing()) {
        <div class="flex items-center gap-2">
          <input
            #folderInput
            [id]="id()"
            type="text"
            [value]="value()"
            (input)="valueChange.emit($any($event.target).value)"
            (keydown)="onInputKeydown($event)"
            [placeholder]="placeholder()"
            [attr.class]="inputClassName()"
          />
          <button type="button" class="qt-button-ghost qt-button-sm" (click)="typing.set(false)">Pick</button>
        </div>
      } @else {
        <select [id]="id()" [attr.class]="selectClassName()" (change)="onSelect($any($event.target).value)">
          <option value="" [selected]="selected() === ''">{{ rootLabel() }}</option>
          @for (path of options(); track path) {
            <option [value]="path" [selected]="selected() === path">{{ crumb(path) }}</option>
          }
          <option [value]="newFolder">New folder…</option>
        </select>
      }
      <p class="mt-1 text-xs qt-text-small">{{ hint() }}</p>
    </div>
  `,
})
export class ItemFolderField {
  readonly id = input.required<string>();
  /** The destination tier's folder tree. */
  readonly folders = input<ReadonlyArray<{ path: string }>>([]);
  readonly value = input.required<string>();
  readonly valueChange = output<string>();
  /** The root option, e.g. `Wardrobe (top level)`. */
  readonly rootLabel = input.required<string>();
  /** The line under the field. */
  readonly hint = input.required<string>();
  /** The typed path's placeholder, e.g. `e.g., Formal/Evening`. */
  readonly placeholder = input.required<string>();
  readonly labelClassName = input('qt-label mb-1');
  readonly inputClassName = input('qt-input');
  readonly selectClassName = input('qt-select');

  protected readonly newFolder = NEW_FOLDER;
  protected readonly typing = signal(false);
  protected readonly crumb = folderCrumb;
  private readonly folderInput = viewChild<ElementRef<HTMLInputElement>>('folderInput');

  /**
   * A folder the tree doesn't list yet (a new one, or one in another tier
   * after the destination changed) is still offered as chosen (v4 `:50-56`).
   */
  protected readonly options = computed(() => {
    const value = this.value();
    const paths = this.folders().map((f) => f.path);
    if (value && !paths.some((p) => isSameFolder(p, value))) paths.push(value);
    return paths.sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }));
  });

  /** v4 `:58`. */
  protected readonly selected = computed(() => this.options().find((p) => isSameFolder(p, this.value())) ?? '');

  constructor() {
    // v4 `autoFocus` on the revealed input.
    effect(() => {
      const el = this.folderInput();
      if (el) el.nativeElement.focus();
    });
  }

  /** New folder… reveals the input without changing the value. */
  protected onSelect(value: string): void {
    if (value === NEW_FOLDER) {
      this.typing.set(true);
      return;
    }
    this.valueChange.emit(value);
  }

  /** Escape returns to the select; the stop keeps the editor open. */
  protected onInputKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.stopPropagation();
      this.typing.set(false);
    }
  }
}
