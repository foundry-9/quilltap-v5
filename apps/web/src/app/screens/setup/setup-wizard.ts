import { NgTemplateOutlet } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { CoreClient } from '../../core/core-client';
import { FirstRunService } from '../../startup/first-run.service';

type Mode = 'needs-setup' | 'needs-vault-storage';

/**
 * The first-run setup wizard (v4 `app/setup/page.tsx`). `needs-setup` mints the
 * encryption key via `dispatch(setup)` and reveals the pepper ONCE; the
 * `needs-vault-storage` mode persists an env pepper into a `.dbkey` via
 * `dispatch(storePepper)`. The server `setup`/`storePepper` variants land in the
 * P4.4 lane — component tests drive this against a mocked CoreClient; everything
 * up to the final dispatch is real. All strings are verbatim from v4.
 */
@Component({
  selector: 'qt-setup-wizard',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgTemplateOutlet],
  template: `
    <div class="qt-auth-page flex items-center justify-center min-h-screen p-4">
      @if (generatedPepper(); as pepper) {
        <!-- The one-time pepper reveal -->
        <div class="qt-card max-w-lg w-full p-6 space-y-4">
          <h1 class="qt-heading-2">Setup Complete</h1>
          <div class="qt-alert qt-alert-warning space-y-2">
            <p class="qt-text-small font-semibold">
              Save this encryption pepper now. It will not be shown again.
            </p>
            <p class="qt-text-xs qt-text-muted">
              If you ever need to recover your data without a passphrase, you will need this value
              set as the
              <code class="qt-code-inline">ENCRYPTION_MASTER_PEPPER</code> environment variable.
            </p>
          </div>
          <div class="relative">
            <pre class="qt-code-block break-all whitespace-pre-wrap select-all">{{ pepper }}</pre>
            <button
              class="qt-button qt-button-ghost absolute top-2 right-2"
              type="button"
              (click)="copyPepper(pepper)"
            >
              {{ copied() ? 'Copied!' : 'Copy' }}
            </button>
          </div>
          <button
            class="qt-button qt-button-primary w-full py-2"
            type="button"
            (click)="continueToApp()"
          >
            Continue to Quilltap
          </button>
        </div>
      } @else if (mode() === 'needs-vault-storage') {
        <!-- Persist an env pepper into a .dbkey -->
        <div class="qt-card max-w-lg w-full p-6 space-y-4">
          <h1 class="qt-heading-2">Secure Your Encryption Key</h1>
          <p class="qt-text-muted">
            Your encryption key is set via environment variable. Store it in an encrypted .dbkey
            file so Quilltap can start without the environment variable in the future.
          </p>
          <ng-container [ngTemplateOutlet]="passphraseFields"></ng-container>
          @if (error()) {
            <p class="qt-alert qt-alert-error text-sm">{{ error() }}</p>
          }
          <div class="flex gap-2">
            <button
              class="qt-button qt-button-primary flex-1"
              type="button"
              [disabled]="loading()"
              (click)="store()"
            >
              {{ loading() ? 'Storing...' : 'Store Key File' }}
            </button>
            <button
              class="qt-button qt-button-secondary flex-1 opacity-60"
              type="button"
              (click)="skip.emit()"
            >
              Skip for Now
            </button>
          </div>
        </div>
      } @else {
        <!-- First-run key generation -->
        <div class="qt-card max-w-lg w-full p-6 space-y-4">
          <h1 class="qt-heading-2">Welcome to Quilltap</h1>
          <p class="qt-text-muted">
            Quilltap needs an encryption key to protect your API keys and sensitive data. One will
            be generated automatically. You can optionally protect it with a passphrase.
          </p>
          <ng-container [ngTemplateOutlet]="passphraseFields"></ng-container>
          @if (error()) {
            <p class="qt-alert qt-alert-error text-sm">{{ error() }}</p>
          }
          <button
            class="qt-button qt-button-primary w-full py-2"
            type="button"
            [disabled]="loading()"
            (click)="setup()"
          >
            {{ submitLabel() }}
          </button>
        </div>
      }
    </div>

    <ng-template #passphraseFields>
      <div>
        <label class="qt-text-label block mb-1" for="qt-setup-pass">Passphrase (optional)</label>
        <input
          id="qt-setup-pass"
          class="qt-input w-full p-2"
          type="password"
          placeholder="Leave empty for no passphrase"
          autocomplete="new-password"
          [value]="passphrase()"
          (input)="passphrase.set($any($event.target).value)"
        />
        <p class="qt-text-xs qt-text-muted mt-1">{{ helperText() }}</p>
      </div>
      @if (passphrase()) {
        <div>
          <label class="qt-text-label block mb-1" for="qt-setup-confirm">Confirm passphrase</label>
          <input
            id="qt-setup-confirm"
            class="qt-input w-full p-2"
            type="password"
            placeholder="Confirm your passphrase"
            autocomplete="new-password"
            [value]="confirm()"
            (input)="confirm.set($any($event.target).value)"
          />
        </div>
      }
    </ng-template>
  `,
})
export class SetupWizard {
  private readonly core = inject(CoreClient);
  private readonly firstRun = inject(FirstRunService);

  readonly mode = input<Mode>('needs-setup');
  /** Emitted once setup / storage completes and the user proceeds. */
  readonly complete = output<void>();
  /** Emitted when the user skips vault storage. */
  readonly skip = output<void>();

  protected readonly passphrase = signal('');
  protected readonly confirm = signal('');
  protected readonly error = signal<string | null>(null);
  protected readonly loading = signal(false);
  protected readonly generatedPepper = signal<string | null>(null);
  protected readonly copied = signal(false);

  protected readonly submitLabel = computed(() => {
    if (this.loading()) {
      return 'Setting up...';
    }
    return this.passphrase() ? 'Set Up with Passphrase' : 'Set Up without Passphrase';
  });
  protected readonly helperText = computed(() =>
    this.mode() === 'needs-vault-storage'
      ? 'If set, you will need this passphrase when starting without the environment variable.'
      : 'If set, you will need to enter this passphrase each time Quilltap starts.',
  );

  private validate(): boolean {
    if (this.passphrase()) {
      if (this.passphrase() !== this.confirm()) {
        this.error.set('Passphrases do not match');
        return false;
      }
      if (this.passphrase().length < 8) {
        this.error.set('Passphrase must be at least 8 characters');
        return false;
      }
    }
    return true;
  }

  async setup(): Promise<void> {
    this.error.set(null);
    if (!this.validate()) {
      return;
    }
    this.loading.set(true);
    try {
      const resp = await this.core.dispatch({ type: 'setup', passphrase: this.passphrase() });
      if (resp.type === 'setup') {
        this.generatedPepper.set(resp.data.pepper);
        // A fresh instance has no connection profiles — route into the provider
        // wizard once the shell mounts (v4 `navigateAfterSetup`).
        this.firstRun.markSetupComplete();
        return;
      }
      this.error.set(resp.type === 'error' ? resp.data.message : 'Setup failed');
    } catch {
      this.error.set('Failed to complete setup');
    } finally {
      this.loading.set(false);
    }
  }

  async store(): Promise<void> {
    this.error.set(null);
    if (!this.validate()) {
      return;
    }
    this.loading.set(true);
    try {
      const resp = await this.core.dispatch({ type: 'storePepper', passphrase: this.passphrase() });
      if (resp.type === 'ack') {
        this.continueToApp();
        return;
      }
      this.error.set(resp.type === 'error' ? resp.data.message : 'Failed to store pepper');
    } catch {
      this.error.set('Failed to store pepper');
    } finally {
      this.loading.set(false);
    }
  }

  continueToApp(): void {
    this.core.connect();
    this.complete.emit();
  }

  async copyPepper(pepper: string): Promise<void> {
    try {
      await navigator.clipboard?.writeText(pepper);
      this.copied.set(true);
      setTimeout(() => this.copied.set(false), 2000);
    } catch {
      // Clipboard may be unavailable; the value is select-all-able in the block.
    }
  }
}
