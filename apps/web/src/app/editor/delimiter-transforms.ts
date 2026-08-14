import { TextSelection, type Command } from 'prosemirror-state';
import type { Node as PmNode, Schema } from 'prosemirror-model';

import type { NarrationDelimiters, TemplateDelimiter } from '../core/core-contract';
import { toggleWrap, type TextResult, type TextState } from './source-transforms';

/**
 * The roleplay-DELIMITER half of the composer formatting toolbar — the part the
 * form-field toolbar never had (P4.9L).
 *
 * v4 splits it across four sites, all of them ported here so the two editing
 * surfaces (the rich editor and the raw-source textarea) cannot disagree, which
 * is the invariant v4's own module docstring names:
 *
 *  - `lib/chat/annotations.ts` — {@link delimiterToPrefixSuffix} (:66-81) and
 *    {@link getDelimiterTooltip} (:61-75, byte-contractual `title=` strings).
 *  - `lib/chat/text-transforms.ts` — {@link toggleLinePrefix} (:68-88) and
 *    {@link insertTagPrefix} (:96-104). Its third function, `toggleWrap`, was
 *    already ported for the markdown buttons and is re-used from
 *    {@link source-transforms}.
 *  - `components/chat/FormattingToolbar.tsx` — `handleDelimiterClick`'s SOURCE
 *    branch (:317-331) as {@link applySourceDelimiter}, and the narration-button
 *    synthesis + dedupe `useMemo` (:382-413) as {@link visibleDelimiters}.
 *  - `components/chat/lexical/plugins/FormattingCommandPlugin.tsx` —
 *    `APPLY_DELIMITER_COMMAND`'s handler (:176-234) as
 *    {@link applyDelimiterCommand}, the one place a ProseMirror adapter cannot
 *    transliterate Lexical (see the notes on that function).
 *
 * The differential is `delimiter-transforms.spec.ts`, which byte-diffs all four
 * against `__fixtures__/delimiter-oracle.json` — recorded by driving v4's REAL
 * toolbar, transforms and command handler (`apps/web/oracle/
 * delimiter-transforms.test.ts`, whose header carries the regen recipe). Fix the
 * port, not the vectors.
 *
 * @module editor/delimiter-transforms
 */

/**
 * A delimiter's insertion form (v4 `delimiterToPrefixSuffix`,
 * `lib/chat/annotations.ts:66-81`).
 *
 * - `wrap` → its open/close (a string means the SAME on both sides).
 * - `linePrefix` → `{ prefix: marker, suffix: '' }`.
 * - `tagPrefix` → `{ prefix: open, suffix: close }`.
 */
export function delimiterToPrefixSuffix(delimiter: TemplateDelimiter): {
  prefix: string;
  suffix: string;
} {
  switch (delimiter.kind) {
    case 'linePrefix':
      return { prefix: delimiter.marker, suffix: '' };
    case 'tagPrefix':
      return { prefix: delimiter.open, suffix: delimiter.close };
    case 'wrap':
    default:
      if (typeof delimiter.delimiters === 'string') {
        return { prefix: delimiter.delimiters, suffix: delimiter.delimiters };
      }
      return { prefix: delimiter.delimiters[0], suffix: delimiter.delimiters[1] };
  }
}

/**
 * A delimiter button's tooltip (v4 `getDelimiterTooltip`,
 * `lib/chat/annotations.ts:61-75`) — byte-contractual: it is both the `title`
 * and the handle every spec and e2e beat locates the button by.
 *
 * The `wrap` arm's `suffix || 'EOL'` fallback is only reachable through a
 * tuple whose close is empty, which Zod refuses on a write but legacy rows can
 * still carry; the oracle records that case rather than reasoning about it.
 */
export function getDelimiterTooltip(delimiter: TemplateDelimiter): string {
  switch (delimiter.kind) {
    case 'linePrefix':
      return `${delimiter.name} (${delimiter.marker}…EOL)`;
    case 'tagPrefix':
      return `${delimiter.name} (${delimiter.open}TOKEN${delimiter.close} at line start)`;
    case 'wrap':
    default: {
      const { prefix, suffix } = delimiterToPrefixSuffix(delimiter);
      const suffixText = suffix || 'EOL';
      return `${delimiter.name} (${prefix}...${suffixText})`;
    }
  }
}

/**
 * Toggle a line-start `marker` on every line the selection touches, expanding a
 * partial-line selection to whole lines first (v4 `toggleLinePrefix`,
 * `lib/chat/text-transforms.ts:68-88`).
 *
 * It toggles OFF only when EVERY touched line already carries the marker, so a
 * mixed selection adds a second marker to the lines that had one — recorded, not
 * assumed (`linePrefix: a mixed multi-line selection ADDS to every line`).
 */
export function toggleLinePrefix(state: TextState, marker: string): TextResult {
  const { value, start, end } = state;

  const lineStart = value.lastIndexOf('\n', start - 1) + 1;
  const lineEnd = value.indexOf('\n', end);
  const blockEnd = lineEnd === -1 ? value.length : lineEnd;

  const lines = value.slice(lineStart, blockEnd).split('\n');
  const allPrefixed = lines.every((line) => line.startsWith(marker));
  const transformed = (
    allPrefixed
      ? lines.map((line) => line.slice(marker.length))
      : lines.map((line) => `${marker}${line}`)
  ).join('\n');

  return {
    value: value.slice(0, lineStart) + transformed + value.slice(blockEnd),
    cursor: lineStart + transformed.length,
  };
}

/**
 * Insert `open`+`close` at the START of the caret's line, cursor between them so
 * the writer can type the token (v4 `insertTagPrefix`,
 * `lib/chat/text-transforms.ts:96-104`).
 *
 * It never toggles off: a second click stacks another pair. That is v4's
 * behavior, recorded.
 */
export function insertTagPrefix(state: TextState, open: string, close: string): TextResult {
  const { value, start } = state;
  const lineStart = value.lastIndexOf('\n', start - 1) + 1;
  const inserted = `${open}${close}`;
  return {
    value: value.slice(0, lineStart) + inserted + value.slice(lineStart),
    cursor: lineStart + open.length,
  };
}

/**
 * A delimiter button pressed while the raw-source textarea is showing (v4
 * `handleDelimiterClick`'s source branch, `FormattingToolbar.tsx:317-331`):
 * dispatch by kind through the shared pure transforms.
 */
export function applySourceDelimiter(
  state: TextState,
  delimiter: TemplateDelimiter,
): TextResult {
  if (delimiter.kind === 'linePrefix') {
    return toggleLinePrefix(state, delimiter.marker);
  }
  if (delimiter.kind === 'tagPrefix') {
    return insertTagPrefix(state, delimiter.open, delimiter.close);
  }
  const { prefix, suffix } = delimiterToPrefixSuffix(delimiter);
  return toggleWrap(state, prefix, suffix);
}

/**
 * The narration button v4 SYNTHESIZES from the template's `narrationDelimiters`
 * (`FormattingToolbar.tsx:398-404`). It is not a stored delimiter — the template
 * carries only the characters, and the button around them is built here.
 */
export function narrationDelimiterButton(narration: NarrationDelimiters): TemplateDelimiter {
  return {
    kind: 'wrap',
    name: 'Narration',
    buttonName: 'Nar',
    delimiters: narration,
    style: 'qt-chat-narration',
  };
}

/**
 * The delimiter buttons the toolbar shows, in v4's order
 * (`FormattingToolbar.tsx:382-416`): the synthesized narration button first,
 * then the template's own — minus any whose prefix AND suffix both match the
 * narration, so the same marks never get two buttons.
 *
 * The filter runs on `delimiterToPrefixSuffix`, so it is KIND-AGNOSTIC: a
 * `['[' , ']']` narration also swallows a `tagPrefix` opening `[` and closing
 * `]`. Recorded (`the dedupe compares prefix AND suffix…`), and the reason a
 * bracket-narration template shows four buttons where a `*` one shows five.
 */
export function visibleDelimiters(
  templateDelimiters: readonly TemplateDelimiter[],
  narration: NarrationDelimiters | null | undefined,
): TemplateDelimiter[] {
  if (!narration) return [...templateDelimiters];

  const narPrefix = Array.isArray(narration) ? narration[0] : narration;
  const narSuffix = Array.isArray(narration) ? narration[1] : narration;

  const filtered = templateDelimiters.filter((d) => {
    const { prefix, suffix } = delimiterToPrefixSuffix(d);
    return prefix !== narPrefix || suffix !== narSuffix;
  });

  return [narrationDelimiterButton(narration), ...filtered];
}

/**
 * Build the inline nodes for a raw string, turning each `\n` into the dialect's
 * soft break. v4 inserts the transformed text with Lexical's `insertRawText`,
 * which does the same thing — measured: a wrap across two paragraphs comes back
 * as `((one\ntwo))`, ONE newline, i.e. a line break rather than a paragraph
 * split.
 */
function rawTextNodes(schema: Schema, text: string): PmNode[] {
  const nodes: PmNode[] = [];
  text.split('\n').forEach((part, index) => {
    if (index > 0) nodes.push(schema.nodes['hard_break'].create());
    if (part) nodes.push(schema.text(part));
  });
  return nodes;
}

/**
 * Apply a delimiter to the live rich editor — v4's `APPLY_DELIMITER_COMMAND`
 * handler (`FormattingCommandPlugin.tsx:176-234`) as a ProseMirror
 * {@link Command}.
 *
 * Three places this cannot transliterate Lexical, each following v4's INTENT
 * rather than its API (the P4.D75 rule):
 *
 * 1. **The wrap arm's cursor.** v4 inserts the transformed run, then pulls the
 *    caret back by `newText.length - cursor` because `insertRawText` always
 *    lands it at the end. ProseMirror sets an explicit position instead, so the
 *    same two cases collapse to one expression: `from + cursor`. Proven
 *    equivalent by the oracle's caret column (which the spec reads as an offset
 *    from the block start — the only frame the two editors share).
 * 2. **The line arms' block text.** v4 reads `getTopLevelElement()`; PM's
 *    `$from.parent` is the TEXTBLOCK, which is the same node for every shape
 *    this toolbar can reach and, unlike the top-level element, is the one whose
 *    `textContent` offsets map 1:1 onto document positions.
 * 3. **Marks.** v4 replaces the block's children with a single text node, so a
 *    bold run inside it loses its bold; `replaceWith` a bare text node does the
 *    same. Recorded rather than inferred (`wrap: a selected BOLD run — the
 *    raw-text insert drops the mark`).
 *
 * Code blocks are refused, as in v4 (`applied: false`, document untouched) —
 * a `// ` marker inside a fence would be code, not an aside.
 */
export function applyDelimiterCommand(delimiter: TemplateDelimiter): Command {
  return (state, dispatch) => {
    const { schema, selection } = state;
    const { $from, from, to } = selection;

    if (delimiter.kind === 'wrap') {
      const { prefix, suffix } = delimiterToPrefixSuffix(delimiter);
      // v4 runs the transform over a mini flat model of just the selected text
      // (`\n` between blocks, matching Lexical's `getTextContent`).
      const selected = state.doc.textBetween(from, to, '\n');
      const { value, cursor } = toggleWrap(
        { value: selected, start: 0, end: selected.length },
        prefix,
        suffix,
      );
      if (dispatch) {
        const tr = state.tr.replaceWith(from, to, rawTextNodes(schema, value));
        const target = Math.min(from + cursor, tr.doc.content.size);
        tr.setSelection(TextSelection.create(tr.doc, target));
        dispatch(tr.scrollIntoView());
      }
      return true;
    }

    // linePrefix / tagPrefix style the WHOLE line, so they rewrite the caret's
    // block. v4 skips code blocks; so does this.
    const block = $from.parent;
    if (!block.isTextblock || block.type === schema.nodes['code_block']) return false;

    const blockStart = $from.start();
    const blockText = block.textContent;
    const result =
      delimiter.kind === 'linePrefix'
        ? toggleLinePrefix(
            { value: blockText, start: 0, end: blockText.length },
            delimiter.marker,
          )
        : insertTagPrefix({ value: blockText, start: 0, end: 0 }, delimiter.open, delimiter.close);

    if (dispatch) {
      const tr = state.tr.replaceWith(
        blockStart,
        blockStart + block.content.size,
        rawTextNodes(schema, result.value),
      );
      const target = Math.min(blockStart + result.cursor, tr.doc.content.size);
      tr.setSelection(TextSelection.create(tr.doc, target));
      dispatch(tr.scrollIntoView());
    }
    return true;
  };
}
