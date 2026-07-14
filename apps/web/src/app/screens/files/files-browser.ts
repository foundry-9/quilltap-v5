import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type { FileAssociations, FileEntry } from '../../core/core-contract';
import { ErrorAlert } from '../../ui/error-alert';
import { LoadingState } from '../../ui/loading-state';
import { CreateFolderDialog } from './create-folder-dialog';
import { FileDeleteConfirmation } from './file-delete-confirmation';
import { FileGrid } from './file-grid';
import { FileList } from './file-list';
import { FilePreviewModal } from './file-preview-modal';
import {
  breadcrumbSegments,
  deriveSubfolders,
  filesInFolder,
  parentFolder,
  type DbFolder,
  type SortState,
} from './file-model';
import { MoveToProjectDialog } from './move-to-project-dialog';
import { OrphanCleanupDialog, type OrphanCleanupStats } from './orphan-cleanup-dialog';

interface FilesData {
  files: FileEntry[];
  folders: DbFolder[];
}

/** A pending associations-delete (the FILE_HAS_ASSOCIATIONS envelope). */
interface DeleteConfirmation {
  fileId: string;
  filename: string;
  associations: FileAssociations;
}

/**
 * The general Files page (v4 `app/files/FilesView.tsx` → `FileBrowser
 * projectId={null}`, LEGACY mode only). Folder navigation (breadcrumb + go-up +
 * subfolders derived from both DB rows and file-path prefixes), grid/list views
 * with client-side sort, the preview lightbox, and the move / delete / folder /
 * orphan-cleanup dialogs. There is NO upload affordance here (v4 parity: general
 * files arrive via chat-attachment promote, not this page).
 *
 * The mount-mode FileBrowser is the already-ported Scriptorium `qt-file-manager`
 * (`app/files/**`) — untouched this round.
 */
@Component({
  selector: 'qt-files-browser',
  host: { class: 'block h-full' },
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    LoadingState,
    ErrorAlert,
    FileGrid,
    FileList,
    FilePreviewModal,
    CreateFolderDialog,
    MoveToProjectDialog,
    FileDeleteConfirmation,
    OrphanCleanupDialog,
  ],
  template: `
    <div class="qt-page-container flex flex-col h-full p-6">
      <div class="flex items-center justify-between mb-4">
        <h2 class="qt-heading-4">General Files</h2>
        <div class="flex items-center gap-2">
          <button
            type="button"
            class="qt-button qt-button-secondary p-2"
            title="New Folder"
            (click)="showCreateFolder.set(true)"
          >
            {{ folderIcon }}+
          </button>
          <button
            type="button"
            class="qt-button qt-button-secondary p-2"
            [title]="viewMode() === 'list' ? 'Grid view' : 'List view'"
            (click)="toggleView()"
          >
            {{ viewMode() === 'list' ? '▦' : '☰' }}
          </button>
          @if (orphanedCount() > 0) {
            <button
              type="button"
              class="qt-button qt-button-secondary p-2 flex items-center gap-1"
              [title]="cleanupTitle()"
              [disabled]="isCleaningUp() || query.isPending()"
              (click)="onCleanupClick()"
            >
              {{ isCleaningUp() ? '⏳' : broomIcon }}
              <span class="text-xs qt-text-warning">{{ orphanedCount() }}</span>
            </button>
          }
          <button
            type="button"
            class="qt-button qt-button-secondary p-2"
            title="Sync filesystem — scan disk for new or removed files"
            [disabled]="isSyncing() || query.isPending()"
            (click)="onSync()"
          >
            {{ isSyncing() ? '⏳' : refreshIcon }}
          </button>
          <button
            type="button"
            class="qt-button qt-button-secondary p-2"
            title="Refresh"
            [disabled]="query.isPending()"
            (click)="query.refetch()"
          >
            ↻
          </button>
        </div>
      </div>

      <div class="mb-4">
        <div class="flex items-center gap-2 text-sm flex-wrap">
          <button type="button" class="hover:text-primary transition-colors" (click)="goToRoot()">
            Root
          </button>
          @for (crumb of breadcrumbs(); track crumb.path) {
            <span class="flex items-center gap-2">
              <span class="qt-text-secondary">/</span>
              <button
                type="button"
                class="hover:text-primary transition-colors"
                (click)="setFolder(crumb.path)"
              >
                {{ crumb.label }}
              </button>
            </span>
          }
        </div>
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
        <div class="flex-1 overflow-y-auto">
          @if (viewMode() === 'grid') {
            <qt-file-grid
              [files]="visibleFiles()"
              [folders]="subfolders()"
              [currentFolder]="currentFolder()"
              (fileClick)="selectedFile.set($event)"
              (folderClick)="setFolder($event)"
              (goUp)="goUp()"
              (deleteFile)="onDeleteFile($event)"
              (moveToProject)="onMoveToProject($event)"
            />
          } @else {
            <qt-file-list
              [files]="visibleFiles()"
              [folders]="subfolders()"
              [currentFolder]="currentFolder()"
              [sort]="sort()"
              (fileClick)="selectedFile.set($event)"
              (folderClick)="setFolder($event)"
              (goUp)="goUp()"
              (deleteFile)="onDeleteFile($event)"
              (moveToProject)="onMoveToProject($event)"
              (sortChange)="sort.set($event)"
            />
          }
        </div>
      }

      <div class="mt-4 pt-2 border-t qt-border-default">
        <span class="qt-text-xs qt-text-secondary">
          {{ allFiles().length }} file{{ allFiles().length !== 1 ? 's' : '' }} total
          @if (currentFolder() !== '/') {
            • {{ visibleFiles().length }} in current folder
          }
        </span>
      </div>
    </div>

    @if (selectedFile(); as file) {
      <qt-file-preview-modal
        [file]="file"
        [files]="allFiles()"
        (close)="selectedFile.set(null)"
        (navigate)="onPreviewNavigate($event)"
        (delete)="onDeleteFile($event.id)"
        (moveToProject)="onMoveToProject($event)"
      />
    }

    @if (showCreateFolder()) {
      <qt-create-folder-dialog
        [currentFolder]="currentFolder()"
        (close)="showCreateFolder.set(false)"
        (success)="onFolderCreated($event)"
      />
    }

    @if (moveModalFile(); as target) {
      <qt-move-to-project-dialog
        [fileId]="target.id"
        [fileName]="displayName(target)"
        [currentProjectId]="null"
        (close)="moveModalFile.set(null)"
        (success)="onMoveSuccess()"
      />
    }

    @if (deleteConfirmation(); as confirmation) {
      <qt-file-delete-confirmation
        [filename]="confirmation.filename"
        [associations]="confirmation.associations"
        [isDeleting]="isDeleting()"
        (confirm)="onConfirmDeleteWithDissociation()"
        (cancel)="deleteConfirmation.set(null)"
      />
    }

    @if (cleanupStats(); as stats) {
      <qt-orphan-cleanup-dialog
        [stats]="stats"
        [isProcessing]="isCleaningUp()"
        (move)="onCleanupMove()"
        (delete)="onCleanupDelete()"
        (cancel)="cleanupStats.set(null)"
      />
    }
  `,
})
export class FilesBrowser {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  protected readonly folderIcon = '\u{1F4C1}';
  protected readonly broomIcon = '\u{1F9F9}';
  protected readonly refreshIcon = '\u{1F504}';

  protected readonly currentFolder = signal('/');
  protected readonly viewMode = signal<'list' | 'grid'>('grid');
  protected readonly sort = signal<SortState>({ field: 'name', direction: 'asc' });
  protected readonly selectedFile = signal<FileEntry | null>(null);
  protected readonly showCreateFolder = signal(false);
  protected readonly moveModalFile = signal<FileEntry | null>(null);
  protected readonly deleteConfirmation = signal<DeleteConfirmation | null>(null);
  protected readonly isDeleting = signal(false);
  protected readonly isSyncing = signal(false);
  protected readonly isCleaningUp = signal(false);
  protected readonly cleanupStats = signal<OrphanCleanupStats | null>(null);

  protected readonly query = injectQuery(() => ({
    queryKey: ['files', 'general'],
    queryFn: async (): Promise<FilesData> => {
      const [files, folders] = await Promise.all([
        this.core.filesList({ filter: 'general' }),
        this.core.filesFoldersList(),
      ]);
      return { files, folders: folders as unknown as DbFolder[] };
    },
  }));

  protected readonly allFiles = computed(() => this.query.data()?.files ?? []);
  private readonly dbFolders = computed(() => this.query.data()?.folders ?? []);

  protected readonly visibleFiles = computed(() =>
    filesInFolder(this.allFiles(), this.currentFolder(), this.sort()),
  );
  protected readonly subfolders = computed(() =>
    deriveSubfolders(this.allFiles(), this.dbFolders(), this.currentFolder()),
  );
  protected readonly breadcrumbs = computed(() => breadcrumbSegments(this.currentFolder()));

  protected readonly orphanedCount = computed(
    () => this.allFiles().filter((f) => f.fileStatus === 'orphaned').length,
  );
  protected readonly cleanupTitle = computed(() => {
    const n = this.orphanedCount();
    return `${n} untracked file${n !== 1 ? 's' : ''} — click to clean up`;
  });

  constructor() {
    // Fire the fire-and-forget thumbnail batch for the first 100 image ids once
    // the list loads (v4's post-load `generate-thumbnails` effect).
    effect(() => {
      const files = this.query.data()?.files;
      if (!files) return;
      const imageIds = files.filter((f) => f.mimeType?.startsWith('image/')).map((f) => f.id);
      void this.core.filesGenerateThumbnails(imageIds);
    });
  }

  protected displayName(file: FileEntry): string {
    return file.originalFilename || file.filename || 'file';
  }

  protected toggleView(): void {
    this.viewMode.update((v) => (v === 'list' ? 'grid' : 'list'));
  }

  protected goToRoot(): void {
    this.currentFolder.set('/');
  }

  protected setFolder(folder: string): void {
    this.currentFolder.set(folder);
  }

  protected goUp(): void {
    this.currentFolder.set(parentFolder(this.currentFolder()));
  }

  protected onPreviewNavigate(file: FileEntry): void {
    this.selectedFile.set(file);
    if (file.folderPath && file.folderPath !== this.currentFolder()) {
      this.currentFolder.set(file.folderPath);
    }
  }

  protected onMoveToProject(file: FileEntry): void {
    this.moveModalFile.set(file);
  }

  protected async onMoveSuccess(): Promise<void> {
    const moved = this.moveModalFile();
    this.moveModalFile.set(null);
    if (moved && this.selectedFile()?.id === moved.id) this.selectedFile.set(null);
    await this.queryClient.invalidateQueries({ queryKey: ['files', 'general'] });
  }

  protected onFolderCreated(folderPath: string): void {
    void this.queryClient.invalidateQueries({ queryKey: ['files', 'general'] });
    this.currentFolder.set(folderPath);
  }

  // ---------------------------------------------------------------------------
  // Delete (the two-stage associations flow, v4 handleDeleteFile)
  // ---------------------------------------------------------------------------

  protected async onDeleteFile(fileId: string): Promise<void> {
    if (
      typeof window !== 'undefined' &&
      !window.confirm('Are you sure you want to delete this file? This cannot be undone.')
    ) {
      return;
    }
    const resp = await this.core.dispatch({ type: 'fileDelete', fileId });
    if (resp.type === 'error') {
      const assoc = extractAssociations(resp.data);
      if (assoc) {
        const file = this.allFiles().find((f) => f.id === fileId);
        this.deleteConfirmation.set({
          fileId,
          filename: file ? this.displayName(file) : 'file',
          associations: assoc,
        });
        return;
      }
      window.alert(resp.data.message || 'Failed to delete file');
      return;
    }
    if (this.selectedFile()?.id === fileId) this.selectedFile.set(null);
    await this.queryClient.invalidateQueries({ queryKey: ['files', 'general'] });
  }

  protected async onConfirmDeleteWithDissociation(): Promise<void> {
    const confirmation = this.deleteConfirmation();
    if (!confirmation) return;
    this.isDeleting.set(true);
    try {
      await this.core.dispatchData({
        type: 'fileDelete',
        fileId: confirmation.fileId,
        dissociate: true,
      });
      if (this.selectedFile()?.id === confirmation.fileId) this.selectedFile.set(null);
      this.deleteConfirmation.set(null);
      await this.queryClient.invalidateQueries({ queryKey: ['files', 'general'] });
    } catch (err) {
      window.alert(err instanceof Error ? err.message : 'Failed to delete file');
    } finally {
      this.isDeleting.set(false);
    }
  }

  // ---------------------------------------------------------------------------
  // Sync + orphan cleanup (tier 2)
  // ---------------------------------------------------------------------------

  /** v4 handleSync — surfaces lane A's loud `filesSync` refusal faithfully. */
  protected async onSync(): Promise<void> {
    this.isSyncing.set(true);
    try {
      const resp = await this.core.dispatch({ type: 'filesSync' });
      if (resp.type === 'error') {
        window.alert(resp.data.message || 'Failed to sync filesystem');
        return;
      }
      await this.queryClient.invalidateQueries({ queryKey: ['files', 'general'] });
    } finally {
      this.isSyncing.set(false);
    }
  }

  /** Dry-run first, ALWAYS — the report precedes the move/delete choice (v4). */
  protected async onCleanupClick(): Promise<void> {
    this.isCleaningUp.set(true);
    try {
      const data = await this.core.dispatchData({ type: 'filesCleanupOrphans', dryRun: true });
      this.cleanupStats.set({
        orphanedCount: (data['orphanedCount'] as number) ?? 0,
        rescuedCount: (data['rescuedCount'] as number) ?? 0,
        duplicateCount: (data['duplicateCount'] as number) ?? 0,
        uniqueCount: (data['uniqueCount'] as number) ?? 0,
        totalSize: (data['totalSize'] as number) ?? 0,
        uniqueSize: (data['uniqueSize'] as number) ?? 0,
      });
    } catch (err) {
      window.alert(err instanceof Error ? err.message : 'Failed to analyze orphaned files');
    } finally {
      this.isCleaningUp.set(false);
    }
  }

  protected async onCleanupMove(): Promise<void> {
    await this.runCleanup('move');
  }

  protected async onCleanupDelete(): Promise<void> {
    await this.runCleanup('delete');
  }

  private async runCleanup(mode: 'move' | 'delete'): Promise<void> {
    this.isCleaningUp.set(true);
    try {
      await this.core.dispatchData({ type: 'filesCleanupOrphans', mode, dryRun: false });
      this.cleanupStats.set(null);
      await this.queryClient.invalidateQueries({ queryKey: ['files', 'general'] });
    } catch (err) {
      window.alert(err instanceof Error ? err.message : 'Failed to clean up orphaned files');
    } finally {
      this.isCleaningUp.set(false);
    }
  }

  protected errorMessage(): string {
    const err = this.query.error();
    return err instanceof Error ? err.message : 'Failed to load files.';
  }
}

/**
 * Pull the associations envelope off a failed `fileDelete` error, tolerating
 * whichever shape lane A pins (top-level or nested under `details`, mirroring
 * v4's `data.details.associations`). Returns `null` when it isn't the
 * FILE_HAS_ASSOCIATIONS case (a plain failure).
 */
function extractAssociations(error: unknown): FileAssociations | null {
  const data = (error ?? {}) as Record<string, unknown>;
  const candidate =
    (data['associations'] as FileAssociations | undefined) ??
    ((data['details'] as { associations?: FileAssociations } | undefined)?.associations);
  if (candidate && Array.isArray(candidate.characters) && Array.isArray(candidate.messages)) {
    return candidate;
  }
  return null;
}
