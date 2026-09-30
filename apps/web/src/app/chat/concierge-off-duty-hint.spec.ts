import { describe, expect, it } from 'vitest';

import type { ChatSettingsDto } from '../core/core-contract';
import { isConciergeOnDuty } from '../screens/settings/concierge/concierge-settings.api';
import { conciergeNewChatDefault } from './concierge-off-duty-hint';

/**
 * The on-duty rule and the new-chat default (v4 `ChatSidebar.tsx:965` and
 * `useNewChat.ts:436-440` at `97b25fc53`): `conciergeSettings?.enabled !== false`
 * with the query's `data = true` default — only an explicit `false` takes the
 * Concierge off duty. `isConciergeOnDuty` has ONE home
 * (`concierge-settings.api.ts`), shared by the Settings tab, the sidebar and the
 * New Chat form; this spec pins the rule and that the default reads it.
 */

const settings = (concierge: Record<string, unknown> | undefined): ChatSettingsDto =>
  ({ conciergeSettings: concierge }) as unknown as ChatSettingsDto;

describe('isConciergeOnDuty', () => {
  it('is TRUE while the settings are loading, missing or failed', () => {
    expect(isConciergeOnDuty(undefined)).toBe(true);
    expect(isConciergeOnDuty(null)).toBe(true);
  });

  it('is TRUE when the key (or the whole bag) is absent', () => {
    expect(isConciergeOnDuty(settings(undefined))).toBe(true);
    expect(isConciergeOnDuty(settings({}))).toBe(true);
    expect(isConciergeOnDuty(settings({ enabled: undefined }))).toBe(true);
  });

  it('is FALSE only for an explicit `enabled: false`', () => {
    expect(isConciergeOnDuty(settings({ enabled: false }))).toBe(false);
    expect(isConciergeOnDuty(settings({ enabled: true }))).toBe(true);
  });
});

describe('conciergeNewChatDefault', () => {
  it("is 'unmoderated' only on duty AND only when the setting says so exactly", () => {
    expect(conciergeNewChatDefault(settings({ enabled: true, newChatsStartAs: 'unmoderated' }))).toBe(
      'unmoderated',
    );
    expect(conciergeNewChatDefault(settings({ newChatsStartAs: 'unmoderated' }))).toBe('unmoderated');
    expect(conciergeNewChatDefault(settings({ enabled: true, newChatsStartAs: 'locked' }))).toBe(
      'moderated',
    );
  });

  it("is 'moderated' off duty whatever newChatsStartAs says, and for missing settings", () => {
    expect(conciergeNewChatDefault(settings({ enabled: false, newChatsStartAs: 'unmoderated' }))).toBe(
      'moderated',
    );
    expect(conciergeNewChatDefault(undefined)).toBe('moderated');
    expect(conciergeNewChatDefault(null)).toBe('moderated');
  });
});
