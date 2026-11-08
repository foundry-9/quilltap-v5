/**
 * Character kind — the resolver every SPA reader goes through.
 *
 * Transcribed case for case from v4 `__tests__/unit/lib/characters/kind.test.ts:17-48`
 * at the pin `624d0d25c` (the first FIVE `it`s; the two policy `it`s are the
 * server's — PR-5, P4.D388 Tier 3 item 30). The recorded corpus replay is
 * `character-kind.corpus.spec.ts`.
 */
import { describe, expect, it } from 'vitest';

import {
  CHARACTER_KINDS,
  DEFAULT_CHARACTER_KIND,
  characterKindOrDefault,
  isAssistantCharacter,
  resolveCharacterKind,
} from './character-kind';

describe('character kind', () => {
  it('defaults to roleplay', () => {
    expect(DEFAULT_CHARACTER_KIND).toBe('roleplay');
    expect(CHARACTER_KINDS).toEqual(['roleplay', 'assistant']);
  });

  it.each([
    ['roleplay', 'roleplay'],
    ['assistant', 'assistant'],
    [undefined, 'roleplay'],
    [null, 'roleplay'],
    ['', 'roleplay'],
    ['Assistant', 'roleplay'],
    ['butler', 'roleplay'],
    [1, 'roleplay'],
  ])('normalizes %p to %p', (value, expected) => {
    expect(characterKindOrDefault(value)).toBe(expected);
  });

  it('resolves a character with no kind (a raw row, a pre-kind vault) as roleplay', () => {
    expect(resolveCharacterKind({})).toBe('roleplay');
    expect(resolveCharacterKind(null)).toBe('roleplay');
    expect(resolveCharacterKind(undefined)).toBe('roleplay');
  });

  it('resolves a stored kind', () => {
    expect(resolveCharacterKind({ kind: 'assistant' })).toBe('assistant');
    expect(resolveCharacterKind({ kind: 'roleplay' })).toBe('roleplay');
  });

  it('answers whether a character is an assistant', () => {
    expect(isAssistantCharacter({ kind: 'assistant' })).toBe(true);
    expect(isAssistantCharacter({ kind: 'roleplay' })).toBe(false);
    expect(isAssistantCharacter({ kind: 'wizard' })).toBe(false);
    expect(isAssistantCharacter(null)).toBe(false);
  });
});
