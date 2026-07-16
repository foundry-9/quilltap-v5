import type { FormatAction } from './formatting-toolbar';

/**
 * The source-mode text transforms — what a toolbar button does to the raw
 * markdown `<textarea>` when {@link markdown-field} is in source mode.
 *
 * Ported from two v4 sites, because v4 splits the behavior across them:
 *
 *  - `lib/chat/text-transforms.ts` — the pure `toggleWrap` (:37-59).
 *  - `components/chat/FormattingToolbar.tsx` — `sourcePrefixLines` (:150-165)
 *    and the two dispatch branches, `handleMarkdownClick` (:174-205) and
 *    `handleCodeBlockClick` (:254-275). Those are component-internal closures
 *    in v4; here they are pure functions, so the field stays presentation-only.
 *
 * v4's `sourceApply` (:123-134) has no analogue: it writes `textarea.value`,
 * calls `setInput`, and restores the cursor in a `requestAnimationFrame`. That
 * is the caller's job — these functions only compute `{ value, cursor }`.
 *
 * The differential is `source-transforms.spec.ts`, which byte-diffs every
 * function here against vectors RECORDED from v4's real toolbar rendered in
 * jsdom (`harness/oracle/cases/text-transforms.test.ts`). Fix the port, not the
 * vectors.
 *
 * @module editor/source-transforms
 */

/** A flat slice of editable text with a selection range (v4 `TextState`). */
export interface TextState {
  value: string;
  /** Selection start offset. */
  start: number;
  /** Selection end offset (== start when the selection is collapsed). */
  end: number;
}

/** The result of a transform: the new value and a collapsed cursor offset. */
export interface TextResult {
  value: string;
  cursor: number;
}

/**
 * Toggle wrap/unwrap of `open`…`close` around the selection (v4
 * `lib/chat/text-transforms.ts:37-59`, ported verbatim).
 *
 * - Selection already wrapped → unwrap (cursor at the end of the inner text).
 * - Selection present, not wrapped → wrap (cursor after the close delimiter).
 * - No selection → insert `open`+`close`, cursor between them.
 *
 * A multi-line selection is wrapped as one span, not per line.
 */
export function toggleWrap(state: TextState, open: string, close: string): TextResult {
  const { value, start, end } = state;
  const selected = value.slice(start, end);

  if (selected) {
    const isWrapped =
      selected.length >= open.length + close.length &&
      selected.startsWith(open) &&
      selected.endsWith(close);

    if (isWrapped) {
      const inner = selected.slice(open.length, selected.length - close.length);
      return {
        value: value.slice(0, start) + inner + value.slice(end),
        cursor: start + inner.length,
      };
    }

    const wrapped = `${open}${selected}${close}`;
    return {
      value: value.slice(0, start) + wrapped + value.slice(end),
      cursor: start + wrapped.length,
    };
  }

  // No selection: insert the delimiters and drop the cursor between them.
  const inserted = `${open}${close}`;
  return { value: value.slice(0, start) + inserted + value.slice(end), cursor: start + open.length };
}

/**
 * Add `linePrefix` to the start of every line the selection touches, expanding a
 * partial-line selection to whole lines first (v4 `sourcePrefixLines` :150-165).
 *
 * This ADDS unconditionally — it never toggles a prefix off, so a second H1
 * click yields `# # title`, and an ordered list writes `1. ` on every line
 * rather than counting up. Both are v4's behavior, recorded in the vectors. (v4
 * DOES have a toggling `toggleLinePrefix`, but only its chat-only delimiter
 * buttons call it; no markdown-format button ever does.)
 */
export function prefixLines(state: TextState, linePrefix: string): TextResult {
  const { value, start, end } = state;

  const lineStart = value.lastIndexOf('\n', start - 1) + 1;
  const lineEnd = value.indexOf('\n', end);
  const actualEnd = lineEnd === -1 ? value.length : lineEnd;

  const lines = value.slice(lineStart, actualEnd).split('\n');
  const prefixed = lines.map((line) => `${linePrefix}${line}`).join('\n');

  return {
    value: value.slice(0, lineStart) + prefixed + value.slice(actualEnd),
    cursor: lineStart + prefixed.length,
  };
}

/**
 * Insert an empty fenced code block at the cursor, with the cursor landing
 * inside it (v4 `handleCodeBlockClick`'s empty-selection branch, :262-273). A
 * newline is added on a side only when that side has content not already
 * separated by one.
 */
function insertCodeFence(state: TextState): TextResult {
  const { value, start, end } = state;
  const before = value.slice(0, start);
  const after = value.slice(end);
  const needsNewlineBefore = before.length > 0 && !before.endsWith('\n') ? '\n' : '';
  const needsNewlineAfter = after.length > 0 && !after.startsWith('\n') ? '\n' : '';
  const block = `${needsNewlineBefore}\`\`\`\n\n\`\`\`${needsNewlineAfter}`;

  return {
    value: before + block + after,
    // Inside the block, after the opening ```\n.
    cursor: before.length + needsNewlineBefore.length + 4,
  };
}

/**
 * Apply a toolbar {@link FormatAction} to raw source text — v4's source-mode
 * dispatch (`handleMarkdownClick` :176-204 + `handleCodeBlockClick` :256-275).
 * The markdown delimiters are v4 `MARKDOWN_FORMATS` (`lib/chat/annotations.ts`
 * :42-54).
 */
export function applySourceFormat(state: TextState, action: FormatAction): TextResult {
  switch (action.kind) {
    case 'bold':
      return toggleWrap(state, '**', '**');
    case 'italic':
      return toggleWrap(state, '_', '_');
    case 'heading':
      return prefixLines(state, `${'#'.repeat(action.level)} `);
    case 'ul':
      return prefixLines(state, '- ');
    case 'ol':
      return prefixLines(state, '1. ');
    case 'blockquote':
      return prefixLines(state, '> ');
    case 'codeBlock':
      // A selection gets inline backticks; an empty one gets a fenced block.
      return state.value.slice(state.start, state.end)
        ? toggleWrap(state, '`', '`')
        : insertCodeFence(state);
  }
}
