import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';

import { apiUrl } from '../core/api-url';
import { Icon } from '../ui/icon';
import { ToastService } from '../ui/toast.service';

/**
 * The deleted-image placeholder — a port of v4
 * `components/images/DeletedImagePlaceholder.tsx` (76 lines): the dashed
 * "Image Deleted" card that replaces a tile/photo whose bytes no longer load,
 * with a Remove button that deletes the stale reference.
 *
 * `styleClass` ports v4's `className` prop VERBATIM, including its two
 * class-sniffing rules (`DeletedImagePlaceholder.tsx:50-54`):
 * - width/height apply only when the class string carries neither `w-full`
 *   nor `h-full` (fill callers size via the class);
 * - `!p-2` in the class switches compact/thumbnail mode (smaller paddings,
 *   filename hidden).
 * Callers pass the same strings v4's call sites pass.
 *
 * The cleanup calls v4's `DELETE /api/v1/images/{imageId}`. ⚠ In v5 that
 * surface is a LOUD REFUSAL (the P4.9a2 deferral — v4's orphan-cleanup DELETE
 * handler is unported; the web edge answers "recognized but not yet
 * available"), so the live button surfaces the refusal message through the
 * toast, exactly as v4 does (`DeletedImagePlaceholder.tsx:45`).
 */
@Component({
  selector: 'qt-deleted-image-placeholder',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div
      class="flex flex-col items-center justify-center qt-bg-muted border-2 border-dashed border-muted-foreground rounded-lg {{ isCompact() ? 'p-1' : 'p-6' }} {{ styleClass() }}"
      [style.width.px]="appliedWidth()"
      [style.height.px]="appliedHeight()"
    >
      <qt-icon
        name="image"
        class="qt-text-secondary {{ isCompact() ? 'w-6 h-6 mb-1' : 'w-12 h-12 mb-2' }}"
      />
      <p
        class="text-foreground font-medium text-center {{ isCompact() ? 'text-[10px] mb-0.5' : 'text-sm mb-2' }}"
      >
        Image Deleted
      </p>
      @if (!isCompact()) {
        <p class="qt-text-xs text-center mb-4 break-words max-w-full">{{ filename() }}</p>
      }
      <button
        type="button"
        class="bg-destructive hover:qt-bg-destructive/90 qt-text-on-destructive rounded transition-colors {{ isCompact() ? 'px-1.5 py-0.5 text-[9px]' : 'px-3 py-1 text-xs' }}"
        (click)="handleCleanup()"
      >
        Remove
      </button>
    </div>
  `,
})
export class DeletedImagePlaceholder {
  private readonly toasts = inject(ToastService);
  readonly imageId = input.required<string>();
  readonly filename = input.required<string>();
  /** v4 `width = 400` (`:20`). */
  readonly width = input(400);
  /** v4 `height = 300` (`:21`). */
  readonly height = input(300);
  /** v4 `className = ''` (`:22`) — carried verbatim, sniffing rules included. */
  readonly styleClass = input('');

  /** v4 `onCleanup?.()` — emitted after a successful reference delete. */
  readonly cleanup = output<void>();


  /** v4 `:50` — only apply width/height if not using a fill class. */
  private readonly shouldApplyDimensions = computed(
    () => !this.styleClass().includes('w-full') && !this.styleClass().includes('h-full'),
  );
  protected readonly appliedWidth = computed(() =>
    this.shouldApplyDimensions() ? this.width() : null,
  );
  protected readonly appliedHeight = computed(() =>
    this.shouldApplyDimensions() ? this.height() : null,
  );
  /** v4 `:54` — compact/thumbnail mode is detected from the class string. */
  protected readonly isCompact = computed(() => this.styleClass().includes('!p-2'));

  protected async handleCleanup(): Promise<void> {
    // v4 `:24-31` — showConfirmation with this exact copy.
    if (
      !window.confirm('This image file has been deleted. Remove this reference from the database?')
    ) {
      return;
    }
    try {
      const response = await fetch(apiUrl(`/api/v1/images/${this.imageId()}`), {
        method: 'DELETE',
      });
      if (!response.ok) {
        const data = (await response.json()) as { error?: string };
        throw new Error(data.error || 'Failed to remove image reference');
      }
      this.cleanup.emit();
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to remove image reference');
    }
  }
}
