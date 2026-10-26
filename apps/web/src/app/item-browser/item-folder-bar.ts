import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { QtAutofocus } from '../wardrobe/autofocus.directive';
import { folderSegments } from '../wardrobe/folders';

/**
 * The folder bar of a folder-of-items browser (the wardrobe's, the
 * Inventory's): the breadcrumb (root, the borrowed tier when inside one, then
 * each folder), and — where folders are managed — New folder and Select.
 * While searching it says so instead. A port of v4
 * `components/item-browser/ItemFolderBar.tsx` (`4fbdad186`, a PURE lift of the
 * wardrobe browser's bar — every string, class and test id survives; the
 * caller names its own via `rootLabel` / `navLabel` / `testId`).
 */
@Component({
  selector: 'qt-item-folder-bar',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex flex-wrap items-center justify-between gap-2" [attr.data-testid]="testId()">
      <nav [attr.aria-label]="navLabel()" class="flex flex-wrap items-center gap-1 qt-text-xs min-w-0">
        @if (searching()) {
          <span class="qt-text-secondary">Searching everywhere</span>
        } @else {
          <button
            type="button"
            class="qt-text-secondary hover:text-foreground"
            (click)="navigateRoot.emit()"
            [attr.aria-current]="inOwnTier() && currentFolder() === '' ? 'page' : null"
          >
            {{ rootLabel() }}
          </button>
          @if (!inOwnTier()) {
            <span class="qt-text-secondary" aria-hidden="true">›</span>
            <button
              type="button"
              class="qt-text-secondary hover:text-foreground"
              (click)="navigateTier.emit()"
              [attr.aria-current]="currentFolder() === '' ? 'page' : null"
            >
              {{ tierLabel() }}
            </button>
          }
          @for (crumb of crumbs(); track crumb.path; let last = $last) {
            <span class="inline-flex items-center gap-1">
              <span class="qt-text-secondary" aria-hidden="true">›</span>
              <button
                type="button"
                [attr.class]="last ? 'text-foreground font-medium' : 'qt-text-secondary hover:text-foreground'"
                (click)="navigateFolder.emit(crumb.path)"
                [attr.aria-current]="last ? 'page' : null"
              >
                {{ crumb.segment }}
              </button>
            </span>
          }
        }
      </nav>
      @if (managed()) {
        <div class="flex items-center gap-1">
          @if (!searching()) {
            <button
              type="button"
              class="qt-button-ghost qt-button-sm"
              (click)="newFolder.emit()"
              [disabled]="newFolderOpen()"
            >
              New folder
            </button>
          }
          <button
            type="button"
            class="qt-button-sm"
            [class.qt-button-secondary]="selectMode()"
            [class.qt-button-ghost]="!selectMode()"
            [attr.aria-pressed]="selectMode()"
            (click)="toggleSelect.emit()"
          >
            Select
          </button>
        </div>
      }
    </div>
  `,
})
export class ItemFolderBar {
  /** The root crumb, e.g. `Wardrobe`. */
  readonly rootLabel = input.required<string>();
  /** The breadcrumb's accessible name, e.g. `Wardrobe folders`. */
  readonly navLabel = input.required<string>();
  readonly searching = input(false);
  /** The borrowed tier's label, or null in the browser's own tier. */
  readonly tierLabel = input<string | null>(null);
  readonly currentFolder = input('');
  /** Folder controls shown (the tier being browsed is the browser's own). */
  readonly managed = input(false);
  readonly newFolderOpen = input(false);
  readonly selectMode = input(false);
  /** `data-testid`, e.g. `wardrobe-folder-bar`. */
  readonly testId = input.required<string>();
  readonly navigateRoot = output<void>();
  readonly navigateTier = output<void>();
  readonly navigateFolder = output<string>();
  readonly newFolder = output<void>();
  readonly toggleSelect = output<void>();

  protected readonly inOwnTier = computed(() => this.tierLabel() === null);
  protected readonly crumbs = computed(() =>
    folderSegments(this.currentFolder()).map((segment, idx, all) => ({
      segment,
      path: all.slice(0, idx + 1).join('/'),
    })),
  );
}

/** The inline "new folder" name input under the folder bar (v4 `NewFolderInput`). */
@Component({
  selector: 'qt-new-folder-input',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [QtAutofocus],
  template: `
    <div class="flex items-center gap-2">
      <input
        type="text"
        qtAutofocus
        [value]="name()"
        (input)="nameChange.emit($any($event.target).value)"
        (keydown.enter)="$event.preventDefault(); submitted.emit()"
        (keydown.escape)="$event.stopPropagation(); cancelled.emit()"
        placeholder="Folder name — Enter to make it, Esc to think better of it"
        aria-label="New folder name"
        class="qt-input qt-input-sm flex-1"
      />
    </div>
  `,
})
export class NewFolderInput {
  readonly name = input('');
  readonly nameChange = output<string>();
  readonly submitted = output<void>();
  readonly cancelled = output<void>();
}
