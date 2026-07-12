import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { Icon } from './icon';

const MAX_WIDTHS: Record<string, string> = {
  md: '28rem',
  lg: '32rem',
  xl: '36rem',
  '2xl': '42rem',
  '3xl': '48rem',
};

/**
 * A centered overlay dialog (v4 `components/ui/BaseModal.tsx`) over the SPA's
 * `qt-dialog-*` surface classes: a scrim, a titled card with a close button, a
 * scrollable body (`<ng-content>`), and a footer slot
 * (`<ng-content select="[qt-modal-footer]">`). Backdrop + the ✕ both emit `close`.
 */
@Component({
  selector: 'qt-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="qt-dialog-overlay" (click)="onBackdrop()">
      <div
        class="qt-dialog flex flex-col max-h-[90vh]"
        [style.max-width]="maxWidthRem()"
        role="dialog"
        aria-modal="true"
        (click)="$event.stopPropagation()"
      >
        <div class="qt-dialog-header flex items-center justify-between gap-4">
          <h2 class="qt-dialog-title">{{ title() }}</h2>
          <button
            type="button"
            class="qt-button-ghost qt-button-sm"
            aria-label="Close"
            (click)="close.emit()"
          >
            <qt-icon name="close" class="w-4 h-4" />
          </button>
        </div>
        <div class="qt-dialog-body overflow-y-auto">
          <ng-content></ng-content>
        </div>
        <div class="qt-dialog-footer">
          <ng-content select="[qt-modal-footer]"></ng-content>
        </div>
      </div>
    </div>
  `,
})
export class Modal {
  readonly title = input.required<string>();
  /** Width token (v4 `maxWidth`: `md` | `lg` | `xl` | `2xl` | `3xl`). */
  readonly maxWidth = input<string>('lg');
  /** Whether a backdrop click dismisses (v4 `closeOnClickOutside`, default true). */
  readonly closeOnBackdrop = input(true);
  readonly close = output<void>();

  protected readonly maxWidthRem = computed(() => MAX_WIDTHS[this.maxWidth()] ?? MAX_WIDTHS['lg']);

  protected onBackdrop(): void {
    if (this.closeOnBackdrop()) {
      this.close.emit();
    }
  }
}
