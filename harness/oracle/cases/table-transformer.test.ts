/**
 * Tier-1 oracle — v4's GFM table transformer.
 *
 * Drives v4's REAL `COMPOSER_TRANSFORMERS` (which include the real
 * `TABLE_TRANSFORMER`, `MarkdownBridgePlugin.tsx:86`) through a headless Lexical
 * editor: markdown → `$convertFromMarkdownString` → `$convertToMarkdownString` →
 * markdown, i.e. exactly the bridge's own import/export pair, with v4's
 * `shouldPreserveNewLines: true` on both sides and v4's export escape-strip
 * (`stripMarkdownEscapes`, the four `preserve*` flags default true) applied to
 * the output as `MarkdownBridgePlugin` does (:158-180).
 *
 * NOTE — the order says to extend "the P4.6ag gate's own recorder recipe", the
 * "jsdom/Lexical harness that produced the 28 committed entries". No such
 * harness exists: `markdown-round-trip.spec.ts`'s 28 entries were HAND-AUTHORED
 * from per-entry citations to v4 transformers, not machine-recorded (there is no
 * recorder anywhere in `harness/oracle/`). This file is that recorder, built
 * from scratch — which is strictly better for tables, whose export is lossy in
 * ways nobody would guess by reading.
 *
 * `createEditor` needs no DOM: without a root element Lexical skips
 * reconciliation, so update/read run headless. (v4 has no `@lexical/headless`.)
 *
 * Regenerate (from the v4 checkout, Node 24; jest ignores `/.claude/` paths, so
 * the file is mirrored to /tmp, and the mirror needs a node_modules symlink
 * because this oracle imports bare packages — see the P4.6ax status-log record):
 *
 *   V5=~/source/quilltap-v5
 *   rm -rf /tmp/qt-oracle-tables && mkdir -p /tmp/qt-oracle-tables
 *   cp $V5/harness/oracle/cases/table-transformer.test.ts /tmp/qt-oracle-tables/
 *   ln -s ~/source/quilltap-server/node_modules /tmp/qt-oracle-tables/node_modules
 *   cd ~/source/quilltap-server
 *   PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   QT_ORACLE_OUT=/tmp/oracle-tables.ndjson \
 *     npx jest --silent --roots /tmp/qt-oracle-tables -- table-transformer
 *
 * @module harness/oracle/cases/table-transformer
 */

import { writeFileSync } from 'fs'
import { createEditor } from 'lexical'
import { $convertFromMarkdownString, $convertToMarkdownString } from '@lexical/markdown'
import { HeadingNode, QuoteNode } from '@lexical/rich-text'
import { ListNode, ListItemNode } from '@lexical/list'
import { LinkNode } from '@lexical/link'
import { CodeNode, CodeHighlightNode } from '@lexical/code'
import { TableNode, TableCellNode, TableRowNode } from '@lexical/table'

import {
  COMPOSER_TRANSFORMERS,
  stripMarkdownEscapes,
} from '@/components/chat/lexical/plugins/MarkdownBridgePlugin'

/** v4 `DEFAULT_PRESERVED_MARKDOWN_CHARS` — the four `preserve*` flags default true. */
const PRESERVED = ['*', '_', '`', '~']

/** The node set v4 registers for the markdown editor (`MarkdownLexicalEditor.tsx:164-174`). */
const NODES = [
  HeadingNode,
  QuoteNode,
  ListNode,
  ListItemNode,
  LinkNode,
  CodeNode,
  CodeHighlightNode,
  TableNode,
  TableCellNode,
  TableRowNode,
]

/** markdown → Lexical → markdown, through v4's real bridge pair. */
function roundTrip(markdown: string): string {
  const editor = createEditor({ nodes: NODES, onError: (e: Error) => { throw e } })

  let out = ''
  editor.update(
    () => {
      $convertFromMarkdownString(markdown, COMPOSER_TRANSFORMERS, undefined, true)
    },
    { discrete: true },
  )
  editor.getEditorState().read(() => {
    out = $convertToMarkdownString(COMPOSER_TRANSFORMERS, undefined, true)
  })
  return stripMarkdownEscapes(out, PRESERVED)
}

/**
 * The corpus. Tables only (the 28 non-table gate entries are hand-authored and
 * byte-frozen elsewhere). Covers v4's import guards, its always-left/padded
 * export, and the deliberate round-trip lossiness.
 */
const CASES: { name: string; md: string; trace: string }[] = [
  {
    name: 'canonical padded 2-col table',
    md: '| a   | b   |\n| --- | --- |\n| 1   | 2   |',
    trace: 'import :137-197 + export :202-252 — already in v4 export form',
  },
  {
    name: 'unpadded cells are PADDED on export',
    md: '|a|b|\n|---|---|\n|1|2|',
    trace: 'export pads to per-column width, min 3 (:224-228)',
  },
  {
    name: 'ragged widths pad to the widest cell in the column',
    md: '| name | x |\n| --- | --- |\n| a much longer cell | 2 |',
    trace: 'widths[c] = Math.max(3, ...column lengths) (:226)',
  },
  {
    name: 'center alignment is LOST — export always writes left',
    md: '| a   | b   |\n| :-: | :-: |\n| 1   | 2   |',
    trace: "alignments = Array(colCount).fill('left') (:232) — parseAlignment's result is discarded",
  },
  {
    name: 'right alignment is LOST — export always writes left',
    md: '| a   | b   |\n| ---: | ---: |\n| 1   | 2   |',
    trace: 'export :232 — Lexical table cells store no alignment',
  },
  {
    name: 'mixed alignments all flatten to left',
    md: '| a | b | c |\n| :--- | :---: | ---: |\n| 1 | 2 | 3 |',
    trace: 'export :232',
  },
  {
    name: 'header-and-separator only (no data rows)',
    md: '| a   | b   |\n| --- | --- |',
    trace: 'tableLines.length >= 2 is satisfied (:150)',
  },
  {
    name: 'three columns, three data rows',
    md: '| h1 | h2 | h3 |\n| --- | --- | --- |\n| a | b | c |\n| d | e | f |\n| g | h | i |',
    trace: 'data rows from index 2 (:176-190)',
  },
  {
    name: 'a data row with FEWER cells than the header pads with empties',
    md: '| a | b | c |\n| --- | --- | --- |\n| 1 |',
    trace: 'rowCells[c] || \'\' (:185)',
  },
  {
    name: 'a data row with MORE cells than the header is TRUNCATED',
    md: '| a | b |\n| --- | --- |\n| 1 | 2 | 3 |',
    trace: 'the cell loop runs c < colCount (:181)',
  },
  {
    name: 'escaped pipes survive as escaped pipes',
    md: '| a | b |\n| --- | --- |\n| x \\| y | 2 |',
    trace: 'splitRow un-escapes (:57-60), escapeCell re-escapes (:95-97)',
  },
  {
    name: 'inline markdown inside a cell stays LITERAL text',
    md: '| a | b |\n| --- | --- |\n| **bold** | _em_ |',
    trace: 'import builds $createTextNode(cellText) — cells are never inline-parsed (:169-186)',
  },
  {
    name: 'a separator row that is not second means NO table',
    md: '| a | b |\n| 1 | 2 |\n| --- | --- |',
    trace: 'isSeparatorRow(tableLines[1]) fails → return null (:153)',
  },
  {
    name: 'a single pipe row is not a table',
    md: '| a | b |',
    trace: 'tableLines.length < 2 → return null (:150)',
  },
  {
    name: 'a table with no leading pipe is not matched',
    md: 'a | b\n--- | ---\n1 | 2',
    trace: 'regExpStart /^\\|(.+)\\|?\\s*$/ requires a leading pipe (:124)',
  },
  {
    name: 'text before and after a table',
    md: 'before\n\n| a   | b   |\n| --- | --- |\n| 1   | 2   |\n\nafter',
    trace: 'the multiline transformer consumes only its contiguous pipe rows',
  },
  {
    name: 'a two-dash separator is not a separator (needs 3+)',
    md: '| a | b |\n| -- | -- |\n| 1 | 2 |',
    trace: 'isSeparatorRow /^:?-{3,}:?$/ (:77)',
  },
  {
    name: 'empty cells in the header',
    md: '| a |  | c |\n| --- | --- | --- |\n| 1 | 2 | 3 |',
    trace: 'headerCells[c] || \'\' (:171)',
  },
  {
    name: 'a long separator collapses to the column width',
    md: '| a | b |\n| -------- | -------- |\n| 1 | 2 |',
    trace: "alignmentToSeparator repeats '-' to max(3,width) (:83-90)",
  },
  {
    name: 'a one-column table',
    md: '| only |\n| --- |\n| 1 |',
    trace: 'colCount 1',
  },
]

describe('oracle: v4 GFM table transformer (COMPOSER_TRANSFORMERS round trip)', () => {
  it('records the corpus', () => {
    const rows = CASES.map((c) =>
      JSON.stringify({ name: c.name, md: c.md, out: roundTrip(c.md), trace: c.trace }),
    )

    const out = process.env.QT_ORACLE_OUT
    if (!out) throw new Error('QT_ORACLE_OUT is unset')
    writeFileSync(out, rows.join('\n') + '\n')
    expect(rows).toHaveLength(CASES.length)
  })
})
