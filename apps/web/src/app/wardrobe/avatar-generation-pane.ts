import { ChangeDetectionStrategy, Component, inject, input, output } from '@angular/core';

import { triggerBlobDownload } from '../core/download-utils';
import { ToastService } from '../ui/toast.service';

/** v4 `wardrobe-control-dialog.tsx:55-61` — the wardrobe pane's own profile
 *  summary shape (NOT the shared ImageProfilePicker's; this pane keeps v4's
 *  inline select with its `{name}{(default)} — {provider}/{modelName}` labels
 *  and its preselect-the-default behavior, `:286-287`). */
export interface ImageProfileSummary {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  isDefault: boolean;
}

/**
 * The avatar-generation pane at the bottom of the dialog's Outfit Builder tab
 * (v4 `wardrobe-control-dialog.tsx:1291-1391`, props `:1278-1289`): the image
 * model select, the Generate/Preview button, and — out of chat — the preview
 * image with its Download / discard controls. In chat, generation queues a
 * chat-avatar replacement; out of chat it yields a one-off downloadable
 * preview (the non-persisting `preview-avatar` route).
 */
@Component({
  selector: 'qt-avatar-generation-pane',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-card py-3 px-3 qt-bg-muted/30">
      <div class="flex flex-wrap items-center gap-2">
        <label for="wardrobe-image-profile" class="text-sm qt-text-secondary">
          Image model
        </label>
        <!-- The options load asynchronously, so the selection rides [selected]
             per option, never [value] (the standing dogfood-#6 rule). -->
        <select
          id="wardrobe-image-profile"
          class="qt-select flex-1 min-w-[12rem]"
          (change)="selectImageProfile.emit($any($event.target).value || null)"
        >
          @if (imageProfiles().length === 0) {
            <option value="" disabled [selected]="!selectedImageProfileId()">
              No image profiles configured
            </option>
          }
          @for (p of imageProfiles(); track p.id) {
            <option [value]="p.id" [selected]="p.id === selectedImageProfileId()">
              {{ p.name }}{{ p.isDefault ? ' (default)' : '' }} — {{ p.provider }}/{{
                p.modelName
              }}
            </option>
          }
        </select>
        <button
          type="button"
          class="qt-button-secondary qt-button-sm"
          (click)="generate.emit()"
          [disabled]="generating() || imageProfiles().length === 0"
        >
          {{ generating() ? 'Generating…' : inChat() ? 'Generate avatar' : 'Preview' }}
        </button>
      </div>

      <p class="mt-2 qt-text-xs qt-text-small">
        {{
          inChat()
            ? "Replaces this chat's avatar with the staged outfit."
            : 'Generates a one-off preview. Download to keep.'
        }}
      </p>

      @if (!inChat() && previewUrl(); as url) {
        <div class="mt-3 flex flex-col sm:flex-row gap-3 items-start">
          <div class="relative">
            <img
              [src]="url"
              [alt]="'Preview of ' + selectedCharacterName()"
              class="qt-bg-muted rounded border qt-border-default max-h-[40vh]"
            />
            <button
              type="button"
              (click)="discardPreview.emit()"
              aria-label="Discard preview"
              title="Discard preview"
              class="absolute top-1 right-1 w-6 h-6 rounded-full qt-bg-default border qt-border-default flex items-center justify-center qt-text-secondary hover:text-foreground shadow-sm"
            >
              ×
            </button>
          </div>
          <div class="flex flex-col gap-2">
            <button type="button" (click)="handleDownload(url)" class="qt-button-primary qt-button-sm">
              Download
            </button>
          </div>
        </div>
      }
    </div>
  `,
})
export class AvatarGenerationPane {
  private readonly toasts = inject(ToastService);

  readonly selectedCharacterName = input.required<string>();
  readonly imageProfiles = input.required<ImageProfileSummary[]>();
  readonly selectedImageProfileId = input.required<string | null>();
  readonly generating = input.required<boolean>();
  readonly inChat = input.required<boolean>();
  readonly previewUrl = input.required<string | null>();
  readonly previewFilename = input.required<string | null>();

  readonly selectImageProfile = output<string | null>();
  readonly generate = output<void>();
  readonly discardPreview = output<void>();

  /**
   * v4 `af1bc479` — the preview download went from a hidden anchor click to
   * fetching the bytes and handing them to the shared download util, so the
   * Electron shell (which has no right-click Save Image) gets its native save
   * dialog. v5 has no Electron arm — the util's browser fallback is the honest
   * behaviour here (its own header records that) — but the fetch-then-blob
   * shape and the failure sentence are v4's.
   */
  protected async handleDownload(previewUrl: string): Promise<void> {
    try {
      const res = await fetch(previewUrl);
      if (!res.ok) throw new Error(`Failed to fetch preview (${res.status})`);
      triggerBlobDownload(await res.blob(), this.previewFilename() ?? 'avatar-preview.webp');
    } catch (error) {
      console.error('Failed to download avatar preview:', {
        error: error instanceof Error ? error.message : String(error),
      });
      this.toasts.showError('Failed to download avatar preview');
    }
  }
}
