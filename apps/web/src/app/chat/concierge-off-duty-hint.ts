import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { RouterLink } from '@angular/router';

import type { ConciergeState } from './concierge-state';

/**
 * Where the off-duty hint sends the reader (v4 `ConciergeOffDutyHint.tsx`,
 * new at `3b463d6b1` #76). The `section` deep link force-opens the On Duty
 * card in ROUTED mode only — v5's Settings shell, like v4's, does not thread
 * `?section=` when hosted as a workspace tab (parity, survey §E.9); On Duty is
 * `defaultOpen` either way.
 */
export const CONCIERGE_OFF_DUTY_SETTINGS_URL = '/settings?tab=concierge&section=on-duty';

/**
 * The one sentence shown beneath a per-chat Concierge select when the
 * Concierge is off duty globally (`conciergeSettings.enabled === false`). The
 * select is disabled rather than hidden, so whoever finds it learns where the
 * switch lives. Shared by the Salon sidebar and the New Chat form (v4 has the
 * same two consumers).
 *
 * Host `display: contents`, so the caller's `class` input lands on the one
 * `<span>` v4 renders and nothing sits between it and its flex/block parent.
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

/**
 * The two `conciergeSettings` fields the Salon side reads (round shared
 * contract §S.2; v4 `3b463d6b1`). P4.D230 lands the full `ChatSettingsDto`
 * shape; this lane reads only `enabled` and `newChatsStartAs`, so it takes the
 * settings row as an opaque object rather than depending on that hunk.
 */
interface ConciergeDutyView {
  conciergeSettings?: {
    enabled?: boolean | null;
    newChatsStartAs?: string | null;
  } | null;
}

/**
 * Is the Concierge at his post? v4 `useChatSettingsQuery((s) =>
 * s.conciergeSettings?.enabled !== false)` with `data = true` as the default:
 * TRUE while the settings are loading, missing or failed, and whenever the
 * key is absent — only an explicit `enabled: false` takes him off duty.
 */
export function isConciergeOnDuty(settings: object | null | undefined): boolean {
  return (settings as ConciergeDutyView | null | undefined)?.conciergeSettings?.enabled !== false;
}

/**
 * The state a new chat starts in when the form says nothing (v4 `useNewChat`
 * at `3b463d6b1`): `newChatsStartAs` while on duty — `'unmoderated'` only when
 * it says so exactly — else `'moderated'`. "Off duty the server ignores
 * `newChatsStartAs` and every chat is created Moderated."
 */
export function conciergeNewChatDefault(settings: object | null | undefined): ConciergeState {
  const onDuty = isConciergeOnDuty(settings);
  return onDuty &&
    (settings as ConciergeDutyView | null | undefined)?.conciergeSettings?.newChatsStartAs ===
      'unmoderated'
    ? 'unmoderated'
    : 'moderated';
}
