import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { QueryClient, injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ProjectFileDto } from '../../../core/core-contract';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { Modal } from '../../../ui/modal';
import { formatBytes } from '../../../ui/format-bytes';
import { FileThumbnail } from '../../files/file-thumbnail';
import { fileUrl } from '../../../images/image-urls';
import { buildMountBlobUrl } from '../../scriptorium/scriptorium.api';
import { ProjectStoreBrowser } from '../../../document-browser/project-store-browser';
import { fetchProjectFiles, projectKeys } from '../projects.api';

/**
 * The project Files card (v4 `FilesCard.tsx`), tier-1 form: the first 10 files,
 * each with v4's `FileThumbnail` (an image thumbnail, else the type glyph),
 * `originalFilename`, `formatBytes(size)` and the category, then v4's `+N more
 * files` line. A store-backed row's thumbnail and preview go through the
 * mount-point blob route, as v4's do. Clicking an image opens a plain lightbox.
 *
 * "Browse All Files" (v4 `c17033f04`, P4.D387 item 19) opens the project's
 * own store in the document-store browser (`qt-project-store-browser`) in a
 * `Project Files` dialog that a backdrop click does not close; closing it
 * re-reads this card's list, since the browser re-reads only its own listing
 * (v4 `onFilesChange`). Files are uploaded there, through the browser's
 * `Upload here`. The card's own `Upload Files` button (v4's multipart upload
 * into the project store) is the standing tier-3 deferral.
 */
@Component({
  selector: 'qt-project-files-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, FileThumbnail, Modal, ProjectStoreBrowser],
  template: `
    <qt-collapsible-card
      title="Files"
      [description]="subtitle()"
      icon="folder"
      [defaultOpen]="defaultOpen()"
    >
      @if (files().length === 0) {
        <div class="text-center qt-text-secondary py-2">
          <p>No files in this project yet.</p>
          <p class="qt-text-small mt-1">Files will appear here when added to project chats.</p>
        </div>
      } @else {
        <div class="max-h-64 overflow-y-auto space-y-1">
          @for (file of firstTen(); track file.id) {
            <button
              type="button"
              class="w-full flex items-center gap-3 p-2 rounded-lg hover:qt-bg-muted transition-colors text-left"
              (click)="onClick(file)"
            >
              <qt-file-thumbnail
                [fileId]="file.id"
                [mimeType]="file.mimeType"
                [alt]="file.originalFilename"
                [size]="40"
                className="rounded flex-shrink-0"
                [mountPointId]="file.mountPointId"
                [relativePath]="file.relativePath"
              />
              <div class="min-w-0 flex-1">
                <p class="qt-label text-foreground truncate">{{ file.originalFilename }}</p>
                <p class="qt-text-xs qt-text-secondary">
                  {{ sizeLabel(file) }} &bull; {{ file.category }}
                </p>
              </div>
            </button>
          }
          @if (files().length > 10) {
            <p class="qt-text-xs qt-text-secondary text-center py-2">
              +{{ files().length - 10 }} more files
            </p>
          }
        </div>
      }

      <div class="pt-2 mt-2 border-t qt-border-default">
        <button
          type="button"
          class="w-full qt-button qt-button-secondary text-sm"
          (click)="browserOpen.set(true)"
        >
          Browse All Files
        </button>
      </div>
    </qt-collapsible-card>

    @if (browserOpen()) {
      <qt-modal
        title="Project Files"
        maxWidth="3xl"
        [closeOnBackdrop]="false"
        (close)="closeBrowser()"
      >
        <div class="min-h-[400px] p-2">
          <qt-project-store-browser [projectId]="projectId()" [scrollList]="true" />
        </div>
      </qt-modal>
    }
    @if (lightbox(); as src) {
      <qt-modal title="Preview" maxWidth="2xl" (close)="lightbox.set(null)">
        <img [src]="src" alt="File preview" class="max-h-[70vh] w-full object-contain" />
      </qt-modal>
    }
  `,
})
export class ProjectFilesCard {
  readonly projectId = input.required<string>();
  readonly defaultOpen = input(false);

  private readonly core = inject(CoreClient);
  private readonly queryClient = inject(QueryClient);
  protected readonly lightbox = signal<string | null>(null);
  /** The Browse All dialog (v4 `showBrowser`). */
  protected readonly browserOpen = signal(false);

  private readonly filesQuery = injectQuery(() => ({
    queryKey: projectKeys.files(this.projectId()),
    queryFn: (): Promise<ProjectFileDto[]> => fetchProjectFiles(this.core, this.projectId()),
  }));

  protected readonly files = computed(() => this.filesQuery.data() ?? []);
  protected readonly firstTen = computed(() => this.files().slice(0, 10));

  protected readonly subtitle = computed(() => {
    const n = this.files().length;
    return `${n} file${n !== 1 ? 's' : ''}`;
  });

  /** v4: closing the browser re-reads the card's own list (`onFilesChange`). */
  protected closeBrowser(): void {
    this.browserOpen.set(false);
    void this.queryClient.invalidateQueries({ queryKey: projectKeys.files(this.projectId()) });
  }

  protected sizeLabel(file: ProjectFileDto): string {
    return formatBytes(file.size ?? 0);
  }

  /** The full-size image URL: the mount blob for a store row, else the file route. */
  protected imageSrc(file: ProjectFileDto): string | null {
    if (!file.mimeType?.startsWith('image/')) {
      return null;
    }
    return file.mountPointId && file.relativePath
      ? buildMountBlobUrl(file.mountPointId, file.relativePath)
      : fileUrl(file.id);
  }

  protected onClick(file: ProjectFileDto): void {
    const src = this.imageSrc(file);
    if (src) {
      this.lightbox.set(src);
    }
    // Non-image files: no preview this round (the FilePreview family is deferred).
  }
}
