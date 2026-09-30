/**
 * Tier-1 oracle — v4's REAL `TextReplacementPlugin` (P4.132).
 *
 * The plugin's trigger rule is "the caret is at the very end of the anchor TEXT
 * NODE" (`TextReplacementPlugin.tsx:117`), and the walk-back never leaves that
 * node. A `LineBreakNode` and every differently-formatted run are separate
 * nodes, so the rule cannot be settled by reading the source — this recorder
 * mounts the real plugin in v4's real Lexical harness, seeds each shape, presses
 * the key, and writes `{defaultPrevented, text}` per row. The v5 spec
 * `src/app/editor/text-replacement.oracle.spec.ts` replays every row.
 *
 * A row's `parts` are the paragraph's children in order: a string is a plain
 * text node, `{bold}` a bold text node, `'br'` a `LineBreakNode`. `caret` is
 * `{node, offset}` — the anchor is set on that child with `textNode.select`.
 *
 * ## Regenerating
 *
 * jest ignores paths outside the checkout, so this file is COPIED into a v4
 * worktree pinned at the baseline (drift-ledger §5.1):
 *
 * ```bash
 * V5=<your quilltap-v5 worktree>
 * PIN=/tmp/qt-v4-pin-p4132-97b25fc53
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" 97b25fc53
 * ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
 * cp "$V5/apps/web/oracle/text-replacement-plugin.capture.test.tsx" \
 *    "$PIN/__tests__/unit/zz-p4132-capture.test.tsx"
 * export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
 * (cd "$PIN" && P4132_OUT="$V5/apps/web/oracle/fixtures/text-replacement-plugin.json" \
 *    npx jest --watchman=false __tests__/unit/zz-p4132-capture.test.tsx)
 * git -C ~/source/quilltap-server worktree remove --force "$PIN"
 * ```
 *
 * Then `npm test` in `apps/web`. Fix the PORT, never the recorded JSON.
 */
import fs from 'fs'
import path from 'path'
import React from 'react'
import {
  $createLineBreakNode,
  $createParagraphNode,
  $createTextNode,
  $getRoot,
  type LexicalEditor,
} from 'lexical'

import { TextReplacementPlugin } from '@/components/chat/lexical/plugins/TextReplacementPlugin'
import { compileRules } from '@/lib/text-replacement/useTextReplacementRules'

import { renderPluginEditor, pressKey, type PluginHarness } from '../helpers/lexicalPluginHarness'

jest.mock('@/lib/text-replacement/useTextReplacementRules', () => {
  const actual = jest.requireActual('@/lib/text-replacement/useTextReplacementRules')
  return { ...actual, useTextReplacementRules: jest.fn() }
})

const { useTextReplacementRules } = jest.requireMock(
  '@/lib/text-replacement/useTextReplacementRules',
) as { useTextReplacementRules: jest.Mock }

const OUT = process.env.P4132_OUT || '/tmp/p4132-capture/text-replacement-plugin.json'

const RULES = compileRules([
  {
    id: 'r1',
    fromText: 'teh',
    toText: 'the',
    enabled: true,
    caseSensitive: false,
    createdAt: '',
    updatedAt: '',
  },
] as never)

type Part = string | 'br' | { bold: string }

interface Shape {
  id: string
  parts: Part[]
  caret: { node: number; offset: number }
  key: string
}

const SHAPES: Shape[] = [
  { id: 'end-of-word', parts: ['teh'], caret: { node: 0, offset: 3 }, key: ' ' },
  { id: 'mid-line-caret-before-space', parts: ['teh world'], caret: { node: 0, offset: 3 }, key: ' ' },
  { id: 'mid-word', parts: ['tehx'], caret: { node: 0, offset: 3 }, key: ' ' },
  { id: 'before-punctuation', parts: ['teh.'], caret: { node: 0, offset: 3 }, key: '.' },
  { id: 'before-comma-key-space', parts: ['teh,'], caret: { node: 0, offset: 3 }, key: ' ' },
  { id: 'after-other-prose', parts: ['x teh'], caret: { node: 0, offset: 5 }, key: ' ' },
  { id: 'empty-word', parts: ['teh '], caret: { node: 0, offset: 4 }, key: ' ' },
  { id: 'comma-key-end', parts: ['teh'], caret: { node: 0, offset: 3 }, key: ',' },
  { id: 'non-trigger-key', parts: ['teh'], caret: { node: 0, offset: 3 }, key: 'x' },
  { id: 'caret-before-break', parts: ['teh', 'br', 'more'], caret: { node: 0, offset: 3 }, key: ' ' },
  { id: 'caret-end-after-break-no-rule', parts: ['teh', 'br', 'more'], caret: { node: 2, offset: 4 }, key: ' ' },
  { id: 'word-after-break', parts: ['first line', 'br', 'teh'], caret: { node: 2, offset: 3 }, key: ' ' },
  { id: 'caret-before-break-then-word', parts: ['teh', 'br', 'teh'], caret: { node: 0, offset: 3 }, key: ' ' },
  { id: 'break-first-word-end', parts: ['teh', 'br'], caret: { node: 0, offset: 3 }, key: ' ' },
  { id: 'bold-split-word-end', parts: [{ bold: 'te' }, 'h'], caret: { node: 1, offset: 1 }, key: ' ' },
  { id: 'plain-then-bold-tail', parts: ['te', { bold: 'h' }], caret: { node: 1, offset: 1 }, key: ' ' },
  { id: 'bold-whole-word', parts: [{ bold: 'teh' }], caret: { node: 0, offset: 3 }, key: ' ' },
  { id: 'plain-then-bold-word', parts: ['x ', { bold: 'teh' }], caret: { node: 1, offset: 3 }, key: ' ' },
  { id: 'bold-word-then-plain-anchor-bold-end', parts: [{ bold: 'teh' }, ' plain'], caret: { node: 0, offset: 3 }, key: ' ' },
  { id: 'bold-word-then-plain-anchor-plain-start', parts: [{ bold: 'teh' }, ' plain'], caret: { node: 1, offset: 0 }, key: ' ' },
  { id: 'bold-word-then-plain-word-anchor-bold-end', parts: [{ bold: 'teh' }, 'more'], caret: { node: 0, offset: 3 }, key: ' ' },
  { id: 'bold-word-then-plain-word-anchor-plain-start', parts: [{ bold: 'teh' }, 'more'], caret: { node: 1, offset: 0 }, key: ' ' },
]

function seed(editor: LexicalEditor, shape: Shape): void {
  editor.update(
    () => {
      const root = $getRoot()
      root.clear()
      const paragraph = $createParagraphNode()
      const nodes = shape.parts.map((p) => {
        if (p === 'br') return $createLineBreakNode()
        if (typeof p === 'string') return $createTextNode(p)
        const n = $createTextNode(p.bold)
        n.setFormat('bold')
        return n
      })
      paragraph.append(...nodes)
      root.append(paragraph)
      const anchor = nodes[shape.caret.node] as ReturnType<typeof $createTextNode>
      anchor.select(shape.caret.offset, shape.caret.offset)
    },
    { discrete: true },
  )
}

function readFlat(editor: LexicalEditor): string {
  let text = ''
  editor.getEditorState().read(() => {
    const walk = (node: { getType(): string; getTextContent(): string; getChildren?: () => unknown[] }): string =>
      node.getType() === 'linebreak'
        ? '\n'
        : node.getChildren
          ? (node.getChildren() as (typeof node)[]).map(walk).join('')
          : node.getTextContent()
    text = walk($getRoot() as never)
  })
  return text
}

describe('v4 TextReplacementPlugin capture', () => {
  let harness: PluginHarness | undefined
  const realFetch = global.fetch

  beforeEach(() => {
    global.fetch = jest.fn(async () => ({
      ok: true,
      status: 200,
      statusText: 'OK',
      json: async () => ({ textReplacementsEnabled: true }),
      text: async () => '{"textReplacementsEnabled":true}',
    })) as unknown as typeof fetch
    useTextReplacementRules.mockReturnValue({ compiled: RULES })
  })
  afterEach(() => harness?.unmount())
  afterAll(() => {
    global.fetch = realFetch
  })

  it('records every shape', () => {
    const rows = SHAPES.map((shape) => {
      harness = renderPluginEditor(<TextReplacementPlugin />)
      seed(harness.editor, shape)
      const { defaultPrevented } = pressKey(harness.editor, shape.key)
      const row = { ...shape, defaultPrevented, text: readFlat(harness.editor) }
      harness.unmount()
      harness = undefined
      return row
    })
    fs.mkdirSync(path.dirname(OUT), { recursive: true })
    fs.writeFileSync(OUT, JSON.stringify(rows, null, 2) + '\n')
    expect(rows.length).toBe(SHAPES.length)
  })
})
