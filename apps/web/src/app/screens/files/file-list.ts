import { NgClass, NgTemplateOutlet } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { FileEntry } from '../../core/core-contract';
import { formatBytes } from '../../ui/format-bytes';
import {
  fileDisplayName,
  formatFileDate,
  getFileIcon,
  getFileTypeLabel,
  type FolderInfo,
  type SortField,
  type SortState,
} from './file-model';

interface SortColumn {
  field: SortField;
  label: string;
}

/**
 * The list (table) view of the FileBrowser (v4
 * `components/files/FileBrowserList.tsx`). Columns: (icon) | Name | Associations
 * | Type | Size | Date | (actions), with sortable Name/Type/Size/Date headers.
 * An up-one-level row (when not at root), then folder rows (row-click navigates;
 * Type "Folder", Size/Date "--"), then file rows (name-button opens; hover
 * Move/Delete), then a full-width empty state.
 */
@Component({
  selector: 'qt-file-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgClass, NgTemplateOutlet],
  template: `
    <div class="w-full overflow-x-auto">
      <table class="w-full border-collapse">
        <thead>
          <tr class="border-b qt-border-default">
            <th class="text-left p-2 w-12"></th>
            <th class="text-left p-2">
              <ng-container [ngTemplateOutlet]="sortHeader" [ngTemplateOutletContext]="{ col: cols[0] }" />
            </th>
            <th class="text-left p-2 hidden md:table-cell">
              <span class="font-medium">Associations</span>
            </th>
            <th class="text-left p-2 hidden sm:table-cell w-28">
              <ng-container [ngTemplateOutlet]="sortHeader" [ngTemplateOutletContext]="{ col: cols[1] }" />
            </th>
            <th class="text-left p-2 hidden lg:table-cell w-24">
              <ng-container [ngTemplateOutlet]="sortHeader" [ngTemplateOutletContext]="{ col: cols[2] }" />
            </th>
            <th class="text-left p-2 hidden sm:table-cell w-28">
              <ng-container [ngTemplateOutlet]="sortHeader" [ngTemplateOutletContext]="{ col: cols[3] }" />
            </th>
            <th class="w-12"></th>
          </tr>
        </thead>
        <tbody>
          @if (currentFolder() !== '/') {
            <tr
              class="border-b qt-border-default hover:qt-bg-surface-alt cursor-pointer transition-colors"
              (click)="goUp.emit()"
            >
              <td class="p-2 text-xl">{{ folderIcon }}</td>
              <td class="p-2 font-medium qt-text-secondary">..</td>
              <td class="p-2 hidden md:table-cell"></td>
              <td class="p-2 hidden sm:table-cell"></td>
              <td class="p-2 hidden lg:table-cell"></td>
              <td class="p-2 hidden sm:table-cell"></td>
              <td class="p-2"></td>
            </tr>
          }

          @for (folder of folders(); track folder.path) {
            <tr
              class="border-b qt-border-default hover:qt-bg-surface-alt cursor-pointer transition-colors"
              (click)="folderClick.emit(folder.path)"
            >
              <td class="p-2 text-xl">{{ folderIcon }}</td>
              <td class="p-2"><span class="font-medium">{{ folder.name }}</span></td>
              <td class="p-2 hidden md:table-cell qt-text-secondary">
                {{ folder.fileCount }} file{{ folder.fileCount !== 1 ? 's' : '' }}
              </td>
              <td class="p-2 hidden sm:table-cell qt-text-secondary">Folder</td>
              <td class="p-2 hidden lg:table-cell qt-text-secondary">--</td>
              <td class="p-2 hidden sm:table-cell qt-text-secondary">--</td>
              <td class="p-2"></td>
            </tr>
          }

          @for (file of files(); track file.id) {
            <tr
              class="border-b qt-border-default hover:qt-bg-surface-alt transition-colors group"
              [ngClass]="{ 'qt-bg-warning/5': file.fileStatus === 'orphaned' }"
            >
              <td class="p-2 text-xl">{{ icon(file) }}</td>
              <td class="p-2">
                <div class="flex items-center gap-2">
                  <button
                    type="button"
                    class="text-left font-medium hover:text-primary transition-colors truncate max-w-xs block"
                    [title]="name(file)"
                    (click)="fileClick.emit(file)"
                  >
                    {{ name(file) }}
                  </button>
                  @if (file.fileStatus === 'orphaned') {
                    <span
                      class="qt-text-xs qt-text-warning px-1.5 py-0.5 rounded qt-bg-warning/10 whitespace-nowrap"
                      title="Untracked file found on disk"
                      >untracked</span
                    >
                  }
                </div>
              </td>
              <td class="p-2 hidden md:table-cell">
                @if (file.linkedTo && file.linkedTo.length > 0) {
                  <span class="qt-text-xs qt-text-secondary"
                    >{{ file.linkedTo.length }} link{{ file.linkedTo.length !== 1 ? 's' : '' }}</span
                  >
                } @else {
                  <span class="qt-text-xs qt-text-secondary">--</span>
                }
              </td>
              <td class="p-2 hidden sm:table-cell qt-text-secondary qt-text-small">{{ typeLabel(file) }}</td>
              <td class="p-2 hidden lg:table-cell qt-text-secondary qt-text-small">{{ size(file) }}</td>
              <td class="p-2 hidden sm:table-cell qt-text-secondary qt-text-small">{{ date(file) }}</td>
              <td class="p-2">
                <div class="flex items-center gap-1">
                  @if (canMove()) {
                    <button
                      type="button"
                      class="qt-button qt-button-secondary p-1 opacity-0 group-hover:opacity-100 transition-opacity text-xs"
                      title="Move to Project"
                      (click)="moveToProject.emit(file)"
                    >
                      {{ folderIcon }}
                    </button>
                  }
                  <button
                    type="button"
                    class="qt-button qt-button-secondary p-1 opacity-0 group-hover:opacity-100 transition-opacity text-xs"
                    title="Delete file"
                    (click)="deleteFile.emit(file.id)"
                  >
                    {{ trashIcon }}
                  </button>
                </div>
              </td>
            </tr>
          }

          @if (isEmpty()) {
            <tr>
              <td colspan="7" class="p-8 text-center qt-text-secondary">
                <div class="text-5xl mb-3">{{ openFolderIcon }}</div>
                <p class="text-lg">No files yet</p>
                <p class="qt-text-small">Upload files to get started</p>
              </td>
            </tr>
          }
        </tbody>
      </table>
    </div>

    <ng-template #sortHeader let-col="col">
      <button
        type="button"
        class="flex items-center gap-1 text-left font-medium hover:text-primary transition-colors"
        (click)="onSort(col.field)"
      >
        <span>{{ col.label }}</span>
        <span
          class="text-xs transition-opacity"
          [class.opacity-100]="sort().field === col.field"
          [class.opacity-40]="sort().field !== col.field"
          >{{ sortIcon(col.field) }}</span
        >
      </button>
    </ng-template>
  `,
})
export class FileList {
  readonly files = input.required<FileEntry[]>();
  readonly folders = input.required<FolderInfo[]>();
  readonly currentFolder = input.required<string>();
  readonly sort = input.required<SortState>();
  readonly canMove = input(true);

  readonly fileClick = output<FileEntry>();
  readonly folderClick = output<string>();
  readonly goUp = output<void>();
  readonly deleteFile = output<string>();
  readonly moveToProject = output<FileEntry>();
  readonly sortChange = output<SortState>();

  protected readonly folderIcon = '\u{1F4C1}';
  protected readonly trashIcon = '\u{1F5D1}️';
  protected readonly openFolderIcon = '\u{1F4C2}';

  protected readonly cols: SortColumn[] = [
    { field: 'name', label: 'Name' },
    { field: 'type', label: 'Type' },
    { field: 'size', label: 'Size' },
    { field: 'date', label: 'Date' },
  ];

  protected readonly isEmpty = computed(
    () => this.files().length === 0 && this.folders().length === 0 && this.currentFolder() === '/',
  );

  /** Toggle direction on the active field, else sort the new field ascending (v4). */
  protected onSort(field: SortField): void {
    const sort = this.sort();
    if (sort.field === field) {
      this.sortChange.emit({ field, direction: sort.direction === 'asc' ? 'desc' : 'asc' });
    } else {
      this.sortChange.emit({ field, direction: 'asc' });
    }
  }

  protected sortIcon(field: SortField): string {
    const sort = this.sort();
    if (sort.field !== field) return '▶';
    return sort.direction === 'asc' ? '▲' : '▼';
  }

  protected name(file: FileEntry): string {
    return fileDisplayName(file);
  }

  protected icon(file: FileEntry): string {
    return getFileIcon(file.mimeType);
  }

  protected typeLabel(file: FileEntry): string {
    return getFileTypeLabel(file.mimeType);
  }

  protected size(file: FileEntry): string {
    return formatBytes(file.size);
  }

  protected date(file: FileEntry): string {
    return formatFileDate(file.createdAt);
  }
}
