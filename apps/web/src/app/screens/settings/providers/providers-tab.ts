import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, RouterLink } from '@angular/router';

import { CollapsibleCard } from '../../../ui/collapsible-card';
import { Icon } from '../../../ui/icon';
import { ApiKeysCard } from './api-keys-card';
import { CheapLlmCard } from './cheap-llm-card';
import { ConnectionProfilesCard } from './connection-profiles-card';

/**
 * The AI Providers tab (v4 `components/settings/tabs/ProvidersTabContent.tsx`):
 * the wizard link card plus the API Keys, Connection Profiles, and Cheap LLM
 * collapsible cards. The `?section=` deep-link force-opens the matching card. The
 * Capabilities Report card is a named deferral.
 */
@Component({
  selector: 'qt-settings-providers',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon, CollapsibleCard, ApiKeysCard, ConnectionProfilesCard, CheapLlmCard],
  template: `
    <div>
      <p class="qt-text-small qt-text-muted italic mb-6">
        The Forge — where you kindle connections to the great language engines of the age.
      </p>

      <div class="space-y-4">
        <a
          routerLink="/settings/wizard"
          class="qt-card block p-4 hover:qt-bg-muted transition-colors"
        >
          <div class="flex items-center justify-between">
            <div>
              <h3 class="font-medium">AI Stack Setup Wizard</h3>
              <p class="qt-text-small qt-text-muted mt-1">
                Re-run the guided setup to configure providers, API keys, models, and profiles all
                in one go.
              </p>
            </div>
            <qt-icon name="chevron-right" class="w-5 h-5 qt-text-muted flex-shrink-0" />
          </div>
        </a>

        <qt-collapsible-card
          title="API Keys"
          description="Manage API keys for LLM providers"
          sectionId="api-keys"
          [forceOpen]="section() === 'api-keys'"
        >
          <qt-api-keys-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Connection Profiles"
          description="Configure LLM connection profiles"
          sectionId="connection-profiles"
          [forceOpen]="section() === 'connection-profiles'"
        >
          <qt-connection-profiles-card />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Cheap LLM Settings"
          description="Configure the lightweight LLM used for background tasks"
          sectionId="cheap-llm"
          [forceOpen]="section() === 'cheap-llm'"
        >
          <qt-cheap-llm-card />
        </qt-collapsible-card>
      </div>
    </div>
  `,
})
export class ProvidersTab {
  private readonly route = inject(ActivatedRoute);
  private readonly queryParams = toSignal(this.route.queryParamMap, { requireSync: true });

  protected readonly section = computed(() => this.queryParams().get('section'));
}
