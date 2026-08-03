/**
 * The v4 composer markdown dialect, ported to a ProseMirror
 * schema/parser/serializer pair (the D17 make-or-break bridge).
 *
 * v4's editor is Lexical with a bespoke transformer set
 * (`components/chat/lexical/plugins/MarkdownBridgePlugin.tsx`). The bytes on the
 * wire — chat message content and Document-Mode file content — are markdown
 * strings in that dialect, so any replacement editor must round-trip them
 * byte-for-byte. This module reproduces the dialect over ProseMirror + its
 * `prosemirror-markdown` (markdown-it) bridge:
 *
 *  1. **Underscore italic.** v4 `COMPOSER_TRANSFORMERS` excludes `ITALIC_STAR`
 *     and `BOLD_ITALIC_STAR` and includes `ITALIC_UNDERSCORE`
 *     (`MarkdownBridgePlugin.tsx:60-87`). Italic is `_text_`; a single `*` is a
 *     roleplay narration delimiter and must survive as literal text. So the
 *     parser refuses single-`*` emphasis (keeping `**` strong and `_` emphasis),
 *     and the serializer emits `em` as `_…_`.
 *  2. **Literal punctuation survives unescaped.** v4 strips Lexical's auto-escapes
 *     for `* _ \` ~` on export (`stripMarkdownEscapes` + the four `preserve*`
 *     flags default true, `MarkdownBridgePlugin.tsx:120-202`). We port
 *     `stripMarkdownEscapes` verbatim and run it over the serializer output —
 *     `prosemirror-markdown`'s `esc()` backslash-escapes exactly those chars, the
 *     same lossiness v4 neutralizes.
 *  3. **Transformer scope.** Bold (`**`), headings, blockquote, fenced code,
 *     ordered/unordered lists, and case-insensitive check lists
 *     (`- [ ]`/`- [x]`). **Strikethrough (`~~`) and highlight (`==`)** are marks
 *     on every v4 editing surface (`MarkdownBridgePlugin.tsx:35-36,71-74`):
 *     `STRIKETHROUGH` rides markdown-it's own built-in `~~` rule; `HIGHLIGHT` is
 *     a hand-rolled `==` inline rule modeled byte-for-byte on that strikethrough
 *     rule (markdown-it ships no `==` support). Both serialize with the
 *     literal-tilde/equals protection v4's `preserve*` flags apply. The link mark
 *     remains out of the gate's scope (deferred).
 *  4. **GFM tables.** v4's bespoke `TABLE_TRANSFORMER`
 *     (`components/chat/lexical/transformers/table-transformer.ts`) is in
 *     `COMPOSER_TRANSFORMERS`, so it is live on every editing surface. Ported
 *     here as a markdown-it block rule + serializer handlers reproducing its
 *     import guards and its deliberately LOSSY export — see {@link qtTableRule}
 *     and the `table` serializer. Vectors recorded from v4's real transformer
 *     drive `markdown-round-trip.spec.ts`.
 *  5. **Sub-list indentation (the `4f088e7c` re-port, P4.D40).** A document's
 *     own nesting unit survives the round trip instead of reflowing to a
 *     fixed width on the first edit: {@link parseMarkdown} detects the unit a
 *     `unitToken` was loaded with and {@link serializeMarkdown} re-indents to
 *     it as a post-pass ({@link list-indentation}, v4's `detectListIndentUnit`
 *     / `applyListIndentUnit`). Since 2026-08-03 {@link parseMarkdown} also
 *     runs v4's IMPORT-side `normalizeListIndentForLexical` pre-pass, so a
 *     child indented deeper than its parent but short of that parent's CONTENT
 *     column parses as a child rather than starting a sibling list — the shape
 *     real Obsidian documents turned out to carry, and which v5 used to
 *     flatten permanently on save.
 *
 * @module editor/markdown-dialect
 */

import MarkdownIt from 'markdown-it';
import { Schema, type Node as PMNode } from 'prosemirror-model';
import {
  MarkdownParser,
  MarkdownSerializer,
  type MarkdownSerializerState,
  defaultMarkdownParser,
  defaultMarkdownSerializer,
  schema as baseSchema,
} from 'prosemirror-markdown';

import {
  applyListIndentUnit,
  DEFAULT_LIST_INDENT_UNIT,
  detectListIndentUnit,
  getListIndentUnit,
  normalizeListIndentForLexical,
  setListIndentUnit,
} from './list-indentation';

const STAR = 0x2a; // '*'
const UNDERSCORE = 0x5f; // '_'
const EQUALS = 0x3d; // '='

/**
 * The dialect schema: the `prosemirror-markdown` basic schema with a `checked`
 * attribute added to `list_item` so check lists (`- [ ]`/`- [x]`) are modelled
 * as list items rather than literal `[ ]` text (v4 `CASE_INSENSITIVE_CHECK_LIST`).
 * `checked` is `null` for a plain list item, `false`/`true` for a checkbox.
 *
 * Two inline marks join the basic set — `strikethrough` (`~~`, rendered `<s>`)
 * and `highlight` (`==`, rendered `<mark>`) — matching v4's `STRIKETHROUGH` and
 * `HIGHLIGHT` transformers, which are part of the dialect on every editing
 * surface.
 *
 * Three block nodes model v4's GFM tables. `table_cell` holds `text*`, NOT
 * inline content, because v4's import builds each cell with a bare
 * `$createTextNode(cellText)` and never inline-parses it
 * (`table-transformer.ts:169-186`) — `| **bold** |` is the literal characters
 * `**bold**` in v4, and must stay literal here. No alignment attribute exists:
 * v4 parses GFM alignment on import and then DISCARDS it (Lexical table cells
 * store none), which is why its export can only ever write left.
 */
export const dialectSchema = new Schema({
  nodes: baseSchema.spec.nodes
    .update('list_item', {
      ...baseSchema.spec.nodes.get('list_item'),
      attrs: { checked: { default: null } },
    })
    .append({
      table: {
        content: 'table_row+',
        group: 'block',
        isolating: true,
        parseDOM: [{ tag: 'table' }],
        toDOM() {
          return ['table', ['tbody', 0]];
        },
      },
      table_row: {
        content: 'table_cell+',
        parseDOM: [{ tag: 'tr' }],
        toDOM() {
          return ['tr', 0];
        },
      },
      table_cell: {
        content: 'text*',
        attrs: { header: { default: false } },
        isolating: true,
        parseDOM: [{ tag: 'td' }, { tag: 'th', attrs: { header: true } }],
        toDOM(node: PMNode) {
          return [node.attrs['header'] ? 'th' : 'td', 0];
        },
      },
    }),
  marks: baseSchema.spec.marks
    .addToEnd('strikethrough', {
      parseDOM: [{ tag: 's' }, { tag: 'del' }, { style: 'text-decoration=line-through' }],
      toDOM() {
        return ['s', 0];
      },
    })
    .addToEnd('highlight', {
      parseDOM: [{ tag: 'mark' }],
      toDOM() {
        return ['mark', 0];
      },
    }),
});

// ---------------------------------------------------------------------------
// Parser (markdown-it configured for the dialect)
// ---------------------------------------------------------------------------

/**
 * Replacement for markdown-it's `emphasis` post-process rule that refuses to
 * form single-`*` emphasis. Ported from markdown-it's own `postProcess`
 * (`rules_inline/emphasis.mjs`) with one added guard: when the delimiter marker
 * is `*` and the pairing would be a single `em` (not `**` strong), it is left as
 * literal text. `_` emphasis and `**`/`__` strong keep the stock behavior. This
 * is the parser half of v4's `ITALIC_STAR`/`BOLD_ITALIC_STAR` exclusion.
 */
function skipStarEmphasisPostProcess(state: {
  tokens: import('markdown-it/lib/token.mjs').default[];
  delimiters: Delimiter[];
  tokens_meta: (TokenMeta | null)[];
}): void {
  const process = (delimiters: Delimiter[]): void => {
    for (let i = delimiters.length - 1; i >= 0; i--) {
      const startDelim = delimiters[i];
      if (startDelim.marker !== UNDERSCORE && startDelim.marker !== STAR) continue;
      // Only opening markers that were matched to a closer.
      if (startDelim.end === -1) continue;

      const endDelim = delimiters[startDelim.end];

      const isStrong =
        i > 0 &&
        delimiters[i - 1].end === startDelim.end + 1 &&
        delimiters[i - 1].marker === startDelim.marker &&
        delimiters[i - 1].token === startDelim.token - 1 &&
        delimiters[startDelim.end + 1].token === endDelim.token + 1;

      // The dialect divergence: a lone `*` is a literal roleplay delimiter, not
      // italic. Leave its text tokens (content `*`) untouched.
      if (startDelim.marker === STAR && !isStrong) continue;

      const ch = String.fromCharCode(startDelim.marker);
      const tokenO = state.tokens[startDelim.token];
      tokenO.type = isStrong ? 'strong_open' : 'em_open';
      tokenO.tag = isStrong ? 'strong' : 'em';
      tokenO.nesting = 1;
      tokenO.markup = isStrong ? ch + ch : ch;
      tokenO.content = '';

      const tokenC = state.tokens[endDelim.token];
      tokenC.type = isStrong ? 'strong_close' : 'em_close';
      tokenC.tag = isStrong ? 'strong' : 'em';
      tokenC.nesting = -1;
      tokenC.markup = isStrong ? ch + ch : ch;
      tokenC.content = '';

      if (isStrong) {
        state.tokens[delimiters[i - 1].token].content = '';
        state.tokens[delimiters[startDelim.end + 1].token].content = '';
        i--;
      }
    }
  };

  process(state.delimiters);
  const tokensMeta = state.tokens_meta;
  for (let curr = 0; curr < tokensMeta.length; curr++) {
    const meta = tokensMeta[curr];
    if (meta && meta.delimiters) process(meta.delimiters);
  }
}

interface Delimiter {
  marker: number;
  length: number;
  token: number;
  end: number;
  open: boolean;
  close: boolean;
}
interface TokenMeta {
  delimiters: Delimiter[];
}

/**
 * A markdown-it core rule (runs after `inline`) that recognizes GitHub-style
 * task-list items — `- [ ] text` / `- [x] text` (case-insensitive, v4
 * `CASE_INSENSITIVE_CHECK_LIST`). It stamps `checked` on the `list_item_open`
 * token and strips the `[ ] ` prefix from the item's text.
 */
function taskListCoreRule(state: { tokens: import('markdown-it/lib/token.mjs').default[] }): void {
  const tokens = state.tokens;
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i].type !== 'list_item_open') continue;
    const para = tokens[i + 1];
    const inline = tokens[i + 2];
    if (!para || para.type !== 'paragraph_open') continue;
    if (!inline || inline.type !== 'inline') continue;

    const m = /^\[([ xX])\]\s+/.exec(inline.content);
    if (!m) continue;

    const checked = m[1] !== ' ';
    tokens[i].attrSet('checked', checked ? '1' : '0');
    inline.content = inline.content.slice(m[0].length);
    const children = inline.children ?? [];
    if (children.length && children[0].type === 'text') {
      children[0].content = children[0].content.slice(m[0].length);
    }
  }
}

/**
 * markdown-it inline tokenizer for `==highlight==` (v4 Lexical `HIGHLIGHT`, tag
 * `==`). markdown-it ships no `==` support, so this is a byte-for-byte port of
 * markdown-it's own strikethrough tokenizer (`rules_inline/strikethrough.mjs`)
 * with the marker swapped to `=`: it pushes paired delimiters onto the shared
 * delimiter list, which `balance_pairs` then matches marker-agnostically (so
 * pairing is free) and {@link highlightPostProcess} converts to `mark` tokens.
 * `length: 0` disables the emphasis "rule of 3", exactly as strikethrough does.
 */
function highlightTokenize(
  state: {
    pos: number;
    src: string;
    push: (type: string, tag: string, nesting: number) => import('markdown-it/lib/token.mjs').default;
    scanDelims: (start: number, canSplitWord: boolean) => { length: number; can_open: boolean; can_close: boolean };
    delimiters: Delimiter[];
    tokens: import('markdown-it/lib/token.mjs').default[];
  },
  silent: boolean,
): boolean {
  const start = state.pos;
  const marker = state.src.charCodeAt(start);
  if (silent) return false;
  if (marker !== EQUALS) return false;

  const scanned = state.scanDelims(state.pos, true);
  let len = scanned.length;
  const ch = String.fromCharCode(marker);
  if (len < 2) return false;

  let token;
  if (len % 2) {
    token = state.push('text', '', 0);
    token.content = ch;
    len--;
  }
  for (let i = 0; i < len; i += 2) {
    token = state.push('text', '', 0);
    token.content = ch + ch;
    state.delimiters.push({
      marker,
      length: 0,
      token: state.tokens.length - 1,
      end: -1,
      open: scanned.can_open,
      close: scanned.can_close,
    });
  }
  state.pos += scanned.length;
  return true;
}

/** Convert paired `=` delimiters in one delimiter list to `mark` open/close
 *  tokens (v4 `HIGHLIGHT`). Ported from markdown-it strikethrough's postProcess. */
function highlightPostProcessList(
  state: { tokens: import('markdown-it/lib/token.mjs').default[] },
  delimiters: Delimiter[],
): void {
  const loneMarkers: number[] = [];
  const max = delimiters.length;
  for (let i = 0; i < max; i++) {
    const startDelim = delimiters[i];
    if (startDelim.marker !== EQUALS) continue;
    if (startDelim.end === -1) continue;

    const endDelim = delimiters[startDelim.end];

    let token = state.tokens[startDelim.token];
    token.type = 'qt_highlight_open';
    token.tag = 'mark';
    token.nesting = 1;
    token.markup = '==';
    token.content = '';

    token = state.tokens[endDelim.token];
    token.type = 'qt_highlight_close';
    token.tag = 'mark';
    token.nesting = -1;
    token.markup = '==';
    token.content = '';

    const prev = state.tokens[endDelim.token - 1];
    if (prev.type === 'text' && prev.content === '=') {
      loneMarkers.push(endDelim.token - 1);
    }
  }

  // Move a leftover single `=` (odd-length sequence) past subsequent closers —
  // the strikethrough rule's exact loneMarker fixup.
  while (loneMarkers.length) {
    const i = loneMarkers.pop() as number;
    let j = i + 1;
    while (j < state.tokens.length && state.tokens[j].type === 'qt_highlight_close') j++;
    j--;
    if (i !== j) {
      const token = state.tokens[j];
      state.tokens[j] = state.tokens[i];
      state.tokens[i] = token;
    }
  }
}

function highlightPostProcess(state: {
  tokens: import('markdown-it/lib/token.mjs').default[];
  tokens_meta: (TokenMeta | null)[];
  delimiters: Delimiter[];
}): void {
  const tokensMeta = state.tokens_meta;
  const max = tokensMeta.length;
  highlightPostProcessList(state, state.delimiters);
  for (let curr = 0; curr < max; curr++) {
    const meta = tokensMeta[curr];
    if (meta && meta.delimiters) highlightPostProcessList(state, meta.delimiters);
  }
}

// ---------------------------------------------------------------------------
// GFM tables (v4 `TABLE_TRANSFORMER`)
// ---------------------------------------------------------------------------

/**
 * Split a pipe-delimited row into cells, trimming outer pipes and whitespace,
 * un-escaping `\|` (v4 `splitRow`, `table-transformer.ts:48-70`, ported
 * verbatim).
 */
function splitRow(row: string): string[] {
  let trimmed = row.trim();
  if (trimmed.startsWith('|')) trimmed = trimmed.slice(1);
  if (trimmed.endsWith('|')) trimmed = trimmed.slice(0, -1);

  const cells: string[] = [];
  let current = '';
  for (let i = 0; i < trimmed.length; i++) {
    if (trimmed[i] === '\\' && i + 1 < trimmed.length && trimmed[i + 1] === '|') {
      current += '|';
      i++; // skip the pipe
    } else if (trimmed[i] === '|') {
      cells.push(current.trim());
      current = '';
    } else {
      current += trimmed[i];
    }
  }
  cells.push(current.trim());
  return cells;
}

/**
 * Is this line a separator row (v4 `isSeparatorRow`, `:75-79`)? Note the `-{3,}`:
 * a GFM-legal `:-:` has only ONE dash, so v4 does NOT see it as a separator and
 * the whole block stays literal text. Recorded, and pinned in the gate.
 */
function isSeparatorRow(line: string): boolean {
  const cells = splitRow(line);
  if (cells.length === 0) return false;
  return cells.every((cell) => /^:?-{3,}:?$/.test(cell.trim()));
}

/** v4 `escapeCell` (`:95-97`) — only `|` is escaped on export, nothing else. */
function escapeCell(text: string): string {
  return text.replace(/\|/g, '\\|');
}

/** v4's `regExpStart` (`:124`): a table's first line needs a leading pipe. */
const TABLE_START = /^\|(.+)\|?\s*$/;

/**
 * Collect the contiguous table lines from `startLine` (v4
 * `handleImportAfterStartMatch`, `:137-155`), or `null` when this is not a
 * table. v4 takes lines while the line starts with `|`, or contains one once at
 * least one line is banked; it then requires ≥2 lines with the separator SECOND.
 */
function collectTableLines(lines: string[]): string[] | null {
  const tableLines: string[] = [];
  for (const raw of lines) {
    const line = raw.trim();
    if (line.startsWith('|') || (line.includes('|') && tableLines.length > 0)) {
      tableLines.push(line);
    } else {
      break;
    }
  }

  // Need at least a header + separator, and the separator must be second.
  if (tableLines.length < 2) return null;
  if (!isSeparatorRow(tableLines[1])) return null;
  return tableLines;
}

interface BlockState {
  src: string;
  bMarks: number[];
  eMarks: number[];
  tShift: number[];
  line: number;
  lineMax: number;
  Token: new (type: string, tag: string, nesting: number) => import('markdown-it/lib/token.mjs').default;
  push: (type: string, tag: string, nesting: number) => import('markdown-it/lib/token.mjs').default;
}

/** The raw source of block line `i`. */
function lineAt(state: BlockState, i: number): string {
  return state.src.slice(state.bMarks[i] + state.tShift[i], state.eMarks[i]);
}

/**
 * The markdown-it block rule for v4's `TABLE_TRANSFORMER` import. Registered as
 * a paragraph terminator, which is what reproduces v4's retry: when the
 * separator is not second, Lexical's transformer declines the line, the line
 * becomes a paragraph, and the transformer is offered the NEXT line — so
 * `| a | b |\n| 1 | 2 |\n| --- | --- |` yields a paragraph plus a table built
 * from lines 2-3. Terminating the paragraph at the first line that DOES start a
 * table gives the same split.
 *
 * Cells are pushed as bare `text` tokens (never `inline`), so markdown-it's core
 * inline rule leaves them alone and the content stays literal — v4's cells are
 * `$createTextNode` and are never inline-parsed.
 */
function qtTableRule(state: BlockState, startLine: number, endLine: number, silent: boolean): boolean {
  const first = lineAt(state, startLine).trim();
  if (!TABLE_START.test(first)) return false;

  const raw: string[] = [];
  for (let i = startLine; i < endLine; i++) raw.push(lineAt(state, i));

  const tableLines = collectTableLines(raw);
  if (!tableLines) return false;
  // A terminator check only asks "does a table start here?".
  if (silent) return true;

  const headerCells = splitRow(tableLines[0]);
  const colCount = headerCells.length;

  const open = state.push('qt_table_open', 'table', 1);
  open.map = [startLine, startLine + tableLines.length];

  const emitRow = (cells: string[], header: boolean): void => {
    state.push('qt_tr_open', 'tr', 1);
    for (let c = 0; c < colCount; c++) {
      const cell = state.push('qt_td_open', header ? 'th' : 'td', 1);
      if (header) cell.attrSet('header', '1');
      const text = state.push('text', '', 0);
      // v4 pads a short row with empty cells and TRUNCATES a long one — the cell
      // loop runs to the HEADER's column count (`:181`).
      text.content = cells[c] || '';
      state.push('qt_td_close', header ? 'th' : 'td', -1);
    }
    state.push('qt_tr_close', 'tr', -1);
  };

  emitRow(headerCells, true);
  // Data rows start after the header (0) and the separator (1).
  for (let r = 2; r < tableLines.length; r++) emitRow(splitRow(tableLines[r]), false);

  state.push('qt_table_close', 'table', -1);
  state.line = startLine + tableLines.length;
  return true;
}

function buildDialectMarkdownIt(): MarkdownIt {
  // Match prosemirror-markdown's defaultMarkdownParser dialect (CommonMark,
  // inline HTML off), then apply the dialect adjustments.
  const md = new MarkdownIt('commonmark', { html: false });
  md.inline.ruler2.at('emphasis', skipStarEmphasisPostProcess as never);
  md.core.ruler.after('inline', 'qt_task_lists', taskListCoreRule as never);
  // v4 STRIKETHROUGH (`~~`): markdown-it's own rule, off in the commonmark preset.
  md.enable('strikethrough');
  // v4 HIGHLIGHT (`==`): our port, threaded in right after strikethrough so
  // `balance_pairs` (which precedes both) has already paired its delimiters.
  md.inline.ruler.after('strikethrough', 'qt_highlight', highlightTokenize as never);
  md.inline.ruler2.after('strikethrough', 'qt_highlight', highlightPostProcess as never);
  // v4 TABLE_TRANSFORMER. `alt: ['paragraph']` makes it a paragraph terminator,
  // which is how v4's line-by-line retry is reproduced (see {@link qtTableRule}).
  // markdown-it's own `table` rule is NOT used: its semantics differ from v4's on
  // every axis that matters (it inline-parses cells, keeps alignment, and does
  // not require a leading pipe).
  md.block.ruler.before('paragraph', 'qt_table', qtTableRule as never, {
    alt: ['paragraph', 'reference', 'blockquote'],
  });
  return md;
}

/** The dialect parser: markdown string → ProseMirror document. */
export const dialectParser = new MarkdownParser(dialectSchema, buildDialectMarkdownIt(), {
  ...defaultMarkdownParser.tokens,
  // A soft line break (a single `\n` inside a paragraph — v4's Shift+Enter line
  // break) becomes a `hard_break` node rather than the default space, so the
  // newline survives the round-trip. v4/Lexical keeps such breaks and exports
  // them as `\n` (see the hard_break serializer below); collapsing them to a
  // space would corrupt every soft-wrapped paragraph, narration line, and
  // multi-line blockquote.
  softbreak: { node: 'hard_break' },
  // Read the `checked` flag stamped by {@link taskListCoreRule}.
  list_item: {
    block: 'list_item',
    getAttrs: (tok) => {
      const raw = tok.attrGet('checked');
      return { checked: raw === null ? null : raw === '1' };
    },
  },
  // v4 STRIKETHROUGH (`s_open`/`s_close`) and HIGHLIGHT (our `qt_highlight_*`).
  s: { mark: 'strikethrough' },
  qt_highlight: { mark: 'highlight' },
  // v4 TABLE_TRANSFORMER (our `qt_table_*`/`qt_tr_*`/`qt_td_*`).
  qt_table: { block: 'table' },
  qt_tr: { block: 'table_row' },
  qt_td: {
    block: 'table_cell',
    getAttrs: (tok) => ({ header: tok.attrGet('header') === '1' }),
  },
});

// ---------------------------------------------------------------------------
// Serializer
// ---------------------------------------------------------------------------

/**
 * v4 `stripMarkdownEscapes` (`MarkdownBridgePlugin.tsx:128-137`), ported
 * verbatim: remove Lexical's / prosemirror-markdown's automatic backslash
 * escapes for the selected characters so literal punctuation survives.
 */
export function stripMarkdownEscapes(markdown: string, chars: string[]): string {
  const filtered = Array.from(new Set(chars.filter((char) => char.length === 1 && char !== '\\')));
  if (filtered.length === 0) return markdown;

  const escapedClass = filtered.map((char) => char.replace(/[\\\]^-]/g, '\\$&')).join('');

  return markdown.replace(new RegExp(`\\\\([${escapedClass}])`, 'g'), '$1');
}

/** v4 `DEFAULT_PRESERVED_MARKDOWN_CHARS` — the four `preserve*` flags default true. */
const PRESERVED_MARKDOWN_CHARS = ['*', '_', '`', '~'];

/**
 * The dialect serializer: the basic serializer with `em` emitting `_…_` (not
 * `*…*`), unordered lists using `-` with check-list prefixes, and everything
 * else inherited. Output is post-processed with {@link stripMarkdownEscapes}
 * (see {@link serializeMarkdown}).
 */
export const dialectSerializer = new MarkdownSerializer(
  {
    ...defaultMarkdownSerializer.nodes,
    // v4 UNORDERED_LIST uses `- `; base prosemirror-markdown defaults to `* `.
    // Check items ride the same list with a `[ ]`/`[x]` box (v4 CHECK_LIST).
    bullet_list(state: MarkdownSerializerState, node: PMNode) {
      state.renderList(node, '  ', (i: number) => {
        const checked = node.child(i).attrs['checked'] as boolean | null;
        const box = checked === null ? '' : checked ? '[x] ' : '[ ] ';
        return '- ' + box;
      });
    },
    // A line break inside a block is a plain `\n`, not the CommonMark
    // backslash-newline. v4/Lexical exports both soft breaks and two-space hard
    // breaks this way, so the two collapse to one `\n` form (the base
    // serializer's own trailing-break guard is kept).
    hard_break(state: MarkdownSerializerState, node: PMNode, parent: PMNode, index: number) {
      for (let i = index + 1; i < parent.childCount; i++) {
        if (parent.child(i).type !== node.type) {
          state.write('\n');
          return;
        }
      }
    },
    /**
     * v4 `TABLE_TRANSFORMER.export` (`table-transformer.ts:202-252`), reproduced
     * including its lossiness. Cells are padded to the widest in their column
     * (min 3), and **the separator is ALWAYS left-aligned** — v4 hard-codes
     * `Array(colCount).fill('left')` (`:232`) because Lexical stores no
     * alignment, so `---:` in, `---` out. Column count comes from the FIRST row,
     * so a long row is truncated and a short one padded with empties.
     */
    table(state: MarkdownSerializerState, node: PMNode) {
      const rows: string[][] = [];
      node.forEach((row) => {
        const cells: string[] = [];
        row.forEach((cell) => cells.push(escapeCell(cell.textContent)));
        rows.push(cells);
      });
      if (rows.length === 0) return;

      const colCount = rows[0].length;
      const grid = rows.map((row) =>
        Array.from({ length: colCount }, (_, c) => row[c] ?? ''),
      );
      const widths = Array.from({ length: colCount }, (_, c) =>
        Math.max(3, ...grid.map((row) => row[c].length)),
      );
      const line = (cells: string[]): string =>
        `|${cells.map((text, c) => ` ${text.padEnd(widths[c])} `).join('|')}|`;

      const lines = [
        line(grid[0]),
        // The always-left separator: `-` repeated to the column width.
        `|${widths.map((w) => ` ${'-'.repeat(w)} `).join('|')}|`,
        ...grid.slice(1).map(line),
      ];

      // Raw (escape: false) — v4 escapes `|` in a cell and nothing else.
      state.text(lines.join('\n'), false);
      state.closeBlock(node);
    },
  },
  {
    ...defaultMarkdownSerializer.marks,
    // v4 italic is `_text_` (ITALIC_UNDERSCORE), never `*text*`.
    em: { open: '_', close: '_', mixable: true, expelEnclosingWhitespace: true },
    // v4 STRIKETHROUGH (`~~`) and HIGHLIGHT (`==`). The literal-`~`/`=` inside
    // any wrapped text is protected by the export escape-strip below.
    strikethrough: { open: '~~', close: '~~', mixable: true, expelEnclosingWhitespace: true },
    highlight: { open: '==', close: '==', mixable: true, expelEnclosingWhitespace: true },
  },
);

/**
 * Parse a markdown string in the v4 dialect into a ProseMirror document.
 *
 * `unitToken` is the per-editor/field identity (v4's `LexicalEditor`
 * analog — any stable object works, typically the owning component
 * instance): when supplied, the document's list-indentation unit is
 * detected and remembered against it, so a later {@link serializeMarkdown}
 * call for the SAME token re-indents to the unit this document was loaded
 * with rather than the default. Omit it for a one-shot parse that never
 * needs its own export to preserve non-default nesting.
 */
export function parseMarkdown(text: string, unitToken?: object): PMNode {
  // Detect on the ORIGINAL bytes — normalizing first would make every document
  // look like a four-space one and the export would reflow it.
  if (unitToken) setListIndentUnit(unitToken, detectListIndentUnit(text));
  // Then lift nesting onto the four-space grid, so a child indented deeper than
  // its parent but short of that parent's CONTENT column still parses as a
  // child rather than starting a sibling list (v4 `MarkdownBridgePlugin`
  // :163 does the same at its import). Without this, saving flattens the
  // nesting permanently — see `normalizeListIndentForLexical`.
  return dialectParser.parse(normalizeListIndentForLexical(text));
}

/**
 * Serialize a ProseMirror document to a markdown string in the v4 dialect,
 * stripping the automatic escapes for `* _ \` ~` (v4's export post-pass), then
 * re-indenting list nesting to `unitToken`'s remembered unit (or
 * {@link DEFAULT_LIST_INDENT_UNIT} without one) — v4's `applyListIndentUnit`
 * post-pass, so a two-space document stays two-space across every edit.
 */
export function serializeMarkdown(doc: PMNode, unitToken?: object): string {
  const raw = dialectSerializer.serialize(doc);
  const unpreserved = stripMarkdownEscapes(raw, PRESERVED_MARKDOWN_CHARS);
  // prosemirror-markdown's `esc()` also backslash-escapes `[`/`]` in text nodes;
  // Lexical's export regex (`[*_\`~\\]`) never does, so v4 emits bare brackets
  // (a `[note]` footnote, a `[` in prose). Strip those escapes too so brackets
  // survive byte-identically. Link syntax is emitted via the mark, not `esc`,
  // so this does not disturb real links.
  const stripped = stripMarkdownEscapes(unpreserved, ['[', ']']);
  const unit = unitToken ? getListIndentUnit(unitToken) : DEFAULT_LIST_INDENT_UNIT;
  return applyListIndentUnit(stripped, unit);
}
