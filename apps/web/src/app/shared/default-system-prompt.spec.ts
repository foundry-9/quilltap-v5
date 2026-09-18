/**
 * The parity spec for the client twin of v4 `lib/characters/
 * default-system-prompt.ts`.
 *
 * **Oracle:** v4's own suite, `__tests__/unit/lib/characters/
 * default-system-prompt.test.ts` at `baa85e19b` — its five cases are
 * transcribed verbatim below (names, fixtures and expectations), the
 * `carina-parser.ts` / `format-date.ts` precedent for a client-safe twin the
 * SPA has no jest venue to run v4's suite against.
 *
 * The three cases under "the arms v4's suite leaves unstated" are the round's
 * §S.2 additions, carried IDENTICALLY by the server twin (P4.D201): they pin
 * the JS-truthiness reading of an empty-string column, the FIRST of two
 * flagged prompts, and the `prompts[0]` fall-through when no flag exists and
 * the column is stale.
 */

import { describe, expect, it } from 'vitest';

import {
  resolveDefaultSystemPrompt,
  resolveDefaultSystemPromptId,
} from './default-system-prompt';

// v4's two fixtures, verbatim.
const main = { id: 'main', isDefault: true, content: 'the everyday voice' };
const frontLine = { id: 'front-line', isDefault: false, content: 'the fighting has started' };

describe('resolveDefaultSystemPrompt (v4 baa85e19b, its five vectors verbatim)', () => {
  it('prefers the column when it names a prompt the character has', () => {
    const resolved = resolveDefaultSystemPrompt({
      systemPrompts: [main, frontLine],
      defaultSystemPromptId: 'front-line',
    });
    expect(resolved?.id).toBe('front-line');
  });

  it('falls through to the isDefault flag when the column is null', () => {
    expect(
      resolveDefaultSystemPromptId({
        systemPrompts: [frontLine, main],
        defaultSystemPromptId: null,
      }),
    ).toBe('main');
  });

  it('falls through to the flag when the column names a prompt that is gone', () => {
    expect(
      resolveDefaultSystemPromptId({
        systemPrompts: [main, frontLine],
        defaultSystemPromptId: 'deleted-long-ago',
      }),
    ).toBe('main');
  });

  it('falls through to the first prompt when nothing is marked default', () => {
    expect(
      resolveDefaultSystemPromptId({
        systemPrompts: [frontLine, { ...main, isDefault: false }],
      }),
    ).toBe('front-line');
  });

  it('answers null for a character with no prompts at all', () => {
    expect(resolveDefaultSystemPrompt({ systemPrompts: [] })).toBeNull();
    expect(resolveDefaultSystemPromptId({})).toBeNull();
    expect(
      resolveDefaultSystemPromptId({ systemPrompts: null, defaultSystemPromptId: 'main' }),
    ).toBeNull();
  });
});

describe('the arms v4’s suite leaves unstated (§S.2, carried by both twins)', () => {
  it('an EMPTY-STRING column is falsy, so it falls through to the flag', () => {
    expect(
      resolveDefaultSystemPromptId({
        systemPrompts: [frontLine, main],
        defaultSystemPromptId: '',
      }),
    ).toBe('main');
  });

  it('two prompts flagged default resolve to the FIRST of them', () => {
    expect(
      resolveDefaultSystemPromptId({
        systemPrompts: [
          { id: 'a', isDefault: false },
          { id: 'b', isDefault: true },
          { id: 'c', isDefault: true },
        ],
      }),
    ).toBe('b');
  });

  it('no flag anywhere + a stale column falls all the way to prompts[0]', () => {
    expect(
      resolveDefaultSystemPromptId({
        systemPrompts: [{ id: 'a' }, { id: 'b' }],
        defaultSystemPromptId: 'gone',
      }),
    ).toBe('a');
  });

  it('the object form returns the PROMPT, not just its id', () => {
    expect(
      resolveDefaultSystemPrompt({
        systemPrompts: [main, frontLine],
        defaultSystemPromptId: 'front-line',
      }),
    ).toBe(frontLine);
  });
});
