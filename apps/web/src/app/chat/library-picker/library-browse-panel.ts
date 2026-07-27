import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type { FolderEntry } from '../../core/core-contract';
import {
  breadcrumbSegments,
  deriveSubfolders,
  filesInFolder,
  parentFolder,
  fileDisplayName,
  getFileTypeLabel,
  type DbFolder,
  type SortState,
} from '../../screens/files/file-model';
import { FileThumbnail } from '../../screens/files/file-thumbnail';
import { formatBytes } from '../../ui/format-bytes';
import { LoadingState } from '../../ui/loading-state';
import { ErrorAlert } from '../../ui/error-alert';
import {
  fetchLegacyFiles,
  fetchMountFiles,
  fetchProjectStores,
  libraryPickerKeys,
} from './library-picker.api';
import { pickPrimaryProjectStore, type PickerFile, type PickerStoreSummary } from './library-picker.model';

/** The scope this panel browses (v4's `projectId` + optional `mountPoint` pair). */
export interface BrowseScope {
  /** `null` = General (v4 `/files?filter=general`); an id = that project. */
  projectId: string | null;
  /**
   * A pre-resolved store. When set the panel browses it directly; when absent
   * AND a `projectId` is set, the panel auto-resolves the project's own store
   * exactly as v4's FileBrowser does.
   */
  mountPoint?: PickerStoreSummary | null;
}

/**
 * The Library picker's browse step (v4 `LibraryFilePickerModal` step 2's
 * `<FileBrowser … showUpload={false}>`, `:384-405`).
 *
 * ## Why this is a bespoke panel and not `qt-files-browser`
 *
 * The order sanctions either choice; this lane ported a narrower panel, because
 * reuse in place was not available:
 *
 * - v4 has ONE `FileBrowser` covering both legacy and mount modes. v5 split it
 *   in two — `qt-files-browser` (the legacy `/files` PAGE) and the Scriptorium
 *   `qt-file-manager` (mount mode, behind its own beta toggle) — so no single v5
 *   component answers both, and the picker needs both in the same step.
 * - `qt-files-browser` takes NO inputs and emits NO file-click: it hardcodes
 *   `filesList({filter:'general'})` and owns page chrome (an `<h2>`, the
 *   sync/orphan-cleanup toolbar, the preview lightbox and the create-folder /
 *   move / delete dialogs). Reusing it would mean parameterising a landed screen
 *   into a mode it was never shaped for.
 *
 * What is reused is the part that matters for fidelity: the **pure** folder model
 * (`file-model.ts` — `filesInFolder`, `deriveSubfolders`, `parentFolder`,
 * `breadcrumbSegments`, `sortFiles`), so the picker's folder navigation and sort
 * are literally v4's, byte for byte, via the same tested code the Files page uses.
 *
 * **Deliberate reduction, recorded:** inside v4's picker the FileBrowser still
 * offers New Folder, Sync, orphan cleanup, the grid/list toggle, per-file Delete
 * and Move-to-Project, and the preview lightbox. This panel offers **none** of
 * them: it is read-only. Every one of those is a mutation (or a second modal) on
 * a dialog whose entire purpose is to pick one file and close, and the order
 * names the target "a read-only no-upload mode". Upload is unreachable in v4 here
 * too (`showUpload={false}`) and is a tier-3 deferral, not an omission.
 *
 * ## The auto-resolve (v4 `FileBrowser.tsx:226-262`)
 *
 * A project scope with no explicit store asks `projectMountPointList` once and
 * narrows with `pickPrimaryProjectStore`. A failure falls back to legacy mode
 * rather than failing the panel — v4's comment: one missing link shouldn't take
 * down the browser. Only DATABASE-backed stores switch the panel into mount
 * mode; filesystem / obsidian mounts keep the legacy path, since their files
 * live on disk.
 */
@Component({
  selector: 'qt-library-browse-panel',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FileThumbnail, LoadingState, ErrorAlert],
  template: `
    <div class="relative min-h-[50vh] flex flex-col">
      @if (linking()) {
        <!-- v4 LinkingOverlay (:610-616) — seals the panel while a pick is in
             flight, so a second click can't start a second write. -->
        <div
          class="absolute inset-0 z-10 flex items-center justify-center"
          style="background: var(--qt-dialog-backdrop)"
          data-testid="library-picker-linking"
        >
          <p class="qt-text-secondary">Attaching…</p>
        </div>
      }

      <div class="mb-3 flex items-center gap-2 text-sm flex-wrap">
        <button type="button" class="hover:text-primary transition-colors" (click)="goToRoot()">
          Root
        </button>
        @for (crumb of breadcrumbs(); track crumb.path) {
          <span class="flex items-center gap-2">
            <span class="qt-text-secondary">/</span>
            <button
              type="button"
              class="hover:text-primary transition-colors"
              (click)="currentFolder.set(crumb.path)"
            >
              {{ crumb.label }}
            </button>
          </span>
        }
        @if (activeStore(); as store) {
          <span
            class="qt-text-xs qt-text-secondary ml-auto"
            [title]="'Linked Scriptorium store: ' + store.name"
          >
            {{ storeIcon }} {{ store.name }}
          </span>
        }
      </div>

      @if (query.isPending()) {
        <div class="flex-1 flex items-center justify-center">
          <qt-loading-state message="Loading files..." />
        </div>
      } @else if (query.isError()) {
        <qt-error-alert
          [message]="'Error: ' + errorMessage()"
          [retryable]="true"
          (retry)="query.refetch()"
        />
      } @else {
        <ul class="flex-1 overflow-y-auto space-y-1">
          @if (currentFolder() !== '/') {
            <li>
              <button
                type="button"
                class="w-full flex items-center gap-3 p-2 rounded-lg hover:qt-bg-surface-alt transition-colors text-left"
                (click)="goUp()"
              >
                <span class="text-xl w-10 text-center">{{ folderIcon }}</span>
                <span class="qt-text-secondary">..</span>
              </button>
            </li>
          }

          @for (folder of subfolders(); track folder.path) {
            <li>
              <button
                type="button"
                class="w-full flex items-center gap-3 p-2 rounded-lg hover:qt-bg-surface-alt transition-colors text-left"
                (click)="currentFolder.set(folder.path)"
              >
                <span class="text-xl w-10 text-center">{{ folderIcon }}</span>
                <span class="min-w-0 flex-1">
                  <span class="qt-label text-foreground block truncate">{{ folder.name }}</span>
                  <span class="qt-text-xs qt-text-secondary">
                    {{ folder.fileCount }} file{{ folder.fileCount !== 1 ? 's' : '' }}
                  </span>
                </span>
              </button>
            </li>
          }

          @for (file of visibleFiles(); track file.id) {
            <li>
              <button
                type="button"
                class="w-full flex items-center gap-3 p-2 rounded-lg hover:qt-bg-surface-alt transition-colors text-left disabled:opacity-50"
                [disabled]="linking()"
                [title]="name(file)"
                (click)="pick.emit(file)"
              >
                <qt-file-thumbnail
                  [fileId]="file.id"
                  [mimeType]="file.mimeType"
                  [alt]="name(file) || 'File'"
                  [size]="40"
                  [mountPointId]="file.mountPointId"
                  [relativePath]="file.relativePath"
                  className="rounded flex-shrink-0"
                />
                <span class="min-w-0 flex-1">
                  <span class="qt-label text-foreground block truncate">{{ name(file) }}</span>
                  <span class="qt-text-xs qt-text-secondary">
                    {{ typeLabel(file) }} &bull; {{ size(file) }}
                  </span>
                </span>
              </button>
            </li>
          }

          @if (isEmpty()) {
            <li
              class="flex flex-col items-center justify-center py-12 text-center qt-text-secondary"
            >
              <span class="text-5xl mb-3">{{ openFolderIcon }}</span>
              <p class="text-lg">No files here</p>
            </li>
          }
        </ul>
      }
    </div>
  `,
})
export class LibraryBrowsePanel {
  private readonly core = inject(CoreClient);

  readonly scope = input.required<BrowseScope>();
  /** A pick is in flight — seal the panel (v4 `linking`). */
  readonly linking = input(false);

  readonly pick = output<PickerFile>();

  protected readonly folderIcon = '\u{1F4C1}';
  protected readonly openFolderIcon = '\u{1F4C2}';
  protected readonly storeIcon = '\u{1F4DA}';

  protected readonly currentFolder = signal('/');
  /** v4's FileBrowser default (`sort` useState, name ascending). */
  private readonly sort: SortState = { field: 'name', direction: 'asc' };

  /**
   * The panel's one query. It resolves the mode FIRST (explicit store → that
   * store; project with no store → the auto-resolve; else legacy) and then reads,
   * so a project whose store lookup fails still lists its legacy files.
   */
  protected readonly query = injectQuery(() => {
    const scope = this.scope();
    return {
      queryKey: [
        ...libraryPickerKeys.legacyFiles(scope.projectId),
        scope.mountPoint?.id ?? null,
      ] as unknown[],
      queryFn: async (): Promise<{
        files: PickerFile[];
        folders: FolderEntry[];
        store: PickerStoreSummary | null;
      }> => {
        const store = await this.resolveStore(scope);
        if (store) {
          return { files: await fetchMountFiles(this.core, store.id), folders: [], store };
        }
        const legacy = await fetchLegacyFiles(this.core, scope.projectId);
        return { ...legacy, store: null };
      },
    };
  });

  protected readonly activeStore = computed(() => this.query.data()?.store ?? null);
  private readonly allFiles = computed(() => this.query.data()?.files ?? []);
  private readonly dbFolders = computed(
    () => (this.query.data()?.folders ?? []) as unknown as DbFolder[],
  );

  protected readonly visibleFiles = computed(
    () => filesInFolder(this.allFiles(), this.currentFolder(), this.sort) as PickerFile[],
  );
  protected readonly subfolders = computed(() =>
    deriveSubfolders(this.allFiles(), this.dbFolders(), this.currentFolder()),
  );
  protected readonly breadcrumbs = computed(() => breadcrumbSegments(this.currentFolder()));
  protected readonly isEmpty = computed(
    () => this.visibleFiles().length === 0 && this.subfolders().length === 0,
  );

  /**
   * v4's mount-mode gate. An explicit store wins verbatim; otherwise a project
   * scope auto-resolves once. Only `database` mounts switch modes.
   */
  private async resolveStore(scope: BrowseScope): Promise<PickerStoreSummary | null> {
    if (scope.mountPoint !== undefined) {
      const explicit = scope.mountPoint;
      return explicit && explicit.mountType === 'database' ? explicit : null;
    }
    if (!scope.projectId) return null;
    try {
      const linked = await fetchProjectStores(this.core, scope.projectId);
      const chosen = pickPrimaryProjectStore(linked);
      return chosen && chosen.mountType === 'database' ? chosen : null;
    } catch {
      // v4: silent fallback to legacy mode — one missing link shouldn't take
      // down the browser.
      return null;
    }
  }

  protected goToRoot(): void {
    this.currentFolder.set('/');
  }

  protected goUp(): void {
    this.currentFolder.set(parentFolder(this.currentFolder()));
  }

  protected name(file: PickerFile): string {
    return fileDisplayName(file) || 'file';
  }

  protected typeLabel(file: PickerFile): string {
    return getFileTypeLabel(file.mimeType);
  }

  protected size(file: PickerFile): string {
    return formatBytes(file.size);
  }

  protected errorMessage(): string {
    const err = this.query.error();
    return err instanceof Error ? err.message : 'Failed to load files.';
  }
}
