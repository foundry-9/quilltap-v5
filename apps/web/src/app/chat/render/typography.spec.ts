import { describe, expect, it } from 'vitest';

import rawFixtures from './__fixtures__/markdown-fixtures.json';
import { renderMarkdownToHtml, type MarkdownRenderOptions } from './markdown-renderer';
import { DEFAULT_DIALOGUE_DETECTION, DEFAULT_RENDERING_PATTERNS } from './roleplay-rendering';
import {
  SMARTYPANTS_OPTIONS,
  isQuoteSensitiveRoleplayConfig,
  shouldCurlQuotes,
} from './typography';

interface RenderFixture {
  name: string;
  input: string;
  html: string;
  options?: MarkdownRenderOptions;
}

const fixtures = rawFixtures as RenderFixture[];

function fixtureByName(name: string): RenderFixture {
  const found = fixtures.find((f) => f.name === name);
  if (!found) throw new Error(`missing fixture "${name}" — regenerate the corpus`);
  return found;
}

/**
 * Smart typography, Part A (v4 `lib/markdown/typography.ts`, `2d31810f`).
 *
 * The `quotes-*` vectors in `markdown-fixtures.json` are captured from v4's REAL
 * renderer at `48396682` and are the oracle for every claim below; the
 * assertions here name what each vector is FOR, so a byte that moves is legible
 * as a rule that changed rather than as an anonymous diff.
 */
describe('SMARTYPANTS_OPTIONS', () => {
  it('is quotes-only — dashes and ellipses are Part B, backticks are catastrophic', () => {
    // The four keys are `retext-smartypants` v6's real names; an unrecognised
    // key would be silently ignored, which is the failure mode that would leave
    // `dashes` on.
    expect(SMARTYPANTS_OPTIONS).toEqual({
      quotes: true,
      ellipses: false,
      dashes: false,
      backticks: false,
    });
  });

  it('leaves `--verbose` alone in prose (the permanent dashes:false decision)', () => {
    const v = fixtureByName('quotes-on-dashes-untouched');
    expect(v.html).toContain('--verbose ---');
    expect(renderMarkdownToHtml(v.input, v.options ?? {})).toBe(v.html);
  });

  it('leaves `...` alone (Part B owns it, at type time)', () => {
    const v = fixtureByName('quotes-on-ellipses-untouched');
    expect(v.html).toContain('Wait...');
    expect(renderMarkdownToHtml(v.input, v.options ?? {})).toBe(v.html);
  });
});

describe('isQuoteSensitiveRoleplayConfig', () => {
  it('says NO for the shipped defaults — the built-ins carry no rpBody group', () => {
    expect(isQuoteSensitiveRoleplayConfig(DEFAULT_RENDERING_PATTERNS, DEFAULT_DIALOGUE_DETECTION)).toBe(
      false,
    );
  });

  it('says YES for a wrap delimiter that is a straight double quote', () => {
    expect(
      isQuoteSensitiveRoleplayConfig(
        [{ pattern: '"(?<rpBody>[^"]+)"', className: 'qt-chat-narration' }],
        DEFAULT_DIALOGUE_DETECTION,
      ),
    ).toBe(true);
  });

  it('says YES for a wrap delimiter that is a straight APOSTROPHE', () => {
    expect(
      isQuoteSensitiveRoleplayConfig(
        [{ pattern: "'(?<rpBody>[^']+)'", className: 'qt-chat-narration' }],
        DEFAULT_DIALOGUE_DETECTION,
      ),
    ).toBe(true);
  });

  it('says NO for a wrap delimiter that claims no quote character', () => {
    expect(
      isQuoteSensitiveRoleplayConfig(
        [{ pattern: '%%(?<rpBody>[^%]+)%%', className: 'qt-chat-narration' }],
        DEFAULT_DIALOGUE_DETECTION,
      ),
    ).toBe(false);
  });

  it('says NO for a quote-bearing pattern with NO rpBody group (the legacy built-ins)', () => {
    // This is exactly the shipped dialogue pattern's shape: it matches straight
    // quotes but was not built from a declared wrap delimiter, so it neither
    // escapes nor re-matches the raw string the way an rpBody pattern does.
    expect(
      isQuoteSensitiveRoleplayConfig(
        [{ pattern: '["“][^"”]+["”]', className: 'qt-chat-dialogue' }],
        DEFAULT_DIALOGUE_DETECTION,
      ),
    ).toBe(false);
  });

  it('says YES for a detection naming a straight quote without its curly forms', () => {
    expect(
      isQuoteSensitiveRoleplayConfig(DEFAULT_RENDERING_PATTERNS, {
        openingChars: ['"'],
        closingChars: ['"'],
        className: 'qt-chat-dialogue',
      }),
    ).toBe(true);
  });

  it('says NO once ONE curly counterpart is declared alongside the straight one', () => {
    expect(
      isQuoteSensitiveRoleplayConfig(DEFAULT_RENDERING_PATTERNS, {
        openingChars: ['"', '“'],
        closingChars: ['"'],
        className: 'qt-chat-dialogue',
      }),
    ).toBe(false);
  });

  it('says NO for a detection that names no quote at all (guillemets)', () => {
    expect(
      isQuoteSensitiveRoleplayConfig(DEFAULT_RENDERING_PATTERNS, {
        openingChars: ['«'],
        closingChars: ['»'],
        className: 'qt-chat-custom-dialogue',
      }),
    ).toBe(false);
  });

  it('says NO for a nullish detection — the caller resolves the fallback first', () => {
    expect(isQuoteSensitiveRoleplayConfig(DEFAULT_RENDERING_PATTERNS, null)).toBe(false);
    expect(isQuoteSensitiveRoleplayConfig(DEFAULT_RENDERING_PATTERNS, undefined)).toBe(false);
  });
});

describe('shouldCurlQuotes', () => {
  const safe = {
    renderingPatterns: DEFAULT_RENDERING_PATTERNS,
    dialogueDetection: DEFAULT_DIALOGUE_DETECTION,
  };

  it('is off unless the user asked for it', () => {
    expect(shouldCurlQuotes({ ...safe, displayQuotes: false })).toBe(false);
    expect(shouldCurlQuotes({ ...safe, displayQuotes: undefined })).toBe(false);
  });

  it('is on with the shipped defaults once the user asks', () => {
    expect(shouldCurlQuotes({ ...safe, displayQuotes: true })).toBe(true);
  });

  it('a quote-sensitive template overrides the user setting', () => {
    expect(
      shouldCurlQuotes({
        displayQuotes: true,
        renderingPatterns: [{ pattern: '"(?<rpBody>[^"]+)"', className: 'qt-chat-narration' }],
        dialogueDetection: DEFAULT_DIALOGUE_DETECTION,
      }),
    ).toBe(false);
  });
});

/**
 * The three structural skips, read off v4's captured bytes. There are no hand-
 * written bail conditions for these in either app: `code`, `inlineCode`, `math`
 * and `inlineMath` are distinct mdast node types and a link target is a `url`
 * property, so the plugin never sees them as text.
 */
describe('renderMarkdownToHtml — the structural skips (displayQuotes on)', () => {
  it('never curls inside inline code', () => {
    const v = fixtureByName('quotes-on-inline-code');
    expect(renderMarkdownToHtml(v.input, v.options ?? {})).toBe(v.html);
    expect(v.html).toContain('<code>git commit -m "wip"</code>');
    // ...while the surrounding prose IS curled, which is what makes this a skip
    // rather than a switched-off feature.
    expect(v.html).toContain('“done”');
  });

  it('never curls inside a fenced code block', () => {
    const v = fixtureByName('quotes-on-code-fence');
    expect(renderMarkdownToHtml(v.input, v.options ?? {})).toBe(v.html);
    expect(v.html).toContain('print("hello")');
    expect(v.html).toContain('“afterwards”');
  });

  it('never curls inside math — the raw LaTeX survives into KaTeX’s annotation', () => {
    const v = fixtureByName('quotes-on-math');
    expect(renderMarkdownToHtml(v.input, v.options ?? {})).toBe(v.html);
    expect(v.html).toContain('\\text{"x"}');
    expect(v.html).toContain('“prose”');
  });

  it('never curls a link target', () => {
    const v = fixtureByName('quotes-on-link-url');
    expect(renderMarkdownToHtml(v.input, v.options ?? {})).toBe(v.html);
    expect(v.html).toContain("https://example.com/a&#x27;b");
    expect(v.html).toContain('“hello”');
  });
});

/**
 * The suppression rules end-to-end, each as a captured PAIR: the suppressing
 * config and its nearest safe neighbour over comparable input. A pair is what
 * makes "suppressed" mean "v4 emitted straight quotes here", not "something
 * rendered".
 */
describe('renderMarkdownToHtml — quote-curling suppression (v4-captured pairs)', () => {
  it('an rpBody wrap delimited by a straight quote suppresses the whole render', () => {
    const off = fixtureByName('quotes-suppressed-by-rpbody-quote-delimiter');
    const on = fixtureByName('quotes-not-suppressed-by-safe-rpbody');
    expect(renderMarkdownToHtml(off.input, off.options ?? {})).toBe(off.html);
    expect(renderMarkdownToHtml(on.input, on.options ?? {})).toBe(on.html);
    // Both renders asked for curling; only the safe one got it.
    expect(off.html).not.toContain('“');
    expect(on.html).toContain('“again”');
  });

  it('a straight-only dialogue detection suppresses; declaring the curly forms does not', () => {
    const off = fixtureByName('quotes-suppressed-by-straight-only-detection');
    const on = fixtureByName('quotes-not-suppressed-by-curly-aware-detection');
    expect(off.input).toBe(on.input);
    expect(renderMarkdownToHtml(off.input, off.options ?? {})).toBe(off.html);
    expect(renderMarkdownToHtml(on.input, on.options ?? {})).toBe(on.html);
    expect(off.html).toContain('"Hello there,"');
    expect(on.html).toContain('“Hello there,”');
  });

  it('a detection naming no quote at all does not suppress', () => {
    const v = fixtureByName('quotes-not-suppressed-by-guillemet-detection');
    expect(renderMarkdownToHtml(v.input, v.options ?? {})).toBe(v.html);
    expect(v.html).toContain('“hello”');
  });
});

/**
 * The setting itself: on/off pairs over identical input. The `-off` twin is the
 * proof that the vectors are not simply "what the renderer does now".
 */
describe('renderMarkdownToHtml — displayQuotes on/off pairs', () => {
  it.each([
    ['quotes-on-dialogue', 'quotes-off-dialogue'],
    ['quotes-on-apostrophes', 'quotes-off-apostrophes'],
  ])('%s differs from %s only in the quote characters', (onName, offName) => {
    const on = fixtureByName(onName);
    const off = fixtureByName(offName);
    expect(on.input).toBe(off.input);
    expect(on.html).not.toBe(off.html);
    expect(renderMarkdownToHtml(on.input, on.options ?? {})).toBe(on.html);
    expect(renderMarkdownToHtml(off.input, off.options ?? {})).toBe(off.html);
  });

  it('defaults to OFF when the option is absent (v4 `displayQuotes = false`)', () => {
    const off = fixtureByName('quotes-off-dialogue');
    expect(renderMarkdownToHtml(off.input, {})).toBe(off.html);
  });

  it('curls apostrophes too — a cosmetic wrong guess, never a stored byte', () => {
    const on = fixtureByName('quotes-on-apostrophes');
    // smartypants' well-known failures (`'80s` becomes a closing quote). The
    // feature accepts them BECAUSE nothing is written down: the stored content
    // is still what the writer typed.
    expect(on.html).toContain('’80s');
    expect(on.input).toContain("'80s");
  });

  it('covers both arms of the typographer (the corpus is not silently truncated)', () => {
    const curled = fixtures.filter((f) => f.options?.displayQuotes === true);
    const plain = fixtures.filter((f) => f.options?.displayQuotes === false);
    expect(curled.length).toBeGreaterThanOrEqual(11);
    expect(plain.length).toBeGreaterThanOrEqual(2);
  });
});
