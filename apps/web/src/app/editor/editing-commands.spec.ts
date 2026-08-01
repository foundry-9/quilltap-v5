import type { Node as PMNode } from 'prosemirror-model';
import { EditorState, TextSelection, type Command } from 'prosemirror-state';
import { describe, expect, it } from 'vitest';

import {
  dialectFormattingKeymap,
  dialectInputRules,
  dialectListNavigationKeymap,
  selectionInListItem,
} from './editing-commands';
import { dialectSchema, parseMarkdown, serializeMarkdown } from './markdown-dialect';

const keys = dialectFormattingKeymap(dialectSchema);

/** Build a state over `md`, select the first paragraph's text, run `key`, serialize. */
function runOnParagraph(md: string, key: string): string {
  const doc = parseMarkdown(md);
  let state = EditorState.create({ doc, schema: dialectSchema });
  // Select the text of the first text block (positions 1 .. 1+len).
  const first = doc.firstChild!;
  const start = 1;
  const end = start + first.content.size;
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, start, end)));

  const cmd = keys[key] as Command;
  let next = state;
  cmd(state, (tr) => {
    next = state.apply(tr);
  });
  return serializeMarkdown(next.doc);
}

describe('dialectFormattingKeymap (v4 FormattingCommandPlugin)', () => {
  it('Mod-b wraps the selection in bold (**…**)', () => {
    expect(runOnParagraph('hello world', 'Mod-b')).toBe('**hello world**');
  });

  it('Mod-i wraps the selection in underscore italic (_…_), never *…*', () => {
    expect(runOnParagraph('hello world', 'Mod-i')).toBe('_hello world_');
  });

  it('Mod-` wraps the selection in inline code', () => {
    expect(runOnParagraph('hello', 'Mod-`')).toBe('`hello`');
  });

  it('Shift-Ctrl-2 makes the block an h2', () => {
    expect(runOnParagraph('a title', 'Shift-Ctrl-2')).toBe('## a title');
  });

  it('Ctrl-> wraps the block in a blockquote', () => {
    expect(runOnParagraph('quote me', 'Ctrl->')).toBe('> quote me');
  });

  it('Shift-Ctrl-8 makes a bullet list (serialized with `-`)', () => {
    expect(runOnParagraph('item', 'Shift-Ctrl-8')).toBe('- item');
  });

  it('Shift-Ctrl-9 makes an ordered list', () => {
    expect(runOnParagraph('item', 'Shift-Ctrl-9')).toBe('1. item');
  });
});

// ---------------------------------------------------------------------------
// Emphasis-on-type input rules (v4 Lexical MarkdownShortcut scope)
// ---------------------------------------------------------------------------

const irPlugin = dialectInputRules(dialectSchema);

/**
 * Simulate typing the closing delimiter's final character. `before` is the
 * literal paragraph text already in the document (delimiters and all bar the
 * last char), `typed` is the keystroke that completes the closer. Drives the
 * `inputRules` plugin's `handleTextInput` exactly as ProseMirror would, then
 * serializes — so we prove the on-type conversion, not just the parser.
 */
function typeClosing(before: string, typed: string): string {
  const para = before
    ? dialectSchema.node('paragraph', null, dialectSchema.text(before))
    : dialectSchema.node('paragraph');
  const doc = dialectSchema.node('doc', null, para);
  let state = EditorState.create({ doc, schema: dialectSchema, plugins: [irPlugin] });
  const cursor = 1 + before.length; // paragraph text starts at doc position 1
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, cursor)));

  const view = {
    state,
    composing: false,
    dispatch(tr: import('prosemirror-state').Transaction) {
      view.state = view.state.apply(tr);
    },
  };
  const handled = irPlugin.props.handleTextInput!.call(
    irPlugin,
    view as never,
    cursor,
    cursor,
    typed,
    () => view.state.tr.insertText(typed, cursor, cursor),
  );
  if (!handled) {
    // ProseMirror's default: insert the typed character when no rule fired.
    view.state = view.state.apply(view.state.tr.insertText(typed, cursor, cursor));
  }
  return serializeMarkdown(view.state.doc);
}

describe('dialectInputRules — emphasis-on-type (v4 MarkdownShortcut)', () => {
  it('`**bold**` converts when the closing `*` is typed', () => {
    expect(typeClosing('**bold*', '*')).toBe('**bold**');
  });

  it('`_italic_` converts on the closing `_` (underscore italic)', () => {
    expect(typeClosing('_italic', '_')).toBe('_italic_');
  });

  it('`` `code` `` converts on the closing backtick', () => {
    expect(typeClosing('`code', '`')).toBe('`code`');
  });

  it('`~~strike~~` converts on the closing `~` (STRIKETHROUGH)', () => {
    expect(typeClosing('~~struck~', '~')).toBe('~~struck~~');
  });

  it('`==highlight==` converts on the closing `=` (HIGHLIGHT)', () => {
    expect(typeClosing('==marked=', '=')).toBe('==marked==');
  });

  it('single `*narration*` NEVER auto-formats (ITALIC_STAR excluded)', () => {
    expect(typeClosing('*word', '*')).toBe('*word*');
  });

  it('intra-word underscore does not italicize (`a_b_` stays literal)', () => {
    expect(typeClosing('a_b', '_')).toBe('a_b_');
  });

  // --- BOLD_UNDERSCORE (@lexical/markdown `{format:['bold'], intraword:false,
  // tag:'__'}`, in v4's COMPOSER_TRANSFORMERS at MarkdownBridgePlugin.tsx:71) ---

  it('`__bold__` converts on the closing `_` and normalizes to `**bold**`', () => {
    // Strong serializes as `**` (BOLD_STAR is the first bold transformer, so
    // Lexical's export dedups to it) — v4 normalizes the same way.
    expect(typeClosing('__bold_', '_')).toBe('**bold**');
  });

  it('`__bold__` converts mid-line after prose', () => {
    expect(typeClosing('say __now_', '_')).toBe('say **now**');
  });

  it('intra-word `a__b__` does NOT bolden (BOLD_UNDERSCORE intraword: false)', () => {
    expect(typeClosing('a__b_', '_')).toBe('a__b__');
  });

  it('whitespace inside `__ __` blocks the rule (CommonMark flanking)', () => {
    expect(typeClosing('__hi _', '_')).toBe('__hi __');
  });

  it('a single `_word_` still italicizes, not boldens, alongside the `__` rule', () => {
    expect(typeClosing('_word', '_')).toBe('_word_');
  });

  it('whitespace inside the delimiters blocks the rule (CommonMark flanking)', () => {
    expect(typeClosing('**hi *', '*')).toBe('**hi **');
  });

  it('formats mid-line, after existing prose (`say **now**`)', () => {
    expect(typeClosing('say **now*', '*')).toBe('say **now**');
  });
});

// ---------------------------------------------------------------------------
// selectionInListItem + Tab/Shift-Tab (v4 `$selectionIsInListItem` +
// `FormattingCommandPlugin`'s `KEY_TAB_COMMAND`, list-item-selection.test.ts
// and item (e) of the P4.D40 order)
// ---------------------------------------------------------------------------

/** The offset just past the first occurrence of `text` in the document. */
function textPos(doc: PMNode, text: string): number {
  let found = -1;
  doc.descendants((node, pos) => {
    if (found !== -1) return false;
    if (node.isText) {
      const at = node.text?.indexOf(text) ?? -1;
      if (at !== -1) found = pos + at;
    }
    return true;
  });
  if (found === -1) throw new Error(`text not found: ${text}`);
  return found;
}

function stateFor(md: string): EditorState {
  return EditorState.create({ doc: parseMarkdown(md), schema: dialectSchema });
}

describe('selectionInListItem (v4 $selectionIsInListItem)', () => {
  it.each([
    ['at the start of the text', 0],
    ['mid-word', 2],
    ['at the end of the text', 4],
  ])('is true with the caret %s', (_label, into) => {
    const state = stateFor('- alpha\n- beta');
    const pos = textPos(state.doc, 'beta') + into;
    const next = state.apply(state.tr.setSelection(TextSelection.create(state.doc, pos)));
    expect(selectionInListItem(next)).toBe(true);
  });

  it("is true inside any of an item's inline runs", () => {
    // "**Jackie** (3 nodes)" is a bold run followed by a plain one; the caret
    // must count as "in a list item" in either.
    const state = stateFor('- alpha\n- **Jackie** (3 nodes)');
    const inBold = state.apply(
      state.tr.setSelection(TextSelection.create(state.doc, textPos(state.doc, 'Jackie') + 2)),
    );
    expect(selectionInListItem(inBold)).toBe(true);

    const inPlain = state.apply(
      state.tr.setSelection(TextSelection.create(state.doc, textPos(state.doc, 'nodes') + 2)),
    );
    expect(selectionInListItem(inPlain)).toBe(true);
  });

  it('is true when the caret sits on an empty list item', () => {
    const state = stateFor('- alpha\n- ');
    const list = state.doc.firstChild!;
    const empty = list.lastChild!;
    expect(empty.textContent).toBe('');
    const next = state.apply(state.tr.setSelection(TextSelection.atEnd(state.doc)));
    expect(selectionInListItem(next)).toBe(true);
  });

  it('is true for a selection spanning several list items', () => {
    const state = stateFor('- alpha\n- beta\n- gamma');
    const from = textPos(state.doc, 'alpha') + 1;
    const to = textPos(state.doc, 'gamma') + 2;
    const next = state.apply(state.tr.setSelection(TextSelection.create(state.doc, from, to)));
    expect(selectionInListItem(next)).toBe(true);
  });

  it('is false in a paragraph', () => {
    const state = stateFor('Just prose.\n\n- alpha');
    const next = state.apply(
      state.tr.setSelection(TextSelection.create(state.doc, textPos(state.doc, 'prose') + 3)),
    );
    expect(selectionInListItem(next)).toBe(false);
  });
});

describe('dialectListNavigationKeymap (v4 KEY_TAB_COMMAND)', () => {
  const nav = dialectListNavigationKeymap(dialectSchema);

  it('Tab nests the item the caret sits in', () => {
    const state = stateFor('- alpha\n- beta');
    const at = state.apply(
      state.tr.setSelection(TextSelection.create(state.doc, textPos(state.doc, 'beta') + 2)),
    );
    let next = at;
    const handled = (nav['Tab'] as Command)(at, (tr) => {
      next = at.apply(tr);
    });
    expect(handled).toBe(true);
    expect(serializeMarkdown(next.doc)).toBe('- alpha\n  - beta');
  });

  it('Shift-Tab lifts the item back out', () => {
    const state = stateFor('- alpha\n  - beta');
    const at = state.apply(
      state.tr.setSelection(TextSelection.create(state.doc, textPos(state.doc, 'beta') + 2)),
    );
    let next = at;
    const handled = (nav['Shift-Tab'] as Command)(at, (tr) => {
      next = at.apply(tr);
    });
    expect(handled).toBe(true);
    expect(serializeMarkdown(next.doc)).toBe('- alpha\n- beta');
  });

  it('Tab on a list item that cannot sink is still handled (no focus change)', () => {
    // v4's Tab handler unconditionally confirms/handles once the selection is
    // in a list — even when indenting has nothing to do (the first item of a
    // list has no earlier sibling to nest under).
    const state = stateFor('- alpha\n- beta');
    const at = state.apply(
      state.tr.setSelection(TextSelection.create(state.doc, textPos(state.doc, 'alpha') + 2)),
    );
    let dispatched = false;
    const handled = (nav['Tab'] as Command)(at, () => {
      dispatched = true;
    });
    expect(handled).toBe(true);
    // sinkListItem has nothing to do for a first item — no transaction fires.
    expect(dispatched).toBe(false);
  });

  it('Tab outside a list falls through (browser moves focus)', () => {
    const state = stateFor('Just prose.\n\n- alpha');
    const at = state.apply(
      state.tr.setSelection(TextSelection.create(state.doc, textPos(state.doc, 'prose') + 3)),
    );
    let dispatched = false;
    const handled = (nav['Tab'] as Command)(at, () => {
      dispatched = true;
    });
    expect(handled).toBe(false);
    expect(dispatched).toBe(false);
  });

  it('Shift-Tab outside a list falls through', () => {
    const state = stateFor('Just prose.');
    const at = state.apply(
      state.tr.setSelection(TextSelection.create(state.doc, textPos(state.doc, 'prose') + 3)),
    );
    const handled = (nav['Shift-Tab'] as Command)(at, () => {
      throw new Error('should not dispatch outside a list');
    });
    expect(handled).toBe(false);
  });
});
