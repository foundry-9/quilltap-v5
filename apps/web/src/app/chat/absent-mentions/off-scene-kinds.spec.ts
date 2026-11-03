/** v4 `off-scene-kinds.ts:32-40` at `236c04e35` — the three rows of P4.D347's `is_record_only_message`. */
import { describe, expect, it } from 'vitest';

import { isOffSceneMentionIntroduction } from './off-scene-kinds';

describe('isOffSceneMentionIntroduction', () => {
  it('a Host spoken-of introduction is a record', () => {
    expect(
      isOffSceneMentionIntroduction({
        systemSender: 'host',
        systemKind: 'off-scene-characters',
        content: 'Abigail is spoken of.',
      }),
    ).toBe(true);
  });

  it('a legacy left-behind notice (the prefix) is not', () => {
    expect(
      isOffSceneMentionIntroduction({
        systemSender: 'host',
        systemKind: 'off-scene-characters',
        content: 'The Host observes that Abigail stayed behind.',
      }),
    ).toBe(false);
  });

  it('the left-behind kind, another sender, or no content are not', () => {
    expect(
      isOffSceneMentionIntroduction({
        systemSender: 'host',
        systemKind: 'off-scene-left-behind',
        content: 'x',
      }),
    ).toBe(false);
    expect(
      isOffSceneMentionIntroduction({
        systemSender: 'lantern',
        systemKind: 'off-scene-characters',
        content: 'x',
      }),
    ).toBe(false);
    expect(
      isOffSceneMentionIntroduction({ systemSender: 'host', systemKind: 'off-scene-characters' }),
    ).toBe(true);
  });
});
