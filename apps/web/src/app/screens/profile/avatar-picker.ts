import {
  ChangeDetectionStrategy,
  Component,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { apiUrl } from '../../core/api-url';
import { CoreClient } from '../../core/core-client';
import type { FileEntry } from '../../core/core-contract';

/**
 * Pick an avatar from the stored image files (tier 2 of P4.9c; v4
 * `components/images/avatar-selector.tsx` + the `ImageGallery` it wraps).
 *
 * ## A REDUCED port, deliberately
 *
 * v4's selector composes `ImageGallery` (a paged, tag-filtered gallery over
 * `GET /api/v1/images`) and `ImageUploadDialog`. v5 has no `images` list verb —
 * `apps/web/src/app/images/**` belongs to lane P4.9a this round, and adding a
 * verb outside §3 is not this lane's to do. So this picker reads the general
 * files list that is already live (`filesList`) and shows the rows whose
 * category the server's avatar gate accepts (`IMAGE` / `AVATAR`), which is the
 * same set the gate would allow anyway.
 *
 * DEFERRED, loudly (named in the lane's status-log record): importing a NEW
 * image from inside the picker (v4's `ImageUploadDialog` leg), tag filtering
 * by context, and paging. Nothing here is a stub — the picker either shows the
 * images that exist or says plainly that there are none.
 */
@Component({
  selector: 'qt-avatar-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-dialog-overlay" (click)="cancelled.emit()">
      <div
        class="qt-dialog max-w-4xl w-full mx-4 max-h-[90vh] overflow-hidden flex flex-col"
        role="dialog"
        aria-modal="true"
        aria-label="Select Avatar"
        (click)="$event.stopPropagation()"
      >
        <div class="qt-dialog-header flex items-center justify-between">
          <h2 class="qt-dialog-title">Select Avatar</h2>
        </div>

        <div class="qt-dialog-body flex-1 overflow-y-auto">
          @if (loading()) {
            <p class="qt-text-muted text-sm">Sorting through the portrait gallery…</p>
          } @else if (error(); as message) {
            <p class="qt-text-destructive text-sm">{{ message }}</p>
          } @else if (images().length === 0) {
            <p class="qt-text-muted text-sm">
              No images are on file yet. Add one from the Files screen and it will appear here.
            </p>
          } @else {
            <div class="grid grid-cols-3 md:grid-cols-5 gap-3">
              @for (image of images(); track image.id) {
                <button
                  type="button"
                  class="qt-card-interactive p-1 rounded overflow-hidden"
                  [class.ring-2]="selectedId() === image.id"
                  [attr.aria-pressed]="selectedId() === image.id"
                  [title]="image.originalFilename"
                  (click)="selectedId.set(image.id)"
                >
                  <img
                    [src]="src(image.id)"
                    [alt]="image.originalFilename"
                    class="w-full h-24 object-cover"
                  />
                </button>
              }
            </div>
          }
        </div>

        <div class="qt-dialog-footer flex justify-between items-center">
          <button type="button" class="qt-button qt-button-ghost" (click)="chosen.emit(null)">
            Clear Avatar
          </button>
          <div class="flex space-x-3">
            <button type="button" class="qt-button qt-button-secondary" (click)="cancelled.emit()">
              Cancel
            </button>
            <button
              type="button"
              class="qt-button qt-button-primary"
              [disabled]="!selectedId()"
              (click)="confirm()"
            >
              Set as Avatar
            </button>
          </div>
        </div>
      </div>
    </div>
  `,
})
export class AvatarPicker {
  private readonly core = inject(CoreClient);

  /** The currently-set avatar's file id, if any (pre-selects it, as v4 does). */
  readonly currentImageId = input<string | null>(null);

  /** A chosen file id, or `null` for "clear the avatar" (v4's Clear button). */
  readonly chosen = output<string | null>();
  readonly cancelled = output<void>();

  protected readonly images = signal<FileEntry[]>([]);
  protected readonly selectedId = signal<string | null>(null);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);

  constructor() {
    // An input is not bound yet in the constructor, so the pre-selection has to
    // ride an effect (reading `currentImageId()` here would only ever see the
    // default).
    effect(() => this.selectedId.set(this.currentImageId()));
    void this.load();
  }

  private async load(): Promise<void> {
    try {
      const files = await this.core.filesList({ filter: 'general' });
      // The same set the server's avatar gate accepts (`api::user_profile`).
      this.images.set(files.filter((f) => f.category === 'IMAGE' || f.category === 'AVATAR'));
    } catch {
      this.error.set('The portrait gallery could not be opened just now.');
    } finally {
      this.loading.set(false);
    }
  }

  protected src(id: string): string {
    return apiUrl(`/api/v1/files/${id}`);
  }

  protected confirm(): void {
    const id = this.selectedId();
    if (id) {
      this.chosen.emit(id);
    }
  }
}
