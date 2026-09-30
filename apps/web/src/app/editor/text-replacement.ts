import { Mark } from 'prosemirror-model';
import { Plugin } from 'prosemirror-state';

import type { TextReplacementRule } from '../core/core-contract';
import { triggerLeafText } from './char-insert/trigger-context';
import { isInCodeContext } from './code-context';

/**
 * The composer text-replacement plugin — the ProseMirror analogue of v4's
 * `components/chat/lexical/plugins/TextReplacementPlugin.tsx`. On a trigger
 * character, the word just before the caret is looked up in the compiled rules
 * and replaced in place (with the trigger char) in a single transaction (one
 * undo). Composer-only: form fields never install it, and it no-ops unless the
 * host passes non-empty compiled rules.
 *
 * Code is never rewritten: fenced blocks and inline `code` runs both bail
 * through the shared {@link isInCodeContext} guard (v4 bug 63 — what happens
 * when two typing aids keep their own bail lists).
 *
 * @module editor/text-replacement
 */

/** Two O(1) lookup maps + an `empty` short-circuit (v4 `CompiledRules`). */
export interface CompiledRules {
  caseSensitive: Map<string, string>;
  caseInsensitive: Map<string, string>;
  empty: boolean;
}

const EMPTY_COMPILED: CompiledRules = {
  caseSensitive: new Map(),
  caseInsensitive: new Map(),
  empty: true,
};

/**
 * Compile a rule list into case-sensitive / case-insensitive maps (v4
 * `compileRules`). Disabled rules are skipped; within each map the last entry
 * wins (collisions are reported at the API layer).
 */
export function compileRules(rules: TextReplacementRule[]): CompiledRules {
  if (rules.length === 0) return EMPTY_COMPILED;

  const caseSensitive = new Map<string, string>();
  const caseInsensitive = new Map<string, string>();

  for (const rule of rules) {
    if (!rule.enabled) continue;
    if (rule.caseSensitive) {
      caseSensitive.set(rule.fromText, rule.toText);
    } else {
      caseInsensitive.set(rule.fromText.toLowerCase(), rule.toText);
    }
  }

  const empty = caseSensitive.size === 0 && caseInsensitive.size === 0;
  return { caseSensitive, caseInsensitive, empty };
}

/**
 * Look a word up in compiled rules — case-sensitive wins over case-insensitive
 * (v4 `findReplacement`). Returns the replacement, or `undefined` for no match.
 */
export function findReplacement(word: string, compiled: CompiledRules): string | undefined {
  const cs = compiled.caseSensitive.get(word);
  if (cs !== undefined) return cs;
  return compiled.caseInsensitive.get(word.toLowerCase());
}

/**
 * Word-boundary trigger characters (v4 `TRIGGER_CHARS`). Newline is excluded —
 * submit / paragraph-break handlers own that key. `\t` matches v4 verbatim
 * (inert: a Tab keydown's `key` is `"Tab"`, never `"\t"` — kept for fidelity).
 */
export const TRIGGER_CHARS = new Set([
  ' ',
  ' ', // NBSP
  '\t',
  '.',
  ',',
  ';',
  ':',
  '!',
  '?',
  ')',
]);

/**
 * Where the word walk stops: a trigger char, or a soft line break. v4 reads only
 * the anchor text node (`TextReplacementPlugin.tsx:107-121`), and a
 * `LineBreakNode` is a separate node — so the word can never reach across one.
 * v5 reads the whole textblock, where the `hard_break` reads as `\n` (the shared
 * {@link triggerLeafText}); `\n` ends the word here but is NOT a keydown trigger.
 */
function isBoundaryChar(ch: string): boolean {
  return TRIGGER_CHARS.has(ch) || ch === '\n';
}

/**
 * Build the text-replacement plugin. `getRules` is read live on each keydown
 * (so Angular can swap the compiled rules without rebuilding the editor);
 * `null`/empty rules make the plugin inert. Trigger semantics are v4-exact:
 * collapsed caret at the end of the current word, IME composition skipped, the
 * word walked back to a boundary char, replaced + trigger char inserted in one
 * transaction, and the keystroke consumed.
 */
export function textReplacementPlugin(getRules: () => CompiledRules | null): Plugin {
  return new Plugin({
    props: {
      handleKeyDown(view, event) {
        const rules = getRules();
        if (!rules || rules.empty) return false;
        if (view.composing) return false; // IME
        if (!TRIGGER_CHARS.has(event.key)) return false;

        const { selection } = view.state;
        if (!selection.empty) return false;
        // Never rewrite code as it is typed — v4 bug 63. `code_block` IS a
        // textblock, so the `parent.isTextblock` check below passes happily
        // inside a fence, and the inline `code` mark was never consulted at
        // all; both are the shared guard's job now, so the two typing aids
        // cannot drift apart again.
        if (isInCodeContext(view.state)) return false;
        const $from = selection.$from;
        const parent = $from.parent;
        if (!parent.isTextblock) return false;

        const offset = $from.parentOffset;
        // The caret's RUN — v4 reads only the anchor TEXT NODE
        // (`TextReplacementPlugin.tsx:107-121` at `97b25fc53`), and Lexical keeps
        // every differently-formatted stretch as its own node. v5's twin of a
        // node is a maximal span of leaves with an identical mark set. The run
        // is the one ending at the caret (the node before it — where typing
        // leaves the caret). Inline leaf nodes collapse to one placeholder
        // position (a soft break reads `\n`, every other leaf ￼) so the strings
        // stay 1:1 with document positions for the walk-back below.
        const leaves: { start: number; end: number; text: string; marks: readonly Mark[] }[] = [];
        parent.forEach((node, nodeOffset) => {
          leaves.push({
            start: nodeOffset,
            end: nodeOffset + node.nodeSize,
            text: node.isText ? node.text! : triggerLeafText(node),
            marks: node.marks,
          });
        });
        const at = leaves.findIndex((l) => l.start < offset && offset <= l.end);
        if (at < 0) return false; // caret at the start of the block: empty word
        const marks = leaves[at].marks;
        let first = at;
        while (first > 0 && Mark.sameSet(leaves[first - 1].marks, marks)) first--;
        let last = at;
        while (last + 1 < leaves.length && Mark.sameSet(leaves[last + 1].marks, marks)) last++;
        const runText = leaves.slice(first, last + 1).map((l) => l.text).join('');
        const runStart = leaves[first].start;
        const before = runText.slice(0, offset - runStart);
        const after = runText.slice(offset - runStart);

        // Only at the very end of the text node — v4's `offset !== text.length`
        // guard (`:117`). Anything but a soft break after the caret in the same
        // run refuses: a boundary char (`teh| world`) is text in the node too.
        // A `LineBreakNode` is a separate node, so the caret before one still
        // fires. The image placeholder refuses as well (v5's own choice — v4
        // has no inline decorator node).
        if (after.length > 0 && after[0] !== '\n') return false;

        // Walk back across non-boundary chars to the word start.
        let start = before.length;
        while (start > 0 && !isBoundaryChar(before[start - 1])) start--;
        if (start === before.length) return false; // caret sits on a boundary

        const word = before.slice(start);
        const replacement = findReplacement(word, rules);
        if (replacement === undefined) return false;

        event.preventDefault();
        // Replace the word and insert the trigger char in one transaction, so a
        // single undo reverts (v4's "text-replacement"-tagged update).
        const wordStart = $from.pos - word.length;
        const tr = view.state.tr.insertText(replacement + event.key, wordStart, $from.pos);
        view.dispatch(tr.scrollIntoView());
        return true;
      },
    },
  });
}
