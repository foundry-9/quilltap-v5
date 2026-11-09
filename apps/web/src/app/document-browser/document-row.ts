import {
  ChangeDetectionStrategy,
  Component,
  computed,
  input,
  linkedSignal,
  output,
  signal,
} from '@angular/core';

import { folderCrumb } from '../item-browser/item-folders';
import { ItemActionMenu, type MenuEntry } from '../item-browser/item-action-menu';
import { QtAutofocus } from '../wardrobe/autofocus.directive';
import { formatBytes } from '../ui/format-bytes';
import { Icon, type IconName } from '../ui/icon';
import { documentPreviewKind, type BrowserAction } from './document-actions';
import { buildMountFileRawUrl } from './document-store.api';
import type { DocumentEntry } from './document-store.source';
import type { PreviewKind } from './preview-kind';

const KIND_ICONS: Record<PreviewKind, IconName> = {
  image: 'image',
  pdf: 'file',
  docx: 'file',
  markdown: 'book',
  text: 'file',
  unsupported: 'file',
};

/** v4 `DocumentRow.tsx:32-44` — each action's menu label. */
export const ACTION_LABELS: Record<BrowserAction, string> = {
  open: 'Open',
  rename: 'Rename',
  move: 'Move to…',
  copy: 'Copy to…',
  link: 'Hard link to…',
  'show-links': 'Where else this appears…',
  unlink: 'Unlink from its twins',
  describe: 'Edit description…',
  workbench: "Open in Pascal's Workbench",
  download: 'Download',
  delete: 'Delete',
};

/** v4 `formatModified` — `toLocaleDateString(undefined, {month:'short', day:'numeric', year:'numeric'})`. */
export function formatModified(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  });
}

/** An action the row hands to its browser (every action but rename, which the row does itself). */
export interface DocumentRowAction {
  action: BrowserAction;
  entry: DocumentEntry;
}

/**
 * One file in a document-store browser — a port of v4
 * `components/document-browser/DocumentRow.tsx` (`c34986421`, pin
 * `624d0d25c`; P4.D387 item 8): its kind icon (an image row shows its own
 * bytes through the item route's `?raw=1` leg), the name (a button that
 * opens, or toggles the selection in select mode), the folder under the name
 * for a search hit, the `Hard-linked` / `Conversion failed` / `Converting`
 * chips, size, modified date, and the ⋮ menu of the actions it is handed
 * (`documentFileActions` decides them). Rename happens in place: Enter
 * submits, blur submits, Escape reverts — and stops there, so it never closes
 * the dialog the browser sits in (v4 `e.stopPropagation()`; `qt-modal`'s
 * `anotherDialogOwns` would not catch it, the input being inside the host
 * dialog).
 *
 * Zoneless: the rename draft is a `linkedSignal` off the entry, so a renamed
 * entry arriving through the listing never leaves a stale draft.
 */
@Component({
  selector: 'qt-document-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, ItemActionMenu, QtAutofocus],
  template: `
    <div
      class="qt-card-interactive py-2 px-3 flex items-center gap-3"
      data-testid="document-row"
      [attr.data-path]="entry().relativePath"
    >
      @if (selectMode()) {
        <input
          type="checkbox"
          [checked]="selected()"
          (change)="toggled.emit()"
          [attr.aria-label]="'Select ' + entry().fileName"
          class="flex-shrink-0"
        />
      }
      @if (kind() === 'image') {
        <img
          [src]="rawUrl()"
          alt=""
          loading="lazy"
          class="w-8 h-8 rounded object-cover flex-shrink-0 border qt-border-default"
        />
      } @else {
        <qt-icon [name]="icon()" class="w-5 h-5 qt-text-secondary flex-shrink-0 mx-1.5" />
      }
      <div class="flex-1 min-w-0">
        @if (renaming()) {
          <input
            type="text"
            qtAutofocus
            [value]="name()"
            (input)="name.set($any($event.target).value)"
            (keydown)="onRenameKey($event)"
            (blur)="submit()"
            [attr.aria-label]="'Rename ' + entry().fileName"
            class="qt-input qt-input-sm w-full"
          />
        } @else {
          <button
            type="button"
            class="block w-full text-left text-sm text-foreground truncate"
            [attr.title]="entry().relativePath"
            (click)="selectMode() ? toggled.emit() : acted.emit({ action: 'open', entry: entry() })"
          >
            {{ entry().fileName }}
          </button>
        }
        @if (showFolder() && entry().folder) {
          <div class="qt-text-xs qt-text-secondary truncate">{{ crumb() }}</div>
        }
      </div>
      <div class="hidden sm:flex items-center gap-1.5 flex-shrink-0">
        @if (entry().linkGroupId) {
          <span class="qt-badge-info" title="Hard-linked: an edit here reaches every twin"
            >Hard-linked</span
          >
        }
        @if (entry().conversionStatus === 'failed') {
          <span class="qt-badge-destructive" [attr.title]="entry().conversionError ?? null"
            >Conversion failed</span
          >
        }
        @if (converting()) {
          <span class="qt-badge-warning">Converting</span>
        }
      </div>
      <span class="qt-text-xs qt-text-secondary flex-shrink-0 w-16 text-right">{{ size() }}</span>
      <span class="qt-text-xs qt-text-secondary flex-shrink-0 w-24 text-right hidden md:inline">{{
        modified()
      }}</span>
      <qt-item-action-menu
        [label]="'Actions for ' + entry().fileName"
        [entries]="menuEntries()"
        [disabled]="selectMode()"
      />
    </div>
  `,
})
export class DocumentRow {
  readonly entry = input.required<DocumentEntry>();
  readonly actions = input.required<readonly BrowserAction[]>();
  readonly selectMode = input(false);
  readonly selected = input(false);
  /** Show the folder under the name (a search hit). */
  readonly showFolder = input(false);
  /** Rename the file; resolves whether it took (v4 `onRename`). */
  readonly rename = input.required<(entry: DocumentEntry, name: string) => Promise<boolean>>();
  /** Every action but rename (v4 `onAction`). */
  readonly acted = output<DocumentRowAction>();
  /** The select checkbox, or the name in select mode (v4 `onToggle`). */
  readonly toggled = output<void>();

  protected readonly renaming = signal(false);
  protected readonly name = linkedSignal(() => this.entry().fileName);
  protected readonly kind = computed(() => documentPreviewKind(this.entry()));
  protected readonly icon = computed(() => KIND_ICONS[this.kind()]);
  protected readonly rawUrl = computed(() =>
    buildMountFileRawUrl(this.entry().mountPointId, this.entry().relativePath),
  );
  protected readonly crumb = computed(() => folderCrumb(this.entry().folder));
  protected readonly size = computed(() => formatBytes(this.entry().fileSizeBytes));
  protected readonly modified = computed(() => formatModified(this.entry().lastModified));
  protected readonly converting = computed(
    () =>
      this.entry().conversionStatus === 'pending' &&
      ['pdf', 'docx'].includes(this.entry().fileType),
  );

  /** One entry list per (actions, entry) — a fresh array per check would trip NG0100. */
  protected readonly menuEntries = computed<MenuEntry[]>(() => {
    const entry = this.entry();
    return this.actions().map((action) => ({
      key: action,
      label: ACTION_LABELS[action],
      destructive: action === 'delete',
      onSelect: () => {
        if (action === 'rename') {
          this.name.set(entry.fileName);
          this.renaming.set(true);
        } else {
          this.acted.emit({ action, entry });
        }
      },
    }));
  });

  protected onRenameKey(event: KeyboardEvent): void {
    if (event.key === 'Enter') {
      event.preventDefault();
      void this.submit();
    } else if (event.key === 'Escape') {
      event.stopPropagation();
      this.name.set(this.entry().fileName);
      this.renaming.set(false);
    }
  }

  protected async submit(): Promise<void> {
    if (!this.renaming()) return;
    const entry = this.entry();
    const next = this.name().trim();
    if (!next || next === entry.fileName) {
      this.renaming.set(false);
      return;
    }
    if (await this.rename()(entry, next)) this.renaming.set(false);
  }
}
