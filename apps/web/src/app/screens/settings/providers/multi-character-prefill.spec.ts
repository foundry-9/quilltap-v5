import { describe, expect, it } from 'vitest';

import { defaultMultiCharacterPrefill } from './multi-character-prefill';

/**
 * Parity with v4's own suite, case for case: v4's client is the SPA's oracle,
 * and the case table below is transcribed from
 * `__tests__/unit/lib/llm/multi-character-prefill.test.ts:8-28` (the
 * `defaultMultiCharacterPrefill` describe; the `profileUsesNamePrefill` half
 * lives server-side in v5 and is pinned there).
 */
describe('defaultMultiCharacterPrefill', () => {
  it('is off for Anthropic — 4.6+ rejects a request ending on an assistant message', () => {
    expect(defaultMultiCharacterPrefill('ANTHROPIC')).toBe(false);
  });

  it('matches the provider case-insensitively', () => {
    expect(defaultMultiCharacterPrefill('anthropic')).toBe(false);
    // v4 upper-cases before the set lookup, so any casing lands the same.
    expect(defaultMultiCharacterPrefill('Anthropic')).toBe(false);
  });

  it('is on for every other provider — the historic behaviour', () => {
    for (const provider of ['OPENAI', 'OLLAMA', 'DEEPSEEK', 'GROK', 'Z_AI', 'OPENROUTER']) {
      expect(defaultMultiCharacterPrefill(provider)).toBe(true);
    }
  });

  it('is on when the provider is unknown', () => {
    expect(defaultMultiCharacterPrefill(null)).toBe(true);
    expect(defaultMultiCharacterPrefill(undefined)).toBe(true);
    expect(defaultMultiCharacterPrefill('')).toBe(true);
  });

  it('does not treat a name merely CONTAINING anthropic as hostile', () => {
    // The membership test is exact — v4 uses a Set, not a substring match, so
    // an OpenAI-compatible endpoint proxying Anthropic keeps the prefill.
    expect(defaultMultiCharacterPrefill('ANTHROPIC_COMPATIBLE')).toBe(true);
  });
});
