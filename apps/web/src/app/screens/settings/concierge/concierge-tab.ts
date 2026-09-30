import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute } from '@angular/router';

import { CollapsibleCard } from '../../../ui/collapsible-card';
import { ChatSettingsCard } from '../chat/chat-settings.api';
import { isConciergeOnDuty } from './concierge-settings.api';
import { DisplayCard } from './display-card';
import { OnDutyCard } from './on-duty-card';
import { PreScreeningCard } from './pre-screening-card';
import { RefusalsCard } from './refusals-card';
import { UncensoredDeskCard } from './uncensored-desk-card';

/**
 * The Concierge's own tab (`/settings?tab=concierge` — v4 `components/
 * settings/tabs/ConciergeTabContent.tsx`, `3b463d6b1`, subsystem `concierge`):
 * the on-duty switch, the uncensored desk, the refusal rule, display, and the
 * optional pre-screen. Every card saves through `ConciergeSettingsCard.update`
 * (the whole `conciergeSettings` object, deep-merged at send time).
 *
 * The intro is hard-coded (ruling E.6): v4 reads it from the Foundry subsystem
 * registry (`useSubsystemInfo('concierge').description`, theme-overridable),
 * which v5 does not have — every v5 tab hard-codes its intro, and this is that
 * registry's default string.
 *
 * `?section=` force-opens (and scrolls to) a card in routed mode only; hosted
 * as a workspace tab there is no `ActivatedRoute` and the Settings shell does
 * not thread its `section` (v4 parity, ruling E.9 — On Duty and the Desk are
 * open by default anyway).
 */
@Component({
  selector: 'qt-settings-concierge',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    CollapsibleCard,
    OnDutyCard,
    UncensoredDeskCard,
    RefusalsCard,
    DisplayCard,
    PreScreeningCard,
  ],
  template: `
    @if (loading()) {
      <div class="flex items-center justify-center py-8">
        <div class="qt-text-secondary">Loading settings...</div>
      </div>
    } @else if (!settings()) {
      <div class="qt-alert-error">Failed to load the Concierge's settings</div>
    } @else {
      <div>
        <p class="qt-text-small qt-text-muted italic mb-6">
          Who gets asked when the usual providers refuse, and how flagged content is shown
        </p>

        @if (!onDuty()) {
          <div class="qt-alert-warning mb-4">
            <p class="qt-text-small">
              The Concierge is off duty. Nothing is rerouted, announced, switched or screened until
              he is back at his post.
            </p>
          </div>
        }

        <div class="space-y-4">
          <qt-collapsible-card
            title="On Duty"
            description="Whether the Concierge is at his post at all, or has gone off to see a man about a dog."
            sectionId="on-duty"
            [defaultOpen]="true"
            [forceOpen]="section() === 'on-duty'"
          >
            <qt-concierge-on-duty-card />
          </qt-collapsible-card>

          <qt-collapsible-card
            title="The Uncensored Desk"
            description="The less squeamish parties the Concierge sends for when the usual providers clutch their pearls."
            sectionId="uncensored-desk"
            [defaultOpen]="true"
            [forceOpen]="section() === 'uncensored-desk'"
          >
            <qt-concierge-uncensored-desk-card />
          </qt-collapsible-card>

          <qt-collapsible-card
            title="When a Provider Refuses"
            description="How many polite refusals the Concierge endures before he moves a chat along, and how new chats begin."
            sectionId="refusals"
            [defaultOpen]="true"
            [forceOpen]="section() === 'refusals'"
          >
            <qt-concierge-refusals-card />
          </qt-collapsible-card>

          <qt-collapsible-card
            title="Display"
            description="Whether flagged content is laid out on the table, draped in gauze, or tucked discreetly behind a curtain."
            sectionId="display"
            [defaultOpen]="true"
            [forceOpen]="section() === 'display'"
          >
            <qt-concierge-display-card />
          </qt-collapsible-card>

          <qt-collapsible-card
            title="Pre-Screening (Advanced)"
            description="An optional doorman who reads every message before it goes in, at the price of a call apiece."
            sectionId="pre-screening"
            [forceOpen]="section() === 'pre-screening'"
          >
            <qt-concierge-pre-screening-card />
          </qt-collapsible-card>
        </div>
      </div>
    }
  `,
})
export class ConciergeTab extends ChatSettingsCard {
  // Optional so the tab renders when hosted as a workspace tab (no ActivatedRoute).
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly queryParams = this.route
    ? toSignal(this.route.queryParamMap, { requireSync: true })
    : undefined;

  protected readonly section = computed(() => this.queryParams?.().get('section') ?? null);

  /** v4 `concierge.enabled` — the shared on-duty derivation. */
  protected readonly onDuty = computed(() => isConciergeOnDuty(this.settings()));
}
