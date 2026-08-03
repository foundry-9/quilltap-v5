/**
 * List indentation for the v4 composer markdown dialect (the `4f088e7c`
 * editor re-port).
 *
 * v4's Lexical bridge hard-codes a four-space import grid
 * (`LEXICAL_LIST_INDENT_SIZE`) and, on export, reflows every nested list to a
 * fixed unit unless it remembers the unit the document was loaded with — see
 * `components/chat/lexical/transformers/list-indentation.ts` in the v4
 * checkout. This module ports BOTH halves of that fix:
 *
 * - {@link detectListIndentUnit} reads the nesting unit a document was
 *   written with (v4's exact stack algorithm and ordered/bullet-parented
 *   disambiguation, ported verbatim).
 * - {@link applyListIndentUnit} re-indents serializer output to that unit as a
 *   POST-PASS, so a two-space document stays two-space instead of reflowing
 *   to whatever the stock serializer's own delimiter width happens to emit
 *   (v4's item (b): one edit must not reflow every nested line). A child is
 *   never emitted narrower than its parent's marker
 *   (`Math.max(step, markerWidth(parentMarker))` — v4's item (c); v5's stock
 *   `ordered_list` serializer already has this property natively, and this
 *   port preserves it when the unit is not the default).
 * - {@link indentSourceLines} / {@link outdentSourceLines} are the raw-source
 *   (textarea) editing helpers a toolbar button or Tab-equivalent uses in
 *   source mode (v4's item (f)).
 * - {@link setListIndentUnit} / {@link getListIndentUnit} are v4's per-editor
 *   WeakMap memory, generalized from `LexicalEditor` to an arbitrary token
 *   object so v5's `parseMarkdown`/`serializeMarkdown` funnel
 *   (`markdown-dialect.ts`) can thread it through without depending on
 *   ProseMirror or Angular.
 *
 * - {@link normalizeListIndentForLexical} is the IMPORT-side four-space
 *   rewrite (v4's item (a)). This one was DEFERRED at the original port and
 *   adopted 2026-08-03: the reasoning was that v5's CommonMark parser resolves
 *   depth from structure natively and so needed no pre-pass, which held for
 *   every document v4 itself can write and failed for documents written by
 *   anything else. See that function's own doc for the evidence that changed
 *   the ruling.
 *
 * Fenced code blocks and a leading YAML frontmatter block pass through
 * {@link mapListLines} untouched, exactly as v4's `mapListLines` does
 * (item (d)).
 *
 * @module editor/list-indentation
 */

/**
 * A list item line: leading whitespace, a bullet (`-`, `*`, `+`) or ordered
 * marker (`1.`, `1)`), then either whitespace + content or nothing but
 * whitespace. Requiring whitespace before content is what keeps `---` (a
 * thematic break) and `*emphasis*` from being mistaken for list markers.
 */
const LIST_ITEM_RE = /^([ \t]*)([-*+]|\d{1,9}[.)])([ \t]+\S.*|[ \t]*)$/;

/** Opening or closing fence of a fenced code block. */
const FENCE_RE = /^[ \t]*(`{3,}|~{3,})/;

/** Unit used when a document has no nested list to learn from. */
export const DEFAULT_LIST_INDENT_UNIT = 2;

/** Columns a tab advances when measuring indentation. */
const TAB_WIDTH = 4;

/** Bounds on a detected unit, so a malformed document can't produce nonsense. */
const MIN_UNIT = 1;
const MAX_UNIT = 8;

/**
 * Per-editor indentation unit, learned from the document that was loaded into
 * that editor (v4 `editorIndentUnits`, keyed on `LexicalEditor`). Keyed weakly
 * on an arbitrary token object so a disposed editor/field takes its entry
 * with it.
 */
const editorIndentUnits = new WeakMap<object, number>();

/** Remember the indentation unit to export with for this editor. */
export function setListIndentUnit(token: object, unit: number): void {
  editorIndentUnits.set(token, clampUnit(unit));
}

/** The unit remembered for this editor, or {@link DEFAULT_LIST_INDENT_UNIT}. */
export function getListIndentUnit(token: object): number {
  return editorIndentUnits.get(token) ?? DEFAULT_LIST_INDENT_UNIT;
}

function clampUnit(unit: number): number {
  if (!Number.isFinite(unit)) return DEFAULT_LIST_INDENT_UNIT;
  return Math.min(MAX_UNIT, Math.max(MIN_UNIT, Math.round(unit)));
}

/** Column width of a run of leading whitespace, with tabs expanded. */
function indentWidth(whitespace: string): number {
  let width = 0;
  for (const char of whitespace) {
    width = char === '\t' ? width + TAB_WIDTH - (width % TAB_WIDTH) : width + 1;
  }
  return width;
}

/** True for `1.` / `1)` style markers, which are wider than a bullet. */
function isOrderedMarker(marker: string): boolean {
  return /\d/.test(marker);
}

/** Columns a marker occupies including the single space after it. */
function markerWidth(marker: string): number {
  return marker.length + 1;
}

/** A list item line, with its depth resolved from the surrounding document. */
interface ListLineInfo {
  /** 0-based nesting depth. */
  depth: number;
  /** Column width of the line's original leading whitespace. */
  width: number;
  /** The marker itself, e.g. `-` or `1.`. */
  marker: string;
  /** Everything after the marker, including the space that follows it. */
  remainder: string;
  /** The parent item's marker, or null at depth 0. */
  parentMarker: string | null;
  /** The parent item's original indent width, or null at depth 0. */
  parentWidth: number | null;
}

/**
 * Walk `markdown` line by line, resolving each list item's nesting depth from
 * a stack of open levels, and let `map` rewrite the line (returning null
 * keeps it as-is). Fenced code blocks and a leading YAML frontmatter block are
 * copied through untouched (v4 `mapListLines`, ported verbatim).
 */
function mapListLines(
  markdown: string,
  map: (info: ListLineInfo) => string | null,
): string {
  const lines = markdown.split('\n');
  const out: string[] = [];
  /** One entry per currently open nesting level; index === depth. */
  const stack: { width: number; marker: string }[] = [];
  let fence: string | null = null;
  let index = 0;

  // Copy a leading `---` frontmatter block verbatim: its YAML sequences look
  // exactly like bullet lists and must not be re-indented.
  if (lines[0] !== undefined && lines[0].trim() === '---') {
    const close = lines.findIndex((line, i) => i > 0 && line.trim() === '---');
    if (close > 0) {
      for (; index <= close; index++) out.push(lines[index]);
    }
  }

  for (; index < lines.length; index++) {
    const line = lines[index];
    const fenceMatch = FENCE_RE.exec(line);

    if (fence !== null) {
      out.push(line);
      if (fenceMatch && fenceMatch[1][0] === fence[0] && fenceMatch[1].length >= fence.length) {
        fence = null;
      }
      continue;
    }
    if (fenceMatch) {
      fence = fenceMatch[1];
      out.push(line);
      continue;
    }

    const match = LIST_ITEM_RE.exec(line);
    if (!match) {
      // A non-blank line at column 0 that isn't a list item closes the list.
      // Blank lines don't (a list may have blank lines between its items), and
      // neither do indented lines (they continue the current item).
      if (line.trim() !== '' && !/^[ \t]/.test(line)) stack.length = 0;
      out.push(line);
      continue;
    }

    const [, whitespace, marker, remainder] = match;
    const width = indentWidth(whitespace);

    while (stack.length > 0 && width < stack[stack.length - 1].width) stack.pop();
    if (stack.length === 0 || width > stack[stack.length - 1].width) {
      stack.push({ width, marker });
    } else {
      // Same level as the item above: inherit its slot, but record this
      // marker so children of *this* item measure against the right width.
      stack[stack.length - 1] = { width, marker };
    }

    const depth = stack.length - 1;
    const parent = depth > 0 ? stack[depth - 1] : null;
    const rewritten = map({
      depth,
      width,
      marker,
      remainder,
      parentMarker: parent?.marker ?? null,
      parentWidth: parent?.width ?? null,
    });
    out.push(rewritten ?? line);
  }

  return out.join('\n');
}

/**
 * The four-space grid v4's Lexical import counts nesting in
 * (v4 `LEXICAL_LIST_INDENT_SIZE`). v5 has no Lexical, but the width is not
 * arbitrary here either: it is the smallest step that clears EVERY marker's
 * content column (`20. ` is the widest at four), so a normalized document
 * re-parses under CommonMark as the tree its author wrote.
 */
const LEXICAL_LIST_INDENT_SIZE = 4;

/**
 * Rewrite list indentation onto the four-space grid so the parser resolves the
 * nesting the author actually wrote. Depth comes from document STRUCTURE (the
 * `mapListLines` stack), so two-space, three-space, four-space and tab-indented
 * documents all import identically (v4 `normalizeListIndentForLexical`, ported
 * verbatim).
 *
 * WHY v5 needs this despite implementing CommonMark. CommonMark requires a
 * child to reach its parent item's CONTENT column; anything short of it starts
 * a sibling list instead. v4's structural stack has no such rule, so the two
 * disagree on any child indented deeper than its parent but short of that
 * content column — `1. a` with a two-column `- b` under it, and, as real
 * documents turned out to prefer, `20. a` with a THREE-column child (a
 * two-digit marker needs four) or a tab child under a nested ordered item.
 *
 * The P4.D40 disposition was that v5 needed no analog, since v4's own export
 * rule cannot author those bytes. That held for v4-written documents and not
 * for anyone else's: the 2026-08-03 dogfood scan found 30 instances across five
 * real Obsidian documents, and because v5 writes the flattened form back on the
 * next save, the nesting was lost permanently rather than merely rendered
 * wrongly — the same failure shape v4's own `4f088e7c` existed to fix. Running
 * the pre-pass on import is that commit's other half.
 *
 * Pairs with {@link applyListIndentUnit}: normalize on the way in, re-apply the
 * document's own remembered unit on the way out, so a two-space document is
 * still two-space after a round trip.
 */
export function normalizeListIndentForLexical(markdown: string): string {
  return mapListLines(markdown, (info) => {
    const width = info.depth * LEXICAL_LIST_INDENT_SIZE;
    return ' '.repeat(width) + info.marker + info.remainder;
  });
}

/**
 * Re-indent list items to `unit` columns per level. A child is never indented
 * less than its parent's marker is wide, so ordered parents keep their
 * children at (at least) three columns and the result re-parses as the same
 * tree (v4 `applyListIndentUnit`, ported verbatim).
 */
export function applyListIndentUnit(markdown: string, unit: number): string {
  const step = clampUnit(unit);
  /** Emitted indent width per depth, so children measure against the new text. */
  const emitted: number[] = [];

  return mapListLines(markdown, (info) => {
    let width = 0;
    if (info.depth > 0) {
      const parentWidth = emitted[info.depth - 1] ?? 0;
      const parentMarker = info.parentMarker ?? '-';
      width = parentWidth + Math.max(step, markerWidth(parentMarker));
    }
    emitted.length = info.depth + 1;
    emitted[info.depth] = width;
    return ' '.repeat(width) + info.marker + info.remainder;
  });
}

/** A textarea selection, as the source-mode formatting helpers see it. */
export interface SourceSelection {
  value: string;
  start: number;
  end: number;
}

/** The rewritten text plus where the caret should land. */
export interface SourceEdit {
  value: string;
  cursor: number;
}

/**
 * Add or remove one level of indentation on every list-item line touched by
 * the selection, for the raw-Markdown (source) editing mode. Lines that
 * aren't list items are left alone, mirroring the rich-text buttons —
 * Markdown has nowhere to put an indented paragraph (v4 `shiftSourceListLines`,
 * ported verbatim).
 */
function shiftSourceListLines(
  { value, start, end }: SourceSelection,
  unit: number,
  direction: 1 | -1,
): SourceEdit {
  const step = clampUnit(unit);
  const lineStart = value.lastIndexOf('\n', start - 1) + 1;
  const lineEnd = value.indexOf('\n', end);
  const blockEnd = lineEnd === -1 ? value.length : lineEnd;

  const shifted = value
    .slice(lineStart, blockEnd)
    .split('\n')
    .map((line) => {
      const match = LIST_ITEM_RE.exec(line);
      if (!match) return line;
      if (direction === 1) return ' '.repeat(step) + line;
      const [, whitespace] = match;
      if (whitespace === '') return line;
      // Drop one tab, or up to `step` spaces — whichever the line actually uses.
      const trimmed = whitespace.startsWith('\t')
        ? whitespace.slice(1)
        : whitespace.replace(new RegExp(`^ {1,${step}}`), '');
      return trimmed + line.slice(whitespace.length);
    })
    .join('\n');

  return {
    value: value.slice(0, lineStart) + shifted + value.slice(blockEnd),
    cursor: lineStart + shifted.length,
  };
}

/** Nest the selected source lines one level deeper. */
export function indentSourceLines(state: SourceSelection, unit: number): SourceEdit {
  return shiftSourceListLines(state, unit, 1);
}

/** Lift the selected source lines out one nesting level. */
export function outdentSourceLines(state: SourceSelection, unit: number): SourceEdit {
  return shiftSourceListLines(state, unit, -1);
}

/**
 * Read a document's own nesting unit so exports can preserve it.
 *
 * Bullet-parented nesting is the honest signal — an ordered parent forces at
 * least three columns regardless of the author's preference, so a
 * three-column step under `1. ` is read as the conventional two-space style
 * (v4 `detectListIndentUnit`, ported verbatim).
 */
export function detectListIndentUnit(markdown: string): number {
  const bulletSteps: number[] = [];
  const orderedSteps: number[] = [];

  mapListLines(markdown, (info) => {
    if (info.parentWidth === null || info.parentMarker === null) return null;
    const step = info.width - info.parentWidth;
    if (step <= 0) return null;
    if (isOrderedMarker(info.parentMarker)) {
      orderedSteps.push(step);
    } else {
      bulletSteps.push(step);
    }
    return null;
  });

  if (bulletSteps.length > 0) return clampUnit(Math.min(...bulletSteps));
  if (orderedSteps.length > 0) {
    const step = Math.min(...orderedSteps);
    return clampUnit(step === 3 ? DEFAULT_LIST_INDENT_UNIT : step);
  }
  return DEFAULT_LIST_INDENT_UNIT;
}
