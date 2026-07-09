import { Injectable, inject, signal } from '@angular/core';

import { CoreClient } from '../core/core-client';

/**
 * The startup-gate state machine (v4 `PepperVaultGate` + the health gates,
 * collapsed to the states v5 `/health` exposes). One `GET /health` on boot maps
 * to a screen: operational → shell; needs-setup → wizard; needs-passphrase →
 * unlock; lock-conflict / unhealthy → the v4-voiced error screens (the latter
 * retries while the server is still booting).
 */
export type StartupState =
  | { kind: 'loading' }
  | { kind: 'operational' }
  | { kind: 'needs-setup' }
  | { kind: 'needs-passphrase' }
  | { kind: 'lock-conflict'; detail: unknown }
  | { kind: 'unhealthy'; message: string };

/** Poll cadence while the server is still booting (v4 `scheduleRetry` = 2 s). */
const RETRY_MS = 2000;

@Injectable({ providedIn: 'root' })
export class StartupService {
  private readonly core = inject(CoreClient);
  private readonly _state = signal<StartupState>({ kind: 'loading' });
  private retryTimer: ReturnType<typeof setTimeout> | null = null;

  readonly state = this._state.asReadonly();

  /** Kick off (or re-run) the readiness check. Idempotent to call repeatedly. */
  async check(): Promise<void> {
    if (this.retryTimer) {
      clearTimeout(this.retryTimer);
      this.retryTimer = null;
    }
    const health = await this.core.fetchHealth();
    switch (health.kind) {
      case 'healthy':
        this._state.set({ kind: 'operational' });
        return;
      case 'locked':
        this._state.set(
          health.pepperState === 'needs-setup' ? { kind: 'needs-setup' } : { kind: 'needs-passphrase' },
        );
        return;
      case 'lock-conflict':
        this._state.set({ kind: 'lock-conflict', detail: health.lockConflict });
        return;
      case 'unhealthy':
      case 'unreachable':
        // The server is not ready (still booting) — surface + retry.
        this._state.set({ kind: 'unhealthy', message: health.message });
        this.scheduleRetry();
        return;
    }
  }

  /** Re-check after an unlock / setup completes (flips the gate to the shell). */
  recheck(): Promise<void> {
    return this.check();
  }

  private scheduleRetry(): void {
    if (typeof setTimeout === 'undefined') {
      return;
    }
    this.retryTimer = setTimeout(() => void this.check(), RETRY_MS);
  }
}
