import { setBlockType, toggleMark, wrapIn } from 'prosemirror-commands';
import {
  InputRule,
  inputRules,
  textblockTypeInputRule,
  wrappingInputRule,
} from 'prosemirror-inputrules';
import type { MarkType, ResolvedPos, Schema } from 'prosemirror-model';
import { liftListItem, sinkListItem, wrapInList } from 'prosemirror-schema-list';
import { type Command, type EditorState, type Plugin, TextSelection } from 'prosemirror-state';

/**
 * Markdown input shortcuts + the formatting command set for the dialect editor
 * (v4 `FormattingCommandPlugin` + Lexical's `MarkdownShortcut`, scoped to
 * `COMPOSER_TRANSFORMERS`). Block rules + the on-type inline emphasis rules;
 * the formatting keymap gives the same toggles a toolbar/keyboard entry.
 *
 * No single-`*` inline input rule EVER: v4 excludes `ITALIC_STAR`, so a typed
 * `*narration*` must stay literal text (dialect quirk #6, permanent). The
 * INCLUDED inline delimiters auto-format when the CLOSING delimiter is typed
 * (v4 Lexical MarkdownShortcut, whose `scanDelimiters`/`processEmphasis` is
 * CommonMark flanking): `_italic_` (ITALIC_UNDERSCORE), `**bold**` (BOLD_STAR),
 * `__bold__` (BOLD_UNDERSCORE), `` `code` `` (INLINE_CODE), `~~strike~~`
 * (STRIKETHROUGH), `==highlight==` (HIGHLIGHT). The two underscore rules require
 * a non-word left flank (Lexical `intraword: false` on both ITALIC_UNDERSCORE
 * and BOLD_UNDERSCORE — no intra-word `a_b_`/`a__b__`); the `**`/`` ` ``/`~~`/
 * `==` rules flank on punctuation too (markdown allows `a**b**`). None fire when
 * either inner edge is whitespace (CommonMark flanking).
 *
 * @module editor/editing-commands
 */

/**
 * A ProseMirror input rule that wraps typed `<delim>content<delim>` in a mark
 * once the closing delimiter completes. Group 1 of `regexp` is the content
 * (delimiters excluded), `$`-anchored. Mirrors the widely-used ProseMirror
 * `markInputRule`: the closing delimiter's final char is the just-typed char
 * (never inserted), so we delete only the in-document delimiters and add the
 * mark over the content. No-ops inside a block that disallows the mark (e.g. a
 * fenced code block) so delimiters are never silently stripped there.
 */
function markInputRule(regexp: RegExp, markType: MarkType): InputRule {
  return new InputRule(regexp, (state, match, start, end) => {
    const content = match[1];
    if (content == null) return null;
    if (!state.doc.resolve(start).parent.type.allowsMarkType(markType)) return null;

    const leading = match[0].search(/\S/);
    const contentStart = start + match[0].indexOf(content);
    const contentEnd = contentStart + content.length;

    const tr = state.tr;
    // Delete the in-document trailing delimiter first (positions before it are
    // unaffected), then the leading delimiter (preserving any captured flank ws).
    if (contentEnd < end) tr.delete(contentEnd, end);
    if (contentStart > start + leading) tr.delete(start + leading, contentStart);

    const markFrom = start + leading;
    tr.addMark(markFrom, markFrom + content.length, markType.create());
    tr.removeStoredMark(markType);
    return tr;
  });
}

/** The markdown input rules within the v4 transformer scope. */
export function dialectInputRules(schema: Schema): Plugin {
  const rules: InputRule[] = [
    // --- block rules (COMPOSER_TRANSFORMERS element transformers) ------------
    // `> ` → blockquote (QUOTE).
    wrappingInputRule(/^\s*>\s$/, schema.nodes['blockquote']),
    // `1. ` → ordered list (ORDERED_LIST), continuing an adjacent list's count.
    wrappingInputRule(
      /^(\d+)\.\s$/,
      schema.nodes['ordered_list'],
      (match) => ({ order: +match[1] }),
      (match, node) => node.childCount + (node.attrs['order'] as number) === +match[1],
    ),
    // `- `/`+ `/`* ` → bullet list (UNORDERED_LIST; serializes back to `- `).
    wrappingInputRule(/^\s*([-+*])\s$/, schema.nodes['bullet_list']),
    // ``` ``` → fenced code block (CODE).
    textblockTypeInputRule(/^```$/, schema.nodes['code_block']),
    // `# `..`###### ` → heading (HEADING).
    textblockTypeInputRule(new RegExp('^(#{1,6})\\s$'), schema.nodes['heading'], (match) => ({
      level: match[1].length,
    })),

    // --- inline emphasis-on-type rules (Lexical MarkdownShortcut scope) ------
    // Underscore italic — non-word left flank (no intra-word `a_b_`).
    markInputRule(/(?:^|[^\w_])_((?:[^_\s][^_]*[^_\s])|[^_\s])_$/, schema.marks['em']),
    // `**bold**` — flanks on punctuation too (markdown allows `a**b**`).
    markInputRule(/\*\*((?:[^*\s][^*]*[^*\s])|[^*\s])\*\*$/, schema.marks['strong']),
    // `__bold__` (BOLD_UNDERSCORE) — like underscore italic, a non-word left
    // flank (Lexical `intraword: false`), so `a__b__` stays literal. Strong
    // serializes as `**`, so a typed `__bold__` normalizes to `**bold**` — v4
    // does the same (Lexical's export dedups bold to BOLD_STAR, the first
    // matching transformer), and the round-trip gate pins it.
    markInputRule(/(?:^|[^\w_])__((?:[^_\s][^_]*[^_\s])|[^_\s])__$/, schema.marks['strong']),
    // `` `code` `` inline code.
    markInputRule(/`((?:[^`\s][^`]*[^`\s])|[^`\s])`$/, schema.marks['code']),
    // `~~strike~~` (STRIKETHROUGH).
    markInputRule(/~~((?:[^~\s][^~]*[^~\s])|[^~\s])~~$/, schema.marks['strikethrough']),
    // `==highlight==` (HIGHLIGHT).
    markInputRule(/==((?:[^=\s][^=]*[^=\s])|[^=\s])==$/, schema.marks['highlight']),
  ];
  return inputRules({ rules });
}

/**
 * The formatting command keybindings (v4 `FormattingCommandPlugin`): bold /
 * italic / inline code toggles, headings, lists, blockquote, and list
 * indent/outdent. Merged into the editor's keymap.
 *
 * `Mod-[`/`Mod-]` (list lift/sink) are a v5-only addition — v4 has no
 * keybinding for `INDENT_LIST_ITEM_COMMAND`/`OUTDENT_LIST_ITEM_COMMAND`
 * outside its toolbar and Tab. Kept: they're additive (no v4 binding to
 * collide with) and match the same commands Tab now reaches
 * ({@link dialectListNavigationKeymap}). Recorded per the P4.D40 order.
 */
export function dialectFormattingKeymap(schema: Schema): Record<string, Command> {
  const keys: Record<string, Command> = {
    'Mod-b': toggleMark(schema.marks['strong']),
    'Mod-i': toggleMark(schema.marks['em']),
    'Mod-`': toggleMark(schema.marks['code']),
    'Ctrl->': wrapIn(schema.nodes['blockquote']),
    'Shift-Ctrl-0': setBlockType(schema.nodes['paragraph']),
    'Shift-Ctrl-8': wrapInList(schema.nodes['bullet_list']),
    'Shift-Ctrl-9': wrapInList(schema.nodes['ordered_list']),
    'Mod-[': liftListItem(schema.nodes['list_item']),
    'Mod-]': sinkListItem(schema.nodes['list_item']),
  };
  for (let level = 1; level <= 6; level++) {
    keys[`Shift-Ctrl-${level}`] = setBlockType(schema.nodes['heading'], { level });
  }
  return keys;
}

/**
 * True when a resolved position sits inside a list item at any depth —
 * walking `$pos.node(depth)` covers every shape a ProseMirror selection
 * endpoint can take (a text point, an empty item's element point, a point at
 * either boundary of a non-empty item), so unlike v4's Lexical analog
 * (`$selectionIsInListItem`, whose `getNodes()`-only check went blind on a
 * collapsed *element* point) there is no separate ambiguous-caret case to
 * special-case here — ProseMirror always resolves a position's full ancestor
 * chain.
 */
function resolvedPosInListItem($pos: ResolvedPos, schema: Schema): boolean {
  const listItem = schema.nodes['list_item'];
  for (let depth = $pos.depth; depth >= 0; depth--) {
    if ($pos.node(depth).type === listItem) return true;
  }
  return false;
}

/**
 * True when the current selection touches a list item — at either endpoint,
 * so a selection spanning several items (or from just outside one into it)
 * still counts (v4 `$selectionIsInListItem`, item (e)). Confining
 * indent/outdent to lists is deliberate: Markdown has no notion of an
 * indented paragraph, so indenting one would vanish silently on the next
 * save.
 */
export function selectionInListItem(state: EditorState): boolean {
  const { $from, $to } = state.selection;
  return resolvedPosInListItem($from, state.schema) || resolvedPosInListItem($to, state.schema);
}

/**
 * Tab nests the selected list item(s), Shift-Tab lifts them — confined to
 * list items so Tab still moves focus everywhere else (v4
 * `FormattingCommandPlugin`'s `KEY_TAB_COMMAND` handler, item (e)). Within a
 * list, Tab/Shift-Tab are ALWAYS treated as handled (default prevented) even
 * when sinking/lifting has nothing to do — e.g. Tab on a list's first item —
 * matching v4's unconditional `return true` once the selection is confirmed
 * to be inside a list.
 */
export function dialectListNavigationKeymap(schema: Schema): Record<string, Command> {
  const listItem = schema.nodes['list_item'];
  return {
    Tab: (state, dispatch, view) => {
      if (!selectionInListItem(state)) return false;
      sinkListItem(listItem)(state, dispatch, view);
      return true;
    },
    'Shift-Tab': (state, dispatch, view) => {
      if (!selectionInListItem(state)) return false;
      liftListItem(listItem)(state, dispatch, view);
      return true;
    },
  };
}

/**
 * Enter on a blank trailing line inside a fenced code block leaves the block:
 * the blank line is trimmed away and a fresh paragraph opens after it, caret at
 * its start.
 *
 * The port of v4's code-block escape (`components/chat/lexical/plugins/
 * KeyboardPlugin.tsx:48-110`), which without this has no counterpart here —
 * ``` opens a code block, and every subsequent Enter only ever adds another
 * line to it, so a writer who fences a snippet mid-message can never get back
 * out to prose (dogfood finding #82). Nothing else in the dialect ends a block:
 * the ``` input rule opens one and has no closing form.
 *
 * v4 checks this BEFORE its chat/composition-mode branch, so the escape
 * outranks Enter-submits — which is why this binds ahead of the submit command
 * rather than after it.
 *
 * The conditions are v4's, one for one: a collapsed caret, inside a code block,
 * at the very end of its text, with at least two lines and a blank last one
 * (`trim() === ''`, so a line of spaces counts as blank). The trailing blank is
 * removed with v4's own `/\n\s*$/` — which strips one newline plus any trailing
 * whitespace, not every trailing blank line.
 */
export function exitCodeBlockOnBlankLine(schema: Schema): Command {
  const codeBlock = schema.nodes['code_block'];
  const paragraph = schema.nodes['paragraph'];

  return (state, dispatch) => {
    const { selection } = state;
    if (!selection.empty) return false;

    const $from = selection.$from;
    if ($from.parent.type !== codeBlock) return false;
    // The caret must sit at the very end of the block's text (v4 requires the
    // anchor to be the last child, at its full length).
    if ($from.parentOffset !== $from.parent.content.size) return false;

    const text = $from.parent.textContent;
    const lines = text.split('\n');
    if (lines.length < 2) return false;
    if (lines[lines.length - 1].trim() !== '') return false;

    if (dispatch) {
      const trimmed = text.replace(/\n\s*$/, '');
      const end = $from.end();
      const tr = state.tr;
      // A code block holds text only, so text offsets and document positions
      // run 1:1 and the trailing blank can be deleted as a range rather than
      // by rewriting the block the way v4's node API forces it to.
      if (trimmed.length < text.length) tr.delete(end - (text.length - trimmed.length), end);
      const after = tr.mapping.map($from.after());
      tr.insert(after, paragraph.createAndFill()!);
      tr.setSelection(TextSelection.create(tr.doc, after + 1));
      dispatch(tr.scrollIntoView());
    }
    return true;
  };
}
