import { setBlockType, toggleMark, wrapIn } from 'prosemirror-commands';
import {
  inputRules,
  textblockTypeInputRule,
  wrappingInputRule,
  type InputRule,
} from 'prosemirror-inputrules';
import type { Schema } from 'prosemirror-model';
import { liftListItem, sinkListItem, wrapInList } from 'prosemirror-schema-list';
import { type Command, type Plugin } from 'prosemirror-state';

/**
 * Markdown input shortcuts + the formatting command set for the dialect editor
 * (v4 `FormattingCommandPlugin` + Lexical's `MarkdownShortcut`, scoped to
 * `COMPOSER_TRANSFORMERS`). No toolbar — v5 has none today; these are keyboard
 * and type-as-you-go affordances only.
 *
 * DELIBERATELY no inline-emphasis input rule: v4 excludes `ITALIC_STAR`, so a
 * typed `*narration*` must stay literal text (dialect quirk #6). Emphasis is
 * reached only through the `Mod-i`/`Mod-b` commands, never by typing the
 * delimiters.
 *
 * @module editor/editing-commands
 */

/** The block-level markdown input rules within the v4 transformer scope. */
export function dialectInputRules(schema: Schema): Plugin {
  const rules: InputRule[] = [
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
