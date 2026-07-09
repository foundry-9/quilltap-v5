import { ChangeDetectionStrategy, Component, effect, inject, OnInit } from '@angular/core';

import { CoreClient } from './core/core-client';
import { SetupWizard } from './screens/setup/setup-wizard';
import { StartupScreen } from './screens/startup/startup-screen';
import { Unlock } from './screens/unlock/unlock';
import { Shell } from './shell/shell';
import { StartupService } from './startup/startup.service';

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
  imports: [StartupScreen, SetupWizard, Unlock, Shell],
  template: `
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
