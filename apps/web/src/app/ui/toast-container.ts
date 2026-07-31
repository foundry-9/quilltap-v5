import { ChangeDetectionStrategy, Component, inject } from '@angular/core';

import { ToastService, type ToastType } from './toast.service';

/** v4 `getToastStyles` (`lib/toast.tsx:38-63`) — one class per type. */
const TYPE_CLASS: Record<ToastType, string> = {
  success: 'qt-toast-success',
  error: 'qt-toast-error',
  warning: 'qt-toast-warning',
  info: 'qt-toast-info',
};

/**
 * The toast stack — v4's `renderToasts` (`lib/toast.tsx:65-89`), mounted once
 * in the app root. See `toast.service.ts` for the mechanism note.
 *
 * v4 renders the container div (fixed, bottom-right, `pointer-events: none`)
 * with an inner flex column; v5 collapses the two into one element, since v4's
 * outer and inner divs carry the same `display:flex; flex-direction:column;
 * gap:10px`. `role="toast-container"` is v4's, verbatim.
 */
@Component({
  selector: 'qt-toast-container',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div role="toast-container" class="qt-toast-container">
      @for (toast of toasts(); track toast.id) {
        <div class="qt-toast" [class]="'qt-toast ' + typeClass(toast.type)">{{ toast.message }}</div>
      }
    </div>
  `,
})
export class ToastContainer {
  protected readonly toasts = inject(ToastService).toasts;

  protected typeClass(type: ToastType): string {
    return TYPE_CLASS[type];
  }
}
