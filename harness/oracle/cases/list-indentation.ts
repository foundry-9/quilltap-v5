/**
 * Oracle case (P4.D40): the export-side half of v4's sub-list-indentation fix
 * (`4f088e7c`, `components/chat/lexical/transformers/list-indentation.ts`).
 *
 * Drives v4's REAL pure module directly — no jest/jsdom rig needed, it has no
 * side-effecting imports (`LexicalEditor` is a type-only import, erased at
 * compile time). Covers `detectListIndentUnit`, `applyListIndentUnit`, and the
 * source-mode `indentSourceLines`/`outdentSourceLines` helpers, mirroring the
 * inventory of v4's own `__tests__/unit/components/chat/lexical/transformers/
 * list-indentation.test.ts` (160 lines), plus `normalizeListIndentForLexical`
 * — the import-side four-space rewrite.
 *
 * That last family was NOT covered until 2026-08-03. The P4.D40 disposition
 * held that v5 needed no analog, because its CommonMark parser resolves nesting
 * from structure natively and v4 cannot author the one shape they disagree on.
 * The ruling was evidence-conditional, and the evidence arrived: a dogfood scan
 * of the Friday copy's document stores found 30 instances of that shape across
 * five real Obsidian documents, where v5's save flattened the nesting for good.
 * v5 now runs the pre-pass on import, and these rows are what pins it to v4.
 *
 * Emits ONE deterministic JSON corpus (pretty-printed) that
 * `apps/web/src/app/editor/list-indentation.oracle.spec.ts` replays row-for-row
 * against the v5 port. Every function here is pure (no crypto/Date/random), so
 * the corpus regenerates byte-stably.
 *
 * Run from inside the v4 server checkout (imports resolve `@/` there):
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/list-indentation.ts \
 *     > ~/source/quilltap-v5/apps/web/src/app/editor/__fixtures__/list-indentation-oracle.json
 *
 * Never hand-edit the emitted fixture; regenerate and grep a known row.
 *
 * @module harness/oracle/cases/list-indentation
 */

import {
  applyListIndentUnit,
  detectListIndentUnit,
  indentSourceLines,
  normalizeListIndentForLexical,
  outdentSourceLines,
} from '@/components/chat/lexical/transformers/list-indentation'

// ---------------------------------------------------------------------------
// detectListIndentUnit — every arm of v4's test file.
// ---------------------------------------------------------------------------

interface DetectCase {
  name: string
  md: string
  unit: number
}

const detectInputs: { name: string; md: string }[] = [
  { name: 'two-space bullets', md: '- a\n  - b' },
  { name: 'three-space bullets', md: '- a\n   - b' },
  { name: 'four-space bullets', md: '- a\n    - b' },
  { name: 'tab bullets', md: '- a\n\t- b' },
  { name: 'no nesting at all', md: '- a\n- b' },
  { name: 'no lists at all', md: 'Just prose.' },
  {
    name: 'three-column step under an ordered parent reads as the two-space style',
    md: '1. a\n   - b',
  },
  {
    name: 'prefers the bullet-parented step when both kinds are present',
    md: '1. a\n     - b\n\n- c\n    - d',
  },
]
const detectCases: DetectCase[] = detectInputs.map((c) => ({ ...c, unit: detectListIndentUnit(c.md) }))

// ---------------------------------------------------------------------------
// applyListIndentUnit — re-indent + marker-width-floor + idempotency arms.
// ---------------------------------------------------------------------------

interface ApplyCase {
  name: string
  md: string
  unit: number
  out: string
}

const applyInputs: { name: string; md: string; unit: number }[] = [
  {
    name: 're-indents exported output back to the document unit',
    md: ['- Jackie (3 nodes)', '    - Implant', '    - Notation Engine'].join('\n'),
    unit: 2,
  },
  {
    name: 'never indents a child narrower than a `1. ` parent marker',
    md: '1. a\n    - b',
    unit: 2,
  },
  {
    name: 'never indents a child narrower than a `10. ` parent marker',
    md: '10. a\n    - b',
    unit: 2,
  },
  {
    name: 'accumulates nested levels from the emitted parent indent',
    md: ['- a', '    - b', '        - c'].join('\n'),
    unit: 2,
  },
  // --- idempotency: applying a document's own detected unit is a no-op -------
  { name: 'idempotent at 2-unit, three levels deep', md: '- a\n  - b\n    - c\n- d', unit: 2 },
  { name: 'idempotent at 4-unit, three levels deep', md: '- a\n    - b\n        - c\n- d', unit: 4 },
  {
    name: 'idempotent nested ordered list (marker-width floor already satisfied)',
    md: '1. a\n   1. b\n2. c',
    unit: 2,
  },
  // --- tier-2 item 8: three-space bullets stay three-space -------------------
  { name: 'three-space bullets are idempotent at unit 3', md: '- a\n   - b\n- c', unit: 3 },
]
const applyCases: ApplyCase[] = applyInputs.map((c) => ({ ...c, out: applyListIndentUnit(c.md, c.unit) }))

// ---------------------------------------------------------------------------
// Source-mode indent/outdent — chained cases recorded from v4's REAL output at
// each step, so an intermediate value is v4's own, never reimplemented.
// ---------------------------------------------------------------------------

interface SourceCase {
  name: string
  value: string
  start: number
  end: number
  unit: number
  direction: 'indent' | 'outdent'
  outValue: string
  outCursor: number
}

const sourceCases: SourceCase[] = []
function recordSource(
  name: string,
  value: string,
  start: number,
  end: number,
  unit: number,
  direction: 'indent' | 'outdent',
): { value: string; cursor: number } {
  const result = direction === 'indent' ? indentSourceLines({ value, start, end }, unit) : outdentSourceLines({ value, start, end }, unit)
  sourceCases.push({ name, value, start, end, unit, direction, outValue: result.value, outCursor: result.cursor })
  return result
}

{
  const value = ['- a', '- b', '- c'].join('\n')
  recordSource('indents the line the caret sits on', value, 5, 5, 2, 'indent')
  recordSource('indents every list line the selection touches', value, 1, 9, 2, 'indent')
}

{
  // Outdent by at most one unit, stopping at column zero — chained through
  // v4's own successive outputs.
  const indented = ['- a', '    - b'].join('\n')
  const once = recordSource('outdents by one unit', indented, 6, 6, 2, 'outdent')
  const twice = recordSource('outdents again to column zero', once.value, 6, 6, 2, 'outdent')
  recordSource('outdent at column zero is a no-op', twice.value, 5, 5, 2, 'outdent')
}

recordSource('leaves non-list lines alone', ['Some prose.', '- a'].join('\n'), 0, 0, 2, 'indent')
recordSource('outdents a tab-indented line', '- a\n\t- b', 5, 5, 2, 'outdent')

// ---------------------------------------------------------------------------
// normalizeListIndentForLexical — the IMPORT-side four-space rewrite.
//
// Added 2026-08-03, when the P4.D40 ruling's evidence condition was met: the
// dogfood scan of the Friday copy's document stores found 30 instances of the
// (a)-edge shape across five real Obsidian documents, where a v5 save flattened
// the nesting permanently. See the order's status header.
//
// Two groups: v4's own test inventory (the first seven), then the four shapes
// the scan actually found — which the original disposition never contemplated,
// because a two-digit ordered marker needs FOUR columns and a tab lands short
// of a nested parent's content column.
// ---------------------------------------------------------------------------

interface NormalizeCase {
  name: string
  md: string
  out: string
}

const normalizeInputs: { name: string; md: string }[] = [
  // --- v4's own test file, arm for arm ---------------------------------------
  {
    name: 'lifts two-space nesting onto the four-space grid',
    md: ['- Jackie (3 nodes)', '  - Implant', '  - Notation Engine', '- Charlie'].join('\n'),
  },
  { name: 'three-space nesting', md: '- a\n   - b' },
  { name: 'tab nesting', md: '- a\n\t- b' },
  { name: 'already four-space nesting', md: '- a\n    - b' },
  {
    name: 'resolves depth from structure, not raw column count',
    md: ['- a', '  - b', '    - c', '  - d', '- e'].join('\n'),
  },
  { name: 'nested ordered list', md: '1. a\n   1. b\n2. c' },
  { name: 'mixed markers under an ordered parent', md: '1. a\n   - b' },
  {
    name: 'starts a fresh list after an intervening block',
    md: ['- a', '  - b', '', 'Some prose.', '', '- c', '  - d'].join('\n'),
  },
  {
    name: 'leaves fenced code blocks and frontmatter untouched',
    md: [
      '---', 'tags:', '  - alpha', '  - beta', '---', '',
      '```yaml', 'list:', '  - not a bullet', '```', '',
      '- a', '  - b',
    ].join('\n'),
  },
  {
    name: 'leaves thematic breaks and emphasis alone',
    md: ['---', '', '*emphasis* and text', '', '- a'].join('\n'),
  },

  // --- the shapes the 2026-08-03 dogfood scan found on real documents --------
  {
    name: 'DOGFOOD: two-digit ordered marker with a 3-column child (Future Quilltap Plans.md:76)',
    md: '20. Personal assistant mode\n   - **Risk:** Security, billing, ethics\n   - **Hosting stance:** Will not host',
  },
  {
    name: 'DOGFOOD: one-space bullet child (Best. Song. Ever. (Outlines).md:110)',
    md: '* The book\n * How to read it\n * My take on the theme',
  },
  {
    name: 'DOGFOOD: tab child under a 2-column nested ordered item (Worldview Syllabus.md:28)',
    md: '4. Week four\n  5. (March 30): [[Week 5 - Sinning]]\n\t- **Key Scripture:** Romans 7:14-25\n\t- **Topics:** Slavery to sin',
  },
  {
    name: 'DOGFOOD: the v4-canonical control that was never at risk',
    md: '1. a\n   - b',
  },
]
const normalizeCases: NormalizeCase[] = normalizeInputs.map((c) => ({
  ...c,
  out: normalizeListIndentForLexical(c.md),
}))

// ---------------------------------------------------------------------------
// Emit.
// ---------------------------------------------------------------------------

const corpus = {
  _meta: {
    note: 'Generated by harness/oracle/cases/list-indentation.ts from v4 real components/chat/lexical/transformers/list-indentation.ts. Do not hand-edit; regenerate.',
    baseline: 'c4d4b0de',
  },
  detect: detectCases,
  apply: applyCases,
  source: sourceCases,
  normalize: normalizeCases,
}

process.stdout.write(JSON.stringify(corpus, null, 2) + '\n')
