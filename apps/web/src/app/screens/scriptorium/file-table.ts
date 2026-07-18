import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';

import { CoreClient } from '../../core/core-client';
import { formatBytes } from '../../ui/format-bytes';
import { Icon } from '../../ui/icon';
import { FileDetailRow, isBlobBacked } from './file-detail-row';
import {
  buildMountBlobUrl,
  deleteMountFile,
  describeMountFile,
  listMountBlobs,
  uploadMountFile,
} from './scriptorium.api';
import type { DocumentStoreBlob, DocumentStoreFile } from './scriptorium.api';

type SortField =
  | 'fileName'
  | 'fileType'
  | 'fileSizeBytes'
  | 'conversionStatus'
  | 'chunkCount'
  | 'lastModified';
type SortDir = 'asc' | 'desc';

const BADGE_BY_TYPE: Record<string, string> = {
  markdown: 'qt-badge-success',
  txt: 'qt-badge-disabled',
  json: 'qt-badge-info',
  jsonl: 'qt-badge-info',
  pdf: 'qt-badge-destructive',
  docx: 'qt-badge-info',
  blob: 'qt-badge-disabled',
};

const BADGE_BY_STATUS: Record<string, string> = {
  converted: 'qt-badge-success',
  pending: 'qt-badge-warning',
  failed: 'qt-badge-destructive',
  skipped: 'qt-badge-disabled',
};

const SORT_COLUMNS: [SortField, string][] = [
  ['fileName', 'File'],
  ['fileType', 'Type'],
  ['fileSizeBytes', 'Size'],
  ['conversionStatus', 'Status'],
  ['chunkCount', 'Embeddings'],
  ['lastModified', 'Modified'],
];

/**
 * The unified file table (v4 `app/scriptorium/[id]/components/FileTable.tsx`):
 * native-text documents, pdf/docx extracts, and binary blobs side-by-side. Sort
 * (6 fields) + filter, badges + embedding indicator, expandable blob detail
 * (lazy `mountBlobsList`, de-duped), description edit (`mountFileUpdate`), delete
 * (`mountFileDelete`), and database-mount multipart upload (the item-route PUT
 * leg). Download links use the persisted `/blobs/...` byte route.
 */
@Component({
  selector: 'qt-file-table',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule, Icon, FileDetailRow],
  template: `
    @if (loading()) {
      <div class="flex items-center justify-center py-12">
        <p class="qt-text-secondary">Loading files...</p>
      </div>
    } @else {
      <div>
        <div class="flex flex-wrap items-center gap-4 mb-4">
          <div class="flex items-center gap-6 qt-body">
            <span class="qt-text-secondary">
              <strong class="text-foreground">{{ files().length }}</strong> files
            </span>
            <span class="qt-text-secondary">
              <strong class="text-foreground">{{ totalSizeLabel() }}</strong> total
            </span>
            <span class="qt-text-secondary">
              <strong class="text-foreground">{{ totalChunks() }}</strong> chunks
            </span>
            @if (conversionStats().converted > 0) {
              <span class="qt-text-success text-xs">{{ conversionStats().converted }} converted</span>
            }
            @if (conversionStats().pending > 0) {
              <span class="qt-text-warning text-xs">{{ conversionStats().pending }} pending</span>
            }
            @if (conversionStats().failed > 0) {
              <span class="qt-text-destructive text-xs">{{ conversionStats().failed }} failed</span>
            }
          </div>
          <div class="ml-auto flex items-center gap-2">
            <input
              type="text"
              placeholder="Filter files..."
              [(ngModel)]="filter"
              class="qt-input text-sm py-1 px-3 w-48"
            />
            @if (canUpload()) {
              <label class="qt-button-primary inline-flex items-center gap-1.5 cursor-pointer text-sm">
                <input
                  type="file"
                  multiple
                  class="hidden"
                  [disabled]="uploading()"
                  (change)="onUpload($event)"
                />
                {{ uploading() ? 'Uploading…' : 'Upload' }}
              </label>
            }
          </div>
        </div>

        @if (operationError(); as err) {
          <div class="mb-4 rounded-xl qt-border-destructive/30 qt-bg-destructive/10 border p-3">
            <p class="text-sm qt-text-destructive">{{ err }}</p>
          </div>
        }

        @if (files().length === 0) {
          <div
            class="rounded-2xl border border-dashed qt-border-default/70 qt-bg-card/80 px-8 py-12 text-center qt-shadow-sm"
          >
            <p class="qt-text-secondary">
              {{
                canUpload()
                  ? 'No files yet. Use the Upload button above to add documents, images, or any binary file.'
                  : 'No files indexed yet. Run a scan to discover files.'
              }}
            </p>
          </div>
        } @else {
          <div class="overflow-x-auto rounded-xl border qt-border-default qt-bg-card">
            <table class="w-full text-sm">
              <thead>
                <tr class="border-b qt-border-default">
                  @for (col of columns; track col[0]) {
                    <th
                      (click)="handleSort(col[0])"
                      class="px-4 py-3 text-left qt-text-secondary uppercase tracking-wider cursor-pointer hover:text-foreground select-none"
                    >
                      <span class="inline-flex items-center">
                        {{ col[1] }}
                        @if (sortField() === col[0]) {
                          <qt-icon [name]="sortDir() === 'asc' ? 'arrow-up' : 'arrow-down'" class="w-3 h-3 ml-1" />
                        } @else {
                          <qt-icon name="sort" class="w-3 h-3 ml-1 qt-text-secondary opacity-40" />
                        }
                      </span>
                    </th>
                  }
                  <th class="px-4 py-3"></th>
                </tr>
              </thead>
              <tbody>
                @for (file of filteredAndSorted(); track file.id) {
                  <tr
                    class="border-b qt-border-default/50 hover:qt-bg-muted/50 transition-colors cursor-pointer"
                    (click)="toggleExpand(file)"
                  >
                    <td class="px-4 py-3">
                      <div class="font-medium text-foreground truncate max-w-xs" [title]="file.relativePath">
                        {{ file.fileName }}
                      </div>
                      <div class="text-xs qt-text-secondary truncate max-w-xs" [title]="file.relativePath">
                        {{ file.relativePath }}
                      </div>
                    </td>
                    <td class="px-4 py-3">
                      <span class="inline-flex items-center" [class]="badgeForType(file.fileType)">
                        {{ file.fileType.toUpperCase() }}
                      </span>
                    </td>
                    <td class="px-4 py-3 text-foreground whitespace-nowrap">{{ sizeLabel(file.fileSizeBytes) }}</td>
                    <td class="px-4 py-3">
                      <span
                        class="inline-flex items-center"
                        [class]="badgeForStatus(file.conversionStatus)"
                        [title]="file.conversionError || ''"
                      >
                        {{ statusLabel(file.conversionStatus) }}
                      </span>
                    </td>
                    <td class="px-4 py-3">
                      @if (file.chunkCount === 0) {
                        <span class="text-xs qt-text-secondary">None</span>
                      } @else {
                        <span class="inline-flex items-center gap-1 qt-body-sm">
                          <span class="h-1.5 w-1.5 rounded-full qt-dot-success"></span>
                          {{ file.chunkCount }} chunk{{ file.chunkCount !== 1 ? 's' : '' }}
                        </span>
                      }
                    </td>
                    <td class="px-4 py-3 text-xs qt-text-secondary whitespace-nowrap">
                      {{ modifiedLabel(file.lastModified) }}
                    </td>
                    <td class="px-4 py-3 text-xs qt-text-secondary">
                      {{ expandedId() === file.id ? '▾' : '▸' }}
                    </td>
                  </tr>
                  @if (expandedId() === file.id) {
                    <tr class="border-b qt-border-default/50">
                      <td colspan="7" class="px-4 py-3 qt-bg-muted/30">
                        @if (isBlobBacked(file.fileType) && blobDetails()[file.id] === undefined) {
                          <p class="text-xs qt-text-secondary">Loading…</p>
                        } @else {
                          <qt-file-detail-row
                            [file]="file"
                            [blob]="blobDetails()[file.id] ?? null"
                            [blobUrl]="blobUrlFor(file)"
                            [canUpload]="canUpload()"
                            (delete)="handleBlobDelete(file)"
                            (saveDescription)="handleDescriptionSave(file, $event)"
                            (openWorkbench)="openInWorkbench($event)"
                          />
                        }
                      </td>
                    </tr>
                  }
                }
              </tbody>
            </table>
          </div>
        }
      </div>
    }
  `,
})
export class FileTable {
  private readonly router = inject(Router);

  /**
   * Open a `Tools/*.tool.json` on Pascal's Workbench (v4 `FileTable:105-113`).
   * v4 prefers a workspace tab and falls back to this query-param push; v5 has
   * no workspace tabs (`p4.9j`), so the fallback is the only path.
   */
  protected openInWorkbench(relativePath: string): void {
    void this.router.navigate(['/custom-tools'], {
      queryParams: { mount: this.mountPointId(), path: relativePath },
    });
  }

  private readonly core = inject(CoreClient);

  readonly files = input.required<DocumentStoreFile[]>();
  readonly loading = input(false);
  readonly mountPointId = input.required<string>();
  /** The store's mount type (`DocMountPointDto.mountType` is `string`; only the
   * `=== 'database'` upload gate reads it). */
  readonly mountType = input.required<string>();
  readonly refresh = output<void>();

  protected readonly columns = SORT_COLUMNS;
  protected readonly isBlobBacked = isBlobBacked;

  protected readonly sortField = signal<SortField>('fileName');
  protected readonly sortDir = signal<SortDir>('asc');
  protected readonly filter = signal('');
  protected readonly uploading = signal(false);
  protected readonly operationError = signal<string | null>(null);
  protected readonly expandedId = signal<string | null>(null);
  protected readonly blobDetails = signal<Record<string, DocumentStoreBlob | null>>({});
  private readonly blobFetched = new Set<string>();

  protected readonly canUpload = computed(() => this.mountType() === 'database');

  protected readonly filteredAndSorted = computed(() => {
    const field = this.sortField();
    const dir = this.sortDir();
    const f = this.filter();
    let result = this.files();
    if (f) {
      const lower = f.toLowerCase();
      result = result.filter(
        (file) =>
          file.fileName.toLowerCase().includes(lower) ||
          file.relativePath.toLowerCase().includes(lower) ||
          file.fileType.toLowerCase().includes(lower),
      );
    }
    return [...result].sort((a, b) => {
      const aVal = a[field];
      const bVal = b[field];
      const cmp =
        typeof aVal === 'string'
          ? aVal.localeCompare(bVal as string)
          : (aVal as number) - (bVal as number);
      return dir === 'asc' ? cmp : -cmp;
    });
  });

  protected readonly totalSizeLabel = computed(() =>
    formatBytes(this.files().reduce((sum, f) => sum + f.fileSizeBytes, 0)),
  );
  protected readonly totalChunks = computed(() =>
    this.files().reduce((sum, f) => sum + f.chunkCount, 0),
  );
  protected readonly conversionStats = computed(() => ({
    converted: this.files().filter((f) => f.conversionStatus === 'converted').length,
    pending: this.files().filter((f) => f.conversionStatus === 'pending').length,
    failed: this.files().filter((f) => f.conversionStatus === 'failed').length,
    skipped: this.files().filter((f) => f.conversionStatus === 'skipped').length,
  }));

  protected handleSort(field: SortField): void {
    if (this.sortField() === field) {
      this.sortDir.update((d) => (d === 'asc' ? 'desc' : 'asc'));
    } else {
      this.sortField.set(field);
      this.sortDir.set('asc');
    }
  }

  protected badgeForType(type: string): string {
    return `inline-flex items-center ${BADGE_BY_TYPE[type] ?? 'qt-badge-disabled'}`;
  }
  protected badgeForStatus(status: string): string {
    return `inline-flex items-center ${BADGE_BY_STATUS[status] ?? 'qt-badge-disabled'}`;
  }
  protected statusLabel(status: string): string {
    return status.charAt(0).toUpperCase() + status.slice(1);
  }
  protected sizeLabel(bytes: number): string {
    return formatBytes(bytes);
  }
  protected modifiedLabel(lastModified: string): string {
    return new Date(lastModified).toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  }
  protected blobUrlFor(file: DocumentStoreFile): string | null {
    return isBlobBacked(file.fileType)
      ? buildMountBlobUrl(this.mountPointId(), file.relativePath)
      : null;
  }

  protected async onUpload(event: Event): Promise<void> {
    const input = event.target as HTMLInputElement;
    const fileList = input.files;
    if (!fileList || fileList.length === 0) {
      return;
    }
    if (!this.canUpload()) {
      this.operationError.set('Uploads are only supported on database-backed stores.');
      return;
    }
    this.uploading.set(true);
    this.operationError.set(null);
    try {
      for (const file of Array.from(fileList)) {
        // Default layout: images into images/, other files at the root.
        const isImage = (file.type || '').startsWith('image/');
        const defaultPath = isImage ? `images/${file.name}` : file.name;
        await uploadMountFile(this.mountPointId(), defaultPath, file);
      }
      this.refresh.emit();
    } catch (err) {
      this.operationError.set(err instanceof Error ? err.message : String(err));
    } finally {
      this.uploading.set(false);
      input.value = '';
    }
  }

  protected async handleBlobDelete(file: DocumentStoreFile): Promise<void> {
    const ok = window.confirm(
      `Delete ${file.relativePath}? Markdown references to this blob will 404 until re-uploaded.`,
    );
    if (!ok) {
      return;
    }
    try {
      await deleteMountFile(this.core, this.mountPointId(), file.relativePath);
      this.refresh.emit();
    } catch (err) {
      this.operationError.set(err instanceof Error ? err.message : String(err));
    }
  }

  protected async handleDescriptionSave(file: DocumentStoreFile, description: string): Promise<void> {
    try {
      await describeMountFile(this.core, this.mountPointId(), file.relativePath, description);
      // The update returns metadata, not the blob row; patch the cached
      // description optimistically (a later expand re-fetches the rest).
      this.blobDetails.update((prev) => {
        const existing = prev[file.id];
        return existing ? { ...prev, [file.id]: { ...existing, description } } : prev;
      });
    } catch (err) {
      this.operationError.set(err instanceof Error ? err.message : String(err));
    }
  }

  protected toggleExpand(file: DocumentStoreFile): void {
    const next = this.expandedId() === file.id ? null : file.id;
    this.expandedId.set(next);
    if (next && isBlobBacked(file.fileType)) {
      void this.loadBlobDetail(file);
    }
  }

  private async loadBlobDetail(file: DocumentStoreFile): Promise<void> {
    // Mark before the first await so a concurrent call bails — no dup fetch.
    if (this.blobFetched.has(file.id)) {
      return;
    }
    this.blobFetched.add(file.id);
    try {
      const blobs = await listMountBlobs(this.core, this.mountPointId());
      const blob = blobs.find((b) => b.relativePath === file.relativePath) ?? null;
      this.blobDetails.update((prev) => ({ ...prev, [file.id]: blob }));
    } catch {
      this.blobDetails.update((prev) => ({ ...prev, [file.id]: null }));
    }
  }
}
