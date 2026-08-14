/**
 * The one place an inserted character enters a document (v4
 * `components/chat/char-insert/insert-char.ts`).
 *
 * All four surfaces — the `:` and `\` typeaheads and the two toolbar pickers —
 * commit through here, so they cannot drift apart in what they insert, how it
 * undoes, or whether the pick is remembered.
 *
 * @module editor/char-insert/insert-char
 */

import { closeHistory } from 'prosemirror-history';
import type { EditorView } from 'prosemirror-view';

import { recordRecent } from './recents-storage';
import type { CharProfile } from './types';

/**
 * History tag, carried as transaction meta. v4 wraps every commit in ONE
 * `editor.update(fn, { tag: 'char-insert' })` so a single Cmd/Ctrl+Z removes the
 * character and restores the literal `:smi` / `\to` the user typed — the Layer
 * 1.5 undo contract.
 *
 * ProseMirror has no tag concept, so the contract is kept two ways: one
 * transaction per commit (undo is per transaction), and `closeHistory` on that
 * transaction so `prosemirror-history` cannot fold it into the group of
 * keystrokes that produced the trigger — which would make one undo swallow the
 * typed text as well. The meta is the readable marker, matching v4's tag name.
 *
 * One tag for both profiles: it identifies the KIND of edit, and emoji and
 * Unicode insertions behave identically under undo.
 */
export const CHAR_INSERT_TAG = 'char-insert';

/**
 * Insert a character at the current cursor, for a picker (which has no trigger
 * text to overwrite). The typeaheads use {@link commitCharOverTrigger} instead,
 * and share this module's history contract and recents bookkeeping.
 *
 * No trailing space: an inserted character is punctuation-adjacent, and writers
 * frequently want `word😄`, `😄😄` or `x→y`. Deliberate divergence from v4's
 * chip path — please do not "fix" it into consistency.
 */
export function insertCharAtSelection(
  view: EditorView,
  profile: CharProfile,
  char: string,
): void {
  const tr = view.state.tr.insertText(char);
  tr.setMeta(CHAR_INSERT_TAG, true);
  closeHistory(tr);
  view.dispatch(tr.scrollIntoView());
  recordRecent(profile, char);
}

/**
 * Replace a live trigger (`:smile`, `\to`) with the character it resolved to —
 * the typeahead's commit path, for both the menu-free commit keystroke and a
 * menu pick.
 *
 * `from`/`to` are document positions spanning the opener through the last query
 * character; `suffix` is the profile's `insertSuffix` (empty for emoji, the
 * space the writer typed for Unicode) and is empty on the menu path, where no
 * commit keystroke was swallowed.
 */
export function commitCharOverTrigger(
  view: EditorView,
  profile: CharProfile,
  char: string,
  from: number,
  to: number,
  suffix: string,
): void {
  const tr = view.state.tr.insertText(char + suffix, from, to);
  tr.setMeta(CHAR_INSERT_TAG, true);
  closeHistory(tr);
  view.dispatch(tr.scrollIntoView());
  recordRecent(profile, char);
}
