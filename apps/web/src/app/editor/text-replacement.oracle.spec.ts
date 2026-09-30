/**
 * Tier-1 replay of v4's REAL `TextReplacementPlugin` (P4.132).
 *
 * `oracle/fixtures/text-replacement-plugin.json` is written by
 * `oracle/text-replacement-plugin.capture.test.tsx`, run INSIDE a v4 pin: it
 * mounts the real Lexical plugin, seeds each shape, presses the key and records
 * `{defaultPrevented, text}`. Every row is replayed here through the ProseMirror
 * plugin and the boolean + resulting text must match — fix the PORT, never the
 * JSON.
 *
 * Two Lexical facts the rows pin: v4's guard is "caret at the end of the anchor
 * TEXT NODE" (`TextReplacementPlugin.tsx:117`), and a `LineBreakNode` and every
 * differently-formatted run are separate nodes. v5's twin of a node is a run of
 * leaves with an identical mark set.
 */
import { EditorState, TextSelection } from 'prosemirror-state';
import { EditorView } from 'prosemirror-view';
import { describe, expect, it } from 'vitest';

import oracle from '../../../oracle/fixtures/text-replacement-plugin.json';
import { dialectSchema } from './markdown-dialect';
import { compileRules, textReplacementPlugin } from './text-replacement';

type Part = string | { bold: string };
interface Row {
  id: string;
  parts: Part[];
  caret: { node: number; offset: number };
  key: string;
  defaultPrevented: boolean;
  text: string;
}

const RULES = compileRules([
  {
    id: 'r1',
    fromText: 'teh',
    toText: 'the',
    caseSensitive: false,
    enabled: true,
    sortOrder: 0,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
  },
]);

/**
 * Rows whose v4 answer depends on WHICH Lexical node the caret anchors to — the
 * position between a bold run and the plain run after it is ONE ProseMirror
 * position, so v5 cannot tell `(bold, end)` from `(plain, 0)`. v5 takes the
 * node BEFORE the caret (where typing leaves it); the other anchoring is a
 * recorded divergence, pinned in both directions.
 */
const EXPECTED_DIVERGENCES = new Set([
  'bold-word-then-plain-anchor-plain-start',
  'bold-word-then-plain-word-anchor-plain-start',
]);

function build(parts: readonly (Part | 'br')[]) {
  const s = dialectSchema;
  const strong = s.marks['strong'];
  const nodes = parts.map((p) =>
    p === 'br'
      ? s.nodes['hard_break'].create()
      : typeof p === 'string'
        ? s.text(p)
        : s.text(p.bold, [strong.create()]),
  );
  const mount = document.createElement('div');
  document.body.appendChild(mount);
  const plugin = textReplacementPlugin(() => RULES);
  const state = EditorState.create({
    doc: s.nodes['doc'].create(null, s.nodes['paragraph'].create(null, nodes)),
    plugins: [plugin],
  });
  return { view: new EditorView(mount, { state }), plugin, nodes };
}

describe('textReplacementPlugin — replay of v4 TextReplacementPlugin (Lexical)', () => {
  const rows = oracle as unknown as Row[];

  it('recorded every shape', () => {
    expect(rows.length).toBe(22);
  });

  let diverged = 0;
  for (const row of rows) {
    it(`${row.id}`, () => {
      const { view, plugin, nodes } = build(row.parts as (Part | 'br')[]);
      let pos = 1 + row.caret.offset;
      for (let i = 0; i < row.caret.node; i++) pos += nodes[i].nodeSize;
      view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, pos)));
      const handled =
        plugin.props!.handleKeyDown!.call(plugin, view, new KeyboardEvent('keydown', { key: row.key })) ===
        true;
      const text = view.state.doc.textBetween(0, view.state.doc.content.size, undefined, '\n');
      if (EXPECTED_DIVERGENCES.has(row.id)) {
        diverged++;
        // v4 did NOT fire (anchor at the start of the next node); v5 does.
        expect(row.defaultPrevented, `${row.id}: VANISHED — v4 now fires`).toBe(false);
        expect(handled, `${row.id}: WRONG SHAPE`).toBe(true);
      } else {
        expect(handled).toBe(row.defaultPrevented);
        expect(text).toBe(row.text);
      }
      view.destroy();
    });
  }

  it('exercised every recorded divergence', () => {
    expect(diverged).toBe(EXPECTED_DIVERGENCES.size);
  });
});
