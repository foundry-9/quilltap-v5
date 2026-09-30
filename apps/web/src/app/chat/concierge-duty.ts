import type { ChatSettingsDto } from '../core/core-contract';
import {
  DEFAULT_CONCIERGE_SETTINGS,
  type ConciergeSettings,
} from '../screens/settings/chat/chat-settings.types';
import type { ConciergeState } from './concierge-state';

/**
 * The Concierge's on-duty derivation — a PURE home in the chat tree, so the
 * sidebar, the New Chat form and the Settings tab read one rule without the chat
 * tree importing a settings card class.
 *
 * @module chat/concierge-duty
 */

/**
 * v4 `ConciergeTabContent`'s effective object (and the server's
 * `readConciergeSettings`): the defaults, overlaid by whatever is stored, with
 * `display` and `preScreen` merged one level down so a stored object that
 * predates a field still reads that field's default.
 */
export function effectiveConcierge(
  stored: Partial<ConciergeSettings> | null | undefined,
): ConciergeSettings {
  return {
    ...DEFAULT_CONCIERGE_SETTINGS,
    ...stored,
    display: { ...DEFAULT_CONCIERGE_SETTINGS.display, ...stored?.display },
    preScreen: { ...DEFAULT_CONCIERGE_SETTINGS.preScreen, ...stored?.preScreen },
  };
}

/**
 * Is the Concierge at his post? The ONE on-duty derivation — the Settings tab,
 * the Salon sidebar and the New Chat form all read it. v4 spells the same rule
 * inline at two sites (`ChatSidebar.tsx:965`, `useNewChat.ts:436` at
 * `97b25fc53`): `settings.conciergeSettings?.enabled !== false`, with the query's
 * `data = true` default — so TRUE while the settings are loading, missing or
 * failed, and whenever the key is absent; only an explicit `enabled: false`
 * takes him off duty. Read through {@link effectiveConcierge} so a stored object
 * predating the field gets the default (`true`).
 */
export function isConciergeOnDuty(settings: ChatSettingsDto | null | undefined): boolean {
  return effectiveConcierge(settings?.conciergeSettings).enabled !== false;
}

/**
 * The state a new chat starts in when the form says nothing (v4 `useNewChat`
 * at `3b463d6b1`): `newChatsStartAs` while on duty — `'unmoderated'` only when
 * it says so exactly — else `'moderated'`. "Off duty the server ignores
 * `newChatsStartAs` and every chat is created Moderated."
 */
export function conciergeNewChatDefault(
  settings: ChatSettingsDto | null | undefined,
): ConciergeState {
  return isConciergeOnDuty(settings) &&
    settings?.conciergeSettings?.newChatsStartAs === 'unmoderated'
    ? 'unmoderated'
    : 'moderated';
}
