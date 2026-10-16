import { NgTemplateOutlet } from '@angular/common';
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
import { Icon } from '../ui/icon';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';
import { QtAutofocus } from './autofocus.directive';
import { folderName, folderSegments, isSameFolder, joinFolder } from './folders';
import type { WardrobeContainer } from './wardrobe-container';
import {
  WardrobeComponentModeFieldset,
  defaultComponentMode,
  type ComponentMode,
} from './wardrobe-component-mode-fieldset';
import { createWardrobeFolder, createWardrobeFoldersStore } from './wardrobe-folders.api';

interface PickerRow {
  path: string;
  label: string;
  depth: number;
}

/**
 * "Move to folder…" / "Copy to folder…" — pick where in one wardrobe to file
 * one or more items, or copies of them. A port of v4
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
  imports: [Icon, Modal, NgTemplateOutlet, QtAutofocus, WardrobeComponentModeFieldset],
  host: {
    '(document:keydown.escape)': 'onEscape()',
  },
  template: `
    <qt-modal
      [title]="verb() + ' ' + subject() + ' to a folder'"
      maxWidth="md"
      overlayClass="z-[70]"
      [closeOnBackdrop]="!working()"
      (close)="closed.emit()"
    >
      <p class="qt-text-xs qt-text-secondary mb-2">
        @if (mode() === 'move') {
          Folders are for your browsing; what anyone can wear stays exactly as it was.
        } @else {
          Each copy is a new item with its own ID and a "(copy)" title, filed in the folder you choose.
        }
      </p>
      @if (folders.loading()) {
        <div class="text-sm qt-text-secondary px-2 py-3">Loading folders…</div>
      } @else {
        <ul role="listbox" aria-label="Folders" class="space-y-0.5 max-h-[50vh] overflow-y-auto">
          @for (row of rows(); track row.path || '(root)') {
            <li>
              <button
                type="button"
                role="option"
                [attr.aria-selected]="isSelected(row.path)"
                [attr.aria-disabled]="isCurrent(row.path)"
                [disabled]="isCurrent(row.path)"
                (click)="highlighted.set(row.path)"
                class="w-full text-left px-2 py-1 rounded text-sm disabled:opacity-50"
                [class.qt-bg-muted]="isSelected(row.path)"
                [class.text-foreground]="isSelected(row.path)"
                [class.qt-text-secondary]="!isSelected(row.path)"
                [style.padding-left.px]="row.depth * 16 + 8"
                [attr.title]="isCurrent(row.path) ? 'Already here' : null"
              >
                <span class="inline-flex items-center gap-1.5">
                  @if (row.path) {
                    <qt-icon name="folder" class="w-3.5 h-3.5" />
                  }
                  {{ row.label }}
                </span>
                @if (isCurrent(row.path)) {
                  <span class="qt-text-xs ml-2">(here now)</span>
                }
              </button>
              @if (newFolderOpen() && isSelected(row.path)) {
                <ng-container [ngTemplateOutlet]="newFolderInput" />
              }
            </li>
          }
          @if (newFolderOpen() && highlighted() === null) {
            <ng-container [ngTemplateOutlet]="newFolderInput" />
          }
        </ul>
      }
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

      <div qt-modal-footer class="flex items-center justify-between gap-2 w-full">
        <button
          type="button"
          class="qt-button-ghost qt-button-sm"
          (click)="newFolderOpen.set(true)"
          [disabled]="newFolderOpen()"
        >
          New folder…
        </button>
        <div class="flex gap-2">
          <button
            type="button"
            (click)="closed.emit()"
            class="qt-button-secondary qt-button-sm"
            [disabled]="working()"
          >
            Cancel
          </button>
          <button
            type="button"
            class="qt-button-primary qt-button-sm"
            [disabled]="!canSubmit()"
            (click)="submit()"
          >
            {{ working() ? (mode() === 'move' ? 'Moving…' : 'Copying…') : verb() }}
          </button>
        </div>
      </div>
    </qt-modal>

    <ng-template #newFolderInput>
      <div class="flex items-center gap-2 px-2 py-1" [style.padding-left.px]="newFolderIndent()">
        <input
          type="text"
          qtAutofocus
          [value]="newFolderName()"
          (input)="newFolderName.set($any($event.target).value)"
          (keydown.enter)="$event.preventDefault(); submitNewFolder()"
          (keydown.escape)="$event.stopPropagation(); closeNewFolder()"
          placeholder="Folder name"
          aria-label="New folder name"
          class="qt-input qt-input-sm flex-1"
        />
        <button
          type="button"
          class="qt-button-secondary qt-button-sm"
          [disabled]="pending() || !newFolderName().trim()"
          (click)="submitNewFolder()"
        >
          Create
        </button>
      </div>
    </ng-template>
  `,
})
export class WardrobeFolderPickerDialog {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

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
  protected readonly highlighted = signal<string | null>(null);
  protected readonly newFolderOpen = signal(false);
  protected readonly newFolderName = signal('');
  protected readonly pending = signal(false);
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

  protected readonly rows = computed<PickerRow[]>(() => [
    { path: '', label: 'Wardrobe (top level)', depth: 0 },
    ...this.folders.folders().map((f) => ({
      path: f.path,
      label: folderName(f.path),
      depth: folderSegments(f.path).length,
    })),
  ]);

  protected readonly verb = computed(() => (this.mode() === 'move' ? 'Move' : 'Copy'));

  protected readonly canSubmit = computed(() => {
    const target = this.highlighted();
    return target !== null && !this.isCurrent(target) && !this.working();
  });

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

  protected readonly newFolderIndent = computed(() => {
    const h = this.highlighted();
    return ((h ? folderSegments(h).length : 0) + 1) * 16 + 8;
  });

  /** A copy may land in the folder it came from; a move there would do nothing. */
  protected isCurrent(path: string): boolean {
    const current = this.currentFolder();
    return this.mode() === 'move' && current !== null && isSameFolder(path, current);
  }

  protected isSelected(path: string): boolean {
    const h = this.highlighted();
    return h !== null && isSameFolder(h, path);
  }

  protected submit(): void {
    const target = this.highlighted();
    if (target === null) return;
    // The key rides only with outfits among the items (v4 `hasOutfits ?
    // componentMode : undefined`) — absent, never `undefined`.
    this.submitted.emit(
      this.hasOutfits() ? { folder: target, components: this.componentMode() } : { folder: target },
    );
  }

  protected closeNewFolder(): void {
    this.newFolderOpen.set(false);
    this.newFolderName.set('');
  }

  protected async submitNewFolder(): Promise<void> {
    const name = this.newFolderName().trim();
    if (!name) return;
    this.pending.set(true);
    try {
      const created = await createWardrobeFolder(
        this.core,
        this.container(),
        joinFolder(this.highlighted() ?? '', name),
      );
      this.highlighted.set(created.path);
      this.closeNewFolder();
    } catch (error) {
      this.toasts.showError(error instanceof Error ? error.message : 'The folder would not be made');
    } finally {
      this.pending.set(false);
      // v4 `onSettled`: every folder tree and list re-reads, success or failure.
      void this.folders.refresh();
      this.foldersChanged.emit();
    }
  }

  /** v4 `closeOnEscape={!working}` — the picker's own; the dialog's gate counts it. */
  protected onEscape(): void {
    if (this.working()) return;
    this.closed.emit();
  }
}
