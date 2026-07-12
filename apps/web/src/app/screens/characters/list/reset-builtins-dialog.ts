import { ChangeDetectionStrategy, Component, output, signal } from '@angular/core';

import { Modal } from '../../../ui/modal';
import { resetBuiltinCharacters } from '../characters.api';

/**
 * The Reset Built-in Characters confirm dialog (v4 `AuroraView.tsx` reset
 * dialog). Calls the WEB-EDGE `?action=reset-builtins` route (live since P4.4u4)
 * and emits `done` with the v4 success-toast text on success, or surfaces the
 * server error inline. Copy is v4-verbatim.
 */
@Component({
  selector: 'qt-reset-builtins-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Reset Built-in Characters" maxWidth="lg" (close)="close.emit()">
      <p class="qt-text-small mb-2">
        This will delete existing Lorian and Riya records if present, then re-import their first-run
        versions.
      </p>
      @if (error()) {
        <div class="qt-alert-error mt-3">{{ error() }}</div>
      }

      <div qt-modal-footer class="flex gap-3 justify-end">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="resetting()"
          (click)="close.emit()"
        >
          Cancel
        </button>
        <button
          type="button"
          class="qt-button qt-button-destructive"
          [disabled]="resetting()"
          (click)="reset()"
        >
          {{ resetting() ? 'Resetting...' : 'Reset Built-ins' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class ResetBuiltinsDialog {
  readonly close = output<void>();
  readonly done = output<string>();

  protected readonly resetting = signal(false);
  protected readonly error = signal<string | null>(null);

  protected async reset(): Promise<void> {
    this.resetting.set(true);
    this.error.set(null);
    try {
      await resetBuiltinCharacters();
      this.done.emit('Built-in characters reset successfully.');
      this.close.emit();
    } catch (err) {
      this.error.set(
        err instanceof Error && err.message ? err.message : 'Failed to reset built-in characters',
      );
    } finally {
      this.resetting.set(false);
    }
  }
}
