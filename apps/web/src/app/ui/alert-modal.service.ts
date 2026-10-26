import { ChangeDetectionStrategy, Component, Injectable, inject, signal } from '@angular/core';

/** One open alert: its message, its labelled buttons, and who is waiting. */
interface AlertRequest {
  message: string;
  buttons: string[];
  resolve: (choice: string | null) => void;
}

/**
 * The labelled-button alert — v4 `lib/alert.tsx` `showAlert(message,
 * buttons)` over `components/alert-dialog.tsx` (P4.D330 R-W / R-4, Tier 2
 * item 24). `show` resolves to the label of the button the operator chose, or
 * `null` when they dismissed it (Escape or the backdrop — v4 resolves
 * `undefined`); the callers test the label exactly as v4's do
 * (`=== 'Detach all and delete'`). No buttons given → v4's one `Close`.
 *
 * One alert at a time: a second `show` while one is up dismisses the first
 * (it resolves `null`), so no caller is left waiting on a dialog that is gone.
 * The host ({@link AlertModalHost}) is mounted once in the shell.
 */
@Injectable({ providedIn: 'root' })
export class AlertModalService {
  private readonly _current = signal<AlertRequest | null>(null);
  readonly current = this._current.asReadonly();

  show(message: string, buttons?: string[]): Promise<string | null> {
    this._current()?.resolve(null);
    return new Promise<string | null>((resolve) => {
      this._current.set({ message, buttons: buttons && buttons.length > 0 ? buttons : ['Close'], resolve });
    });
  }

  /** Close the open alert with this choice (null = dismissed). */
  answer(choice: string | null): void {
    const current = this._current();
    if (!current) return;
    this._current.set(null);
    current.resolve(choice);
  }
}

/**
 * v4 `AlertDialog`'s surface: a full-screen backdrop button (`Close dialog`)
 * and a centred `qt-dialog` holding the message (`whitespace-pre-wrap
 * break-words`) and one `qt-button` per label — the LAST `qt-button-primary`,
 * the others `qt-button-secondary` (v4 `getButtonStyle`). It sits at v4's
 * `z-[100]` / `z-[101]`, above the Inventory dialog's overlays. Escape and the
 * backdrop dismiss.
 *
 * **Recorded divergence.** v4 shows a `📋 Copy` button only when no buttons
 * are given; every v5 caller names its buttons, so it is not ported.
 */
@Component({
  selector: 'qt-alert-modal-host',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '(document:keydown.escape)': 'onEscape($event)' },
  template: `
    @if (alerts.current(); as alert) {
      <button
        type="button"
        class="qt-dialog-overlay cursor-default border-none z-[100]"
        aria-label="Close dialog"
        (click)="alerts.answer(null)"
      ></button>
      <div class="fixed top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 z-[101] pointer-events-auto">
        <div class="qt-dialog p-6" role="alertdialog" aria-modal="true">
          <div class="qt-dialog-body !px-0 !py-0 mb-6">
            <p class="text-sm text-foreground whitespace-pre-wrap break-words" data-testid="alert-modal-message">{{ alert.message }}</p>
          </div>
          <div class="flex gap-3 justify-end">
            @for (label of alert.buttons; track label; let last = $last) {
              <button
                type="button"
                [attr.class]="last ? 'qt-button qt-button-primary' : 'qt-button qt-button-secondary'"
                (click)="alerts.answer(label)"
              >
                {{ label }}
              </button>
            }
          </div>
        </div>
      </div>
    }
  `,
})
export class AlertModalHost {
  protected readonly alerts = inject(AlertModalService);

  /** Escape dismisses the alert — and only the alert. */
  protected onEscape(event: Event): void {
    if (!this.alerts.current()) return;
    event.stopImmediatePropagation();
    this.alerts.answer(null);
  }
}
