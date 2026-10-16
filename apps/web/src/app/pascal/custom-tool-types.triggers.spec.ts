/**
 * The `triggers` arm of the browser schema twin (v4 `362426c8f`, "trigger
 * phrases run custom tools when their words are said"), pinned at `25b8fab6a`.
 *
 * A CASE-FOR-CASE transcription of v4's `custom-tool-definition.test.ts` +44:
 * the `triggers` describe (`:724-748`) — as-written storage, the whitespace
 * sentence, the case-fold repeat, `KNOWN_TOP_LEVEL_KEYS` — and the Zod half of
 * "is deliberately weaker than Zod on trigger case-folding and whitespace"
 * (`:701-710`). The JSON-Schema-mirror half of that case and the six mirror
 * rows (`:639-644`) are NOT transcribed: they read
 * `public/schemas/qtap-custom-tool.schema.json`, which the SPA serves rather
 * than imports (P4.D313 vendors it with byte guards — recorded by name in the
 * P4.D307 lane record).
 *
 * The byte-level proof is the shared corpus (`custom-tool-types.corpus.spec.ts`,
 * the `triggers-*` rows of its `P4.D307 rows` block); this file is v4's own
 * suite, so the twin answers v4's questions in v4's words.
 *
 * `KNOWN_TOP_LEVEL_KEYS` is read STRUCTURALLY so this file runs (red) against a
 * twin that predates the export rather than failing at import.
 */

import { describe, expect, it } from 'vitest';

import * as types from './custom-tool-types';
import { formatDefinitionIssues, safeParse } from './custom-tool-types';

const BASE = {
  name: 'probe',
  description: 'A probe.',
  outcomes: [{ when: true, message: '-', state: 'info' }],
};

function accepts(doc: unknown): boolean {
  return safeParse(doc).success;
}

describe('the JSON Schema mirror agrees with Zod — the Zod half (the mirror itself is served, not imported)', () => {
  it('is deliberately weaker than Zod on trigger case-folding and whitespace', () => {
    // JSON Schema's uniqueItems is case-sensitive and it has no trim rule; both
    // are stated in the `triggers` description and enforced by the loader.
    const caseDuplicate = { ...BASE, triggers: ['Abracadabra', 'abracadabra'] };
    const padded = { ...BASE, triggers: [' abracadabra'] };
    expect(accepts(caseDuplicate)).toBe(false);
    expect(accepts(padded)).toBe(false);
  });
});

describe('triggers', () => {
  const issues = (doc: unknown): string => {
    const result = safeParse(doc);
    return result.success ? '' : formatDefinitionIssues(result.issues);
  };

  it('stores phrases as written', () => {
    const parsed = safeParse({ ...BASE, triggers: ['Open Sesame'] });
    // Read structurally, so the file compiles (and runs red) against a twin
    // whose `QtapCustomTool` predates the key.
    const data = parsed.success ? (parsed.data as unknown as { triggers?: unknown }) : null;
    expect(data?.triggers).toEqual(['Open Sesame']);
  });

  it('rejects leading or trailing whitespace, naming the trigger', () => {
    expect(issues({ ...BASE, triggers: ['abracadabra', 'open sesame '] })).toContain(
      'trigger 2 has leading or trailing whitespace',
    );
  });

  it('rejects a phrase that repeats another ignoring case', () => {
    expect(issues({ ...BASE, triggers: ['Abracadabra', 'ABRACADABRA'] })).toContain(
      'trigger 2 repeats trigger 1',
    );
  });

  it('is a known top-level key', () => {
    const known = (types as unknown as { KNOWN_TOP_LEVEL_KEYS?: readonly string[] }).KNOWN_TOP_LEVEL_KEYS;
    expect(known).toContain('triggers');
  });
});

describe('the caps (v4 `custom-tool.types.ts:88-103`)', () => {
  it('carries v4 values', () => {
    const caps = types as unknown as Record<string, unknown>;
    expect(caps['MAX_TRIGGERS']).toBe(8);
    expect(caps['MIN_TRIGGER_PHRASE_LENGTH']).toBe(3);
    expect(caps['MAX_TRIGGER_PHRASE_LENGTH']).toBe(80);
    expect(caps['MAX_WARDROBE_EFFECT_ITEMS']).toBe(8);
    expect(caps['MAX_WARDROBE_ITEM_REF_LENGTH']).toBe(200);
  });
});
