/**
 * Wardrobe pronoun guidance's rules, one case each (v4 `lib/wardrobe/
 * pronoun-guidance.ts` at `236c04e35`); the recorded corpus is
 * `pronoun-guidance.corpus.spec.ts`.
 */
import { describe, expect, it } from 'vitest';

import {
  filterByPronounGuidance,
  fitsPronounGuidance,
  formatPronounGuidance,
  hasNoPronounGuidance,
  normalizePronounGuidance,
} from './pronoun-guidance';

const HE = { subject: 'he', object: 'him', possessive: 'his' };
const XE = { subject: 'xe', object: 'xem', possessive: 'xyr' };

describe('pronoun guidance', () => {
  it('normalises to known subjects, de-duplicated, in picker order; labels read as their subject', () => {
    expect(normalizePronounGuidance(['It', 'She/Her/Her', 'she', 'xe'])).toEqual(['she', 'it']);
    expect(normalizePronounGuidance('They/Them/Their')).toEqual(['they']);
    expect(normalizePronounGuidance(42)).toEqual([]);
  });

  it('an item without guidance is offered to everyone', () => {
    expect(hasNoPronounGuidance({ pronounGuidance: [] })).toBe(true);
    expect(fitsPronounGuidance({}, HE)).toBe(true);
  });

  it('a character without standard pronouns is never filtered', () => {
    expect(fitsPronounGuidance({ pronounGuidance: ['she'] }, XE)).toBe(true);
    expect(filterByPronounGuidance([{ id: 'a', pronounGuidance: ['she'] }], XE)).toEqual({
      items: [{ id: 'a', pronounGuidance: ['she'] }],
      hiddenCount: 0,
    });
  });

  it('guidance offers an item only to its sets; keepIds keeps what is worn', () => {
    const gown = { id: 'gown', pronounGuidance: ['she'] };
    expect(fitsPronounGuidance(gown, HE)).toBe(false);
    expect(filterByPronounGuidance([gown], HE)).toEqual({ items: [], hiddenCount: 1 });
    expect(filterByPronounGuidance([gown], HE, new Set(['gown']))).toEqual({
      items: [gown],
      hiddenCount: 0,
    });
  });

  it('formats as picker labels joined by a comma', () => {
    expect(formatPronounGuidance(['they', 'she'])).toBe('She/Her/Her, They/Them/Their');
    expect(formatPronounGuidance(null)).toBe('');
  });
});
