import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  inject,
  input,
  output,
} from '@angular/core';

import { DialogFocus, mintDialogId } from './dialog-focus';
import { isInFloatingLayer } from './floating-layer';
import { Icon } from './icon';


const MAX_WIDTHS: Record<string, string> = {
  md: '28rem',
  lg: '32rem',
  xl: '36rem',
  '2xl': '42rem',
  '3xl': '48rem',
  // v4's `max-w-4xl` — the Library file picker's width (`LibraryFilePickerModal
  // :364`), the first consumer of a token wider than 3xl.
  '4xl': '56rem',
};

/**
 * A centered overlay dialog (v4 `components/ui/BaseModal.tsx`) over the SPA's
 * `qt-dialog-*` surface classes: a scrim, a titled card with a close button, a
 * scrollable body (`<ng-content>`), and a footer slot
 * (`<ng-content select="[qt-modal-footer]">`). Backdrop + the ✕ both emit `close`.
 *
 * Renders IN PLACE (no portal); closes on a document Escape unless
 * `closeOnEscape` is false (v4 `BaseModal.tsx:96` / `useClickOutside.ts:98-110`,
 * converged P4.D358 R-N); announces as a dialog named by its title and traps
 * focus (`qtDialogFocus`).
 *
 * The Escape listener is DOCUMENT-level, bubble phase, and does not stop
 * propagation, as v4's is — so two stacked `qt-modal`s BOTH close on one Escape,
 * exactly as two stacked v4 `BaseModal`s do (`MODAL_ESCAPE_NO_STOP_PROPAGATION`).
 * A host that needs the inner-popover behaviour keeps its own capture-phase
 * handler and binds `[closeOnEscape]="false"` (v4's `escapeCapture` opt-in):
 * one owner of Escape per host, never both.
 */
@Component({
  selector: 'qt-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [DialogFocus, Icon],
  template: `
    <div class="qt-dialog-overlay" [class]="overlayClass()" (click)="onBackdrop()">
      <div
        class="qt-dialog flex flex-col max-h-[90vh]"
        [style.max-width]="maxWidthRem()"
        role="dialog"
        aria-modal="true"
        [attr.aria-labelledby]="titleId"
        tabindex="-1"
        qtDialogFocus
        (click)="$event.stopPropagation()"
      >
        <div class="qt-dialog-header flex items-center justify-between gap-4">
          <h2 class="qt-dialog-title" [id]="titleId">{{ title() }}</h2>
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
  /** Width token (v4 `maxWidth`: `md` | `lg` | `xl` | `2xl` | `3xl` | `4xl`). */
  readonly maxWidth = input<string>('lg');
  /** Whether a backdrop click dismisses (v4 `closeOnClickOutside`, default true). */
  readonly closeOnBackdrop = input(true);
  /** Whether a document Escape dismisses (v4 `closeOnEscape`, default true — `BaseModal.tsx:96`). */
  readonly closeOnEscape = input(true);
  /**
   * Additional classes for the full-screen overlay — e.g. a raised `z-[…]`
   * when the modal opens over a viewer already above the dialog layer.
   * (v4 `BaseModal.tsx:31-35` `overlayClassName`, `01a83539d`; appended to
   * `qt-dialog-overlay` as v4 `:107` does. Tailwind sees only literal class
   * strings, so callers pass the `z-[…]` token as a literal.)
   */
  readonly overlayClass = input('');
  readonly close = output<void>();

  protected readonly maxWidthRem = computed(() => MAX_WIDTHS[this.maxWidth()] ?? MAX_WIDTHS['lg']);
  /** One `aria-labelledby` target per instance — v4's `useId()` (C9 R1). */
  protected readonly titleId = mintDialogId();

  constructor() {
    // v4 `useClickOutside.ts:98-110`'s `onEscape`: a document keydown, bubble
    // phase, no stopPropagation. `defaultPrevented` means an inner control
    // already consumed the key. An Escape pressed in the topmost floating
    // layer (the always-on Brahma Console) is not this modal's — v4
    // `useClickOutside.ts` returns on `isInFloatingLayer(event.target)` first
    // (`99463fc56`; P4.D359's guard, carried onto P4.D358's listener at union).
    const onDocumentKeydown = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape' || !this.closeOnEscape() || event.defaultPrevented) return;
      if (isInFloatingLayer(event.target)) return;
      this.close.emit();
    };
    document.addEventListener('keydown', onDocumentKeydown);
    inject(DestroyRef).onDestroy(() => document.removeEventListener('keydown', onDocumentKeydown));
  }

  protected onBackdrop(): void {
    if (this.closeOnBackdrop()) {
      this.close.emit();
    }
  }
}
