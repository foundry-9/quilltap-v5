import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import { Icon } from '../ui/icon';

/**
 * The in-chat image lightbox — a port of v4 `components/chat/ImageModal.tsx`.
 * A full-screen overlay over one image with save-to-character-gallery (rides the
 * existing `characterPhotoSaveById` variant), download, copy-to-clipboard, and
 * delete (`chatFileDelete`). Escape or a backdrop click closes it.
 *
 * v4 rendered through a React portal at document.body to escape stacking
 * contexts; the Angular host places this modal at the conversation's top level,
 * so the `fixed inset-0 z-[100]` overlay already covers the viewport. v4's
 * toasts become a transient inline notice (the SPA has no toast service yet).
 */
@Component({
  selector: 'qt-image-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div
      class="fixed inset-0 z-[100] flex items-center justify-center qt-bg-overlay backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      (click)="close.emit()"
    >
      <div class="absolute top-4 right-4 flex gap-2 z-10">
        @if (canSaveToCharacter()) {
          <button
            type="button"
            class="p-2 qt-bg-overlay-btn rounded-full qt-text-overlay disabled:opacity-50"
            [disabled]="tagging()"
            [title]="'Save to ' + (characterName() || 'character') + '\\'s photo album'"
            (click)="onSaveToCharacter($event)"
          >
            <qt-icon name="user" class="w-6 h-6" />
          </button>
        }
        @if (canSaveToUserCharacter()) {
          <button
            type="button"
            class="p-2 qt-bg-overlay-btn rounded-full qt-text-overlay disabled:opacity-50"
            [disabled]="tagging()"
            [title]="'Save to ' + (userCharacterName() || 'user character') + '\\'s photo album'"
            (click)="onSaveToUserCharacter($event)"
          >
            <qt-icon name="user" class="w-6 h-6" />
          </button>
        }
        <button
          type="button"
          class="p-2 qt-bg-overlay-btn rounded-full qt-text-overlay"
          title="Download"
          (click)="onDownload($event)"
        >
          <qt-icon name="download" class="w-6 h-6" />
        </button>
        <button
          type="button"
          class="p-2 qt-bg-overlay-btn rounded-full qt-text-overlay"
          title="Copy to clipboard"
          (click)="onCopy($event)"
        >
          <qt-icon name="copy" class="w-6 h-6" />
        </button>
        <button
          type="button"
          class="p-2 qt-bg-overlay-btn rounded-full qt-text-overlay"
          title="Close"
          (click)="onCloseButton($event)"
        >
          <qt-icon name="close" class="w-6 h-6" />
        </button>
      </div>

      @if (canDelete()) {
        <div class="absolute bottom-4 right-4 z-10">
          <button
            type="button"
            class="p-2 qt-bg-destructive/80 rounded-full qt-text-overlay disabled:opacity-50"
            [disabled]="deleting()"
            title="Delete image"
            (click)="onDelete($event)"
          >
            <qt-icon name="trash" class="w-6 h-6" />
          </button>
        </div>
      }

      <div
        class="relative max-w-[90vw] max-h-[90vh] flex items-center justify-center"
        (click)="$event.stopPropagation()"
      >
        <img [src]="src()" [alt]="filename()" class="max-w-full max-h-[90vh] w-auto h-auto object-contain" />
      </div>

      @if (notice(); as n) {
        <div
          class="absolute top-4 left-1/2 -translate-x-1/2 qt-text-overlay text-sm qt-bg-overlay-caption px-3 py-1 rounded"
        >
          {{ n }}
        </div>
      }

      <div
        class="absolute bottom-4 left-1/2 -translate-x-1/2 qt-text-overlay-muted text-sm qt-bg-overlay-caption px-3 py-1 rounded"
      >
        {{ filename() }}
      </div>
    </div>
  `,
})
export class ImageModal {
  private readonly core = inject(CoreClient);

  readonly src = input.required<string>();
  readonly filename = input.required<string>();
  readonly fileId = input<string | undefined>(undefined);
  readonly characterId = input<string | undefined>(undefined);
  readonly characterName = input<string | undefined>(undefined);
  readonly userCharacterId = input<string | undefined>(undefined);
  readonly userCharacterName = input<string | undefined>(undefined);

  readonly close = output<void>();
  /** Emitted after a successful delete so the parent refetches (v4 `onDelete`). */
  readonly deleted = output<void>();

  protected readonly tagging = signal(false);
  protected readonly deleting = signal(false);
  protected readonly notice = signal<string | null>(null);

  protected readonly canSaveToCharacter = computed(() => !!this.fileId() && !!this.characterId());
  protected readonly canSaveToUserCharacter = computed(
    () => !!this.fileId() && !!this.userCharacterId(),
  );
  protected readonly canDelete = computed(() => !!this.fileId());

  constructor() {
    // Escape closes; lock body scroll while open (v4's keydown + overflow effect).
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') this.close.emit();
    };
    if (typeof document !== 'undefined') {
      document.addEventListener('keydown', onKeyDown);
      const prevOverflow = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
      inject(DestroyRef).onDestroy(() => {
        document.removeEventListener('keydown', onKeyDown);
        document.body.style.overflow = prevOverflow;
      });
    }
  }

  protected onCloseButton(e: Event): void {
    e.stopPropagation();
    this.close.emit();
  }

  protected async onSaveToCharacter(e: Event): Promise<void> {
    e.stopPropagation();
    await this.saveToGallery(this.characterId(), this.characterName() || 'character');
  }

  protected async onSaveToUserCharacter(e: Event): Promise<void> {
    e.stopPropagation();
    await this.saveToGallery(this.userCharacterId(), this.userCharacterName() || 'user character');
  }

  private async saveToGallery(characterId: string | undefined, label: string): Promise<void> {
    const fileId = this.fileId();
    if (!fileId || !characterId) return;
    this.tagging.set(true);
    try {
      const resp = await this.core.dispatch({
        type: 'characterPhotoSaveById',
        characterId,
        fileId,
      });
      if (resp.type === 'error') throw new Error(resp.data.message);
      this.flash(`Saved to ${label}'s photo album`);
    } catch (err) {
      this.flash(err instanceof Error ? err.message : 'Failed to save to photo album');
    } finally {
      this.tagging.set(false);
    }
  }

  protected async onDownload(e: Event): Promise<void> {
    e.stopPropagation();
    try {
      const response = await fetch(this.src());
      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = this.filename();
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    } catch {
      this.flash('Failed to download image');
    }
  }

  protected async onCopy(e: Event): Promise<void> {
    e.stopPropagation();
    try {
      const response = await fetch(this.src());
      const blob = await response.blob();
      await navigator.clipboard.write([new ClipboardItem({ [blob.type]: blob })]);
      this.flash('Image copied to clipboard');
    } catch {
      this.flash('Failed to copy image to clipboard');
    }
  }

  protected async onDelete(e: Event): Promise<void> {
    e.stopPropagation();
    const fileId = this.fileId();
    if (!fileId) return;
    if (
      typeof window !== 'undefined' &&
      !window.confirm('Permanently delete this photo? This cannot be undone.')
    ) {
      return;
    }
    this.deleting.set(true);
    try {
      const resp = await this.core.dispatch({ type: 'chatFileDelete', fileId });
      if (resp.type === 'error') throw new Error(resp.data.message);
      this.deleted.emit();
      this.close.emit();
    } catch (err) {
      this.flash(err instanceof Error ? err.message : 'Failed to delete image');
    } finally {
      this.deleting.set(false);
    }
  }

  private flash(message: string): void {
    this.notice.set(message);
    setTimeout(() => this.notice.set(null), 2500);
  }
}
