import { ChangeDetectionStrategy, Component } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ConciergeSettingsCard } from './concierge-settings.api';

/**
 * The Concierge's master switch (v4 `components/settings/concierge-settings/
 * OnDutyCard.tsx`, `3b463d6b1`). Off means he does nothing at all: no
 * failover, no announcements, no auto-switch, no pre-screen — and every
 * chat's Moderated / Unmoderated / Locked select is disabled until he
 * returns. PUT key: `conciergeSettings.enabled`. Copy verbatim.
 */
@Component({
  selector: 'qt-concierge-on-duty-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert],
  template: `
    <div class="space-y-3">
      @if (saveError(); as msg) {
        <qt-error-alert [message]="msg" class="mb-3" />
      }
      <div>
        <label class="qt-settings-toggle-row">
          <input
            type="checkbox"
            class="qt-checkbox mt-1"
            [checked]="concierge().enabled"
            [disabled]="saving()"
            (change)="update({ enabled: $any($event.target).checked })"
          />
          <div class="flex-1">
            <div class="qt-settings-section-heading">The Concierge is on duty</div>
            <div class="qt-text-small mt-1">
              When a provider declines a Moderated chat, the Concierge carries the request to the
              uncensored desk, says so in the chat, and may move the chat to Unmoderated after
              repeated refusals. Turn him off and nothing is rerouted, announced, switched or
              screened; every chat is answered by its own provider alone, and the per-chat Concierge
              select is disabled.
            </div>
          </div>
        </label>
      </div>
    </div>
  `,
})
export class OnDutyCard extends ConciergeSettingsCard {}
