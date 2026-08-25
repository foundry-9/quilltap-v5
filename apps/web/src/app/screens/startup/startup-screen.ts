import { ChangeDetectionStrategy, Component, input } from '@angular/core';

import type { StartupState } from '../../startup/startup.service';

/**
 * The pre-shell startup / error screens (v4 `StartupProgress` +
 * `InstanceLockGate`). Renders the `loading` / `unhealthy` "starting up" card
 * (the server is still booting; the gate retries) and the 409 `lock-conflict`
 * "Database In Use" overlay. Copy is verbatim from v4.
 */
@Component({
  selector: 'qt-startup-screen',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @switch (state().kind) {
      @case ('lock-conflict') {
        <div
          class="fixed inset-0 z-[9999] flex items-center justify-center p-4 qt-pretheme-bg"
          role="alertdialog"
          aria-labelledby="qt-lock-title"
        >
          <div class="qt-card max-w-xl w-full p-6 space-y-4">
            <div class="text-4xl" aria-hidden="true">&#x1F512;</div>
            <h1 id="qt-lock-title" class="qt-heading-lg">Database In Use</h1>
            <p class="qt-text-secondary">
              Another Quilltap instance is already using this database. Running two instances against the same data can
              cause corruption.
            </p>
            @if (reason()) {
              <div class="qt-bg-surface-alt p-3 rounded-lg text-sm">
                <span class="qt-text-secondary">Held by:</span> <span class="qt-code">{{ reason() }}</span>
              </div>
            }
            <div class="space-y-2">
              <p class="qt-text-secondary font-semibold">To resolve this:</p>
              <ol class="list-decimal list-inside space-y-2 qt-text-secondary text-sm">
                <li>
                  Stop the other Quilltap instance, or close the other browser tab if you simply have one open
                  elsewhere.
                </li>
                <li>
                  If the other process is gone but left a stale lock, run:
                  <code class="qt-code block mt-1">npx quilltap db --lock-clean</code>
                </li>
                <li>
                  If you need to force access (use with caution):
                  <code class="qt-code block mt-1">npx quilltap db --lock-override</code>
                </li>
              </ol>
            </div>
            <p class="qt-text-xs qt-text-muted">This page will dismiss automatically when the conflict resolves.</p>
          </div>
        </div>
      }
      @default {
        <!-- loading / unhealthy: the server is still booting; the gate retries. -->
        <div class="qt-pretheme-bg flex items-center justify-center min-h-screen p-4">
          <div class="qt-card max-w-md w-full p-6 space-y-3 text-center">
            <p class="qt-text-secondary text-xs uppercase tracking-wide">Quilltap is starting up</p>
            <h1 class="qt-heading-2 qt-text-primary">Just getting our bearings</h1>
            <div class="flex justify-center py-2"><div class="qt-spinner text-primary"></div></div>
            @if (message()) {
              <p class="qt-text-secondary text-sm">{{ message() }}</p>
            }
          </div>
        </div>
      }
    }
  `,
})
export class StartupScreen {
  readonly state = input.required<StartupState>();

  protected message(): string | null {
    const s = this.state();
    return s.kind === 'unhealthy' ? s.message : null;
  }

  protected reason(): string | null {
    const s = this.state();
    if (s.kind !== 'lock-conflict') {
      return null;
    }
    const detail = s.detail as { reason?: string } | null;
    return detail?.reason ?? null;
  }
}
