import { describe, expect, it } from 'vitest';

import { fieldAppliesToModel, modelMatchesPattern } from './model-matchers';
import vectors from './__fixtures__/model-matchers-vectors.json';

/**
 * Parity specs for the `appliesToModels` matcher twin. The oracle is v4's
 * PURE module `lib/plugins/model-matchers.ts` at `84f33ce94`.
 *
 * Two halves, and both are needed:
 *   1. v4's own unit table (`lib/plugins/__tests__/model-matchers.test.ts`),
 *      transcribed 1:1 — including the gating rationale in its header.
 *   2. The recorded differential below, from v4's REAL functions run at the
 *      pin, which asks the regex questions v4's eleven expectations never do.
 */
describe('modelMatchesPattern (v4 unit table 1:1)', () => {
  it('matches an exact id', () => {
    expect(modelMatchesPattern('flux-lora', 'flux-lora')).toBe(true);
    expect(modelMatchesPattern('hidream', 'flux-lora')).toBe(false);
  });

  it('matches a family prefix', () => {
    expect(modelMatchesPattern('flux-lora/inpainting', 'flux-lora')).toBe(true);
    expect(modelMatchesPattern('flux', 'flux-lora')).toBe(false);
  });

  it('matches a trailing glob', () => {
    expect(modelMatchesPattern('wavespeed-ai/krea-v2/turbo-lora', 'wavespeed-ai/*')).toBe(true);
    expect(modelMatchesPattern('pruna-ai/p-image/edit-lora', 'wavespeed-ai/*')).toBe(false);
  });

  it('matches a leading glob', () => {
    expect(modelMatchesPattern('z-image-turbo-lora', '*-lora')).toBe(true);
    expect(modelMatchesPattern('z-image-turbo', '*-lora')).toBe(false);
  });

  it('does not let regex metacharacters in a pattern run wild', () => {
    // A literal '.' must stay literal, or `gpt-image-1.5` would match
    // `gpt-image-125` and a field would appear on the wrong model.
    expect(modelMatchesPattern('gpt-image-125', 'gpt-image-1.5*')).toBe(false);
    expect(modelMatchesPattern('gpt-image-1.5', 'gpt-image-1.5*')).toBe(true);
  });

  it('never matches an empty pattern', () => {
    expect(modelMatchesPattern('anything', '')).toBe(false);
  });
});

describe('fieldAppliesToModel (v4 unit table 1:1)', () => {
  it('renders unconditionally with no matcher list', () => {
    expect(fieldAppliesToModel(undefined, 'hidream')).toBe(true);
    expect(fieldAppliesToModel([], 'hidream')).toBe(true);
  });

  it('renders unconditionally when the model is unknown', () => {
    expect(fieldAppliesToModel(['flux-lora'], undefined)).toBe(true);
  });

  it('renders when any matcher hits', () => {
    expect(fieldAppliesToModel(['hidream', 'flux-2-*'], 'flux-2-dev-lora')).toBe(true);
  });

  it('hides when no matcher hits', () => {
    expect(fieldAppliesToModel(['hidream', 'flux-2-*'], 'recraft-v3')).toBe(false);
  });
});

/**
 * The recorded differential. `model-matchers-vectors.json` is the output of
 * v4's REAL `lib/plugins/model-matchers.ts` over a fixed corpus, recorded from
 * a worktree pinned at `2ece98c90` (recipe:
 * `harness/oracle/cases/model-matchers.test.ts`). The transcription above
 * reproduces v4's eleven expectations; these vectors reach what those cannot —
 * the escape class's actual membership, the anchors, and the guard order.
 */
describe('model-matchers vs v4’s recorded output (2ece98c90)', () => {
  for (const v of vectors.patterns) {
    it(`modelMatchesPattern(${JSON.stringify(v.model)}, ${JSON.stringify(v.pattern)})`, () => {
      expect(modelMatchesPattern(v.model, v.pattern)).toBe(v.matches);
    });
  }

  for (const v of vectors.fields) {
    it(`fieldAppliesToModel(${JSON.stringify(v.appliesToModels)}, ${JSON.stringify(v.model)})`, () => {
      expect(fieldAppliesToModel(v.appliesToModels ?? undefined, v.model ?? undefined)).toBe(
        v.applies,
      );
    });
  }

  it('the corpus discriminates the anchors from a substring search', () => {
    // A guard on the corpus itself, so a regeneration that silently dropped
    // the anchor cases cannot pass as coverage. `^…$` is what makes a leading
    // glob refuse a prefix that is not part of the pattern.
    const leading = vectors.patterns.find(
      (p) => p.model === 'prefix-flux-2-dev' && p.pattern === 'flux-2-*',
    );
    expect(leading?.matches).toBe(false);
  });

  it('the corpus discriminates the escape class’s actual membership', () => {
    // Only `.` is in reach of v4's own suite. `{}`, `()`, `[]`, `|`, `+` are
    // not: a twin that escapes FEWER of them than /[.*+?^${}()|[\]\\]/g does
    // passes every transcribed case and fails here — dropping `{}` reddens
    // the quantifier rows, dropping `()` reddens the group row.
    const brace = vectors.patterns.find((p) => p.model === 'aab' && p.pattern === 'a{2}*');
    expect(brace?.matches).toBe(false);
    const group = vectors.patterns.find((p) => p.model === 'a(b)c' && p.pattern === 'a(b)*');
    expect(group?.matches).toBe(true);
    // Escaping MORE is not symmetrical: `-` and `/` are absent from the class
    // and `\-`/`\/` mean themselves outside one, so a wider class is
    // behaviour-neutral rather than wrong. These rows pin the shape, not a
    // failure mode.
    const dash = vectors.patterns.find((p) => p.model === 'a-b/c' && p.pattern === 'a-b/*');
    expect(dash?.matches).toBe(true);
  });

  it('the corpus discriminates the empty-pattern guard from the prefix arm', () => {
    // Every string starts with '', so the prefix arm alone would say true.
    const empty = vectors.patterns.find((p) => p.model === 'flux' && p.pattern === '');
    expect(empty?.matches).toBe(false);
    // ...but an empty entry inside a NON-empty list is not the empty-list arm:
    // it matches nothing, so a list of only empties hides the field.
    const onlyEmpty = vectors.fields.find(
      (f) => f.appliesToModels?.length === 1 && f.appliesToModels[0] === '',
    );
    expect(onlyEmpty?.applies).toBe(false);
  });
});
