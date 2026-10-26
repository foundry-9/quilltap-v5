import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import { ItemFolderPickerDialog } from '../item-browser/item-folder-picker-dialog';
import type { WardrobeContainer } from './wardrobe-container';
import {
  WardrobeComponentModeFieldset,
  defaultComponentMode,
  type ComponentMode,
} from './wardrobe-component-mode-fieldset';
import { createWardrobeFolder, createWardrobeFoldersStore } from './wardrobe-folders.api';

/**
 * "Move to folder…" / "Copy to folder…" — pick where in one wardrobe to file
 * one or more items, or copies of them. The picker itself is the shared
 * `qt-item-folder-picker-dialog` since v4 `4fbdad186` (the item-browser lift,
 * P4.D330 R-X); this wrapper adds the wardrobe's folder tree, its two notes and
 * the outfit components choice. A port of v4
 * `components/wardrobe/WardrobeFolderPickerDialog.tsx` (`4a38222e6`;
 * `97f0d8cb8` moved its open state into the actions store; `bbbcd2388` added
 * the copy and the components ask).
 *
 * The wardrobe's folder tree (`wardrobeFoldersList`) as an indented list with
 * the top level first; for a move, the folder the items already share is
 * shown but can't be chosen (a copy may land beside its original). **New
 * folder…** opens an input under the highlighted folder and creates the
 * folder there, then highlights it. When any of the items is an outfit, the
 * same all-or-nothing components choice the cross-wardrobe transfer offers
 * comes along ({@link WardrobeComponentModeFieldset}). **Move** / **Copy**
 * hands the folder and that choice to the host (`submitted` — one
 * `wardrobeItemsMove` / `wardrobeItemsCopy` call). Filing changes nothing
 * about what anyone can wear.
 *
 * v4's names, ported (R-4): `working` (was `moving`) and `submitted` — the
 * twin of v4's `onSubmit(folder, components?)`; `submit` itself is a native
 * DOM event name an Angular output may not take.
 *
 * Mounted only while open (the host's `@if` is v4's `isOpen`). Its open state
 * is the actions store's `folderPicker`, which the wardrobe dialog's Escape
 * gate counts (bug 229, R-O): one Escape here closes the picker and nothing
 * else.
 *
 * **Recorded mechanism divergences.** v4's `BaseModal` portal + `closeOnEscape`
 * are local here (`qt-modal` renders in place and handles no Escape — the
 * `images/save-image-dialog.ts` precedent): a document Escape listener gated
 * by `working`, and a raised overlay (`z-[70]`, the item editor's layer) so the
 * picker covers the wardrobe dialog it opens from. v4's folder mutation
 * invalidates every folder tree and list in its `onSettled`; this picker
 * re-reads its own tree and emits `foldersChanged` (success or failure) for
 * the host to refresh the rest.
 */
@Component({
  selector: 'qt-wardrobe-folder-picker-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ItemFolderPickerDialog, WardrobeComponentModeFieldset],
  template: `
    <qt-item-folder-picker-dialog
      [mode]="mode()"
      [subject]="subject()"
      [folders]="folders.folders()"
      [loading]="folders.loading()"
      rootLabel="Wardrobe (top level)"
      [createFolder]="createFolder"
      [currentFolder]="currentFolder()"
      [working]="working()"
      [note]="note()"
      (closed)="closed.emit()"
      (submitted)="submit($event)"
    >
      @if (hasOutfits()) {
        <div class="mt-3">
          <qt-wardrobe-component-mode-fieldset
            [mode]="mode()"
            [value]="componentMode()"
            (valueChange)="componentMode.set($event)"
            [legend]="componentsLegend()"
            destination="folder"
            name="wardrobe-folder-components"
            [disabled]="working()"
          />
        </div>
      }
    </qt-item-folder-picker-dialog>
  `,
})
export class WardrobeFolderPickerDialog {
  private readonly core = inject(CoreClient);

  /** Move the items, or file copies of them. */
  readonly mode = input<'move' | 'copy'>('move');
  /** The wardrobe the items live in — the only one whose folders are offered. */
  readonly container = input.required<WardrobeContainer>();
  /** What is being moved, for the title: one item's title, or a count. */
  readonly subject = input.required<string>();
  /**
   * The folder every item being moved already sits in (disabled in the
   * list), or null when they come from different folders.
   */
  readonly currentFolder = input<string | null>(null);
  /**
   * The outfits among the items (composites). When there are any, the
   * components choice is offered; otherwise nothing is asked.
   */
  readonly outfits = input<ReadonlyArray<{ componentItemIds?: readonly string[] }>>([]);
  /** Busy while the move or copy is in flight. */
  readonly working = input(false);
  readonly closed = output<void>();
  /** The chosen folder; `components` is set only when outfits were among the items. */
  readonly submitted = output<{ folder: string; components?: ComponentMode }>();
  /** A folder was made (or the attempt settled) — the host refreshes its trees and lists. */
  readonly foldersChanged = output<void>();

  protected readonly folders = createWardrobeFoldersStore(this.core);
  /**
   * The components choice, starting at v4's `defaultComponentMode(mode)`. The
   * dialog mounts per open (the host's `@if`), so v4's reset on
   * `[isOpen, mode]` collapses into one effect over `mode` (which a mounted
   * picker never changes) — the transfer dialog's idiom.
   */
  protected readonly componentMode = signal<ComponentMode>('move');

  constructor() {
    effect(() => {
      const mode = this.mode();
      untracked(() => this.componentMode.set(defaultComponentMode(mode)));
    });
    effect(() => {
      const container = this.container();
      untracked(() => void this.folders.load(container));
    });
  }

  /** The wardrobe's two notes, by mode (v4 keeps them in the wrapper). */
  protected readonly note = computed(() =>
    this.mode() === 'move'
      ? 'Folders are for your browsing; what anyone can wear stays exactly as it was.'
      : 'Each copy is a new item with its own ID and a "(copy)" title, filed in the folder you choose.',
  );

  protected readonly hasOutfits = computed(() => this.outfits().length > 0);

  /** RAW id counts — borrowed pieces and duplicates included; a label, not what travels. */
  protected readonly componentsLegend = computed(() => {
    const outfits = this.outfits();
    const n = outfits.reduce((sum, o) => sum + (o.componentItemIds ?? []).length, 0);
    const noun = n === 1 ? 'component' : 'components';
    return outfits.length === 1
      ? `This outfit bundles ${n} ${noun}`
      : `${outfits.length} of these are outfits, bundling ${n} ${noun} between them`;
  });

  /**
   * v4 `onCreateFolder` — the folder is made in this wardrobe; afterwards
   * (success or failure, v4's `onSettled`) the picker's tree re-reads and the
   * host refreshes the rest. A refusal reaches the shared dialog, which toasts
   * the server's sentence.
   */
  protected readonly createFolder = async (path: string): Promise<{ path: string }> => {
    try {
      return await createWardrobeFolder(this.core, this.container(), path);
    } finally {
      void this.folders.refresh();
      this.foldersChanged.emit();
    }
  };

  protected submit(folder: string): void {
    // The key rides only with outfits among the items (v4 `hasOutfits ?
    // componentMode : undefined`) — absent, never `undefined`.
    this.submitted.emit(this.hasOutfits() ? { folder, components: this.componentMode() } : { folder });
  }
}
