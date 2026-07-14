import { describe, expect, it } from 'vitest';

import { parseMarkdown, serializeMarkdown } from './markdown-dialect';

/**
 * The D17 make-or-break gate: does a ProseMirror bridge round-trip v4's composer
 * markdown dialect byte-for-byte?
 *
 * Every corpus entry traces to a v4 transformer or `preserve*` flag in
 * `components/chat/lexical/plugins/MarkdownBridgePlugin.tsx` (cited per entry).
 * The IDEMPOTENT corpus asserts `serialize(parse(x)) === x`. The NORMALIZING
 * corpus documents inputs v4 ITSELF rewrites on round-trip (e.g. `__bold__` →
 * `**bold**`, because BOLD_STAR precedes BOLD_UNDERSCORE and Lexical's export
 * dedups a format to its first transformer's tag) — v5 must match v4's
 * normalized output, and that first-save normalization is exactly what
 * `USES_RICH_MARKDOWN_EDITOR` + `computeAbsorbNext` absorb once.
 *
 * The spec IS this lane's differential (the dialect is v4's). Fix the port, not
 * the spec.
 */

function roundTrip(input: string): string {
  return serializeMarkdown(parseMarkdown(input));
}

/** Inputs already in v4-canonical form: byte-identical round-trip. */
const IDEMPOTENT: { name: string; md: string; trace: string }[] = [
  // --- inline emphasis (COMPOSER_TRANSFORMERS :67-87) -----------------------
  {
    name: 'underscore italic',
    md: 'an _italic_ word',
    trace: 'ITALIC_UNDERSCORE included; ITALIC_STAR excluded',
  },
  {
    name: 'star bold',
    md: 'a **bold** word',
    trace: 'BOLD_STAR included',
  },
  {
    name: 'bold then italic in one line',
    md: '**bold** and _italic_ together',
    trace: 'BOLD_STAR + ITALIC_UNDERSCORE',
  },
  {
    name: 'inline code',
    md: 'call `render()` now',
    trace: 'INLINE_CODE included',
  },
  // --- strikethrough + highlight marks (MarkdownBridgePlugin :35-36,71-74) ---
  {
    name: 'strikethrough',
    md: 'a ~~struck~~ word',
    trace: 'STRIKETHROUGH included (`~~`)',
  },
  {
    name: 'highlight',
    md: 'a ==marked== word',
    trace: 'HIGHLIGHT included (`==`)',
  },
  {
    name: 'strikethrough and highlight adjacent in one line',
    md: 'both ~~struck~~ and ==marked== here',
    trace: 'STRIKETHROUGH + HIGHLIGHT',
  },
  {
    name: 'strikethrough wrapping underscore italic',
    md: '~~a _b_ c~~',
    trace: 'STRIKETHROUGH + ITALIC_UNDERSCORE nesting',
  },
  {
    name: 'highlight wrapping star bold',
    md: '==a **b** c==',
    trace: 'HIGHLIGHT + BOLD_STAR nesting',
  },
  {
    name: 'underscore italic wrapping strikethrough',
    md: 'nested _em with ~~struck~~ inside_',
    trace: 'ITALIC_UNDERSCORE + STRIKETHROUGH nesting',
  },
  {
    name: 'literal lone equals survives, single `==` unpaired stays literal',
    md: 'a lone = and 2 == 2 math',
    trace: 'HIGHLIGHT needs a matched pair; a lone `=`/`==` is literal',
  },
  {
    name: 'literal empty double-tilde survives',
    md: 'literal tildes ~~ empty stays',
    trace: 'STRIKETHROUGH needs non-empty content; `~~ ` stays literal (preserveTildes)',
  },
  // --- literal roleplay punctuation (preserve* flags :143-151) --------------
  {
    name: 'single-star narration survives literal',
    md: '*He turns to leave, saying nothing.*',
    trace: 'ITALIC_STAR excluded + preserveAsterisks (:60-66, :102)',
  },
  {
    name: 'inline single stars stay literal',
    md: 'she said *quietly* and left',
    trace: 'ITALIC_STAR excluded + preserveAsterisks',
  },
  {
    name: 'arithmetic asterisks survive',
    md: 'compute 5 * 3 * 2 here',
    trace: 'preserveAsterisks',
  },
  {
    name: 'literal underscores survive',
    md: 'a lone _ and a pair _ _ of them',
    trace: 'preserveUnderscores (:107)',
  },
  {
    name: 'literal backtick survives',
    md: 'a stray ` backtick in prose',
    trace: 'preserveBackticks (:112)',
  },
  {
    name: 'literal tildes survive',
    md: 'about ~ 3 to ~ 5 items',
    trace: 'preserveTildes (:117)',
  },
  {
    name: 'mixed literal punctuation in a narration line',
    md: '*She counts: ~5 apples, a _crate_ of them, and 2 * 3 more.*',
    trace: 'all four preserve* flags + ITALIC_STAR excluded',
  },
  // --- block elements (COMPOSER_TRANSFORMERS element transformers) ----------
  {
    name: 'headings h1..h3',
    md: '# Title\n\n## Section\n\n### Subsection',
    trace: 'HEADING included',
  },
  {
    name: 'blockquote',
    md: '> a quoted line',
    trace: 'QUOTE included',
  },
  {
    name: 'unordered list uses dash',
    md: '- first\n- second\n- third',
    trace: 'UNORDERED_LIST included (v4 uses `- `)',
  },
  {
    name: 'ordered list',
    md: '1. first\n2. second\n3. third',
    trace: 'ORDERED_LIST included',
  },
  {
    name: 'check list unchecked and checked',
    md: '- [ ] todo\n- [x] done',
    trace: 'CASE_INSENSITIVE_CHECK_LIST (:49-58)',
  },
  {
    name: 'fenced code with language',
    md: '```ts\nconst x = 1;\nconst y = 2;\n```',
    trace: 'CODE included',
  },
  {
    name: 'fenced code with a blank interior line',
    md: '```\nline one\n\nline three\n```',
    trace: 'CODE included',
  },
  {
    name: 'multi-line blockquote keeps its line breaks',
    md: '> line one\n> line two',
    trace: 'QUOTE + Lexical line breaks exported as `\\n`',
  },
  // --- line breaks (v4 Shift+Enter line break, exported as `\n`) -------------
  {
    name: 'soft line break inside a paragraph survives',
    md: 'line one\nline two',
    trace: 'Lexical LineBreakNode round-trips as `\\n` (softbreak → hard_break)',
  },
  {
    name: 'narration line then dialog line',
    md: '*She pauses at the door.*\nThen she speaks, quietly.',
    trace: 'ITALIC_STAR excluded + soft line break',
  },
  // --- nesting + a chapter-like body ----------------------------------------
  {
    name: 'nested unordered list',
    md: '- parent\n  - child a\n  - child b',
    trace: 'UNORDERED_LIST nesting',
  },
  {
    name: 'nested check list',
    md: '- [ ] a\n  - [x] b',
    trace: 'CASE_INSENSITIVE_CHECK_LIST nesting',
  },
  {
    name: 'check list mixed with a plain item',
    md: '- plain item\n- [x] done',
    trace: 'CHECK_LIST + UNORDERED_LIST in one list',
  },
  {
    name: 'brackets in prose survive unescaped',
    md: 'see [note] and [1] below',
    trace: 'Lexical export never escapes brackets',
  },
  {
    name: 'frontmatter-free chapter body',
    md: [
      '# Chapter One',
      '',
      'The morning was _grey_ and **still**.',
      '',
      '*She walked to the window and looked out.*',
      '',
      '> "It always rains here," she said.',
      '',
      '- pack the bags',
      '- lock the door',
      '',
      'And so it began.',
    ].join('\n'),
    trace: 'combined transformer scope + preserve* flags',
  },
];

/** Inputs v4 itself normalizes on round-trip: v5 must match the v4 output. */
const NORMALIZING: { name: string; md: string; expected: string; trace: string }[] = [
  {
    name: 'underscore bold normalizes to star bold',
    md: '__bold__',
    expected: '**bold**',
    trace:
      'BOLD_UNDERSCORE parses; Lexical export dedups bold to BOLD_STAR (first) → `**` (LexicalMarkdown exportTextFormat `applied` set)',
  },
  {
    name: 'two-space hard break normalizes to a plain newline',
    md: 'line one  \nline two',
    expected: 'line one\nline two',
    trace: 'Lexical exports a line break as `\\n`, dropping the two trailing spaces',
  },
];

describe('D17 gate — v4 composer dialect round-trip', () => {
  for (const entry of IDEMPOTENT) {
    it(`round-trips byte-for-byte: ${entry.name}`, () => {
      expect(roundTrip(entry.md), entry.trace).toBe(entry.md);
    });
  }

  for (const entry of NORMALIZING) {
    it(`matches v4 normalization: ${entry.name}`, () => {
      expect(roundTrip(entry.md), entry.trace).toBe(entry.expected);
    });
  }
});
