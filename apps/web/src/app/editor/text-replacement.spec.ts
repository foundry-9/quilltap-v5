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

  it('skips a caret mid-line before a space (`teh| world` — v4 `offset !== text.length`, :117)', () => {
    const { view, plugin } = makeView('teh world', RULES);
    expect(trigger(view, plugin, 4, ' ')).toBe(false);
    expect(view.state.doc.textContent).toBe('teh world');
    view.destroy();
  });

  it('skips a caret before punctuation (`teh|.`)', () => {
    const { view, plugin } = makeView('teh.', RULES);
    expect(trigger(view, plugin, 4, '.')).toBe(false);
    expect(view.state.doc.textContent).toBe('teh.');
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

/**
 * P4.D224 / P4.125 — a soft line break is a word boundary. v4's Lexical plugin
 * reads ONLY the anchor text node (`TextReplacementPlugin.tsx:107-121` at
 * `97b25fc53`): a `LineBreakNode` is a separate node, so after `first line⏎` the
 * text node is just `teh` and the rule fires. v5 reads the whole textblock, where
 * the `hard_break` must therefore read as `\n` (the `triggerLeafText` seam the
 * `:` / `@` typeaheads share) and `\n` must end the word walk — while never
 * becoming a keydown TRIGGER (`TRIGGER_CHARS` `:48` has no newline).
 */
describe('textReplacementPlugin — after a soft line break (P4.D224)', () => {
  const RULES = compileRules([rule({ fromText: 'teh', toText: 'the' })]);
  const schema = dialectSchema;

  function makeBroken(...parts: ('br' | 'img' | string)[]) {
    const nodes = parts.map((p) =>
      p === 'br'
        ? schema.nodes['hard_break'].create()
        : p === 'img'
          ? schema.nodes['image'].create({ src: 'x.png', alt: 'x' })
          : schema.text(p),
    );
    const mount = document.createElement('div');
    document.body.appendChild(mount);
    const plugin = textReplacementPlugin(() => RULES);
    const state = EditorState.create({
      doc: schema.nodes['doc'].create(null, schema.nodes['paragraph'].create(null, nodes)),
      plugins: [plugin],
    });
    const view = new EditorView(mount, { state });
    return { view, plugin };
  }

  it('fires on `first line⏎teh` + Space', () => {
    const { view, plugin } = makeBroken('first line', 'br', 'teh');
    const end = view.state.doc.child(0).content.size + 1;
    expect(trigger(view, plugin, end, ' ')).toBe(true);
    expect(view.state.doc.textBetween(0, view.state.doc.content.size, undefined, '\n')).toBe(
      'first line\nthe ',
    );
    view.destroy();
  });

  it('fires when the word is followed by a soft break (caret before the break)', () => {
    const { view, plugin } = makeBroken('teh', 'br', 'more');
    expect(trigger(view, plugin, 4, ' ')).toBe(true);
    view.destroy();
  });

  it('still refuses after an image leaf (v5-only: v4 has no inline decorator node)', () => {
    const { view, plugin } = makeBroken('img', 'teh');
    const end = view.state.doc.child(0).content.size + 1;
    expect(trigger(view, plugin, end, ' ')).toBe(false);
    view.destroy();
  });

  it('a newline key is never a trigger', () => {
    const { view, plugin } = makeBroken('first line', 'br', 'teh');
    const end = view.state.doc.child(0).content.size + 1;
    expect(trigger(view, plugin, end, '\n')).toBe(false);
    view.destroy();
  });
});
