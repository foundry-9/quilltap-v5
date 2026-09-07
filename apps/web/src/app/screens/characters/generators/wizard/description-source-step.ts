import { ChangeDetectionStrategy, Component, inject, input, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../../core/core-client';
import { Icon } from '../../../../ui/icon';
import { Modal } from '../../../../ui/modal';
import type { DescriptionSourceType } from '../edit-generators.api';
import { ImageGallery } from '../../../../images/image-gallery';
import type { ImageData } from '../../../../images/images.api';
import { WizardState } from './wizard-state';

interface SourceOption {
  value: DescriptionSourceType;
  label: string;
  description: string;
}

/** v4 `DescriptionSourceStep.tsx:133-159`, verbatim copy. */
const SOURCE_OPTIONS: SourceOption[] = [
  {
    value: 'existing',
    label: 'Use existing character data',
    description: 'Derive physical appearance from the description and personality fields',
  },
  {
    value: 'upload',
    label: 'Upload a new image',
    description: 'Upload an image to analyze and generate physical description',
  },
  {
    value: 'gallery',
    label: 'Select from gallery',
    description: "Choose an existing image from the character's gallery",
  },
  {
    value: 'document',
    label: 'Upload a document',
    description: 'Upload a text, Markdown, or PDF file with character details',
  },
  {
    value: 'skip',
    label: 'Skip physical description',
    description: "Don't generate physical descriptions for this character",
  },
];

/**
 * Wizard step 2 — v4 `ai-wizard/steps/DescriptionSourceStep.tsx` (438 lines):
 * source selection (existing/upload/gallery/document/skip), the upload and
 * gallery sub-flows, and the vision-profile fallback picker.
 *
 * Uploads dispatch v5's already-landed `imageUpload`/`fileUpload` verbs (v4's
 * `POST /api/v1/images` and `POST /api/v1/files?action=upload`); gallery
 * browsing reuses `characterPhotoList` (the `AvatarPickerModal` precedent) —
 * v4's own `<ImageGallery>` component has no v5 port, so this step hosts its
 * own minimal grid rather than v4's richer picker (search/pagination/tags).
 */
@Component({
  selector: 'qt-wizard-description-source-step',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Modal, ImageGallery],
  template: `
    <div class="space-y-6">
      <div>
        <h3 class="qt-heading-4 text-foreground mb-2">Physical Description Source</h3>
        <p class="text-sm qt-text-secondary">
          Choose how to generate physical descriptions for image generation.
        </p>
      </div>

      <div class="space-y-3">
        @for (option of sourceOptions; track option.value) {
          <label
            [class]="
              'flex items-start gap-3 p-4 rounded-lg border cursor-pointer transition-colors ' +
              (wizard.descriptionSource() === option.value
                ? 'qt-border-primary qt-bg-primary/5'
                : 'qt-border-default hover:border-muted-foreground/50')
            "
          >
            <input
              type="radio"
              name="descriptionSource"
              class="mt-1"
              [checked]="wizard.descriptionSource() === option.value"
              (change)="wizard.setDescriptionSource(option.value)"
            />
            <div>
              <div class="font-medium text-foreground">{{ option.label }}</div>
              <div class="text-sm qt-text-secondary">{{ option.description }}</div>
            </div>
          </label>
        }
      </div>

      @if (wizard.descriptionSource() === 'upload') {
        <div class="p-4 rounded-lg border qt-border-default qt-bg-muted/20 space-y-4">
          <h4 class="font-medium text-foreground">Upload Image</h4>

          @if (wizard.uploadedImageUrl(); as url) {
            <div class="space-y-3">
              <div class="relative w-32 h-32 rounded-lg overflow-hidden border qt-border-default">
                <img [src]="url" alt="Uploaded" class="w-full h-full object-cover" />
              </div>
              <button
                type="button"
                class="text-sm qt-text-destructive hover:underline"
                (click)="wizard.handleImageUpload('', '')"
              >
                Remove image
              </button>
            </div>
          } @else {
            <div>
              <input
                #imageInput
                type="file"
                accept="image/jpeg,image/png,image/gif,image/webp"
                class="hidden"
                id="wizard-image-upload"
                [disabled]="uploadingImage()"
                (change)="onImageFileSelected(imageInput.files)"
              />
              <label
                for="wizard-image-upload"
                [class]="
                  'flex flex-col items-center justify-center w-full h-32 border-2 border-dashed rounded-lg cursor-pointer transition-colors ' +
                  (uploadingImage()
                    ? 'border-muted qt-bg-muted/50 cursor-not-allowed'
                    : 'qt-border-default hover:qt-border-primary hover:qt-bg-primary/5')
                "
              >
                @if (uploadingImage()) {
                  <div class="flex items-center gap-2 qt-text-secondary">
                    <svg class="w-5 h-5 animate-spin" fill="none" viewBox="0 0 24 24">
                      <circle
                        class="opacity-25"
                        cx="12"
                        cy="12"
                        r="10"
                        stroke="currentColor"
                        stroke-width="4"
                      />
                      <path
                        class="opacity-75"
                        fill="currentColor"
                        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                      />
                    </svg>
                    <span>Uploading...</span>
                  </div>
                } @else {
                  <qt-icon name="image" class="w-8 h-8 qt-text-secondary mb-2" />
                  <span class="text-sm qt-text-secondary">Click to upload image</span>
                  <span class="text-xs qt-text-secondary mt-1">JPEG, PNG, GIF, WebP</span>
                }
              </label>
            </div>
          }

          @if (uploadError()) {
            <p class="text-sm qt-text-destructive">{{ uploadError() }}</p>
          }
        </div>
      }

      @if (wizard.descriptionSource() === 'gallery') {
        <div class="p-4 rounded-lg border qt-border-default qt-bg-muted/20 space-y-4">
          <h4 class="font-medium text-foreground">Select from Gallery</h4>

          @if (wizard.selectedGalleryImageUrl(); as url) {
            <div class="space-y-3">
              <div class="relative w-32 h-32 rounded-lg overflow-hidden border qt-border-default">
                <img [src]="url" alt="Selected" class="w-full h-full object-cover" />
              </div>
              <button type="button" class="qt-button-secondary text-sm" (click)="showGallery.set(true)">
                Change selection
              </button>
            </div>
          } @else {
            <button type="button" class="qt-button-secondary" (click)="showGallery.set(true)">
              Browse Gallery
            </button>
          }

          @if (showGallery()) {
            <qt-modal title="Select Image" maxWidth="4xl" (close)="showGallery.set(false)">
              <!-- v4 mounts its shared ImageGallery (tagType CHARACTER, tagId = the
                   character) over GET /api/v1/images (DescriptionSourceStep.tsx:316-321)
                   and selects a FILES row id (:94-97). The §3 unification review replaced
                   a character-photos album read here (vault LINK ids the wizard server
                   cannot resolve) with the same shape over v5's imagesList verb. -->
              <qt-image-gallery
                [images]="galleryImages()"
                [loading]="imagesQuery.isPending()"
                [error]="imagesQuery.isError() ? 'Failed to load images' : null"
                [onSelectImage]="selectGalleryImage"
                [selectedImageId]="wizard.selectedGalleryImageId() ?? undefined"
                (reloadImages)="imagesQuery.refetch()"
              />
            </qt-modal>
          }
        </div>
      }

      @if (wizard.descriptionSource() === 'document') {
        <div class="p-4 rounded-lg border qt-border-default qt-bg-muted/20 space-y-4">
          <h4 class="font-medium text-foreground">Upload Document</h4>

          @if (wizard.uploadedDocumentName(); as name) {
            <div class="space-y-3">
              <div class="flex items-center gap-3 p-3 rounded-lg border qt-border-default bg-background">
                <qt-icon name="file" class="w-8 h-8 qt-text-secondary flex-shrink-0" />
                <div class="flex-1 min-w-0">
                  <div class="font-medium text-foreground truncate">{{ name }}</div>
                  <div class="text-sm qt-text-secondary">Document uploaded</div>
                </div>
              </div>
              <button
                type="button"
                class="text-sm qt-text-destructive hover:underline"
                (click)="wizard.handleDocumentUpload('', '')"
              >
                Remove document
              </button>
            </div>
          } @else {
            <div>
              <input
                #documentInput
                type="file"
                accept=".txt,.md,.markdown,.pdf,text/plain,text/markdown,application/pdf"
                class="hidden"
                id="wizard-document-upload"
                [disabled]="uploadingDocument()"
                (change)="onDocumentFileSelected(documentInput.files)"
              />
              <label
                for="wizard-document-upload"
                [class]="
                  'flex flex-col items-center justify-center w-full h-32 border-2 border-dashed rounded-lg cursor-pointer transition-colors ' +
                  (uploadingDocument()
                    ? 'border-muted qt-bg-muted/50 cursor-not-allowed'
                    : 'qt-border-default hover:qt-border-primary hover:qt-bg-primary/5')
                "
              >
                @if (uploadingDocument()) {
                  <div class="flex items-center gap-2 qt-text-secondary">
                    <svg class="w-5 h-5 animate-spin" fill="none" viewBox="0 0 24 24">
                      <circle
                        class="opacity-25"
                        cx="12"
                        cy="12"
                        r="10"
                        stroke="currentColor"
                        stroke-width="4"
                      />
                      <path
                        class="opacity-75"
                        fill="currentColor"
                        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                      />
                    </svg>
                    <span>Uploading...</span>
                  </div>
                } @else {
                  <qt-icon name="file" class="w-8 h-8 qt-text-secondary mb-2" />
                  <span class="text-sm qt-text-secondary">Click to upload document</span>
                  <span class="text-xs qt-text-secondary mt-1">Text, Markdown, or PDF</span>
                }
              </label>
            </div>
          }

          @if (documentUploadError()) {
            <p class="text-sm qt-text-destructive">{{ documentUploadError() }}</p>
          }
        </div>
      }

      @if (
        wizard.needsVisionProfile() &&
        (wizard.descriptionSource() === 'upload' || wizard.descriptionSource() === 'gallery')
      ) {
        <div class="p-4 rounded-lg border qt-border-warning/50 qt-bg-warning/10 space-y-3">
          <div class="flex items-start gap-2">
            <qt-icon name="alert-triangle" class="w-5 h-5 qt-text-warning mt-0.5 flex-shrink-0" />
            <div>
              <h4 class="font-medium text-foreground">Vision Profile Required</h4>
              <p class="text-sm qt-text-secondary">
                Your selected AI model cannot process images. Please select a vision-capable model to
                analyze the image.
              </p>
            </div>
          </div>

          @if (wizard.visionProfiles().length > 0) {
            <div>
              <label for="visionProfile" class="qt-label">Vision Profile *</label>
              <select
                id="visionProfile"
                class="qt-select"
                [value]="wizard.visionProfileId() ?? ''"
                (change)="wizard.setVisionProfileId($any($event.target).value)"
              >
                <option value="">Select a vision-capable profile...</option>
                @for (profile of wizard.visionProfiles(); track profile.id) {
                  <option [value]="profile.id" [selected]="wizard.visionProfileId() === profile.id">
                    {{ profile.name }} ({{ profile.provider }} - {{ profile.modelName }})
                  </option>
                }
              </select>
            </div>
          } @else {
            <p class="text-sm qt-text-destructive">
              No vision-capable profiles available. Please create a profile with OpenAI, Anthropic,
              Google, or Grok.
            </p>
          }
        </div>
      }
    </div>
  `,
})
export class WizardDescriptionSourceStep {
  protected readonly wizard = inject(WizardState);
  private readonly core = inject(CoreClient);

  /** The character id, for gallery scoping (undefined ⇒ New Character — no gallery yet). */
  readonly characterId = input<string | undefined>(undefined);

  protected readonly sourceOptions = SOURCE_OPTIONS;
  protected readonly showGallery = signal(false);
  protected readonly uploadingImage = signal(false);
  protected readonly uploadError = signal<string | null>(null);
  protected readonly uploadingDocument = signal(false);
  protected readonly documentUploadError = signal<string | null>(null);

  /** v4 `<ImageGallery tagId={characterId}>` → `GET /api/v1/images?tagId=` (`imagesList`). */
  protected readonly imagesQuery = injectQuery(() => ({
    queryKey: ['images', 'list', this.characterId() ?? ''],
    enabled: this.showGallery() && !!this.characterId(),
    queryFn: async (): Promise<ImageData[]> => {
      const data = await this.core.dispatchData({ type: 'imagesList', tagId: this.characterId()! });
      return (data['data'] as ImageData[] | undefined) ?? [];
    },
  }));

  protected galleryImages(): ImageData[] {
    return this.imagesQuery.data() ?? [];
  }

  /** v4 `handleGalleryImageSelect` (`DescriptionSourceStep.tsx:94-97`). */
  protected readonly selectGalleryImage = (image: ImageData): void => {
    this.wizard.handleGallerySelect(image.id, image.url || `/api/v1/files/${image.id}`);
    this.showGallery.set(false);
  };

  protected async onImageFileSelected(files: FileList | null): Promise<void> {
    const file = files?.[0];
    if (!file) return;
    this.uploadingImage.set(true);
    this.uploadError.set(null);
    try {
      const base64 = await fileToBase64(file);
      const result = await this.core.dispatchData({
        type: 'imageUpload',
        filename: file.name,
        contentType: file.type,
        data: base64,
      });
      const receipt = result['data'] as { id?: string; url?: string; filepath?: string } | undefined;
      if (!receipt?.id) {
        throw new Error('Failed to upload image');
      }
      this.wizard.handleImageUpload(receipt.id, receipt.url || receipt.filepath || '');
    } catch (err) {
      this.uploadError.set(err instanceof Error ? err.message : 'Failed to upload image');
    } finally {
      this.uploadingImage.set(false);
    }
  }

  protected async onDocumentFileSelected(files: FileList | null): Promise<void> {
    const file = files?.[0];
    if (!file) return;
    this.uploadingDocument.set(true);
    this.documentUploadError.set(null);
    try {
      const base64 = await fileToBase64(file);
      const result = await this.core.dispatchData({
        type: 'fileUpload',
        filename: file.name,
        contentType: file.type,
        data: base64,
      });
      // `fileUpload` answers `{ data }` (`files.rs` — the documented body); no guessing.
      const receipt = result['data'] as { id?: string } | undefined;
      if (!receipt?.id) {
        throw new Error('Failed to upload document');
      }
      this.wizard.handleDocumentUpload(receipt.id, file.name);
    } catch (err) {
      this.documentUploadError.set(err instanceof Error ? err.message : 'Failed to upload document');
    } finally {
      this.uploadingDocument.set(false);
    }
  }
}

/** Read a File's bytes as base64 (no data: prefix) — for the two upload verbs. */
function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error('Failed to read file'));
    reader.onload = () => {
      const result = reader.result as string;
      const commaIndex = result.indexOf(',');
      resolve(commaIndex >= 0 ? result.slice(commaIndex + 1) : result);
    };
    reader.readAsDataURL(file);
  });
}
