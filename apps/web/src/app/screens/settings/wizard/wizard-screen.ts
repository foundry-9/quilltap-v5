import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router } from '@angular/router';

import { ProviderWizard } from './provider-wizard';

/**
 * The `/settings/wizard` route (v4 `app/settings/wizard/` + `app/setup/providers/`).
 * `?mode=setup` runs the fresh-instance flow (complete → the Salon); the default
 * `settings` mode re-enters from the Providers tab (complete/cancel → Settings).
 */
@Component({
  selector: 'qt-wizard-screen',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ProviderWizard],
  template: `
    <qt-provider-wizard [mode]="mode()" (complete)="onComplete()" (cancel)="onCancel()" />
  `,
})
export class WizardScreen {
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly queryParams = toSignal(this.route.queryParamMap, { requireSync: true });

  protected readonly mode = computed<'setup' | 'settings'>(() =>
    this.queryParams().get('mode') === 'setup' ? 'setup' : 'settings',
  );

  protected onComplete(): void {
    // Fresh-instance setup lands on the Salon (configured to the point of
    // chatting); settings re-entry returns to the Providers tab.
    void this.router.navigateByUrl(this.mode() === 'setup' ? '/salon' : '/settings?tab=providers');
  }

  protected onCancel(): void {
    void this.router.navigateByUrl(this.mode() === 'setup' ? '/salon' : '/settings?tab=providers');
  }
}
