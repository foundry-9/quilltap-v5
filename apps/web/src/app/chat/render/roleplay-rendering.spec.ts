import { describe, expect, it } from 'vitest';

import { narrowRenderingPatterns } from './roleplay-rendering';

/**
 * The roleplay-rendering core (`roleplay-rendering.ts`, the SPA's TS port of v4
 * `lib/chat/roleplay-rendering.ts`). The captured fixtures in
 * `markdown-renderer.spec.ts` pin the composed output; this file pins the
 * pieces no single fixture states one-to-one.
 */
describe('narrowRenderingPatterns', () => {
  it('passes the five known scopes through (v4 778b97b2b — a v5 defect retired, P4.D352)', () => {
    // Before P4.D352 every non-`line` scope was coerced to `inline`, so Novel's
    // `[\s\S]` unmarked pattern became an inline rule claiming every character
    // and its `\*` / `\*{2,}` pre-parse patterns became inline rules too.
    const narrowed = narrowRenderingPatterns([
      { pattern: '[\\s\\S]', className: 'qt-roleplay-plain qt-rp-italic', scope: 'unmarked' },
      { pattern: '\\*', className: '', scope: 'literal' },
      { pattern: '\\*{2,}', className: '', scope: 'strip' },
      { pattern: '^// .+$', className: 'qt-chat-ooc', scope: 'line', flags: 'm' },
      { pattern: '\\(\\(.+?\\)\\)', className: 'qt-chat-ooc', scope: 'inline' },
    ]);
    expect(narrowed.map((p) => p.scope)).toEqual(['unmarked', 'literal', 'strip', 'line', 'inline']);
  });

  it('coerces an absent or unknown scope to inline, as v4 compiles it', () => {
    const narrowed = narrowRenderingPatterns([
      { pattern: 'a', className: 'x' },
      { pattern: 'b', className: 'y', scope: 'sideways' },
    ]);
    expect(narrowed.map((p) => p.scope)).toEqual(['inline', 'inline']);
  });
});
