import { ChangeDetectionStrategy, Component, OnDestroy, OnInit, effect, inject, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import type { UnlockStateDto } from '../../../core/core-contract';
import { SystemSettingsSignals } from './system-settings-signals.service';

/** v4 `auto-lock-provider.tsx:60` — the sessionStorage key the unlock screen reads. */
const AUTOLOCK_RETURN_KEY = 'quilltap-autolock-return';

/** v4 `:124` — the DOM activity events, captured + passive. */
const ACTIVITY_EVENTS = ['mousemove', 'mousedown', 'keydown', 'scroll', 'touchstart'] as const;

const IDLE_CHECK_MS = 60_000; // v4 `:130,:145`
const ACTIVITY_THROTTLE_MS = 30_000; // v4 `:47-49`
const WARNING_AUTO_DISMISS_MS = 30_000; // v4 `:95-99`

interface AutoLockConfig {
  hasUserPassphrase: boolean;
  autoLockMinutes: number | null;
}

/**
 * The app-wide auto-lock provider (v4 `components/providers/auto-lock-provider.tsx`):
 * a headless component that watches DOM activity and locks Quilltap after the
 * configured idle period. It is the enforcement half v5 has never had — the
 * unlock screen (`screens/unlock/unlock.ts`) is already return-aware and was
 * only waiting for this timer.
 *
 * Mounted once in the shell (v4 mounts it in `app/layout.tsx`; the v5 shell
 * renders only when the vault is operational, so — like v4's `/setup` + `/unlock`
 * skip, `:107-112` — it never runs on the gate screens).
 *
 * The cadence is v4's exactly: fetch config from the unlock-state read, track
 * DOM activity throttled to once per 30 s, check idle every 60 s, warn one
 * minute out, and on timeout call the `lock` verb then hard-redirect to
 * `/unlock` (a full reboot: the health gate then shows the return-aware unlock
 * screen). Two pre-ruled divergences: the config comes from the `unlockState`
 * dispatch verb rather than a raw fetch. The warning banner is NOT a
 * lib/toast site at all: v4's `showWarningToast` here is a LOCAL `useCallback`
 * (`auto-lock-provider.tsx:76-100`, no `lib/toast` import — the P4.25 census's
 * name-grep matched it as a false positive) that renders its own
 * "Auto-Lock Warning" card with a Dismiss button and a 30 s auto-remove,
 * which this banner reproduces. Keeping it IS the faithful port.
 */
@Component({
  selector: 'qt-auto-lock-provider',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (showWarning()) {
      <div
        role="status"
        class="fixed bottom-4 right-4 z-[9999] qt-card p-4 qt-shadow-lg border qt-border max-w-sm"
      >
        <div class="flex items-start gap-3">
          <div class="flex-1">
            <p class="qt-text-small font-semibold">Auto-Lock Warning</p>
            <p class="qt-text-xs qt-text-muted mt-1">
              Quilltap will lock in approximately one minute due to inactivity. Move the mouse or
              press a key to remain.
            </p>
          </div>
          <button type="button" class="qt-button qt-button-ghost text-xs px-2 py-1" (click)="dismissWarning()">
            Dismiss
          </button>
        </div>
      </div>
    }
  `,
})
export class AutoLockProvider implements OnInit, OnDestroy {
  private readonly core = inject(CoreClient);
  private readonly signals = inject(SystemSettingsSignals);

  private config: AutoLockConfig | null = null;
  private lastActivity = 0;
  private warningShown = false;
  private locking = false;
  private throttleTimer: ReturnType<typeof setTimeout> | null = null;
  private warningDismissTimer: ReturnType<typeof setTimeout> | null = null;
  private intervalId: ReturnType<typeof setInterval> | null = null;

  protected readonly showWarning = signal(false);

  // Bound once so add/removeEventListener see the same reference.
  private readonly onActivity = (): void => this.handleActivity();

  constructor() {
    // v4 `:117-121` — re-fetch config when the Auto-Lock card saves.
    effect(() => {
      const n = this.signals.autoLockSettingsChanged();
      if (n > 0) {
        void this.fetchConfig();
      }
    });
  }

  ngOnInit(): void {
    this.lastActivity = Date.now();
    void this.fetchConfig();

    for (const event of ACTIVITY_EVENTS) {
      document.addEventListener(event, this.onActivity, { passive: true, capture: true });
    }

    this.intervalId = setInterval(() => this.checkIdle(), IDLE_CHECK_MS);
  }

  ngOnDestroy(): void {
    for (const event of ACTIVITY_EVENTS) {
      document.removeEventListener(event, this.onActivity, { capture: true });
    }
    if (this.intervalId !== null) clearInterval(this.intervalId);
    if (this.throttleTimer !== null) clearTimeout(this.throttleTimer);
    if (this.warningDismissTimer !== null) clearTimeout(this.warningDismissTimer);
  }

  /** v4 `fetchConfig` (`:26-40`). Silently ignores failure — retries on next change. */
  private async fetchConfig(): Promise<void> {
    try {
      const resp = await this.core.dispatchExpect({ type: 'unlockState' }, 'unlockState');
      const data: UnlockStateDto = resp.data;
      this.config = {
        hasUserPassphrase: data.hasUserPassphrase ?? false,
        autoLockMinutes: data.autoLockMinutes ?? null,
      };
      // v4 `:35-36` — reset the warning when config changes.
      this.warningShown = false;
      this.hideWarning();
    } catch {
      /* silently ignore — will retry on the next settings-change signal */
    }
  }

  /** v4 `handleActivity` (`:42-50`) — throttle to at most once per 30 s. */
  private handleActivity(): void {
    if (this.throttleTimer !== null) return;
    this.lastActivity = Date.now();
    this.warningShown = false;
    this.hideWarning();
    this.throttleTimer = setTimeout(() => {
      this.throttleTimer = null;
    }, ACTIVITY_THROTTLE_MS);
  }

  /** v4's 60 s interval body (`:130-145`). */
  private checkIdle(): void {
    const config = this.config;
    if (!config || !config.hasUserPassphrase || config.autoLockMinutes === null) {
      return;
    }
    const idleMs = Date.now() - this.lastActivity;
    const lockThresholdMs = config.autoLockMinutes * 60_000;
    const warningThresholdMs = Math.max(0, (config.autoLockMinutes - 1) * 60_000);

    if (idleMs >= lockThresholdMs) {
      void this.triggerLock();
    } else if (idleMs >= warningThresholdMs && !this.warningShown) {
      this.showWarningBanner();
    }
  }

  /** v4 `triggerLock` (`:52-74`). */
  private async triggerLock(): Promise<void> {
    if (this.locking) return;
    this.locking = true;

    // v4 `:59-60` — save the current location for the return-aware unlock screen.
    try {
      const returnPath = window.location.pathname + window.location.search;
      sessionStorage.setItem(AUTOLOCK_RETURN_KEY, returnPath);
    } catch {
      /* sessionStorage unavailable — the lock still proceeds */
    }

    try {
      await this.core.dispatch({ type: 'lock' });
    } catch {
      /* the redirect still reboots into the locked state */
    }

    // v4 `:73` — a full reload to /unlock resets all client state.
    window.location.href = '/unlock';
  }

  /** v4 `showWarningToast` (`:76-100`) — one banner, auto-dismissed after 30 s. */
  private showWarningBanner(): void {
    if (this.warningShown) return;
    this.warningShown = true;
    this.showWarning.set(true);
    this.warningDismissTimer = setTimeout(() => this.hideWarning(), WARNING_AUTO_DISMISS_MS);
  }

  protected dismissWarning(): void {
    this.hideWarning();
  }

  private hideWarning(): void {
    this.showWarning.set(false);
    if (this.warningDismissTimer !== null) {
      clearTimeout(this.warningDismissTimer);
      this.warningDismissTimer = null;
    }
  }
}
