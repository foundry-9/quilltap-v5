import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  HostListener,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import type { FileEntry } from '../../core/core-contract';
import { fileUrl } from '../../images/image-urls';
import { formatBytes } from '../../ui/format-bytes';
import {
  fileDisplayName,
  formatFileDate,
  getFileIcon,
  getFileTypeLabel,
  getPreviewType,
} from './file-model';

const MAX_TEXT_SIZE = 1024 * 1024; // 1MB max for text preview (v4 MAX_TEXT_SIZE).

/**
 * The file preview lightbox (v4 `components/files/FilePreview/`): a full-screen
 * modal with an actions bar (prev/next + filename + download/move/delete/close),
 * a type-specific renderer (image / text / pdf / fallback), and side nav arrows.
 * Keyboard: Esc closes, ←/→ navigate. Delete + Move are delegated to the parent
 * FileBrowser (the single delete authority — it owns the associations flow).
 *
 * Renderer divergences from v4 (LOUD, tracked deferrals — the rich stack is lane
 * C's dependency territory and the render pipeline is round-wide HANDS OFF):
 *  - TEXT renders as plain `<pre>` with a copy button. v4's ReactMarkdown +
 *    react-syntax-highlighter + wikilink navigation are deferred.
 *  - PDF renders a download prompt (v4 uses `pdfjs-dist`, not a v5 dependency).
 */
@Component({
  selector: 'qt-file-preview-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div
      class="fixed inset-0 z-50 flex items-center justify-center qt-bg-overlay backdrop-blur-sm"
      (click)="onBackdrop($event)"
    >
      <div
        class="qt-dialog w-full max-w-5xl max-h-[90vh] flex flex-col overflow-hidden"
        role="dialog"
        aria-modal="true"
        (click)="$event.stopPropagation()"
      >
        <!-- Actions bar -->
        <div class="flex items-center justify-between p-4 border-b qt-border-default">
          <div class="flex items-center gap-2">
            <button
              type="button"
              class="qt-button qt-button-secondary p-2 disabled:opacity-50"
              title="Previous file (Left arrow)"
              [disabled]="!hasPrevious()"
              (click)="goPrevious()"
            >
              ←
            </button>
            <span class="qt-text-small qt-text-secondary"
              >{{ currentIndex() + 1 }} of {{ files().length }}</span
            >
            <button
              type="button"
              class="qt-button qt-button-secondary p-2 disabled:opacity-50"
              title="Next file (Right arrow)"
              [disabled]="!hasNext()"
              (click)="goNext()"
            >
              →
            </button>
          </div>

          <div class="flex-1 text-center px-4 truncate">
            <span class="font-medium" [title]="name()">{{ name() }}</span>
          </div>

          <div class="flex items-center gap-2">
            <button
              type="button"
              class="qt-button qt-button-secondary p-2"
              title="Download"
              (click)="download()"
            >
              ⬇️
            </button>
            @if (canMove()) {
              <button
                type="button"
                class="qt-button qt-button-secondary p-2"
                title="Move to Project"
                (click)="moveToProject.emit(file())"
              >
                {{ folderIcon }}
              </button>
            }
            <button
              type="button"
              class="qt-button qt-button-secondary p-2 qt-text-destructive disabled:opacity-50"
              title="Delete"
              (click)="delete.emit(file())"
            >
              {{ trashIcon }}
            </button>
            <button
              type="button"
              class="qt-button qt-button-secondary p-2"
              title="Close (Escape)"
              (click)="close.emit()"
            >
              ✕
            </button>
          </div>
        </div>

        <!-- Preview content -->
        <div class="flex-1 overflow-auto p-4 relative">
          @switch (previewType()) {
            @case ('image') {
              <div class="flex items-center justify-center w-full h-full min-h-[300px]">
                <img
                  [src]="url()"
                  [alt]="name() || 'Image'"
                  class="max-w-full max-h-[70vh] object-contain"
                />
              </div>
            }
            @case ('text') {
              <div class="qt-file-preview-text-container">
                <button
                  type="button"
                  class="qt-copy-button"
                  [class.qt-copy-button-success]="copied()"
                  [title]="copied() ? 'Copied!' : 'Copy to clipboard'"
                  (click)="copyText()"
                >
                  <span class="qt-copy-button-icon">{{ copied() ? '✓' : '📋' }}</span>
                  <span class="qt-copy-button-text">{{ copied() ? 'Copied!' : 'Copy' }}</span>
                </button>
                <div class="qt-file-preview-scroll">
                  @if (textLoading()) {
                    <div class="qt-file-preview-loading">
                      <div class="qt-file-preview-loading-text">Loading file...</div>
                    </div>
                  } @else if (textError()) {
                    <div class="qt-file-preview-empty">
                      <div class="qt-file-preview-empty-icon">{{ pageCurlIcon }}</div>
                      <p>{{ textError() }}</p>
                    </div>
                  } @else if (!textContent()) {
                    <div class="qt-file-preview-empty">
                      <div class="qt-file-preview-empty-icon">{{ pageCurlIcon }}</div>
                      <p>No content available</p>
                    </div>
                  } @else {
                    <pre class="qt-file-preview-code"><code>{{ textContent() }}</code></pre>
                  }
                </div>
              </div>
            }
            @case ('pdf') {
              <div
                class="flex flex-col items-center justify-center w-full h-full min-h-[400px] text-center qt-text-secondary"
              >
                <div class="text-4xl mb-2">{{ pageIcon }}</div>
                <p>PDF preview isn't available yet — download to read it.</p>
                <a
                  [href]="url()"
                  [download]="name()"
                  class="qt-button qt-button-primary mt-4 inline-block"
                  >Download PDF</a
                >
              </div>
            }
            @default {
              <div
                class="flex flex-col items-center justify-center w-full h-full min-h-[300px] text-center p-8"
              >
                <div class="text-7xl mb-4">{{ icon() }}</div>
                <h3 class="text-xl font-semibold mb-2 break-all max-w-md">{{ name() }}</h3>
                <div class="qt-text-secondary space-y-1 mb-6">
                  <p>{{ typeLabel() }}</p>
                  <p>{{ sizeLabel() }}</p>
                  <p>Added {{ addedLabel() }}</p>
                </div>
                <p class="qt-text-secondary mb-4">Preview not available for this file type</p>
                <a [href]="url()" [download]="name()" class="qt-button qt-button-primary"
                  >Download File</a
                >
              </div>
            }
          }
        </div>

        <!-- Side navigation arrows (larger screens) -->
        @if (files().length > 1) {
          @if (hasPrevious()) {
            <button
              type="button"
              class="absolute left-4 top-1/2 -translate-y-1/2 qt-button qt-button-secondary p-4 text-2xl hidden lg:block"
              aria-label="Previous file"
              (click)="goPrevious()"
            >
              ←
            </button>
          }
          @if (hasNext()) {
            <button
              type="button"
              class="absolute right-4 top-1/2 -translate-y-1/2 qt-button qt-button-secondary p-4 text-2xl hidden lg:block"
              aria-label="Next file"
              (click)="goNext()"
            >
              →
            </button>
          }
        }
      </div>
    </div>
  `,
})
export class FilePreviewModal {
  private readonly destroyRef = inject(DestroyRef);

  readonly file = input.required<FileEntry>();
  readonly files = input.required<FileEntry[]>();
  /** Whether the Move affordance is offered (general files only). */
  readonly canMove = input(true);

  readonly close = output<void>();
  /** The parent runs the two-stage delete (with the associations dialog). */
  readonly delete = output<FileEntry>();
  readonly moveToProject = output<FileEntry>();
  /** Navigate to another file (prev/next) — the parent swaps `file`. */
  readonly navigate = output<FileEntry>();

  protected readonly folderIcon = '\u{1F4C1}';
  protected readonly trashIcon = '\u{1F5D1}️';
  protected readonly pageIcon = '\u{1F4C4}';
  protected readonly pageCurlIcon = '\u{1F4C3}';

  protected readonly copied = signal(false);
  protected readonly textContent = signal<string | null>(null);
  protected readonly textLoading = signal(false);
  protected readonly textError = signal<string | null>(null);

  protected readonly previewType = computed(() =>
    getPreviewType(this.file().mimeType, this.file().isPlainText),
  );
  protected readonly url = computed(() => fileUrl(this.file().id));
  protected readonly name = computed(() => fileDisplayName(this.file()));
  protected readonly icon = computed(() => getFileIcon(this.file().mimeType));
  protected readonly typeLabel = computed(() => getFileTypeLabel(this.file().mimeType));
  protected readonly sizeLabel = computed(() => formatBytes(this.file().size));
  protected readonly addedLabel = computed(() => formatFileDate(this.file().createdAt));

  protected readonly currentIndex = computed(() =>
    this.files().findIndex((f) => f.id === this.file().id),
  );
  protected readonly hasPrevious = computed(() => this.currentIndex() > 0);
  protected readonly hasNext = computed(() => this.currentIndex() < this.files().length - 1);

  constructor() {
    // Load the text body when a text file is shown and within the size cap
    // (v4 useFilePreview's gated fetch). A raw text GET, not JSON.
    effect(() => {
      const file = this.file();
      if (this.previewType() !== 'text') {
        this.textContent.set(null);
        this.textError.set(null);
        return;
      }
      if (file.size > MAX_TEXT_SIZE) {
        this.textError.set(
          `File is too large to preview (${(file.size / (1024 * 1024)).toFixed(1)}MB). Maximum is 1MB.`,
        );
        this.textContent.set(null);
        return;
      }
      void this.loadText(file);
    });
  }

  private async loadText(file: FileEntry): Promise<void> {
    const controller = new AbortController();
    this.destroyRef.onDestroy(() => controller.abort());
    this.textLoading.set(true);
    this.textError.set(null);
    try {
      const res = await fetch(fileUrl(file.id), { signal: controller.signal });
      if (!res.ok) throw new Error('Failed to load file content');
      const text = await res.text();
      // Guard against a late resolve after the user navigated away.
      if (this.file().id === file.id) this.textContent.set(text);
    } catch (err) {
      if (this.file().id === file.id) {
        this.textError.set(err instanceof Error ? err.message : 'Failed to load file');
      }
    } finally {
      if (this.file().id === file.id) this.textLoading.set(false);
    }
  }

  protected onBackdrop(event: MouseEvent): void {
    if (event.target === event.currentTarget) this.close.emit();
  }

  @HostListener('document:keydown', ['$event'])
  protected onKeydown(event: KeyboardEvent): void {
    switch (event.key) {
      case 'Escape':
        this.close.emit();
        break;
      case 'ArrowLeft':
        if (this.hasPrevious()) this.goPrevious();
        break;
      case 'ArrowRight':
        if (this.hasNext()) this.goNext();
        break;
    }
  }

  protected goPrevious(): void {
    if (!this.hasPrevious()) return;
    this.navigate.emit(this.files()[this.currentIndex() - 1]);
  }

  protected goNext(): void {
    if (!this.hasNext()) return;
    this.navigate.emit(this.files()[this.currentIndex() + 1]);
  }

  protected download(): void {
    const a = document.createElement('a');
    a.href = this.url();
    a.download = this.name() || 'download';
    document.body.appendChild(a);
    a.click();
    a.remove();
  }

  protected async copyText(): Promise<void> {
    const content = this.textContent();
    if (!content) return;
    try {
      await navigator.clipboard.writeText(content);
      this.copied.set(true);
      setTimeout(() => this.copied.set(false), 2000);
    } catch {
      /* clipboard unavailable — no-op (v4 logs and moves on) */
    }
  }
}
