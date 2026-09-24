/**
 * Shared cursor context for the composer's inline typeaheads (v4
 * `components/chat/lexical/typeahead/trigger-context.ts`, `3376b3dfa`).
 *
 * The `:` / `\` character typeahead and the `@` mention typeahead ask the same
 * question of the editor before opening a menu — what text precedes the cursor
 * — so they share one answer rather than keeping copies that drift. v4 moved
 * its two helpers here verbatim when the mention typeahead arrived; v5 has one
 * helper, because reading the whole textblock already answers v4's second
 * question (`$isGluedToPreviousRun` — see `char-typeahead-plugin.ts`'s header).
 *
 * @module editor/char-insert/trigger-context
 */

import type { Node } from 'prosemirror-model';
import type { EditorState } from 'prosemirror-state';

import { isInCodeContext } from '../code-context';

/**
 * Inline leaf nodes (an image) collapse to ONE placeholder so the extracted
 * string stays 1:1 with document positions — the same device
 * `text-replacement.ts` uses. U+FFFC is in no query alphabet and is not
 * word-opening context, so a leaf immediately before an opener correctly
 * refuses to trigger.
 */
export const LEAF = '\uFFFC';

/**
 * A soft line break reads as `\n`, every other leaf as {@link LEAF}. Still one
 * character per leaf, so offsets stay 1:1 with positions.
 *
 * v4's LineBreakNode's text IS `\n` (measured on `lexical` at `b0b6656b5`), and
 * that is what makes a trigger at the start of a soft-broken line open there:
 * `$isGluedToPreviousRun` sees whitespace before it, and the mention
 * typeahead's line-start rule counts lines by it. Collapsing the `hard_break`
 * to U+FFFC instead made v5's char typeahead refuse `:smi` after Shift+Enter —
 * a latent divergence closed by P4.D224.
 */
export function triggerLeafText(node: Node): string {
  return node.type.name === 'hard_break' ? '\n' : LEAF;
}

/**
 * The text of the current textblock up to the caret, with its document offset.
 *
 * Returns null in every position a typing aid must stay out of: a non-collapsed
 * selection, a fenced code block, an inline `code` run (both via the shared
 * {@link isInCodeContext}).
 */
export function textBeforeCursor(
  state: EditorState,
): { text: string; blockStart: number } | null {
  const { selection } = state;
  if (!selection.empty) return null;
  if (isInCodeContext(state)) return null;

  const $from = selection.$from;
  const parent = $from.parent;
  if (!parent.isTextblock) return null;

  const offset = $from.parentOffset;
  return {
    text: parent.textBetween(0, offset, undefined, triggerLeafText),
    blockStart: $from.start(),
  };
}
