import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import { Icon } from '../ui/icon';
import { QtAutofocus } from '../wardrobe/autofocus.directive';
import { ItemActionMenu, type MenuEntryLike } from './item-action-menu';

/** v4 `ItemFolderRowView`. */
export interface ItemFolderRowView {
  path: string;
  name: string;
  itemCount: number;
}

const plural = (n: number, word: string): string => `${n} ${word}${n === 1 ? '' : 's'}`;

/**
 * One subfolder in a folder-of-items browser (the wardrobe's, the
 * Inventory's): open it, or — where the browser manages folders — rename or
 * delete it from its ⋮ menu. A port of v4 `components/item-browser/
 * ItemFolderRow.tsx` at the pin `624d0d25c` (`4fbdad186` — the wardrobe's
 * `WardrobeFolderRow` lifted; `29ba4d3f8` onto the fixed menu; `b2b783b47`
 * onto the shared `qt-item-action-menu`, so the keyboard, Escape and an
 * outside press all close it — it used to close only on mouse-leave — with
 * `extraEntries` between Rename and Delete).
 *
 * Rename: Enter submits, Escape stops the event and restores the name, blur
 * submits; a blank or unchanged name just closes; the input closes only when
 * `rename` resolves true. One rename at a time — a blur landing while the
 * Enter's call is out does not send it twice (the wardrobe port's guard).
 */
@Component({
  // The wardrobe browser mounts this row under its pre-lift element name, so
  // its DOM (and every wardrobe spec walking it) is unchanged by the lift.
  selector: 'qt-item-folder-row, qt-wardrobe-folder-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, ItemActionMenu, QtAutofocus],
  template: `
    <div class="qt-card-interactive py-2 px-3 flex items-center gap-2" [attr.data-testid]="testId()">
      <qt-icon name="folder" class="w-4 h-4 qt-text-secondary flex-shrink-0" />
      @if (renaming()) {
        <input
          type="text"
          qtAutofocus
          [value]="name()"
          (input)="name.set($any($event.target).value)"
          (keydown.enter)="$event.preventDefault(); submit()"
          (keydown.escape)="$event.stopPropagation(); cancelRename()"
          (blur)="submit()"
          [attr.aria-label]="'Rename folder ' + row().name"
          class="qt-input qt-input-sm flex-1"
        />
      } @else {
        <button
          type="button"
          class="flex-1 min-w-0 text-left text-sm text-foreground truncate"
          (click)="opened.emit()"
        >
          {{ row().name }}
        </button>
      }
      <span class="qt-text-xs qt-text-secondary flex-shrink-0">{{ plural(row().itemCount, countNoun()) }}</span>
      @if (manageable() && !renaming()) {
        <qt-item-action-menu
          [label]="'Folder actions for ' + row().name"
          [room]="120"
          minWidthClass="min-w-[10rem]"
          [entries]="entries()"
        />
      }
    </div>
  `,
})
export class ItemFolderRow {
  readonly row = input.required<ItemFolderRowView>();
  /** Rename / Delete are offered only where folders are managed. */
  readonly manageable = input(false);
  /** Resolves true when the rename stuck (v4 `onRename`). */
  readonly rename = input.required<(name: string) => Promise<boolean>>();
  /** `data-testid` of the row, e.g. `wardrobe-folder-row`. */
  readonly testId = input.required<string>();
  /** The noun counted, e.g. `item`. */
  readonly countNoun = input('item');
  /** More ⋮ entries, between Rename and Delete (Move to…, Copy to…). */
  readonly extraEntries = input<readonly MenuEntryLike[]>([]);
  readonly opened = output<void>();
  readonly deleted = output<void>();

  protected readonly plural = plural;
  /** v4 `:91-103` — Rename, the extras, Delete (destructive). */
  protected readonly entries = computed<readonly MenuEntryLike[]>(() => [
    { key: 'rename', label: 'Rename', onSelect: () => this.startRename() },
    ...this.extraEntries(),
    { key: 'delete', label: 'Delete', destructive: true, onSelect: () => this.deleted.emit() },
  ]);
  protected readonly renaming = signal(false);
  protected readonly name = signal('');
  private submitting = false;

  protected startRename(): void {
    this.name.set(this.row().name);
    this.renaming.set(true);
  }

  protected cancelRename(): void {
    this.name.set(this.row().name);
    this.renaming.set(false);
  }

  /** v4 `:39-46`: blank or unchanged just stops; otherwise stop only on success. */
  protected async submit(): Promise<void> {
    if (!this.renaming() || this.submitting) return;
    const next = this.name().trim();
    if (!next || next === this.row().name) {
      this.renaming.set(false);
      return;
    }
    this.submitting = true;
    try {
      if (await this.rename()(next)) this.renaming.set(false);
    } finally {
      this.submitting = false;
    }
  }
}
