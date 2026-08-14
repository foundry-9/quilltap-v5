import { EditorState, TextSelection } from 'prosemirror-state';
import { describe, expect, it } from 'vitest';

import { isInCodeContext } from './code-context';
import { dialectSchema, parseMarkdown } from './markdown-dialect';

/**
 * The shared code-context guard (v4 `components/chat/lexical/utils/
 * code-context.ts`, landed with bug 63). Both typing aids — text replacement
 * and smart typography — bail through THIS predicate, which is the entire
 * point: bug 63 happened because two plugins kept their own bail lists and one
 * of them was wrong.
 *
 * The two representations of code are easy to conflate and both are checked:
 * a fenced `code_block` (which IS a textblock, so an `isTextblock` check waves
 * it straight through — the exact trap bug 63 fell into) and an ordinary text
 * node carrying the inline `code` mark.
 */

/** A state whose caret sits at document position `pos`. */
function stateAt(markdown: string, pos: number): EditorState {
  const doc = parseMarkdown(markdown);
  const state = EditorState.create({ doc });
  return state.apply(state.tr.setSelection(TextSelection.create(doc, pos)));
}

describe('isInCodeContext', () => {
  it('is false in ordinary prose', () => {
    expect(isInCodeContext(stateAt('hello there', 6))).toBe(false);
  });

  it('is TRUE inside a fenced code block', () => {
    const state = stateAt('```\nconst fn = 1\n```', 5);
    // The trap, stated: the parent passes every "is this a normal block?" test.
    expect(state.selection.$from.parent.isTextblock).toBe(true);
    expect(isInCodeContext(state)).toBe(true);
  });

  it('is TRUE inside an inline code run', () => {
    // `fn` occupies positions 1..3, marked `code`.
    const state = stateAt('`fn` and prose', 3);
    expect(isInCodeContext(state)).toBe(true);
  });

  it('is false in the prose that FOLLOWS an inline code run', () => {
    const state = stateAt('`fn` and prose', 12);
    expect(isInCodeContext(state)).toBe(false);
  });

  it('honours storedMarks — the caret is "in code" the moment the mark is armed', () => {
    // A user who clicks the code button before typing has the mark stored on
    // the selection but no code text yet. `$from.marks()` cannot see that.
    const base = stateAt('hello', 3);
    const armed = base.apply(base.tr.addStoredMark(dialectSchema.marks['code'].create()));
    expect(armed.selection.$from.marks().length).toBe(0);
    expect(isInCodeContext(armed)).toBe(true);
  });
});
