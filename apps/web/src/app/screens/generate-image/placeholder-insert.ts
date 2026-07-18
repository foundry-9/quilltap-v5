/** The result of one placeholder splice: the new text and where the caret goes. */
export interface PlaceholderInsert {
  value: string;
  caret: number;
}

/**
 * Splice `{{name}}` into `text` over the range `[start, end)`, returning the new
 * value and the caret offset that follows the inserted placeholder.
 *
 * A transcription of v4 `app/generate-image/GenerateImageView.tsx:71-89` (the
 * identical body also appears in `components/chat/
 * StandaloneGenerateImageDialog.tsx:43-62`). Two details v4 fixes that are easy
 * to get wrong:
 *
 * - The insert REPLACES the selection (`before + placeholder + after`, where
 *   `after` resumes at `selectionEnd`) — it does not merely insert at the caret.
 * - The caret lands at `start + placeholder.length`, i.e. after the closing
 *   braces, so consecutive inserts chain instead of nesting.
 *
 * Extracted as a pure function so both consumers share one implementation and
 * the behavior can be pinned without a DOM.
 */
export function insertPlaceholderAt(
  text: string,
  name: string,
  start: number,
  end: number,
): PlaceholderInsert {
  const placeholder = `{{${name}}}`;
  const before = text.substring(0, start);
  const after = text.substring(end);
  return { value: before + placeholder + after, caret: start + placeholder.length };
}
