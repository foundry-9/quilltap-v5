/**
 * The pronoun presets' rules, one case each (v4 `lib/characters/
 * pronoun-presets.ts` at `236c04e35`); the recorded corpus is
 * `wardrobe/pronoun-guidance.corpus.spec.ts`.
 */
import { describe, expect, it } from 'vitest';

import {
  STANDARD_PRONOUNS,
  STANDARD_PRONOUN_SUBJECTS,
  standardPronounLabel,
  standardPronounSubjectOf,
} from './pronoun-presets';

describe('pronoun presets', () => {
  it('lists the four standard sets in picker order, labels and values', () => {
    expect(STANDARD_PRONOUNS.map((p) => p.label)).toEqual([
      'He/Him/His',
      'She/Her/Her',
      'They/Them/Their',
      'It/It/Its',
    ]);
    expect(STANDARD_PRONOUN_SUBJECTS).toEqual(['he', 'she', 'they', 'it']);
  });

  it('labels a subject, falling back to the subject itself', () => {
    expect(standardPronounLabel('they')).toBe('They/Them/Their');
    expect(standardPronounLabel('xe' as never)).toBe('xe');
  });

  it('matches a WHOLE set, case- and whitespace-blind; a partial or custom set is null', () => {
    expect(standardPronounSubjectOf({ subject: ' SHE', object: 'Her ', possessive: 'her' })).toBe(
      'she',
    );
    expect(
      standardPronounSubjectOf({ subject: 'she', object: 'her', possessive: 'hers' }),
    ).toBeNull();
    expect(
      standardPronounSubjectOf({ subject: 'xe', object: 'xem', possessive: 'xyr' }),
    ).toBeNull();
    expect(standardPronounSubjectOf(null)).toBeNull();
    expect(standardPronounSubjectOf(undefined)).toBeNull();
  });
});
