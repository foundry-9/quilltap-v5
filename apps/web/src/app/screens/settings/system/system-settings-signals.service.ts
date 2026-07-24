import { Injectable, signal } from '@angular/core';

/**
 * The v5 replacement for v4's two `window` CustomEvents in the Data & System tab
 * (v4 `ChangePassphraseCard.tsx:53` dispatches `quilltap-passphrase-changed`;
 * `AutoLockSettingsCard.tsx:92` dispatches `quilltap-autolock-settings-changed`;
 * `auto-lock-provider.tsx:121` listens for the latter). A root singleton with
 * two bump-counter signals: an emitter increments, consumers react through an
 * `effect`. Same fan-out semantics, no global `window` bus.
 *
 * The counters carry no payload — a change is a *nudge to re-read*, exactly as
 * the v4 events are (they carry no `detail`).
 */
@Injectable({ providedIn: 'root' })
export class SystemSettingsSignals {
  /**
   * Bumped by the Encryption Passphrase card after a successful change (v4
   * `quilltap-passphrase-changed`). The Auto-Lock card re-reads the unlock
   * state so its passphrase gate reflects a just-set or just-removed passphrase.
   */
  readonly passphraseChanged = signal(0);

  /**
   * Bumped by the Auto-Lock card after a successful save (v4
   * `quilltap-autolock-settings-changed`). The app-wide auto-lock provider
   * re-fetches its idle config so a new interval takes effect without a reload.
   */
  readonly autoLockSettingsChanged = signal(0);

  notifyPassphraseChanged(): void {
    this.passphraseChanged.update((n) => n + 1);
  }

  notifyAutoLockSettingsChanged(): void {
    this.autoLockSettingsChanged.update((n) => n + 1);
  }
}
