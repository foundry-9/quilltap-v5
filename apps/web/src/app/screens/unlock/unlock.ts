import { ChangeDetectionStrategy, Component, computed, inject, output, signal } from '@angular/core';

import { CoreClient } from '../../core/core-client';

const AUTOLOCK_RETURN_KEY = 'quilltap-autolock-return';

/**
 * The unlock screen (v4 `app/unlock/page.tsx`). Passphrase → `dispatch(unlock)`;
 * wrong passphrase shows v4's copy; the 3-attempt limit is a client-side cap
 * (disable the form, direct the user to the env-var recovery). All user-facing
 * strings are verbatim from v4.
 */
@Component({
  selector: 'qt-unlock',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-auth-page flex items-center justify-center min-h-screen p-4">
      <div class="qt-card max-w-lg w-full p-6 space-y-4">
        <h1 class="qt-heading-2">{{ heading() }}</h1>
        <p class="qt-text-muted">{{ body() }}</p>

        <div>
          <label class="qt-text-label block mb-1" for="qt-passphrase">Passphrase</label>
          <input
            id="qt-passphrase"
            class="qt-input w-full p-2"
            type="password"
            placeholder="Enter your passphrase"
            autocomplete="current-password"
            [value]="passphrase()"
            [disabled]="locked()"
            (input)="passphrase.set($any($event.target).value)"
            (keydown.enter)="unlock()"
          />
        </div>

        @if (error()) {
          <p class="qt-alert qt-alert-error text-sm">{{ error() }}</p>
        }

        <button class="qt-button qt-button-primary w-full py-2" [disabled]="locked()" (click)="unlock()">
          {{ loading() ? 'Unlocking...' : 'Unlock' }}
        </button>

        @if (isAutoLockReturn()) {
          <p class="qt-text-xs qt-text-muted text-center">
            This lock was triggered by the auto-lock idle timer. Your work remains exactly as you left it.
          </p>
        }
      </div>
    </div>
  `,
})
export class Unlock {
  private readonly core = inject(CoreClient);

  /** Emitted on a successful unlock — the gate re-checks and shows the shell. */
  readonly unlocked = output<void>();

  protected readonly passphrase = signal('');
  protected readonly error = signal<string | null>(null);
  protected readonly loading = signal(false);
  protected readonly attempts = signal(0);
  protected readonly isAutoLockReturn = signal(readAutoLockReturn());

  protected readonly locked = computed(() => this.loading() || this.attempts() >= 3);
  protected readonly heading = computed(() =>
    this.isAutoLockReturn() ? 'The Establishment Has Been Secured' : 'Quilltap Awaits Your Credentials',
  );
  protected readonly body = computed(() =>
    this.isAutoLockReturn()
      ? 'Quilltap has locked itself after a period of inactivity, much as a conscientious butler secures the silver when the household retires for the evening. Kindly supply your passphrase to resume where you left off.'
      : 'Your encryption key is protected by a passphrase, rather like a safe-deposit box that requires both key and signature. Kindly supply it so the establishment may open for the day.',
  );

  async unlock(): Promise<void> {
    if (this.locked()) {
      return;
    }
    this.error.set(null);
    if (!this.passphrase()) {
      this.error.set('A passphrase is required to unlock.');
      return;
    }
    this.loading.set(true);
    try {
      const resp = await this.core.dispatch({ type: 'unlock', passphrase: this.passphrase() });
      if (resp.type === 'unlockState') {
        this.core.connect();
        this.unlocked.emit();
        return;
      }
      // Error envelope — count the failed attempt and surface v4's copy.
      const next = this.attempts() + 1;
      this.attempts.set(next);
      if (next >= 3) {
        this.error.set(
          'Three attempts exhausted. You may set ENCRYPTION_MASTER_PEPPER as an environment variable and restart.',
        );
      } else {
        const message = resp.type === 'error' ? resp.data.message : '';
        this.error.set(message || 'That passphrase did not work. Do try again.');
      }
    } catch {
      this.error.set('Failed to unlock. The server may have gone on holiday.');
    } finally {
      this.loading.set(false);
    }
  }
}

function readAutoLockReturn(): boolean {
  if (typeof sessionStorage === 'undefined') {
    return false;
  }
  try {
    return sessionStorage.getItem(AUTOLOCK_RETURN_KEY) !== null;
  } catch {
    return false;
  }
}
