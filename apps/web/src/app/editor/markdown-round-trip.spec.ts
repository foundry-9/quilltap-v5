import { describe, expect, it } from 'vitest';

import tableVectors from './__fixtures__/table-round-trip-vectors.json';
import { dialectSchema, parseMarkdown, serializeMarkdown } from './markdown-dialect';
import { sinkListItem } from 'prosemirror-schema-list';
import { EditorState, TextSelection } from 'prosemirror-state';

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
 *
 * Every call below threads a fresh per-document `unitToken` through BOTH
 * `parseMarkdown` and `serializeMarkdown` — the P4.D40 sub-list-indentation
 * wiring (v4 item (b)): a document's own nesting unit is detected on parse and
 * re-applied on serialize, so a 2-space list stays 2-space and a 4-space list
 * stays 4-space, rather than every document reflowing to a fixed width. Every
 * entry below is either list-free or already 2-space, so this is a no-op for
 * the pre-existing corpus; the new entries further down exercise non-default
 * units end-to-end through the very same helper.
 */
function roundTrip(input: string): string {
  const unitToken = {};
  return serializeMarkdown(parseMarkdown(input, unitToken), unitToken);
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
  // --- sub-list indentation (P4.D40, v4 `4f088e7c`) --------------------------
  // v5's CommonMark parser never had v4's import-side flattening bug (nesting
  // depth already comes from document structure), so these pin the EXPORT-side
  // fix: a document's own nesting unit survives instead of reflowing to a
  // fixed width. Each `roundTrip` call threads a fresh unit token through both
  // `parseMarkdown` and `serializeMarkdown` (see the helper above), so these
  // exercise `detectListIndentUnit` + `applyListIndentUnit` end-to-end, not
  // just the pure functions (already differential-tested in
  // `list-indentation.oracle.spec.ts`).
  {
    name: 'four-space nested bullets stay four-space',
    md: '- a\n    - b\n        - c\n- d',
    trace: 'detectListIndentUnit reads 4; applyListIndentUnit re-indents to 4, not the default 2',
  },
  {
    name: 'nested ordered list',
    md: '1. a\n   1. b\n2. c',
    trace: 'detectListIndentUnit reads the 3-column ordered-parent step as the conventional 2-style; the marker-width floor keeps the child at 3',
  },
  {
    name: 'wide-ordered parent keeps its child at four columns',
    md: '10. a\n    - b',
    trace: 'applyListIndentUnit: width = max(step, markerWidth("10.")) = max(2, 4) = 4',
  },
  {
    name: 'three-space bullets stay three-space (tier 2 item 8)',
    md: '- a\n   - b\n- c',
    trace: 'detectListIndentUnit reads a literal 3-column bullet step as unit 3 (not the ordered-parent 3→2 special case)',
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
  {
    name: 'tab-nested bullets normalize to the tab-equivalent four-space unit',
    md: '- a\n\t- b',
    expected: '- a\n    - b',
    trace:
      'detectListIndentUnit expands a tab to 4 columns (TAB_WIDTH), so applyListIndentUnit re-indents to 4 spaces — the output never contains a literal tab, matching v4',
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

// ---------------------------------------------------------------------------
// Sub-list indentation, editor-integration arms (P4.D40, v4 `4f088e7c`)
// ---------------------------------------------------------------------------

describe('D17 gate — sub-list indentation unit memory', () => {
  it('is stable across repeated saves at a non-default unit', () => {
    // v4 markdown-list-roundtrip.test.ts "is stable across repeated saves",
    // widened to a 4-space document — each `roundTrip` call independently
    // re-detects the SAME unit from the previous call's own output, so the
    // chain converges rather than drifting toward the default.
    const source = '- a\n    - b\n        - c';
    expect(roundTrip(roundTrip(roundTrip(source)))).toBe(source);
  });

  it('exports an item indented IN the editor at the document unit, not the default', () => {
    // v4 markdown-list-roundtrip.test.ts "exports an item indented in the
    // editor at the document unit" (:98-141), ported to ProseMirror: load a
    // 4-space document (so the default of 2 could not accidentally pass this
    // test), sink the last top-level item the way Tab/the toolbar does, and
    // confirm the export still uses the unit `parseMarkdown` detected — a
    // SINGLE unit token threaded across the parse → edit → serialize
    // sequence, the same token identity `RichEditor` uses across its own
    // lifetime.
    const unitToken = {};
    const doc = parseMarkdown('- alpha\n    - beta\n- gamma', unitToken);
    // "gamma" is the last content in the document — the caret at doc-end
    // sits inside it, mirroring v4's `last.selectEnd()`.
    const state = EditorState.create({
      doc,
      schema: dialectSchema,
      selection: TextSelection.atEnd(doc),
    });

    let next = state;
    sinkListItem(dialectSchema.nodes['list_item'])(state, (tr) => {
      next = state.apply(tr);
    });

    expect(serializeMarkdown(next.doc, unitToken)).toBe(
      ['- alpha', '    - beta', '    - gamma'].join('\n'),
    );
  });
});

/**
 * P4.D40 tier-2 item 7 — the ONE recorded (a)-edge divergence: a 2-column
 * child under an ordered parent. v4's stack algorithm resolves depth from
 * structure alone (width 2 > parent width 0 → nest), so it reads `1. a\n  - b`
 * as a sub-list. CommonMark requires a child to indent to at least the
 * parent's CONTENT column (3, for `1. `) to continue the same list item — at
 * 2 columns it is a new block, so v5 parses two SIBLING lists separated by a
 * blank line.
 *
 * v4 never WRITES these bytes itself — (c) forces every child to at least the
 * parent marker's width, so v4's own `applyListIndentUnit` could never
 * produce a 2-column child under `1. `. Only hand-written or LLM-generated
 * markdown can reach this input. Per the order: pin BOTH directions (the
 * `TABLE_DIVERGENCES` precedent) and request a human ruling at unification —
 * do NOT build a v4-style structural pre-pass without one, since that would
 * fork v5 off CommonMark for an input v4 itself never produces.
 */
describe('D17 gate — the (a)-edge divergence: a 2-col child under an ordered parent', () => {
  it('documents the divergence (human ruling requested at unification)', () => {
    const input = '1. a\n  - b';
    // v5's actual bytes, asserted so the divergence cannot silently drift.
    expect(roundTrip(input), 'v5 parses two sibling lists, not one nested list').toBe(
      '1. a\n\n- b',
    );
    // v4's recorded behavior (nesting), asserted to still be what v5 diverges
    // FROM — if this ever passes, the divergence has been resolved and this
    // whole block should be deleted, not updated.
    expect(roundTrip(input), 'the divergence would be resolved — update the gate').not.toBe(
      '1. a\n   - b',
    );
  });
});

// ---------------------------------------------------------------------------
// GFM tables (v4 TABLE_TRANSFORMER) — RECORDED vectors, not hand-authored
// ---------------------------------------------------------------------------

/**
 * Unlike the corpora above (hand-authored from citations), every expectation
 * here was produced by v4's REAL `TABLE_TRANSFORMER`, driven through
 * `COMPOSER_TRANSFORMERS` in a headless Lexical editor — the same
 * import/export pair v4's bridge uses, with its `shouldPreserveNewLines` and
 * its export escape-strip. The recorder is
 * `harness/oracle/cases/table-transformer.test.ts`; its header carries the
 * regen recipe.
 *
 * The corpus is deliberately full of v4's quirks: a `:-:` separator is not a
 * separator (v4 needs 3+ dashes), alignment is parsed and then discarded so
 * every export is left-aligned, cells are literal text, and a long row is
 * truncated to the header's column count. Fix the port, not the vectors.
 */
const TABLE_DIVERGENCES: Record<string, { v5: string; why: string }> = {
  'a separator row that is not second means NO table': {
    v5: '| a | b |\n\n| 1   | 2   |\n| --- | --- |',
    why:
      'PRE-EXISTING dialect gap, not a table gap: v4/Lexical joins top-level blocks with a ' +
      'single `\\n` and models a blank line as an empty paragraph, while prosemirror-markdown ' +
      'separates blocks with a blank line. The two agree on every input whose blocks are ' +
      'already blank-line separated (all 28 entries above), and disagree only where a block ' +
      'directly abuts another with no blank line — which a table can do (v4 retries the ' +
      'transformer on the next line, making lines 2-3 a table under the line-1 paragraph). ' +
      'The same gap exists today for `text\\n# Heading`, with no table involved. Owning it ' +
      'means changing block separation for the whole dialect: out of this lane, named in the ' +
      'status log.',
  },
};

describe('D17 gate — v4 GFM tables (recorded from the real TABLE_TRANSFORMER)', () => {
  for (const v of tableVectors) {
    const divergence = TABLE_DIVERGENCES[v.name];

    if (!divergence) {
      it(`matches v4: ${v.name}`, () => {
        expect(roundTrip(v.md), v.trace).toBe(v.out);
      });
      continue;
    }

    it(`documents a divergence from v4: ${v.name}`, () => {
      // v5's output is asserted so the divergence cannot silently drift, and
      // v4's recorded bytes are asserted to still be what we diverge FROM.
      expect(roundTrip(v.md), divergence.why).toBe(divergence.v5);
      expect(roundTrip(v.md), 'the divergence would be resolved — update the gate').not.toBe(v.out);
    });
  }

  it('records the whole v4 table corpus (no silent vector loss)', () => {
    expect(tableVectors.length).toBe(20);
  });
});
