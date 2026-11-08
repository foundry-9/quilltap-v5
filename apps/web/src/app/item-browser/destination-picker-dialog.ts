import { NgTemplateOutlet } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';

import { Icon } from '../ui/icon';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';
import { QtAutofocus } from '../wardrobe/autofocus.directive';
import { folderName, folderSegments, isSameFolder, joinFolder } from './item-folders';

/** What the picker files: a move, a copy, or a hard link (v4 `DestinationMode`, `c34986421`). */
export type DestinationMode = 'move' | 'copy' | 'link';

/** v4 `VERBS` — each mode's button verb, in-flight label and title verb. */
const VERBS: Record<DestinationMode, { verb: string; working: string; title: string }> = {
  move: { verb: 'Move', working: 'Moving…', title: 'Move' },
  copy: { verb: 'Copy', working: 'Copying…', title: 'Copy' },
  link: { verb: 'Link', working: 'Linking…', title: 'Hard link' },
};

interface PickerRow {
  path: string;
  label: string;
  depth: number;
}

/**
 * "Move to…" / "Copy to…" / "Hard link to…" for any folder-of-items browser
 * — the wardrobe's and the Inventory's tiers, the Scriptorium's stores: pick
 * the folder (and, where the caller offers one, the container) something goes
 * to. A port of v4 `components/item-browser/DestinationPickerDialog.tsx` at the
 * pin `624d0d25c` — `236c04e35`'s `ItemFolderPickerDialog.tsx` (`4fbdad186`)
 * renamed by `c34986421` with the link mode, `VERBS`, a container slot and a
 * key-scoped highlight (this file was `git mv`'d from
 * `item-folder-picker-dialog.ts`).
 *
 * The container's folder tree as an indented list, top level first; for a
 * MOVE, the folder the items already share is shown but can't be chosen
 * (`Already here`, `(here now)`, `aria-disabled`) — a copy or a link may land
 * beside its original. **New folder…** opens an input under the highlighted
 * folder (or at the end when nothing is highlighted) and creates it there
 * through `createFolder`, then highlights the folder the server answered. The
 * caller supplies the tree, the folder creator, the note, an optional
 * container chooser (projected `[qtDestinationContainer]`, rendered after the
 * note and above the tree; changing `containerKey` clears the highlight — the
 * highlight is `{key, path}`, so returning to a container reads its highlight
 * again, as v4's does) and any extra choice (the wardrobe's components
 * fieldset, the default slot); the submit button emits the folder.
 *
 * **Recorded mechanism divergence:** v4's `BaseModal` portal is local here — a
 * raised overlay (`z-[70]`) so the picker covers the dialog it opens from. Its
 * Escape is `qt-modal`'s own, gated by `[closeOnEscape]="!working()"` (v4
 * `closeOnEscape`, P4.D358 R-N). Mounted only while open (the caller's `@if` is v4's
 * `isOpen`).
 */
@Component({
  selector: 'qt-destination-picker-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Modal, NgTemplateOutlet, QtAutofocus],
  template: `
    <qt-modal
      [title]="verbs().title + ' ' + subject() + ' to a folder'"
      maxWidth="md"
      overlayClass="z-[70]"
      [closeOnBackdrop]="!working()"
      [closeOnEscape]="!working()"
      (close)="closed.emit()"
    >
      <p class="qt-text-xs qt-text-secondary mb-2">{{ note() }}</p>
      <ng-content select="[qtDestinationContainer]" />
      @if (loading()) {
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
                (click)="setHighlighted(row.path)"
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
      <ng-content />

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
            {{ working() ? verbs().working : verbs().verb }}
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
          [disabled]="creating() || pending() || !newFolderName().trim()"
          (click)="submitNewFolder()"
        >
          Create
        </button>
      </div>
    </ng-template>
  `,
})
export class DestinationPickerDialog {
  private readonly toasts = inject(ToastService);

  readonly mode = input<DestinationMode>('move');
  /** What is being moved, for the title. */
  readonly subject = input.required<string>();
  /** The chosen container's key; a change clears the highlighted folder. */
  readonly containerKey = input('');
  /** The container's folder paths. */
  readonly folders = input<ReadonlyArray<{ path: string }>>([]);
  readonly loading = input(false);
  /** The root row's label, e.g. `Wardrobe (top level)`. */
  readonly rootLabel = input.required<string>();
  /** Create a folder (full path) and answer its stored path (v4 `onCreateFolder`). */
  readonly createFolder = input.required<(path: string) => Promise<{ path: string }>>();
  /** The caller's folder write is in flight (v4 `creating`). */
  readonly creating = input(false);
  /** The folder every item already sits in (disabled for a move), or null. */
  readonly currentFolder = input<string | null>(null);
  readonly working = input(false);
  /** The explanatory line under the title. */
  readonly note = input.required<string>();
  readonly closed = output<void>();
  /** The chosen folder (v4 `onSubmit`; `submit` is a native DOM event name). */
  readonly submitted = output<string>();

  /** v4 `highlight` — `{key, path}`, read only in the container it was set in. */
  private readonly highlight = signal<{ key: string; path: string | null }>({ key: '', path: null });
  protected readonly highlighted = computed(() => {
    const h = this.highlight();
    return h.key === this.containerKey() ? h.path : null;
  });
  protected readonly newFolderOpen = signal(false);
  protected readonly newFolderName = signal('');
  /** This picker's own create is in flight. */
  protected readonly pending = signal(false);

  protected readonly rows = computed<PickerRow[]>(() => [
    { path: '', label: this.rootLabel(), depth: 0 },
    ...this.folders().map((f) => ({ path: f.path, label: folderName(f.path), depth: folderSegments(f.path).length })),
  ]);

  protected readonly verbs = computed(() => VERBS[this.mode()]);

  protected readonly canSubmit = computed(() => {
    const target = this.highlighted();
    return target !== null && !this.isCurrent(target) && !this.working();
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

  protected setHighlighted(path: string | null): void {
    this.highlight.set({ key: this.containerKey(), path });
  }

  protected submit(): void {
    const target = this.highlighted();
    if (target === null) return;
    this.submitted.emit(target);
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
      const created = await this.createFolder()(joinFolder(this.highlighted() ?? '', name));
      this.setHighlighted(created.path);
      this.closeNewFolder();
    } catch (error) {
      this.toasts.showError(error instanceof Error ? error.message : 'The folder would not be made');
    } finally {
      this.pending.set(false);
    }
  }
}
