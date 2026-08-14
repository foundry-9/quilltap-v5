import { describe, expect, it } from 'vitest';

import triggerVectors from './__fixtures__/emoji-trigger-vectors.json';
import { EMOJI_PROFILE } from './profiles/emoji';
import { findTrigger } from './trigger';

/**
 * Tier B trigger specs for the EMOJI profile — v4's
 * `lib/char-insert/__tests__/trigger.test.ts` ported case-for-case over the
 * byte-copied corpus.
 *
 * The corpus is the proof that v4's Layer 2.0u generalization was
 * behaviour-neutral (the vectors are unedited from the day `findEmojiTrigger`
 * shipped); replaying it here makes it the proof that the v5 port is neutral
 * too. **Fix the port, not the vectors.**
 *
 * @module editor/char-insert/trigger.spec
 */

const CONFIG = EMOJI_PROFILE.trigger;
const { minQueryLength: MIN_QUERY_LENGTH, maxQueryLength: MAX_QUERY_LENGTH } = CONFIG;

function findEmojiTrigger(text: string) {
  return findTrigger(text, CONFIG);
}

describe('editor/char-insert/trigger — emoji profile', () => {
  describe('corpus vectors', () => {
    for (const testCase of triggerVectors.cases) {
      it(`findTrigger(${JSON.stringify(testCase.text)}) — ${testCase.invariant}`, () => {
        expect(findEmojiTrigger(testCase.text)).toEqual(testCase.expect);
      });
    }
  });

  describe('the offsets a caller deletes', () => {
    it('spans the colon through the last query character', () => {
      const text = 'hello :smi';
      const match = findEmojiTrigger(text)!;
      expect(text.slice(match.start, match.end)).toBe(':smi');
    });

    it('excludes the closing colon from `end` so the caller removes it explicitly', () => {
      const text = 'hello :smile:';
      const match = findEmojiTrigger(text)!;
      expect(match.closed).toBe(true);
      expect(text.slice(match.start, match.end)).toBe(':smile');
      expect(text.slice(match.end)).toBe(':');
    });
  });

  describe('word-opening context', () => {
    for (const opener of ['(', '[', '{', '"', "'", '“', '‘', '—', '–', ' ', '\t']) {
      it(`opens after ${JSON.stringify(opener)}`, () => {
        expect(findEmojiTrigger(`x${opener}:smi`)).not.toBeNull();
      });
    }

    for (const blocker of ['a', '0', '/', '.', '-', ':']) {
      it(`does not open after ${JSON.stringify(blocker)}`, () => {
        expect(findEmojiTrigger(`x${blocker}:smi`)).toBeNull();
      });
    }
  });

  describe('query alphabet', () => {
    for (const query of ['sm1', 'sm_i', 'sm-i', 'sm+i']) {
      it(`accepts ${JSON.stringify(query)}`, () => {
        expect(findEmojiTrigger(`:${query}`)?.query).toBe(query);
      });
    }

    for (const text of [':sm i', ':sm.i', ':sm/i', ':sm!i']) {
      it(`stops at the first disallowed character in ${JSON.stringify(text)}`, () => {
        // The scan walks back from the cursor, so a disallowed character in the
        // middle severs the query from its colon entirely.
        expect(findEmojiTrigger(text)).toBeNull();
      });
    }
  });

  describe('length bounds', () => {
    it('stays shut below the minimum', () => {
      expect(findEmojiTrigger(`:${'x'.repeat(MIN_QUERY_LENGTH - 1)}`)).toBeNull();
      expect(findEmojiTrigger(`:${'x'.repeat(MIN_QUERY_LENGTH)}`)).not.toBeNull();
    });

    it('abandons the match past the maximum', () => {
      expect(findEmojiTrigger(`:${'x'.repeat(MAX_QUERY_LENGTH)}`)).not.toBeNull();
      expect(findEmojiTrigger(`:${'x'.repeat(MAX_QUERY_LENGTH + 1)}`)).toBeNull();
    });
  });

  it('considers only the nearest colon', () => {
    const text = 'a :first and :second';
    const match = findEmojiTrigger(text)!;
    expect(match.query).toBe('second');
    expect(text.slice(match.start, match.end)).toBe(':second');
  });

  it('lowercases the query, because github shortcodes are lowercase', () => {
    expect(findEmojiTrigger(':SMILE')?.query).toBe('smile');
  });

  /**
   * A `g`/`y`-flagged pattern carries `lastIndex` between calls, which would
   * make trigger detection depend on how many keystrokes preceded it. Both
   * profiles must stay stateless.
   */
  it('uses a stateless query pattern', () => {
    expect(CONFIG.queryPattern.global).toBe(false);
    expect(CONFIG.queryPattern.sticky).toBe(false);
  });
});
