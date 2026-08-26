import { ChangeDetectionStrategy, Component, effect, inject, OnInit } from '@angular/core';

import { CoreClient } from './core/core-client';
import { RealtimeService } from './core/realtime.service';
import { SetupWizard } from './screens/setup/setup-wizard';
import { StartupScreen } from './screens/startup/startup-screen';
import { Unlock } from './screens/unlock/unlock';
import { Shell } from './shell/shell';
import { StartupService } from './startup/startup.service';
import { ThemeService } from './theme/theme.service';
import { ToastContainer } from './ui/toast-container';

/**
 * The application root + startup gate (v4 `PepperVaultGate` + the health gates).
 * On boot it reads `GET /health` and routes by state: operational → the shell;
 * needs-setup → the setup wizard; needs-passphrase → the unlock screen;
 * lock-conflict / unhealthy → the v4-voiced error screens. When operational it
 * opens the single global SSE stream.
 */
@Component({
  selector: 'app-root',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [StartupScreen, SetupWizard, Unlock, Shell, ToastContainer],
  template: `
    <!-- v4 mounts its toast container on document.body, so it is available on
         every screen INCLUDING the pre-unlock gates; this sits outside the
         switch for the same reason. -->
    <qt-toast-container />
    @switch (state().kind) {
      @case ('operational') {
        <qt-shell />
      }
      @case ('needs-setup') {
        <qt-setup-wizard (complete)="onResolved()" />
      }
      @case ('needs-passphrase') {
        <qt-unlock (unlocked)="onResolved()" />
      }
      @default {
        <qt-startup-screen [state]="state()" />
      }
    }
  `,
})
export class App implements OnInit {
  private readonly startup = inject(StartupService);
  private readonly core = inject(CoreClient);

  protected readonly state = this.startup.state;

  constructor() {
    // Constructing the theme service HERE stamps `.dark`/`.light` +
    // `data-theme` on <html> before any gate screen paints (v4's
    // ThemeProvider wraps every page, unlock included — dogfood finding #15:
    // with no theme class the auth screens render light-mode text on the
    // hard-coded dark auth backdrop and the card variables don't resolve at
    // all). The Shell still owns the server-preference reload; construction
    // only applies localStorage + the system preference, which is exactly
    // v4's pre-auth behavior.
    inject(ThemeService);
    // Wire the realtime hub (v4 mounts `RealtimeProvider` inside its
    // `QueryProvider`). It is a root service, so nothing constructs it on its
    // own; injecting it here is what starts the frame subscription and the
    // reconnect catch-up sweep. It must exist BEFORE the stream opens below, or
    // the first frames land with nobody listening.
    inject(RealtimeService);
    // Open the one global event stream as soon as the vault is operational.
    effect(() => {
      if (this.startup.state().kind === 'operational') {
        this.core.connect();
      }
    });
  }

  ngOnInit(): void {
    void this.startup.check();
  }

  protected onResolved(): void {
    void this.startup.recheck();
  }
}
