import { describe, expect, it } from 'vitest';

import { isProseTemplate } from './prose-mode';

/**
 * Which templates get the composer's prose mode (the no-dialogue warning and
 * the narration / out-of-character preview) — transcribed from v4
 * `__tests__/unit/lib/punctuation-lint/prose-mode.test.ts` at `236c04e35`.
 *
 * v4's test imports the seeded `NOVEL_TEMPLATE`; the SPA has no copy of it, so
 * the parts the decision reads are restated here from v4
 * `lib/chat/novel-template.ts:26-52` + `:94-119` (the delimiter set and
 * `narrationDelimiters: null`). `engine.oracle.spec.ts` replays the same
 * question against v4's real `isProseTemplate`.
 */

const NOVEL_TEMPLATE = {
  name: 'Novel',
  delimiters: [
    {
      kind: 'wrap',
      name: 'Dialogue',
      buttonName: 'Dia',
      delimiters: '"',
      style: 'qt-chat-dialogue',
      matchCurlyQuotes: true,
      closeOptional: true,
    },
    {
      kind: 'wrap',
      name: 'Emphasis',
      buttonName: 'Emph',
      delimiters: '*',
      style: 'qt-roleplay-plain',
      hideDelimiter: true,
    },
    {
      kind: 'wrap',
      name: 'Out of Character',
      buttonName: 'OOC',
      delimiters: ['((', '))'],
      style: 'qt-chat-ooc',
    },
  ],
  narrationDelimiters: null as string | null,
  markdownAsterisks: false,
};

describe('isProseTemplate', () => {
  it('accepts unmarked narration with quoted dialogue (Novel)', () => {
    expect(isProseTemplate(NOVEL_TEMPLATE)).toBe(true);
  });

  it('refuses a template whose narration is marked', () => {
    expect(isProseTemplate({ ...NOVEL_TEMPLATE, narrationDelimiters: '*' })).toBe(false);
  });

  it('refuses unmarked narration without quoted dialogue', () => {
    expect(
      isProseTemplate({
        narrationDelimiters: null,
        delimiters: [{ kind: 'wrap', delimiters: ['((', '))'] }],
      }),
    ).toBe(false);
  });

  it('refuses no template', () => {
    expect(isProseTemplate(null)).toBe(false);
  });
});
