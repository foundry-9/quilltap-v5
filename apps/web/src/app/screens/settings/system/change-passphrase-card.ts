import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { SystemSettingsSignals } from './system-settings-signals.service';

type Status = 'idle' | 'submitting' | 'success' | 'error';

/**
 * The Encryption Passphrase card (v4 `components/settings/ChangePassphraseCard.tsx`):
 * change or remove the passphrase that wraps the encryption key file. It does NOT
 * re-encrypt the database — it only re-wraps the key with a new passphrase (v4
 * copy `:62-66`). Leaving the new passphrase empty REMOVES protection entirely
 * (the empty-string sentinel, v4 `:64-65`, `:95`).
 *
 * v4 POSTs `/api/v1/system/unlock?action=change-passphrase` `{oldPassphrase,
 * newPassphrase}`; v5 goes through the existing `changePassphrase` dispatch verb
 * (`core-contract.ts:1429`). On success v4 fires a `quilltap-passphrase-changed`
 * window event so the Auto-Lock card re-reads state (v4 `:53`); v5 nudges
 * {@link SystemSettingsSignals} instead — same fan-out.
 */
@Component({
  selector: 'qt-change-passphrase-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <form class="space-y-4" (submit)="handleSubmit($event)">
      <p class="qt-text-small qt-text-muted">
        Change the passphrase that protects your encryption key file. This does not re-encrypt your
        database — it only re-wraps the key with a new passphrase. Leave the new passphrase empty to
        remove passphrase protection entirely.
      </p>

      <div>
        <label for="cp-current" class="block qt-text-label mb-2">Current Passphrase</label>
        <input
          type="password"
          id="cp-current"
          class="qt-input"
          autocomplete="current-password"
          placeholder="Leave empty if no passphrase is set"
          [value]="currentPassphrase()"
          (input)="onCurrentInput($event)"
        />
        <p class="qt-text-xs mt-1 qt-text-muted">
          If you have not previously set a passphrase, leave this field empty.
        </p>
      </div>

      <div>
        <label for="cp-new" class="block qt-text-label mb-2">New Passphrase</label>
        <input
          type="password"
          id="cp-new"
          class="qt-input"
          autocomplete="new-password"
          placeholder="Enter new passphrase (or leave empty to remove)"
          [value]="newPassphrase()"
          (input)="onNewInput($event)"
        />
      </div>

      <div>
        <label for="cp-confirm" class="block qt-text-label mb-2">Confirm New Passphrase</label>
        <input
          type="password"
          id="cp-confirm"
          class="qt-input"
          autocomplete="new-password"
          placeholder="Confirm new passphrase"
          [value]="confirmPassphrase()"
          (input)="onConfirmInput($event)"
        />
        @if (mismatch()) {
          <p class="qt-text-xs mt-1 qt-text-error">Passphrases do not match</p>
        }
      </div>

      @if (status() === 'error' && errorMessage()) {
        <div class="qt-alert-error">{{ errorMessage() }}</div>
      }

      @if (status() === 'success') {
        <div class="qt-alert-success">
          Passphrase changed successfully. The new passphrase will be required on the next restart.
        </div>
      }

      <button type="submit" class="qt-button-primary" [disabled]="!isValid() || mismatch()">
        {{ status() === 'submitting' ? 'Changing...' : 'Change Passphrase' }}
      </button>
    </form>
  `,
})
export class ChangePassphraseCard {
  private readonly core = inject(CoreClient);
  private readonly signals = inject(SystemSettingsSignals);

  protected readonly currentPassphrase = signal('');
  protected readonly newPassphrase = signal('');
  protected readonly confirmPassphrase = signal('');
  protected readonly status = signal<Status>('idle');
  protected readonly errorMessage = signal('');

  /** v4 `:14` — a genuine mismatch only once BOTH fields are non-empty. */
  protected readonly mismatch = computed(
    () =>
      this.newPassphrase().length > 0 &&
      this.confirmPassphrase().length > 0 &&
      this.newPassphrase() !== this.confirmPassphrase(),
  );

  /**
   * v4 `:15` — `!mismatch && new === confirm && status !== 'submitting'`. Note
   * `new === confirm` holds for two EMPTY fields, which is the removal path.
   */
  protected readonly isValid = computed(
    () =>
      !this.mismatch() &&
      this.newPassphrase() === this.confirmPassphrase() &&
      this.status() !== 'submitting',
  );

  protected onCurrentInput(event: Event): void {
    this.currentPassphrase.set((event.target as HTMLInputElement).value);
    this.status.set('idle');
  }

  protected onNewInput(event: Event): void {
    this.newPassphrase.set((event.target as HTMLInputElement).value);
    this.status.set('idle');
  }

  protected onConfirmInput(event: Event): void {
    this.confirmPassphrase.set((event.target as HTMLInputElement).value);
    this.status.set('idle');
  }

  protected async handleSubmit(event: Event): Promise<void> {
    event.preventDefault();
    if (!this.isValid()) {
      return;
    }
    this.status.set('submitting');
    this.errorMessage.set('');

    try {
      const resp = await this.core.dispatch({
        type: 'changePassphrase',
        oldPassphrase: this.currentPassphrase(),
        newPassphrase: this.newPassphrase(),
      });

      if (resp.type === 'error') {
        this.status.set('error');
        this.errorMessage.set(resp.data.message || 'Failed to change passphrase');
        return;
      }

      this.status.set('success');
      this.resetForm();

      // v4 `:53` — notify the Auto-Lock card that passphrase state changed.
      this.signals.notifyPassphraseChanged();
    } catch (err) {
      this.status.set('error');
      this.errorMessage.set(err instanceof Error ? err.message : 'An unexpected error occurred');
    }
  }

  private resetForm(): void {
    this.currentPassphrase.set('');
    this.newPassphrase.set('');
    this.confirmPassphrase.set('');
    this.errorMessage.set('');
  }
}
