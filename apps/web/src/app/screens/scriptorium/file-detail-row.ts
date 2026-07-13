import { ChangeDetectionStrategy, Component, computed, input, OnInit, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';

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

  protected readonly description = signal('');

  protected readonly isBlobBackedFile = computed(() => isBlobBacked(this.file().fileType));
  protected readonly isImage = computed(() => this.blob()?.storedMimeType.startsWith('image/') ?? false);
  protected readonly dirty = computed(() => this.description() !== (this.blob()?.description ?? ''));

  ngOnInit(): void {
    this.description.set(this.blob()?.description ?? '');
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
