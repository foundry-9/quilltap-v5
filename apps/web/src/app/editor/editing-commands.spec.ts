import { EditorState, TextSelection, type Command } from 'prosemirror-state';
import { describe, expect, it } from 'vitest';

import { dialectFormattingKeymap } from './editing-commands';
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
