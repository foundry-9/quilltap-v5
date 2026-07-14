import { NgClass } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { FileEntry } from '../../core/core-contract';
import { formatBytes } from '../../ui/format-bytes';
import { FileThumbnail } from './file-thumbnail';
import { fileDisplayName, type FolderInfo } from './file-model';

/**
 * The grid view of the FileBrowser (v4 `components/files/FileBrowserGrid.tsx`).
 * A responsive card grid: an up-one-level card (when not at root), then the
 * subfolders, then the files (thumbnail + name + size, with hover Move/Delete
 * actions), and a whole-grid empty state.
 */
@Component({
  selector: 'qt-file-grid',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FileThumbnail, NgClass],
  template: `
    <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
      @if (currentFolder() !== '/') {
        <button
          type="button"
          class="flex flex-col items-center gap-2 p-3 rounded-lg hover:qt-bg-surface-alt transition-colors group"
          (click)="goUp.emit()"
        >
          <div
            class="flex items-center justify-center qt-bg-muted rounded-lg text-3xl"
            [style.width.px]="thumbnailSize()"
            [style.height.px]="thumbnailSize()"
          >
            {{ folderIcon }}
          </div>
          <span class="qt-text-small qt-text-secondary">..</span>
        </button>
      }

      @for (folder of folders(); track folder.path) {
        <button
          type="button"
          class="flex flex-col items-center gap-2 p-3 rounded-lg hover:qt-bg-surface-alt transition-colors group"
          (click)="folderClick.emit(folder.path)"
        >
          <div
            class="flex items-center justify-center qt-bg-muted rounded-lg text-3xl"
            [style.width.px]="thumbnailSize()"
            [style.height.px]="thumbnailSize()"
          >
            {{ folderIcon }}
          </div>
          <div class="w-full text-center">
            <span class="font-medium text-sm break-all line-clamp-2" [title]="folder.name">{{
              folder.name
            }}</span>
            <span class="qt-text-xs qt-text-secondary block"
              >{{ folder.fileCount }} file{{ folder.fileCount !== 1 ? 's' : '' }}</span
            >
          </div>
        </button>
      }

      @for (file of files(); track file.id) {
        <div
          class="flex flex-col items-center gap-2 p-3 rounded-lg hover:qt-bg-surface-alt transition-colors group relative"
          [ngClass]="{ 'ring-1 qt-border-warning/50': file.fileStatus === 'orphaned' }"
        >
          <button
            type="button"
            class="flex flex-col items-center gap-2 w-full"
            (click)="fileClick.emit(file)"
          >
            <qt-file-thumbnail
              [fileId]="file.id"
              [mimeType]="file.mimeType"
              [alt]="name(file) || 'File'"
              [size]="thumbnailSize()"
              className="rounded-lg flex-shrink-0"
            />
            <div class="w-full text-center">
              <span class="font-medium text-sm break-all line-clamp-2" [title]="name(file)">{{
                name(file)
              }}</span>
              <span class="qt-text-xs qt-text-secondary block">{{ size(file) }}</span>
              @if (file.fileStatus === 'orphaned') {
                <span
                  class="qt-text-xs qt-text-warning block"
                  title="Untracked file found on disk"
                  >untracked</span
                >
              }
            </div>
          </button>

          <div
            class="absolute top-2 right-2 flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity"
          >
            @if (canMove()) {
              <button
                type="button"
                class="qt-button qt-button-secondary p-1.5 text-xs"
                title="Move to Project"
                (click)="moveToProject.emit(file)"
              >
                {{ folderIcon }}
              </button>
            }
            <button
              type="button"
              class="qt-button qt-button-secondary p-1.5 text-xs"
              title="Delete file"
              (click)="deleteFile.emit(file.id)"
            >
              {{ trashIcon }}
            </button>
          </div>
        </div>
      }

      @if (isEmpty()) {
        <div
          class="col-span-full flex flex-col items-center justify-center py-12 text-center qt-text-secondary"
        >
          <div class="text-5xl mb-3">{{ openFolderIcon }}</div>
          <p class="text-lg">No files yet</p>
          <p class="qt-text-small">Upload files to get started</p>
        </div>
      }
    </div>
  `,
})
export class FileGrid {
  readonly files = input.required<FileEntry[]>();
  readonly folders = input.required<FolderInfo[]>();
  readonly currentFolder = input.required<string>();
  readonly thumbnailSize = input(120);
  /** Whether the Move-to-Project affordance is offered (general files only). */
  readonly canMove = input(true);

  readonly fileClick = output<FileEntry>();
  readonly folderClick = output<string>();
  readonly goUp = output<void>();
  readonly deleteFile = output<string>();
  readonly moveToProject = output<FileEntry>();

  protected readonly folderIcon = '\u{1F4C1}';
  protected readonly trashIcon = '\u{1F5D1}️';
  protected readonly openFolderIcon = '\u{1F4C2}';

  protected readonly isEmpty = computed(
    () => this.files().length === 0 && this.folders().length === 0 && this.currentFolder() === '/',
  );

  protected name(file: FileEntry): string {
    return fileDisplayName(file);
  }

  protected size(file: FileEntry): string {
    return formatBytes(file.size);
  }
}
