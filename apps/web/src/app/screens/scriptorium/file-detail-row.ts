import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  OnInit,
  output,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';

import { triggerBlobDownload } from '../../core/download-utils';
import { ToastService } from '../../ui/toast.service';
import type { DocumentStoreBlob, DocumentStoreFile } from './scriptorium.api';

/** File types backed by the blob store (pdf/docx/generic binary). */
export function isBlobBacked(fileType: string): boolean {
  return fileType === 'blob' || fileType === 'pdf' || fileType === 'docx';
}

/**
 * The expanded detail for one file row (v4 `FileTable.tsx` `FileDetailRow`):
 * blob preview + metadata, a searchable description editor (Save appears when
 * dirty), Open-bytes / Copy-link, and (database mounts) Delete. Native-text
 * documents show a hint instead.
 */
@Component({
  selector: 'qt-file-detail-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule],
  template: `
    <div class="flex flex-wrap gap-4 text-xs qt-text-secondary">
      <!--
        Root-level Tools/*.tool.json rows get an "open in Pascal's Workbench"
        action (v4 FileTable.tsx:477-481). Only the LEGACY file table carries
        it — verified at P4.6bb that v4's SVAR manager has none, so
        qt-file-manager gets none either.
      -->
      @if (isCustomToolDefinition()) {
        <div class="w-full">
          <button type="button" (click)="openInWorkbench()" class="qt-button-secondary text-xs">
            Open in Pascal&apos;s Workbench
          </button>
        </div>
      }
      @if (isBlobBackedFile() && blob(); as b) {
        <div class="flex items-start gap-3 min-w-[16rem]">
          @if (isImage() && blobUrl()) {
            <img
              [src]="blobUrl()"
              [alt]="b.description || b.originalFileName"
              class="w-24 h-24 object-cover rounded qt-border-default border"
            />
          } @else {
            <div
              class="w-24 h-24 flex items-center justify-center rounded qt-bg-card qt-border-default border text-center px-1"
            >
              {{ b.storedMimeType }}
            </div>
          }
          <div class="flex flex-col gap-1">
            <div>
              <strong class="text-foreground">MIME:</strong> {{ b.storedMimeType }}
              @if (b.originalMimeType !== b.storedMimeType) {
                (from {{ b.originalMimeType }})
              }
            </div>
            <div><strong class="text-foreground">Original name:</strong> {{ b.originalFileName }}</div>
            <div>
              <strong class="text-foreground">Extraction:</strong> {{ b.extractionStatus }}
              @if (b.extractionError) {
                — {{ b.extractionError }}
              }
            </div>
          </div>
        </div>

        <div class="flex flex-col gap-2 flex-1 min-w-[18rem]">
          <textarea
            class="qt-input text-xs"
            [(ngModel)]="description"
            placeholder="Description / transcript (searchable)"
            rows="3"
          ></textarea>
          <div class="flex flex-wrap gap-2">
            @if (blobUrl()) {
              <a [href]="blobUrl()" target="_blank" rel="noopener noreferrer" class="qt-button-secondary text-xs">
                Open bytes
              </a>
            }
            @if (blobUrl()) {
              <button type="button" (click)="download()" class="qt-button-secondary text-xs">
                Download
              </button>
            }
            <button type="button" (click)="copyMarkdown()" class="qt-button-secondary text-xs">
              Copy link
            </button>
            @if (dirty()) {
              <button type="button" (click)="save()" class="qt-button-primary text-xs">Save</button>
            }
            @if (canUpload()) {
              <button type="button" (click)="delete.emit()" class="qt-button-destructive text-xs ml-auto">
                Delete
              </button>
            }
          </div>
        </div>
      }
      @if (!isBlobBackedFile()) {
        <div>
          Native text document. Use the LLM's doc_read_file / doc_write_file tools to edit, or run a
          scan to refresh embeddings.
        </div>
      }
    </div>
  `,
})
export class FileDetailRow implements OnInit {
  readonly file = input.required<DocumentStoreFile>();
  readonly blob = input<DocumentStoreBlob | null>(null);
  readonly blobUrl = input<string | null>(null);
  readonly canUpload = input(false);

  readonly delete = output<void>();
  readonly saveDescription = output<string>();
  /** Open this definition on Pascal's Workbench (the table owns the route). */
  readonly openWorkbench = output<string>();

  private readonly toasts = inject(ToastService);

  protected readonly description = signal('');

  /** v4 `isCustomToolDefinition` — ROOT-level `Tools/*.tool.json`, case-insensitive. */
  protected readonly isCustomToolDefinition = computed(() =>
    /^tools\/[^/]+\.tool\.json$/i.test(this.file().relativePath),
  );

  protected openInWorkbench(): void {
    this.openWorkbench.emit(this.file().relativePath);
  }

  protected readonly isBlobBackedFile = computed(() => isBlobBacked(this.file().fileType));
  protected readonly isImage = computed(() => this.blob()?.storedMimeType.startsWith('image/') ?? false);
  protected readonly dirty = computed(() => this.description() !== (this.blob()?.description ?? ''));

  ngOnInit(): void {
    this.description.set(this.blob()?.description ?? '');
  }

  /**
   * v4 `FileDetailRow.handleDownload` (`af1bc479`) — fetch the served bytes and
   * hand them to the shared download util.
   *
   * The name is `file.fileName`, which tracks the STORED bytes: uploads are
   * transcoded to WebP, so `blob.originalFileName`'s extension can mismatch
   * what the endpoint serves. It is only the second chance.
   */
  protected async download(): Promise<void> {
    const url = this.blobUrl();
    if (!url) {
      return;
    }
    try {
      const res = await fetch(url);
      if (!res.ok) {
        throw new Error(`Failed to fetch file (${res.status})`);
      }
      const bytes = await res.blob();
      triggerBlobDownload(bytes, this.file().fileName || this.blob()?.originalFileName || 'file');
    } catch {
      this.toasts.showError('Failed to download file');
    }
  }

  protected async copyMarkdown(): Promise<void> {
    const b = this.blob();
    const f = this.file();
    const markdown = this.isImage()
      ? `![${b?.description || b?.originalFileName || f.fileName}](${f.relativePath})`
      : `[${b?.originalFileName || f.fileName}](${f.relativePath})`;
    try {
      await navigator.clipboard.writeText(markdown);
    } catch {
      /* no-op */
    }
  }

  protected save(): void {
    this.saveDescription.emit(this.description());
  }
}
