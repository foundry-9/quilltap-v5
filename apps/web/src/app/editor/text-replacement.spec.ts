import { EditorState, TextSelection } from 'prosemirror-state';
import { EditorView } from 'prosemirror-view';
import { describe, expect, it } from 'vitest';

import type { TextReplacementRule } from '../core/core-contract';
import { dialectSchema, parseMarkdown } from './markdown-dialect';
import {
  compileRules,
  findReplacement,
  textReplacementPlugin,
  type CompiledRules,
} from './text-replacement';

function rule(over: Partial<TextReplacementRule>): TextReplacementRule {
  return {
    id: 'r1',
    fromText: 'teh',
    toText: 'the',
    caseSensitive: false,
    enabled: true,
    sortOrder: 0,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  };
}

describe('compileRules / findReplacement (v4 helpers)', () => {
  it('compiles enabled rules into case maps; skips disabled', () => {
    const c = compileRules([
      rule({ fromText: 'teh', toText: 'the', caseSensitive: false }),
      rule({ id: 'r2', fromText: 'API', toText: 'Application', caseSensitive: true }),
      rule({ id: 'r3', fromText: 'off', toText: 'nope', enabled: false }),
    ]);
    expect(c.empty).toBe(false);
    expect(c.caseInsensitive.get('teh')).toBe('the');
    expect(c.caseSensitive.get('API')).toBe('Application');
    expect(c.caseInsensitive.has('off')).toBe(false);
    expect(c.caseSensitive.has('off')).toBe(false);
  });

  it('reports empty when there are no active rules', () => {
    expect(compileRules([]).empty).toBe(true);
    expect(compileRules([rule({ enabled: false })]).empty).toBe(true);
  });

  it('case-sensitive wins over case-insensitive; else case-insensitive fallback', () => {
    const c = compileRules([
      rule({ id: 'a', fromText: 'WTF', toText: 'what the', caseSensitive: true }),
      rule({ id: 'b', fromText: 'wtf', toText: 'insensitive', caseSensitive: false }),
    ]);
    expect(findReplacement('WTF', c)).toBe('what the'); // exact case-sensitive hit
    expect(findReplacement('wtf', c)).toBe('insensitive'); // falls to insensitive
    expect(findReplacement('WtF', c)).toBe('insensitive'); // insensitive by lowercase
    expect(findReplacement('nope', c)).toBeUndefined();
  });
});

// --- the ProseMirror plugin (v4 TextReplacementPlugin trigger semantics) ----

function makeView(md: string, rules: CompiledRules | null) {
  const mount = document.createElement('div');
  document.body.appendChild(mount);
  const plugin = textReplacementPlugin(() => rules);
  const state = EditorState.create({ doc: parseMarkdown(md), plugins: [plugin] });
  const view = new EditorView(mount, { state });
  return { view, plugin, mount };
}

/** Put the caret at document position `pos`, then fire a trigger keydown. */
function trigger(
  view: EditorView,
  plugin: ReturnType<typeof textReplacementPlugin>,
  pos: number,
  key: string,
): boolean {
  view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, pos)));
  const event = new KeyboardEvent('keydown', { key });
  return plugin.props!.handleKeyDown!.call(plugin, view, event) === true;
}

describe('textReplacementPlugin', () => {
  const RULES = compileRules([rule({ fromText: 'teh', toText: 'the' })]);

  it('replaces the word before the caret on a trigger char, inserting the char', () => {
    const { view, plugin } = makeView('teh', RULES);
    // "teh" occupies doc positions 1..4; caret at 4 (end of word).
    const handled = trigger(view, plugin, 4, ' ');
    expect(handled).toBe(true);
    expect(view.state.doc.textContent).toBe('the ');
    view.destroy();
  });

  it('is inert when rules are null or empty', () => {
    const nil = makeView('teh', null);
    expect(trigger(nil.view, nil.plugin, 4, ' ')).toBe(false);
    nil.view.destroy();
    const empty = makeView('teh', compileRules([]));
    expect(trigger(empty.view, empty.plugin, 4, ' ')).toBe(false);
    empty.view.destroy();
  });

  it('does not fire on a non-trigger key', () => {
    const { view, plugin } = makeView('teh', RULES);
    expect(trigger(view, plugin, 4, 'x')).toBe(false);
    expect(view.state.doc.textContent).toBe('teh');
    view.destroy();
  });

  it('skips a mid-word edit (caret not at the end of the word)', () => {
    const { view, plugin } = makeView('tehx', RULES);
    // caret after "teh" but "x" follows (non-boundary) → mid-word, skip.
    expect(trigger(view, plugin, 4, ' ')).toBe(false);
    view.destroy();
  });

  it('does not fire when the caret sits on a boundary (empty word)', () => {
    const { view, plugin } = makeView('teh ', RULES);
    // caret at 5 (after the trailing space) → the char before is a boundary.
    expect(trigger(view, plugin, 5, ' ')).toBe(false);
    view.destroy();
  });

  it('fires mid-line after other prose (`type teh` + space)', () => {
    const { view, plugin } = makeView('type teh', RULES);
    // "type teh" → positions 1..9; caret at 9 (end of "teh").
    expect(trigger(view, plugin, 9, ' ')).toBe(true);
    expect(view.state.doc.textContent).toBe('type the ');
    view.destroy();
  });
});

/**
 * Bug 63 — code is never rewritten. Ported case-for-case from v4's
 * `TextReplacementPlugin.test.tsx` (`48396682`), which was written with the fix
 * because the plugin had no tests at all — which is why the missing guards went
 * unnoticed on both sides for so long. v5 reproduced the bug faithfully:
 * `code_block` is a textblock, so the `parent.isTextblock` check passed inside a
 * fence, and the inline `code` mark was never consulted.
 */
describe('textReplacementPlugin — bug 63, code is never rewritten', () => {
  const RULES = compileRules([rule({ fromText: 'fn', toText: 'function' })]);

  it('does not fire inside a fenced code block', () => {
    const { view, plugin } = makeView('```\nconst fn\n```', RULES);
    // "const fn" sits inside the fence; the caret goes to the end of "fn".
    const pos = view.state.doc.resolve(1).start() + 'const fn'.length;
    expect(trigger(view, plugin, pos, ' ')).toBe(false);
    expect(view.state.doc.textContent).toBe('const fn');
    view.destroy();
  });

  it('does not fire inside an inline code run', () => {
    const { view, plugin } = makeView('`fn`', RULES);
    expect(trigger(view, plugin, 3, ' ')).toBe(false);
    expect(view.state.doc.textContent).toBe('fn');
    view.destroy();
  });

  it('STILL fires in ordinary prose (the regression check)', () => {
    const { view, plugin } = makeView('fn', RULES);
    expect(trigger(view, plugin, 3, ' ')).toBe(true);
    expect(view.state.doc.textContent).toBe('function ');
    view.destroy();
  });
});
