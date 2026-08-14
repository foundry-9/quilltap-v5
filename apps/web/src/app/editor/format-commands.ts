import { setBlockType, toggleMark, wrapIn } from 'prosemirror-commands';
import { liftListItem, sinkListItem, wrapInList } from 'prosemirror-schema-list';
import type { Command } from 'prosemirror-state';

import type { FormatAction } from './formatting-toolbar';
import { dialectSchema } from './markdown-dialect';

/**
 * A toolbar {@link FormatAction} as the ProseMirror command that performs it —
 * v5's counterpart to v4's `FormattingCommandPlugin` command set, which
 * `handleMarkdownClick`/`handleCodeBlockClick`/`handleIndentClick` dispatch into
 * Lexical (`FormattingToolbar.tsx:207-236, 277-300, 315-338`).
 *
 * Extracted from `markdown-field.ts` in P4.9L, when the Salon composer became a
 * second host for the same toolbar: the two must dispatch the same command for
 * the same button or a format would mean one thing in a form field and another
 * in a message.
 *
 * `inCodeBlock` is the caret's current block state, which is what makes the
 * code-block button a TOGGLE rather than a one-way conversion (v4 reads the
 * same flag off its editor's update listener).
 *
 * @module editor/format-commands
 */
export function formatCommand(action: FormatAction, inCodeBlock: boolean): Command {
  const schema = dialectSchema;
  switch (action.kind) {
    case 'bold':
      return toggleMark(schema.marks['strong']);
    case 'italic':
      return toggleMark(schema.marks['em']);
    case 'heading':
      return setBlockType(schema.nodes['heading'], { level: action.level });
    case 'ul':
      return wrapInList(schema.nodes['bullet_list']);
    case 'ol':
      return wrapInList(schema.nodes['ordered_list']);
    case 'blockquote':
      return wrapIn(schema.nodes['blockquote']);
    case 'codeBlock':
      // Toggle: leave a code block back to a paragraph, else enter one (v4).
      return inCodeBlock
        ? setBlockType(schema.nodes['paragraph'])
        : setBlockType(schema.nodes['code_block']);
    case 'outdent':
      return liftListItem(schema.nodes['list_item']);
    case 'indent':
      return sinkListItem(schema.nodes['list_item']);
  }
}
