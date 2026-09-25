import { describe, expect, it } from 'vitest';

import {
  describeRetryRefusal,
  isLanternBackgroundRefusal,
  retryUncensoredTurnRequest,
} from './concierge-retry';
import V4 from './concierge-retry.v4.json';

/**
 * "Try uncensored"'s pure helpers (v4 `app/salon/[id]/concierge-retry.ts` at
 * `ce2f1dabf`, #77).
 *
 *  1. v4's `__tests__/unit/app/salon/concierge-retry.test.ts`, by name.
 *  2. The executed-v4 oracle — `concierge-retry.v4.json`, emitted by RUNNING
 *     v4's real module at the round-target pin
 *     (`harness/oracle/cases/concierge-retry.mjs`):
 *
 * ```bash
 * export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
 * QT_V4_PIN=acadcc7cd node ~/source/quilltap-v5/harness/oracle/cases/concierge-retry.mjs \
 *   > ~/source/quilltap-v5/apps/web/src/app/chat/concierge-retry.v4.json
 * ```
 */

describe('concierge-retry helpers', () => {
  // v4 "builds the narrated retry URL". v5 has no URL — §S.3's dispatch verb
  // carries the message id and the stream flag; the chat id rides the message.
  it('builds the narrated retry request', () => {
    expect(retryUncensoredTurnRequest('m1')).toEqual({
      type: 'messageRetryUncensored',
      messageId: 'm1',
      stream: true,
    });
  });

  it('words the two refusals and nothing else', () => {
    expect(describeRetryRefusal('no-understudy')).toBe(
      'There is no uncensored desk to send this to — appoint one under Settings → The Concierge.',
    );
    expect(describeRetryRefusal('locked')).toMatch(/Locked/);
    expect(describeRetryRefusal('something else')).toBeNull();
    expect(describeRetryRefusal(undefined)).toBeNull();
  });

  it("recognises the Lantern's refused backdrop", () => {
    expect(isLanternBackgroundRefusal({ systemSender: 'lantern', systemKind: 'background-refused' })).toBe(true);
    expect(isLanternBackgroundRefusal({ systemSender: 'lantern', systemKind: 'background' })).toBe(false);
    expect(isLanternBackgroundRefusal({ systemSender: 'concierge', systemKind: 'background-refused' })).toBe(
      false,
    );
  });
});

describe('concierge-retry against v4’s own module (executed at acadcc7cd)', () => {
  it('was emitted from the round-target pin, and carries the whole corpus', () => {
    expect(V4._source.pin).toBe('acadcc7cd');
    expect(V4.refusal).toHaveLength(12);
    expect(V4.lantern).toHaveLength(20);
  });

  it('maps v4’s URL ids one-to-one onto the request (the transport divergence, recorded)', () => {
    // v4: `/api/v1/chats/${chatId}/messages/${messageId}?action=retry-uncensored&stream=1`.
    expect(V4.url.out).toBe(
      `/api/v1/chats/${V4.url.chatId}/messages/${V4.url.messageId}?action=retry-uncensored&stream=1`,
    );
    expect(retryUncensoredTurnRequest(V4.url.messageId)).toMatchObject({
      messageId: V4.url.messageId,
      stream: true,
    });
  });

  it.each(V4.refusal.map((r, n) => ({ ...r, n })))('describeRetryRefusal #$n', (row) => {
    const input = 'absent' in row && row.absent ? undefined : (row as { error?: unknown }).error;
    expect(describeRetryRefusal(input)).toBe(row.out);
  });

  it.each(V4.lantern)('isLanternBackgroundRefusal($systemSender, $systemKind)', (row) => {
    expect(isLanternBackgroundRefusal(row as never)).toBe(row.out);
  });
});
