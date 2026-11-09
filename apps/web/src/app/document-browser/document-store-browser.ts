import { NgTemplateOutlet } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  type ElementRef,
  type TemplateRef,
  computed,
  inject,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';
import { Router } from '@angular/router';

import { coreErrorMessage } from '../core/core-client';
import type { DocMountLinkLocation } from '../core/core-contract';
import { FolderBrowserShell } from '../item-browser/folder-browser-shell';
import {
  createFolderBrowserState,
  createItemFolderHandlers,
  pluralize,
  type ItemFolderOps,
} from '../item-browser/folder-browser-state';
import { OWN_TIER, type FolderRowView } from '../item-browser/folder-view';
import type { MenuEntryLike } from '../item-browser/item-action-menu';
import type { DestinationMode } from '../item-browser/destination-picker-dialog';
import { folderName, joinFolder, parentFolder } from '../item-browser/item-folders';
import { AlertModalService } from '../ui/alert-modal.service';
import { formatBytes } from '../ui/format-bytes';
import { ToastService } from '../ui/toast.service';
import { WORKSPACE_HANDLE } from '../workspace/workspace-contract';
import {
  canLinkBetween,
  capabilitiesOf,
  documentFileActions,
  documentFolderActions,
  type BrowserAction,
} from './document-actions';
import { DocumentDescriptionDialog } from './document-description-dialog';
import {
  DocumentDestinationDialog,
  injectStoreCatalogue,
  type DocumentDestination,
} from './document-destination-dialog';
import { DocumentLinksDialog } from './document-links-dialog';
import { DocumentRow, type DocumentRowAction } from './document-row';
import { createDocumentStoreSource, type DocumentEntry } from './document-store.source';
import { DocumentViewerModal } from './document-viewer-modal';
import { downloadDocument } from './document-viewer';

type Transfer =
  | { kind: 'files'; mode: DestinationMode; entries: DocumentEntry[] }
  | { kind: 'folder'; mode: Exclude<DestinationMode, 'link'>; path: string };

/** What a picker's footer template receives (v4 `footerActions(currentFolder)`). */
export interface DocumentBrowserFooterContext {
  $implicit: string;
}

const tierOf = (): string => OWN_TIER;

/** v4 `showConfirmation` — `showAlert(message, ['Cancel', 'Confirm'])` → `'Confirm'`. */
async function confirmWith(alerts: AlertModalService, message: string): Promise<boolean> {
  return (await alerts.show(message, ['Cancel', 'Confirm'])) === 'Confirm';
}

/**
 * A document store's files and folders, browsed the way the wardrobe and the
 * Inventory are — a port of v4
 * `components/document-browser/DocumentStoreBrowser.tsx` (`c34986421`, pin
 * `624d0d25c`; P4.D387 item 12).
 *
 * v4's doc, carried: the shared `FolderBrowserShell` (breadcrumb, folder
 * rows, select mode, New folder) over the store source, with a `DocumentRow`
 * per file and a ⋮ menu on every file and folder. Files open read-only in
 * `DocumentViewerModal`; Move / Copy / Hard link go through
 * `DocumentDestinationDialog`, to any folder of this store or another that
 * can take them. What each row offers comes from `documentFileActions` /
 * `documentFolderActions`, gated by the store's capabilities. Deleting a
 * folder deletes what is inside it, after a confirmation that says so.
 *
 * v5 shape: v4's `onPickFile` is `pickMode` + the `pickFile` output (pick
 * mode — opening a file hands it over instead of showing it; the ⋮ menus
 * stay); `onOpenStore` is the `openStore` output, and with nothing bound a
 * "where else" location in another store is named in a toast (v4's
 * `onOpenStore` absent); `footerActions(currentFolder)` is the `footer`
 * template input, given the folder being browsed as `$implicit` (ruling L-4).
 */
@Component({
  selector: 'qt-document-store-browser',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    FolderBrowserShell,
    DocumentRow,
    DocumentViewerModal,
    DocumentLinksDialog,
    DocumentDescriptionDialog,
    DocumentDestinationDialog,
    NgTemplateOutlet,
  ],
  template: `
    <qt-folder-browser-shell
      [state]="state"
      [hasContainer]="true"
      [loading]="source.storeLoading() || source.listingLoading()"
      testIdPrefix="documents"
      [rootLabel]="source.store()?.name ?? 'Store'"
      countNoun="file"
      [text]="text()"
      [itemTemplate]="row"
      [folderHandlers]="folderHandlers"
      [folderEntries]="folderEntries"
      [manageFolders]="caps().canWrite"
      [scrollList]="scrollList()"
    >
      <div qtBrowserFilters class="flex flex-wrap items-center gap-3">
        <input
          type="search"
          aria-label="Search this store"
          placeholder="Search this store…"
          [value]="query()"
          (input)="query.set($any($event.target).value)"
          class="qt-input qt-input-sm flex-1 min-w-[12rem]"
        />
        <span class="qt-text-xs qt-text-secondary">{{ countLine() }}</span>
      </div>

      <ng-container qtBrowserSelectActions>
        @if (caps().canWrite && caps().canMoveOut) {
          <button
            type="button"
            class="qt-button-secondary qt-button-sm"
            [disabled]="state.selectedItems().length === 0"
            (click)="openTransfer('move', state.selectedItems())"
          >
            Move…
          </button>
        }
        @if (caps().canMoveOut) {
          <button
            type="button"
            class="qt-button-secondary qt-button-sm"
            [disabled]="state.selectedItems().length === 0"
            (click)="openTransfer('copy', state.selectedItems())"
          >
            Copy…
          </button>
        }
        @if (caps().canDelete) {
          <button
            type="button"
            class="qt-button-destructive qt-button-sm"
            [disabled]="state.selectedItems().length === 0"
            (click)="deleteFiles(state.selectedItems())"
          >
            Delete
          </button>
        }
      </ng-container>

      <ng-container qtBrowserFooter>
        @if (footer(); as tpl) {
          <ng-container
            [ngTemplateOutlet]="tpl"
            [ngTemplateOutletContext]="{ $implicit: state.currentFolder() }"
          />
        }
        @if (caps().canWrite) {
          <label
            class="qt-button-primary qt-button-sm cursor-pointer"
            [class.opacity-50]="uploading()"
          >
            <input
              #fileInput
              type="file"
              multiple
              class="sr-only"
              aria-label="Upload files into this folder"
              [disabled]="uploading()"
              (change)="upload($any($event.target).files)"
            />
            {{ uploading() ? 'Uploading…' : 'Upload here' }}
          </label>
        }
      </ng-container>

      @if (viewing(); as v) {
        <qt-document-viewer-modal [entry]="v" (closed)="viewing.set(null)" />
      }
      <qt-document-links-dialog
        [storeId]="storeId()"
        [path]="linksPath()"
        (closed)="linksPath.set(null)"
        (reveal)="revealLocation($event)"
      />
      <qt-document-description-dialog
        [file]="describing()"
        [saveDescription]="saveDescription"
        (closed)="describing.set(null)"
      />
      @if (source.store(); as store) {
        @if (transfer(); as t) {
          <qt-document-destination-dialog
            [mode]="t.mode"
            [source]="store"
            [subject]="transferSubject()"
            [currentFolder]="transferFolder()"
            [working]="working()"
            (closed)="closeTransfer()"
            (submitted)="submitTransfer($event)"
          />
        }
      }
    </qt-folder-browser-shell>

    <ng-template #row let-entry>
      <qt-document-row
        [entry]="entry"
        [actions]="actionsFor(entry)"
        [rename]="renameFile"
        [selectMode]="state.selectMode()"
        [selected]="state.selectedIds().has(entry.id)"
        (toggled)="state.toggleSelected(entry)"
        [showFolder]="searching()"
        (acted)="onAction($event)"
      />
    </ng-template>
  `,
})
export class DocumentStoreBrowser {
  private readonly toasts = inject(ToastService);
  private readonly alerts = inject(AlertModalService);
  private readonly router = inject(Router);
  private readonly workspace = inject(WORKSPACE_HANDLE, { optional: true });

  readonly storeId = input.required<string>();
  /** Cap the list's height (a dialog) instead of letting it grow with the page. */
  readonly scrollList = input(false);
  /** Pick mode (Document Mode's picker, the Library picker): Open hands the file to `pickFile`. */
  readonly pickMode = input(false);
  /** More footer buttons, given the folder being browsed (the picker's New document here). */
  readonly footer = input<TemplateRef<DocumentBrowserFooterContext> | undefined>(undefined);
  readonly pickFile = output<DocumentEntry>();
  /**
   * Whether the host opens another store when asked — v4's `onOpenStore`
   * presence (an output's binding cannot be observed). Without it, a "where
   * else" location in another store is named in a toast.
   */
  readonly opensStores = input(false);
  /** Open another store (a "where else" location there), when `opensStores`. */
  readonly openStore = output<string>();

  protected readonly source = createDocumentStoreSource(() => this.storeId());
  private readonly catalogue = injectStoreCatalogue(() => true);
  protected readonly caps = computed(() => capabilitiesOf(this.source.store()));

  protected readonly query = signal('');
  protected readonly viewing = signal<DocumentEntry | null>(null);
  protected readonly linksPath = signal<string | null>(null);
  protected readonly describing = signal<DocumentEntry | null>(null);
  protected readonly transfer = signal<Transfer | null>(null);
  protected readonly working = signal(false);
  protected readonly uploading = signal(false);
  private readonly fileInput = viewChild<ElementRef<HTMLInputElement>>('fileInput');

  protected readonly searching = computed(() => this.query().trim() !== '');
  private readonly sorted = computed(() =>
    [...this.source.entries()].sort((a, b) =>
      a.fileName.localeCompare(b.fileName, undefined, { sensitivity: 'base' }),
    ),
  );
  private readonly filtered = computed(() => {
    const q = this.query().trim().toLowerCase();
    return q
      ? this.sorted().filter((e) => e.relativePath.toLowerCase().includes(q))
      : this.sorted();
  });

  protected readonly state = createFolderBrowserState<DocumentEntry>({
    containerKey: () => this.storeId(),
    items: this.sorted,
    filtered: this.filtered,
    tree: this.source.tree,
    tierOf: () => tierOf,
    characterView: () => false,
    showShared: () => false,
    searching: this.searching,
  });

  protected readonly folderHandlers = createItemFolderHandlers<DocumentEntry>({
    state: this.state,
    ops: () => {
      if (!this.source.store()) return null;
      const ops: ItemFolderOps = {
        create: (path) => this.source.ops.createFolder(path),
        rename: (path, name) => this.source.ops.renameFolder(path, name),
        remove: async (path) => {
          await this.source.ops.deleteFolder(path);
          return { movedItems: 0 };
        },
      };
      return ops;
    },
    wording: {
      noun: 'file',
      rootPhrase: 'the top of the store',
      created: (name) => `Folder "${name}" made`,
      movedUp: (count) => `${count} removed`,
      deleteMessage: ({ name, count }) =>
        count > 0
          ? `Delete the folder "${name}" and the ${pluralize(count, 'file')} inside it? They go for good.`
          : `Remove the folder "${name}"?`,
    },
    confirm: (message) => confirmWith(this.alerts, message),
    describeError: coreErrorMessage,
  });

  protected readonly text = computed(() => ({
    noContainer: '',
    folderEmpty: this.caps().canWrite
      ? 'Nothing filed here yet. Upload something, or make a folder.'
      : 'Nothing filed here yet.',
    noMatch: 'Nothing by that name.',
  }));

  private readonly linkTargets = computed(() => {
    const store = this.source.store();
    return Boolean(store) && (this.catalogue.data() ?? []).some((s) => canLinkBetween(store!, s));
  });

  /** One actions array per entry and gate state (memoised: a fresh array per check would trip NG0100). */
  private readonly actionsCache = computed(() => {
    const store = this.source.store();
    const linkTargets = this.linkTargets();
    const cache = new Map<string, readonly BrowserAction[]>();
    for (const entry of this.source.entries()) {
      cache.set(
        entry.id,
        store ? documentFileActions(store, entry, { linkTargets }) : ['open', 'download'],
      );
    }
    return cache;
  });
  protected actionsFor(entry: DocumentEntry): readonly BrowserAction[] {
    return this.actionsCache().get(entry.id) ?? ['open', 'download'];
  }

  protected readonly countLine = computed(() => {
    const entries = this.source.entries();
    const total = entries.reduce((sum, e) => sum + e.fileSizeBytes, 0);
    return `${pluralize(entries.length, 'file')} · ${formatBytes(total)}`;
  });

  /** v4 `folderEntries` — Move to… / Copy to… on a folder row, gated by the store. */
  protected readonly folderEntries = (row: FolderRowView): readonly MenuEntryLike[] => {
    const store = this.source.store();
    if (!store) return [];
    const allowed = documentFolderActions(store);
    return [
      allowed.includes('move') && {
        key: 'move',
        label: 'Move to…',
        onSelect: () => this.transfer.set({ kind: 'folder', mode: 'move', path: row.path }),
      },
      allowed.includes('copy') && {
        key: 'copy',
        label: 'Copy to…',
        onSelect: () => this.transfer.set({ kind: 'folder', mode: 'copy', path: row.path }),
      },
    ];
  };

  protected readonly transferSubject = computed(() => {
    const t = this.transfer();
    if (!t) return '';
    if (t.kind === 'folder') return `folder "${folderName(t.path)}"`;
    return t.entries.length === 1
      ? `"${t.entries[0].fileName}"`
      : pluralize(t.entries.length, 'file');
  });

  protected readonly transferFolder = computed<string | null>(() => {
    const t = this.transfer();
    if (!t) return null;
    if (t.kind === 'folder') return parentFolder(t.path);
    return t.entries.every((e) => e.folder === t.entries[0].folder)
      ? (t.entries[0]?.folder ?? null)
      : null;
  });

  protected openTransfer(mode: 'move' | 'copy', entries: DocumentEntry[]): void {
    this.transfer.set({ kind: 'files', mode, entries });
  }

  protected closeTransfer(): void {
    if (!this.working()) this.transfer.set(null);
  }

  /** v4 `openInWorkbench` — a workspace tab when hosted, else the routed Workbench. */
  private openInWorkbench(relativePath: string): void {
    if (this.workspace) {
      this.workspace.openTab('custom-tools', { mountPointId: this.storeId(), path: relativePath });
      return;
    }
    void this.router.navigate(['/custom-tools'], {
      queryParams: { mount: this.storeId(), path: relativePath },
    });
  }

  protected readonly renameFile = async (entry: DocumentEntry, name: string): Promise<boolean> => {
    try {
      await this.source.ops.renameFile(entry.relativePath, joinFolder(entry.folder, name));
      this.toasts.showSuccess(`Renamed to "${name}"`);
      return true;
    } catch (error) {
      this.toasts.showError(coreErrorMessage(error, 'The file would not be renamed'));
      return false;
    }
  };

  protected async deleteFiles(targets: DocumentEntry[]): Promise<void> {
    const message =
      targets.length === 1
        ? `Delete "${targets[0].fileName}"? It goes for good, and links to it elsewhere will find nothing.`
        : `Delete ${pluralize(targets.length, 'file')}? They go for good.`;
    if (!(await confirmWith(this.alerts, message))) return;
    let done = 0;
    for (const entry of targets) {
      try {
        await this.source.ops.deleteFile(entry.relativePath);
        done++;
      } catch (error) {
        this.toasts.showError(
          `${entry.fileName}: ${coreErrorMessage(error, 'would not be deleted')}`,
        );
      }
    }
    if (done > 0) this.toasts.showSuccess(`${pluralize(done, 'file')} deleted`);
    this.state.setSelectMode(false);
  }

  protected onAction({ action, entry }: DocumentRowAction): void {
    switch (action) {
      case 'open':
        if (this.pickMode()) this.pickFile.emit(entry);
        else this.viewing.set(entry);
        return;
      case 'move':
      case 'copy':
      case 'link':
        this.transfer.set({ kind: 'files', mode: action, entries: [entry] });
        return;
      case 'show-links':
        this.linksPath.set(entry.relativePath);
        return;
      case 'unlink':
        this.source.ops
          .unlinkFile(entry.relativePath)
          .then(() => this.toasts.showSuccess(`"${entry.fileName}" goes its own way now`))
          .catch((error: unknown) =>
            this.toasts.showError(coreErrorMessage(error, 'The file would not be unlinked')),
          );
        return;
      case 'describe':
        this.describing.set(entry);
        return;
      case 'workbench':
        this.openInWorkbench(entry.relativePath);
        return;
      case 'download':
        void downloadDocument(this.toasts, entry);
        return;
      case 'delete':
        void this.deleteFiles([entry]);
        return;
      default:
        return;
    }
  }

  protected async submitTransfer(dest: DocumentDestination): Promise<void> {
    const transfer = this.transfer();
    if (!transfer) return;
    this.working.set(true);
    try {
      if (transfer.kind === 'folder') {
        const target = {
          mountPointId: dest.mountPointId,
          path: joinFolder(dest.folder, folderName(transfer.path)),
        };
        const verb = transfer.mode === 'move' ? 'moved' : 'copied';
        try {
          await (
            transfer.mode === 'move' ? this.source.ops.moveFolder : this.source.ops.copyFolder
          )(transfer.path, target);
          this.toasts.showSuccess(`Folder "${folderName(transfer.path)}" ${verb}`);
          this.transfer.set(null);
        } catch (error) {
          this.toasts.showError(coreErrorMessage(error, `The folder would not be ${verb}`));
        }
        return;
      }
      const run =
        transfer.mode === 'move'
          ? this.source.ops.moveFile
          : transfer.mode === 'copy'
            ? this.source.ops.copyFile
            : this.source.ops.linkFile;
      const past =
        transfer.mode === 'move' ? 'moved' : transfer.mode === 'copy' ? 'copied' : 'linked';
      let done = 0;
      for (const entry of transfer.entries) {
        try {
          await run(entry.relativePath, {
            mountPointId: dest.mountPointId,
            path: joinFolder(dest.folder, entry.fileName),
          });
          done++;
        } catch (error) {
          this.toasts.showError(
            `${entry.fileName}: ${coreErrorMessage(error, `would not be ${past}`)}`,
          );
        }
      }
      if (done > 0) {
        this.toasts.showSuccess(`${pluralize(done, 'file')} ${past}`);
        this.transfer.set(null);
        this.state.setSelectMode(false);
      }
    } finally {
      this.working.set(false);
    }
  }

  protected async upload(files: FileList | null): Promise<void> {
    if (!files || files.length === 0) return;
    // Read the folder BEFORE any await (v4's handler closes over one render).
    const currentFolder = this.state.currentFolder();
    this.uploading.set(true);
    let done = 0;
    try {
      for (const file of Array.from(files)) {
        try {
          await this.source.ops.uploadFile(joinFolder(currentFolder, file.name), file);
          done++;
        } catch (error) {
          this.toasts.showError(`${file.name}: ${coreErrorMessage(error, 'would not upload')}`);
        }
      }
      if (done > 0) this.toasts.showSuccess(`${pluralize(done, 'file')} filed`);
    } finally {
      this.uploading.set(false);
      const el = this.fileInput()?.nativeElement;
      if (el) el.value = '';
    }
  }

  /** v4 `revealLocation`: this store → navigate; another → `openStore`, or a toast naming it. */
  protected revealLocation(loc: DocMountLinkLocation): void {
    if (loc.mountPointId === this.storeId()) {
      this.state.navigate(OWN_TIER, parentFolder(loc.relativePath));
    } else if (this.opensStores()) {
      this.openStore.emit(loc.mountPointId);
    } else {
      this.toasts.showSuccess(`It lives in ${loc.mountPointName}, at ${loc.relativePath}`);
    }
  }

  protected readonly saveDescription = async (description: string): Promise<boolean> => {
    const describing = this.describing();
    if (!describing) return false;
    try {
      await this.source.ops.describeFile(describing.relativePath, description);
      this.toasts.showSuccess('Description saved');
      return true;
    } catch (error) {
      this.toasts.showError(coreErrorMessage(error, 'The description would not be saved'));
      return false;
    }
  };
}
