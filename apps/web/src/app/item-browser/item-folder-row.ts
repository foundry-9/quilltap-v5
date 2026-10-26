import { ChangeDetectionStrategy, Component, input, output, signal } from '@angular/core';

import { Icon } from '../ui/icon';
import { QtAutofocus } from '../wardrobe/autofocus.directive';

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
 * ItemFolderRow.tsx` (`4fbdad186` — the wardrobe's `WardrobeFolderRow`
 * lifted, `testId` / `countNoun` added).
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
  imports: [Icon, QtAutofocus],
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
        <div class="relative">
          <button
            type="button"
            class="qt-button-ghost qt-button-sm"
            [attr.aria-label]="'Folder actions for ' + row().name"
            aria-haspopup="menu"
            [attr.aria-expanded]="menuOpen()"
            (click)="menuOpen.set(!menuOpen())"
          >
            ⋮
          </button>
          @if (menuOpen()) {
            <div
              role="menu"
              class="absolute right-0 top-full mt-1 z-30 min-w-[10rem] rounded border qt-border-default qt-bg-default shadow-md"
              (mouseleave)="menuOpen.set(false)"
            >
              <ul class="divide-y qt-border-default">
                <li>
                  <button
                    type="button"
                    role="menuitem"
                    class="block w-full text-left px-3 py-2 text-sm hover:qt-bg-muted"
                    (click)="startRename()"
                  >
                    Rename
                  </button>
                </li>
                <li>
                  <button
                    type="button"
                    role="menuitem"
                    class="block w-full text-left px-3 py-2 text-sm qt-text-destructive hover:qt-bg-muted"
                    (click)="menuOpen.set(false); deleted.emit()"
                  >
                    Delete
                  </button>
                </li>
              </ul>
            </div>
          }
        </div>
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
  readonly opened = output<void>();
  readonly deleted = output<void>();

  protected readonly plural = plural;
  protected readonly menuOpen = signal(false);
  protected readonly renaming = signal(false);
  protected readonly name = signal('');
  private submitting = false;

  protected startRename(): void {
    this.menuOpen.set(false);
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
