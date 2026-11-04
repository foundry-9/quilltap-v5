import { describe, expect, it } from 'vitest';

import fixtures from './__fixtures__/markdown-fixtures.json';
import { applyLineScopedClasses, applyRoleplayPatterns } from './markdown-renderer';
import {
  compileRenderingPatterns,
  escapeMarkdownInBrackets,
  narrowRenderingPatterns,
  segmentsToHtml,
  tokenizeInline,
  unmarkedClassFor,
  type DialogueDetection,
  type RenderingPattern,
} from './roleplay-rendering';
import { isQuoteSensitiveRoleplayConfig } from './typography';

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

/**
 * v4 `__tests__/unit/lib/chat/novel-template.test.ts` (778b97b2b at 236c04e35),
 * the shared-core and post-processing cases transcribed case for case. The
 * patterns are the ones the capture recorded from v4's own `NOVEL_TEMPLATE`
 * (never a hand-derived copy); the schema, generator and prompt-hint cases are
 * server-side (P4.D351).
 */
describe('the Novel template through the shared core (v4 novel-template.test.ts)', () => {
  const novelRow = (fixtures as { name: string; options?: { renderingPatterns?: RenderingPattern[] } }[]).find(
    (f) => f.name === 'template-novel-sample',
  )!;
  const patterns = novelRow.options!.renderingPatterns!;
  const rules = compileRenderingPatterns(patterns);

  const NARRATION = 'qt-roleplay-plain qt-rp-italic';
  const EMPHASIS = 'qt-roleplay-plain qt-rp-bold qt-rp-italic';
  const DIALOGUE = 'qt-chat-dialogue';
  const OOC = 'qt-chat-ooc';

  /** Tokenize as a block's direct text: unmarked runs get the narration class. */
  const html = (text: string) =>
    segmentsToHtml(tokenizeInline(text, rules, { unmarkedClassName: unmarkedClassFor(rules) }));

  it('carries the scopes v4 derives for it, in order', () => {
    expect(patterns.map((p) => p.scope ?? 'inline')).toEqual([
      'strip',
      'literal',
      'inline',
      'inline',
      'inline',
      'unmarked',
    ]);
    expect(unmarkedClassFor(rules)).toBe(NARRATION);
    // literal / strip never become rules.
    expect(rules.map((r) => r.scope)).toEqual(['inline', 'inline', 'inline', 'unmarked']);
  });

  describe('display', () => {
    it('sets plain prose as italic narration', () => {
      expect(html('She set the cup down.')).toBe(
        `<span class="${NARRATION}">She set the cup down.</span>`,
      );
    });

    it('leaves straight-quoted dialogue upright, narration italic around it', () => {
      expect(html('"You\'re late," he said.')).toBe(
        `<span class="${DIALOGUE}">"You're late,"</span><span class="${NARRATION}"> he said.</span>`,
      );
    });

    it('matches curly and mixed quotes alike', () => {
      expect(html('“Hello”')).toBe(`<span class="${DIALOGUE}">“Hello”</span>`);
      expect(html('“Hello"')).toBe(`<span class="${DIALOGUE}">“Hello"</span>`);
    });

    it('runs an unclosed quotation to the end of the paragraph', () => {
      expect(html('"And then I went home.')).toBe(
        `<span class="${DIALOGUE}">"And then I went home.</span>`,
      );
    });

    it('styles emphasis in narration, concealing the asterisks', () => {
      expect(html('It was *urgent*.')).toBe(
        `<span class="${NARRATION}">It was </span><span class="${EMPHASIS}">urgent</span><span class="${NARRATION}">.</span>`,
      );
    });

    it('nests emphasis inside dialogue', () => {
      expect(html('"I said *stop*!"')).toBe(
        `<span class="${DIALOGUE}">"I said <span class="${EMPHASIS}">stop</span>!"</span>`,
      );
    });

    it('styles ((out of character)) text, delimiters visible', () => {
      expect(html('((brb, dinner))')).toBe(`<span class="${OOC}">((brb, dinner))</span>`);
    });

    it('never applies the narration class inside a claimed span', () => {
      const segs = tokenizeInline('"a *b* c"', rules, { unmarkedClassName: NARRATION });
      expect(segs).toHaveLength(1);
      expect(segs[0].children?.every((c) => c.className !== NARRATION)).toBe(true);
    });
  });

  describe('pre-parse (Markdown asterisks off)', () => {
    const pre = (s: string) => escapeMarkdownInBrackets(s, patterns);

    it('escapes single asterisks so Markdown leaves them for the emphasis delimiter', () => {
      expect(pre('It was *urgent*.')).toBe('It was \\*urgent\\*.');
    });

    it('drops doubled asterisks: they mean nothing', () => {
      expect(pre('A **bold** claim.')).toBe('A bold claim.');
      expect(pre('"I **said** so."')).toBe('"I said so."');
      expect(pre('***Both***')).toBe('Both');
    });

    it('does not double-escape asterisks inside dialogue', () => {
      expect(pre('"I said *stop*!"')).toBe('"I said \\*stop\\*!"');
    });

    it('leaves scene breaks, bullets and code alone', () => {
      expect(pre('* * *')).toBe('* * *');
      expect(pre('***')).toBe('***');
      expect(pre('* an item *with* stress')).toBe('* an item \\*with\\* stress');
      expect(pre('use `a*b` here')).toBe('use `a*b` here');
      expect(pre('```\n**x**\n```')).toBe('```\n**x**\n```');
    });

    it('skips a malformed stored pre-parse pattern instead of throwing', () => {
      expect(
        escapeMarkdownInBrackets('a ** b', [{ pattern: '(', className: '', scope: 'strip' }]),
      ).toBe('a ** b');
    });
  });

  describe('server HTML post-processing', () => {
    it('applies the narration class only to text directly in a block', () => {
      const out = applyRoleplayPatterns('<p>She ran. <a href="x">link</a></p>', rules);
      expect(out).toBe(`<p><span class="${NARRATION}">She ran. </span><a href="x">link</a></p>`);
    });

    it('does not style text inside a wrap-block span as narration', () => {
      const out = applyRoleplayPatterns(`<p><span class="${EMPHASIS}">Damn.</span></p>`, rules);
      expect(out).toBe(`<p><span class="${EMPHASIS}">Damn.</span></p>`);
    });

    it('sheds narration spans from a block a line rule claims', () => {
      const lineRules = compileRenderingPatterns([
        ...patterns,
        {
          pattern: '^// (?<rpBody>.+)$',
          className: OOC,
          flags: 'm',
          scope: 'line',
          hideDelimiters: true,
        },
      ]);
      const inline = applyRoleplayPatterns('<p>// an aside</p>', lineRules);
      expect(applyLineScopedClasses(inline, lineRules)).toBe(`<p class="${OOC}">an aside</p>`);
    });
  });

  it('keeps smart quotes available (dialogue matches both forms)', () => {
    const detection = (
      fixtures as { name: string; options?: { dialogueDetection?: DialogueDetection } }[]
    ).find((f) => f.name === 'template-novel-sample')!.options!.dialogueDetection!;
    expect(isQuoteSensitiveRoleplayConfig(patterns, detection)).toBe(false);
  });
});

describe('tokenizeInline — the walk invariants (v4 roleplay-rendering.ts:150-230)', () => {
  const rule = (pattern: string, className: string) =>
    compileRenderingPatterns([{ pattern, className }])[0];

  it('skips a zero-length match instead of looping', () => {
    // An open-ended pattern can match the empty string at the very end.
    const segs = tokenizeInline('abc', [rule('x*$', 'z')]);
    expect(segs).toEqual([{ text: 'abc' }]);
  });

  it('attaches children only when something inside was styled', () => {
    const rules = [rule('"[^"]+"', 'd'), rule('@@[^@]+@@', 'e')];
    expect(tokenizeInline('"plain"', rules)).toEqual([{ text: '"plain"', className: 'd' }]);
    expect(tokenizeInline('"a @@b@@"', rules)[0].children).toEqual([
      { text: '"a ' },
      { text: '@@b@@', className: 'e' },
      { text: '"' },
    ]);
  });

  it('stops nesting after three levels', () => {
    const rules = [
      rule('<<.+>>', 'l0'),
      rule('\\(\\(.+\\)\\)', 'l1'),
      rule('@@.+@@', 'l2'),
      rule('%%.+%%', 'l3'),
      rule('\\^\\^.+\\^\\^', 'l4'),
    ];
    expect(segmentsToHtml(tokenizeInline('<<((@@%%^^x^^%%@@))>>', rules))).toBe(
      // Levels 0-3 are styled (delimiters shown — these rules do not hide
      // them); the fifth rule inside the fourth level is left plain.
      '<span class="l0"><<<span class="l1">((<span class="l2">@@<span class="l3">%%^^x^^%%</span>@@</span>))</span>>></span>',
    );
  });
});
