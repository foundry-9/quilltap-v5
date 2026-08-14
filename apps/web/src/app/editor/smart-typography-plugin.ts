import { closeHistory } from 'prosemirror-history';
import { Plugin, TextSelection, type EditorState } from 'prosemirror-state';
import type { EditorView } from 'prosemirror-view';

import {
  smartTypographyStep,
  type SmartTypographyOptions,
} from '../smart-typography/engine';
import { isInCodeContext } from './code-context';

/**
 * Smart typography, Part B (Tier C — the adapter): turns `--` into an en dash,
 * `---` into an em dash and `...` into an ellipsis as the writer types, in the
 * Salon composer and the Document Mode rich editor.
 *
 * The ProseMirror twin of v4
 * `components/chat/lexical/plugins/SmartTypographyPlugin.tsx` (`48396682`).
 * Unlike Part A's quote curling — which happens at render time and never
 * touches a stored byte — these substitutions write real characters into the
 * message. That is deliberate: a writer typing `--` unambiguously means a dash,
 * the hyphen being a keyboard workaround for a character with no key, so the
 * dash is genuinely part of what was written and belongs in the content. It is
 * also why dashes are permanently OFF at render time: `run it with --verbose`
 * must survive being written in prose, and here it does, because the writer
 * sees the dash appear and hits Backspace once.
 *
 * **Every decision lives in `smart-typography/engine.ts`, not here.** This file
 * is keystroke plumbing that knows about ProseMirror. If an `if` about dashes
 * appears below, it is in the wrong file.
 *
 * Behavior, matching v4's:
 * - Installed ABOVE `textReplacementPlugin` in `buildPlugins()`, so `.` resolves
 *   as typography before it resolves as a word boundary — v4 gets the same
 *   ordering from COMMAND_PRIORITY_NORMAL over LOW. With a rule
 *   `Aris → Aristarchus the Wise` and a writer typing `Aris...`, the first dot
 *   is a replacement trigger, the second matches nothing, and the third — with
 *   `..` behind the caret — is the ellipsis. Correct in both features, which is
 *   the point of fixing the order rather than leaving it to registration
 *   accident.
 * - Code is never rewritten: fenced blocks and inline `code` runs both bail via
 *   the shared {@link isInCodeContext} guard (bug 63).
 * - IME composition skips the plugin.
 * - Paste does NOT trigger (paste is not a keystroke).
 * - Source-mode editors are a plain `<textarea>` in v5, not this component, so
 *   they are excluded structurally rather than by a flag.
 * - ONE history-closing transaction per substitution, so a single Cmd/Ctrl+Z
 *   undoes it and nothing else.
 * - Backspace immediately after a substitution restores the literal characters.
 *   Both are required: Cmd-Z is what the documentation says, Backspace is what
 *   a writer's hands actually do. Note the two are NOT the same operation, in
 *   either app: the revert puts `--` back, while the undo returns to the state
 *   before the substitution — which is `-`, because the second hyphen's
 *   keystroke was consumed rather than inserted. v4's own suite pins exactly
 *   that (`reverts in ONE undo` expects `Well..`, not `Well...`).
 *
 * @module editor/smart-typography-plugin
 */

/** The only two characters that can start a substitution. Cheapest bail. */
const TRIGGER_CHARS = new Set(['-', '.']);

/**
 * The last substitution, kept only until the very next thing that happens.
 * Backspace with the caret at exactly `to` puts `literal` back.
 */
interface RevertMemo {
  /** Document position where the inserted character starts. */
  from: number;
  /** Document position immediately after it — where the caret must be. */
  to: number;
  /** What the writer actually typed, e.g. `--`, `–-`, `...`. */
  literal: string;
  /** What replaced it, e.g. `–`, `—`, `…`. */
  inserted: string;
}

/**
 * Build the plugin. `getOptions` is read live on each keystroke (so Angular can
 * flip the settings without rebuilding the editor); `null` — the default for
 * every host that is not a composer — makes the plugin inert, as does having
 * both rules off.
 */
export function smartTypographyPlugin(
  getOptions: () => SmartTypographyOptions | null,
): Plugin {
  /**
   * Not plugin state: a revert memo is not part of the document, it must not
   * survive a state rebuild, and — see below — it is invalidated by KEYSTROKES
   * rather than by transactions.
   */
  let memo: RevertMemo | null = null;

  const revert = (view: EditorView): boolean => {
    const current = memo;
    memo = null;
    if (!current) return false;

    const { state } = view;
    const selection = state.selection;
    if (!selection.empty) return false;
    if (selection.$from.pos !== current.to) return false;
    // The characters we put there must still be exactly where we put them.
    if (state.doc.textBetween(current.from, current.to) !== current.inserted) return false;

    const tr = state.tr.insertText(current.literal, current.from, current.to);
    const cursor = current.from + current.literal.length;
    tr.setSelection(TextSelection.create(tr.doc, cursor));
    view.dispatch(closeHistory(tr).scrollIntoView());
    return true;
  };

  /** Up to two characters before the caret, within the current text block. */
  const charsBefore = (state: EditorState): string => {
    const $from = state.selection.$from;
    const start = Math.max(0, $from.parentOffset - 2);
    // Inline leaf nodes collapse to one placeholder so the string stays 1:1
    // with document positions; a placeholder is never a trigger character, so
    // it can only ever prevent a substitution, never mis-place one.
    return $from.parent.textBetween(start, $from.parentOffset, undefined, '￼');
  };

  return new Plugin({
    props: {
      handleDOMEvents: {
        // A blur ends the revert window (v4 registers BLUR_COMMAND for this).
        blur: () => {
          memo = null;
          return false;
        },
      },

      handleKeyDown(view, event) {
        if (event.key === 'Backspace') {
          // Modifier-Backspace deletes a word/line; never a revert.
          if (event.metaKey || event.ctrlKey || event.altKey) {
            memo = null;
            return false;
          }
          return revert(view);
        }

        // Any key that is not the immediate Backspace ends the revert window.
        // Invalidating on the KEYSTROKE rather than in an appendTransaction is
        // deliberate, and v4 explains why on its side: editors fire updates for
        // selection reconciliation and no-op commits too, so a
        // transaction-based memo is cleared before the writer's finger reaches
        // Backspace. The revert itself re-verifies the position and the actual
        // characters before touching anything, so this is the invalidation
        // policy, not the safety check.
        memo = null;

        // 1. Cheapest first: not a trigger character at all.
        if (!TRIGGER_CHARS.has(event.key)) return false;
        // A modified `-`/`.` is a shortcut, not typing.
        if (event.metaKey || event.ctrlKey || event.altKey) return false;

        // 2. Off, or both rules off.
        const options = getOptions();
        if (!options) return false;
        if (!options.dashes && !options.ellipsis) return false;

        // 3. IME composition.
        if (view.composing) return false;

        const { state } = view;
        // 4. Collapsed caret only.
        if (!state.selection.empty) return false;
        // 5. Never rewrite code as it is typed — bug 63. Shared with the text
        //    replacement plugin so the bail lists cannot drift.
        if (isInCodeContext(state)) return false;

        const typed = event.key;
        const before = charsBefore(state);

        const result = smartTypographyStep({ before, typed, options });
        if (result === null) return false;

        // The characters the engine wants removed, and therefore what the
        // writer gets back if they hit Backspace next.
        const deleted = before.slice(before.length - result.deleteBefore);
        if (deleted.length !== result.deleteBefore) return false;
        const literal = deleted + typed;

        const to = state.selection.$from.pos;
        const from = to - result.deleteBefore;
        if (from < 0) return false;

        const tr = state.tr.insertText(result.insert, from, to);
        const cursor = from + result.insert.length;
        tr.setSelection(TextSelection.create(tr.doc, cursor));
        // `closeHistory` makes the substitution its own undo step rather than
        // letting it merge into the run of typing that led up to it — the "one
        // Cmd/Ctrl+Z" half of the contract.
        view.dispatch(closeHistory(tr).scrollIntoView());

        memo = { from, to: cursor, literal, inserted: result.insert };

        // One line per SUBSTITUTION — never per keystroke; this handler runs on
        // every key the composer sees.
        console.debug(`[smart-typography] ${result.rule}: ${literal} → ${result.insert}`);

        return true;
      },
    },
  });
}
