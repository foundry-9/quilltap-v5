import {
  DestroyRef,
  Directive,
  ElementRef,
  booleanAttribute,
  effect,
  inject,
  input,
  isDevMode,
  untracked,
} from '@angular/core';

/**
 * What counts as a Tab stop inside a dialog — v4 `hooks/useDialogFocus.ts:5-6`,
 * byte for byte. The terminal's F6 escape hatch (`terminal/terminal.ts`) reuses
 * it: v4 keeps a second copy in `components/terminal/Terminal.tsx:232-233`; v5
 * keeps ONE (`FOCUSABLE_ONE_CONSTANT`, P4.D358 — a DRY fold, no behaviour change).
 */
export const DIALOG_FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

let nextDialogId = 0;

/**
 * Mints one `aria-labelledby` target per dialog instance — v4's `useId()`
 * (C9 R1's `qt-dialog-<n>`; Angular has no `UniqueId` injectable). ONE counter
 * for `qt-modal` and every hand-rolled overlay, so two dialogs never share an id.
 */
export function mintDialogId(): string {
  return `qt-dialog-${++nextDialogId}`;
}

/**
 * Keyboard focus for a modal surface (WCAG 2.4.3 / 2.1.2) — v5's port of v4's
 * `hooks/useDialogFocus.ts` (`58120a7fe`, the opener read moved to render time
 * by `dff76ebef`). ONE directive serves `qt-modal`, the slide-over, every
 * hand-rolled overlay and the floating primitive (P4.D358 R-4; v4 has one hook):
 *
 * - on open, moves focus to the host one frame later — unless something inside
 *   (an autofocused field) already took it;
 * - keeps Tab / Shift+Tab cycling inside the host (the trap; `qtDialogTrap`
 *   false turns it off for a NON-modal floating surface, C9 R2);
 * - on close or destroy, hands focus back to whatever held it before.
 *
 * The host element needs `tabindex="-1"` so it can take focus itself.
 *
 * `qtDialogFocus` is v4's `isOpen`: a host rendered only while open leaves it at
 * its default (`true`); an always-mounted surface binds
 * `[qtDialogFocus]="isOpen()"`.
 *
 * The opener is read when the open edge is SEEN, not after the frame: v4 reads
 * it during render because "by the time an effect runs, an `autoFocus` field
 * inside the dialog has already taken focus, and the opener would be lost". The
 * Angular twin of "during render" for a host born open is the constructor — it
 * runs before any child `viewChild` / `afterNextRender` focus call.
 */
@Directive({
  selector: '[qtDialogFocus]',
  host: { '(keydown)': 'onKeydown($event)' },
})
export class DialogFocus {
  /** v4's `isOpen`. */
  readonly qtDialogFocus = input(true, { transform: booleanAttribute });
  /** False = focus-in / restore only, no Tab trap (C9 R2's floating surface). */
  readonly qtDialogTrap = input(true, { transform: booleanAttribute });

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
  private open = false;
  private opener: HTMLElement | null = null;
  private frame: number | null = null;

  constructor() {
    // Read at construction — the host's earliest moment, before any child can
    // take focus (see the class note).
    let bornActive: HTMLElement | null = document.activeElement as HTMLElement | null;

    effect(() => {
      const isOpen = this.qtDialogFocus();
      untracked(() => {
        // Only a host OPEN at its first check uses the construction-time read;
        // a later false → true edge reads the opener when the edge is seen.
        const born = bornActive;
        bornActive = null;
        if (isOpen === this.open) return;
        if (isOpen) {
          this.opener = born ?? (document.activeElement as HTMLElement | null);
          this.onOpen();
        } else {
          this.onClose();
        }
      });
    });

    inject(DestroyRef).onDestroy(() => {
      if (this.open) this.onClose();
    });
  }

  private onOpen(): void {
    this.open = true;
    if (isDevMode() && this.host.getAttribute('tabindex') !== '-1') {
      // DIALOG_FOCUS_DEV_WARN (P4.D358, a v5-only guard): without tabindex="-1"
      // the host cannot take focus and the frame below is a silent no-op.
      console.warn('qtDialogFocus: the host needs tabindex="-1" to take focus', this.host);
    }
    this.frame = requestAnimationFrame(() => {
      this.frame = null;
      if (!this.host.contains(document.activeElement)) this.host.focus();
    });
  }

  private onClose(): void {
    this.open = false;
    if (this.frame !== null) {
      cancelAnimationFrame(this.frame);
      this.frame = null;
    }
    const opener = this.opener;
    this.opener = null;
    if (opener && opener.isConnected) opener.focus();
  }

  /** v4's returned `onKeyDown` handler — the Tab trap. */
  protected onKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Tab' || !this.qtDialogTrap()) return;
    const dialog = this.host;

    const focusable = dialog.querySelectorAll<HTMLElement>(DIALOG_FOCUSABLE);
    if (focusable.length === 0) {
      event.preventDefault();
      return;
    }

    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;

    if (event.shiftKey && (active === first || active === dialog)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  }
}
