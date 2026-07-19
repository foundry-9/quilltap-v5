import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router } from '@angular/router';

import { WORKSPACE_HANDLE, WORKSPACE_TAB_ID } from '../../../workspace/workspace-contract';
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
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly router = inject(Router);
  /** Workspace-tab seams (p4.9j2); null ⇒ routed mode. */
  private readonly handle = inject(WORKSPACE_HANDLE, { optional: true });
  private readonly tabId = inject(WORKSPACE_TAB_ID, { optional: true });
  private readonly queryParams = this.route
    ? toSignal(this.route.queryParamMap, { requireSync: true })
    : undefined;

  protected readonly mode = computed<'setup' | 'settings'>(() =>
    this.queryParams?.().get('mode') === 'setup' ? 'setup' : 'settings',
  );

  /**
   * Hosted (the `settings-wizard` tab, always the settings-mode re-entry) ⇒ close
   * the tab (v4 `useCloseSelfTab` — the provider wizard); routed ⇒ navigate.
   */
  private closeSelfTab(): boolean {
    if (this.handle && this.tabId != null) {
      this.handle.closeTab(this.tabId);
      return true;
    }
    return false;
  }

  protected onComplete(): void {
    if (this.closeSelfTab()) return;
    // Fresh-instance setup lands on the Salon (configured to the point of
    // chatting); settings re-entry returns to the Providers tab.
    void this.router.navigateByUrl(this.mode() === 'setup' ? '/salon' : '/settings?tab=providers');
  }

  protected onCancel(): void {
    if (this.closeSelfTab()) return;
    void this.router.navigateByUrl(this.mode() === 'setup' ? '/salon' : '/settings?tab=providers');
  }
}
