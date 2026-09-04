import { describe, expect, it } from 'vitest';

import { highlightSegments } from './template-highlighter';

/**
 * P4.75 (the `title=` census, item 5): the hard-coded-name warnings are v4's
 * bytes — `components/characters/TemplateHighlighter.tsx:567` and `:579` at the
 * `0b0617fee` pin — and they use a PLAIN HYPHEN. v5 had typographed both into
 * em dashes; the census found them as v4 strings with no v5 twin (they reach
 * the DOM through `template-display.ts`'s `[title]="seg.title"`, which no
 * attribute scan can see, hence the census's `bound` bucket).
 */
describe('highlightSegments — the hard-coded warning titles (v4 :567, :579)', () => {
  it("keeps v4's hyphen in the character warning (v4 :567)", () => {
    const segs = highlightSegments('Ada went out.', 'Ada', 'Bertie');
    const hit = segs.find((s) => s.kind === 'char-hardcoded');
    expect(hit?.title).toBe('Hard-coded character name - consider replacing with {{char}}');
  });

  it("keeps v4's hyphen in the user warning (v4 :579)", () => {
    const segs = highlightSegments('Bertie went out.', 'Ada', 'Bertie');
    const hit = segs.find((s) => s.kind === 'user-hardcoded');
    expect(hit?.title).toBe('Hard-coded user character name - consider replacing with {{user}}');
  });
});
