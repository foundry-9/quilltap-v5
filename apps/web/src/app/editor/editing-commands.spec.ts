import { EditorState, TextSelection, type Command } from 'prosemirror-state';
import { describe, expect, it } from 'vitest';

import { dialectFormattingKeymap, dialectInputRules } from './editing-commands';
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

  it('whitespace inside the delimiters blocks the rule (CommonMark flanking)', () => {
    expect(typeClosing('**hi *', '*')).toBe('**hi **');
  });

  it('formats mid-line, after existing prose (`say **now**`)', () => {
    expect(typeClosing('say **now*', '*')).toBe('say **now**');
  });
});
