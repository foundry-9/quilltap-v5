import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ProjectFileDto } from '../../../core/core-contract';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { Icon } from '../../../ui/icon';
import { Modal } from '../../../ui/modal';
import { formatBytes } from '../../../ui/format-bytes';
import { normalizeAvatarSrc } from '../../../ui/avatar-stack';
import { fetchProjectFiles, projectKeys } from '../projects.api';

/**
 * The project Files card (v4 `FilesCard.tsx`), tier-1 form: the first 10 files
 * with a thumbnail (image files), name, size, and category. Clicking an image
 * opens a plain lightbox.
 *
 * DEFERRED LOUDLY (tier 3): "Browse All Files" (the ~5k-line FileBrowser /
 * FilePreview family) is a disabled affordance; project file UPLOAD (multipart
 * to the files/mount routes) is not shipped this round — lane C ships only the
 * characters-photos route.
 */
@Component({
  selector: 'qt-project-files-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, Icon, Modal],
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
              @if (thumbFor(file); as src) {
                <img
                  [src]="src"
                  [alt]="file.fileName"
                  class="w-10 h-10 rounded object-cover flex-shrink-0"
                />
              } @else {
                <span
                  class="w-10 h-10 rounded flex items-center justify-center qt-bg-muted flex-shrink-0"
                >
                  <qt-icon name="file" class="w-5 h-5 qt-text-secondary" />
                </span>
              }
              <div class="min-w-0 flex-1">
                <p class="qt-label text-foreground truncate">{{ file.fileName }}</p>
                <p class="qt-text-xs qt-text-secondary">
                  {{ sizeLabel(file) }} &bull; {{ file.category }}
                </p>
              </div>
            </button>
          }
        </div>
      }

      <div class="pt-2 mt-2 border-t qt-border-default">
        <button
          type="button"
          class="w-full qt-button qt-button-secondary text-sm"
          title="The full file browser is not yet available"
          disabled
        >
          Browse All Files
        </button>
      </div>
    </qt-collapsible-card>

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
  protected readonly lightbox = signal<string | null>(null);

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

  protected sizeLabel(file: ProjectFileDto): string {
    return formatBytes(file.fileSizeBytes ?? 0);
  }

  protected thumbFor(file: ProjectFileDto): string | null {
    if (!file.mimeType?.startsWith('image/')) {
      return null;
    }
    return normalizeAvatarSrc(file.thumbnailUrl ?? file.filepath ?? undefined);
  }

  protected onClick(file: ProjectFileDto): void {
    const src = this.thumbFor(file);
    if (src) {
      this.lightbox.set(src);
    }
    // Non-image files: no preview this round (the FilePreview family is deferred).
  }
}
