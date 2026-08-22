import { describe, expect, it } from 'vitest';

import { defaultMultiCharacterPrefill } from './multi-character-prefill';

/**
 * Parity with v4's own suite, case for case: v4's client is the SPA's oracle,
 * and the case table below is transcribed from
 * `__tests__/unit/lib/llm/multi-character-prefill.test.ts:8-52` (the
 * `defaultMultiCharacterPrefill` describe, grown at `97d2fcb5` with the three
 * thinking-turn cases; the `profileUsesNamePrefill` half — including its two
 * new thinking cases — lives server-side in v5 and is pinned there).
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

  it('is off for a profile that will run a thinking turn, on any provider', () => {
    // Bug 85: DeepSeek 400s on continuing a thinking turn whose
    // `reasoning_content` it never saw; bug 68: Ollama never opens the
    // reasoning block behind a prefilled turn.
    for (const provider of ['DEEPSEEK', 'OLLAMA', 'OPENAI', 'Z_AI']) {
      expect(defaultMultiCharacterPrefill(provider, true)).toBe(false);
    }
    expect(defaultMultiCharacterPrefill(null, true)).toBe(false);
  });

  it('keeps the prefill for a thinking-capable provider that is not thinking', () => {
    // Bug 68 rejected a blanket provider rule for precisely this: the prefill
    // is the stronger anchor, and weak non-thinking models need it most.
    expect(defaultMultiCharacterPrefill('DEEPSEEK', false)).toBe(true);
    expect(defaultMultiCharacterPrefill('OLLAMA', false)).toBe(true);
  });

  it('leaves Anthropic off whether or not it is thinking', () => {
    // Anthropic's is the one genuine provider rule: it rejects an assistant
    // tail outright, thinking or not.
    expect(defaultMultiCharacterPrefill('ANTHROPIC', false)).toBe(false);
    expect(defaultMultiCharacterPrefill('ANTHROPIC', true)).toBe(false);
  });
});
