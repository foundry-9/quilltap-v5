import { setBlockType, toggleMark, wrapIn } from 'prosemirror-commands';
import {
  InputRule,
  inputRules,
  textblockTypeInputRule,
  wrappingInputRule,
} from 'prosemirror-inputrules';
import type { MarkType, Schema } from 'prosemirror-model';
import { liftListItem, sinkListItem, wrapInList } from 'prosemirror-schema-list';
import { type Command, type Plugin } from 'prosemirror-state';

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
 * `` `code` `` (INLINE_CODE), `~~strike~~` (STRIKETHROUGH), `==highlight==`
 * (HIGHLIGHT). `__bold__` (BOLD_UNDERSCORE) is NOT wired as an input rule (typed
 * `__bold__` stays literal and still serializes byte-faithfully; it is a named
 * live-typing remainder — see the status log). Underscore italic requires a
 * non-word left flank (no intra-word `a_b_`); the `**`/`` ` ``/`~~`/`==` rules
 * flank on punctuation too (markdown allows `a**b**`). None fire when either
 * inner edge is whitespace (CommonMark flanking).
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
