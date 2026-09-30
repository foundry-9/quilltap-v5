import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { RouterLink } from '@angular/router';

/**
 * The one sentence shown beneath a per-chat Concierge select when the
 * Concierge is off duty globally (`conciergeSettings.enabled === false`). The
 * select is disabled rather than hidden, so whoever finds it learns where the
 * switch lives. Shared by the Salon sidebar and the New Chat form (v4 has the
 * same two consumers).
 *
 * Host `display: contents`, so the caller's `class` input lands on the one
 * `<span>` v4 renders and nothing sits between it and its flex/block parent.
 * The link's deep link (`?tab=concierge&section=on-duty`) force-opens the On Duty
 * card in ROUTED mode only — v5's Settings shell, like v4's, does not thread
 * `?section=` when hosted as a workspace tab (parity, survey §E.9); On Duty is
 * `defaultOpen` either way.
 */
@Component({
  selector: 'qt-concierge-off-duty-hint',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink],
  host: { style: 'display: contents' },
  template: `<span [class]="className()"
    >The Concierge is off duty — turn him on in
    <a
      routerLink="/settings"
      [queryParams]="{ tab: 'concierge', section: 'on-duty' }"
      class="qt-link"
      >Settings → The Concierge</a
    >.</span
  >`,
})
export class ConciergeOffDutyHint {
  readonly className = input('');
}
