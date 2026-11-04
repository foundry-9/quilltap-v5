// punctuation-lint-plugin.ts — the composer's punctuation warnings and prose-
// mode preview, as ProseMirror decorations (P4.D352; v4 `PunctuationLintPlugin
// .tsx` at 236c04e35, commits `1dd0a0c01` + `2b2b190b6`).
import type { Node as PMNode } from 'prosemirror-model';
import { Plugin, PluginKey, TextSelection, type EditorState } from 'prosemirror-state';
import { Decoration, DecorationSet } from 'prosemirror-view';

import {
  classifyProse,
  lintPunctuation,
  PUNCTUATION_RULE_MESSAGES,
  type LintCaret,
} from '../punctuation-lint/engine';

/**
 * Paints a warning over marks in the composer that will probably confuse the
 * formatting engine — a double quote opened inside dialogue that is already
 * open, an asterisk with no partner, and the rest of the list in
 * `punctuation-lint/engine.ts`. Hovering a painted mark shows why.
 *
 * **It never touches the text.** Decorations draw spans in the view only, so
 * the document, the undo history, the Markdown export and the caret are exactly
 * what they would be without the plugin.
 *
 * **Prose mode** (`proseMode`, for a template like Novel — see
 * `punctuation-lint/prose-mode.ts`) adds two things on the same terms: a softer
 * `qt-punctuation-no-dialogue` tint over every narrated run while the draft has
 * no dialogue at all, and a preview of how the message will be set —
 * `qt-composer-narration` over narration and `qt-composer-ooc` over
 * `((out of character))` asides — because neither is otherwise visible while
 * typing. The preview ignores the warnings toggle; it is not a warning.
 *
 * **Every decision lives in the engine.** This file only turns the editor into
 * the engine's input (one string per text block, code masked out, the caret's
 * offset) and turns the engine's offsets back into ranges.
 *
 * ### The recorded adapter divergence (ruling R-2)
 *
 * v4 paints with the CSS Custom Highlight API (`CSS.highlights` +
 * `::highlight(name)`); v5 paints with `Decoration.inline` and four CLASS rules
 * carrying the same tokens (`_chat.css`, declared in ascending v4 priority so
 * the cascade stands in for `Highlight.priority`). Consequences, each recorded:
 * - v4's shared-`Highlight` `contributions` registry has no counterpart: a
 *   highlight name is document-global, a `DecorationSet` belongs to one view,
 *   so several mounted composers (workspace panes) each paint their own.
 * - v4's `[punctuation-lint] CSS Custom Highlight API unavailable; warnings
 *   disabled` line is not ported — nothing here depends on the API.
 * - v4 explains a hover by setting the editable root's `title` from
 *   `caretPositionFromPoint`, because a `Highlight` has no element; each warning
 *   decoration here is a span carrying its own `title`, so the "specific before
 *   general" hover order is moot per element.
 * - v4 mounts its plugin last (`LexicalComposerWrapper.tsx:181-183`: "Read-
 *   only: paints warnings, never edits, so its order is immaterial"); a
 *   decorations-only ProseMirror plugin is order-independent too.
 *
 * Offsets map back through the same per-text-node segment table v4 keeps, so a
 * range that starts or ends inside masked inline code (or on a non-text inline
 * node) is dropped exactly where v4's `rangeFor` drops it.
 */

/** The class the stylesheet paints for a specific warning (v4's highlight name). */
export const PUNCTUATION_HIGHLIGHT_NAME = 'qt-punctuation-warning';
/** Prose mode: the whole-draft "no dialogue yet" tint. */
export const NO_DIALOGUE_HIGHLIGHT_NAME = 'qt-punctuation-no-dialogue';
/** Prose mode: the narration preview. */
export const NARRATION_HIGHLIGHT_NAME = 'qt-composer-narration';
/** Prose mode: the out-of-character preview. */
export const OOC_HIGHLIGHT_NAME = 'qt-composer-ooc';

/**
 * Paint order where decorations overlap: a specific warning over the
 * no-dialogue tint over the preview (v4 `HIGHLIGHT_PRIORITY`). Decorations are
 * added in ASCENDING priority, and `_chat.css` declares the four rules in the
 * same order.
 */
export const HIGHLIGHT_PRIORITY: Readonly<Record<string, number>> = {
  [PUNCTUATION_HIGHLIGHT_NAME]: 3,
  [NO_DIALOGUE_HIGHLIGHT_NAME]: 2,
  [OOC_HIGHLIGHT_NAME]: 1,
  [NARRATION_HIGHLIGHT_NAME]: 0,
};

/** What the host decides: whether warnings are on, and whether the chat is prose. */
export interface PunctuationLintInputs {
  /** `chat_settings.smartTypographySettings.punctuationWarnings ?? true`. */
  enabled: boolean;
  /** The chat's template leaves narration unmarked and quotes its dialogue. */
  proseMode: boolean;
}

/** The transaction meta a host sets to repaint after its inputs change. */
export const punctuationLintKey = new PluginKey<PunctuationLintState>('punctuation-lint');
export const PUNCTUATION_LINT_REFRESH = 'refresh';

/** A text node's place in its block's linted string. */
interface Segment {
  /** Document position of the text node's first character. */
  pos: number;
  start: number;
  length: number;
}

interface LintBlock {
  /** `null` for a code block — a barrier the engine skips. */
  text: string | null;
  segments: Segment[];
  /** The owning textblock's content start, or -1 (a code block, a running block). */
  contentStart: number;
}

interface PunctuationLintState {
  decorations: DecorationSet;
  /** Painted warnings (specific + no-dialogue) — the count the debug line reports. */
  explained: number;
  /** The inputs this paint was made with (`null` = inert). */
  inputs: PunctuationLintInputs | null;
}

// ---------------------------------------------------------------------------
// Editor → engine
// ---------------------------------------------------------------------------

function isCodeBlock(node: PMNode): boolean {
  return node.type.name === 'code_block';
}

/** One textblock's inline content as a linted string. */
function textblockBlock(node: PMNode, contentStart: number): LintBlock {
  const block: LintBlock = { text: '', segments: [], contentStart };
  node.forEach((child, offset) => {
    const text = block.text as string;
    if (child.isText) {
      const content = child.text ?? '';
      if (child.marks.some((m) => m.type.name === 'code')) {
        // Inline code is opaque: a run of non-space characters the engine
        // can neither flag nor be misled by.
        block.text = text + 'x'.repeat(content.length);
      } else {
        block.segments.push({
          pos: contentStart + offset,
          start: text.length,
          length: content.length,
        });
        block.text = text + content;
      }
    } else if (child.type.name === 'hard_break') {
      block.text = text + '\n';
    } else {
      // Any other inline leaf (an image, a mention atom): one object character
      // per one document position, as v4 does for a decorator node.
      block.text = text + '￼';
    }
  });
  return block;
}

/**
 * Flatten the document into text blocks (v4 `$collectBlocks`). A textblock's
 * inline content is one block; a code block is a `null` barrier; a container
 * (blockquote, list, list item) contributes its children's blocks; a leaf block
 * (a horizontal rule) is an object character in its container's running block.
 * An empty container still yields `''` — it is where the caret may be.
 */
export function collectBlocks(doc: PMNode): LintBlock[] {
  const blocks: LintBlock[] = [];

  const walk = (container: PMNode, contentStart: number) => {
    const before = blocks.length;
    let current: LintBlock | null = null;
    const flush = () => {
      if (current) blocks.push(current);
      current = null;
    };
    container.forEach((child, offset) => {
      const pos = contentStart + offset;
      if (isCodeBlock(child)) {
        flush();
        blocks.push({ text: null, segments: [], contentStart: -1 });
      } else if (child.isTextblock) {
        flush();
        blocks.push(textblockBlock(child, pos + 1));
      } else if (!child.isLeaf) {
        flush();
        walk(child, pos + 1);
      } else {
        current ??= { text: '', segments: [], contentStart: -1 };
        current.text = (current.text as string) + '￼';
      }
    });
    flush();
    if (blocks.length === before) blocks.push({ text: '', segments: [], contentStart: -1 });
  };

  walk(doc, 0);
  return blocks;
}

/**
 * Where the caret sits in block coordinates (v4 `$caretFor`), or `null` without
 * a collapsed text caret. A caret inside masked inline code has no segment and
 * so no offset — v4's text-point arm finds no segment there either; an empty
 * block's caret is its start (v4's element-point arm).
 */
function caretFor(state: EditorState, blocks: LintBlock[]): LintCaret | null {
  const selection = state.selection;
  if (!(selection instanceof TextSelection) || !selection.empty) return null;
  const { $from } = selection;
  if (!$from.parent.isTextblock) return null;
  const index = blocks.findIndex((b) => b.contentStart === $from.start());
  if (index < 0) return null;
  const block = blocks[index];
  if (block.text === '') return { block: index, offset: 0 };
  const pos = $from.pos;
  const segment = block.segments.find((s) => pos >= s.pos && pos <= s.pos + s.length);
  if (segment) return { block: index, offset: segment.start + (pos - segment.pos) };
  // No text node at the caret (beside a line break or an object): v4's element
  // point — offset 0 is the block's start, anything else its end.
  if (pos === $from.start()) return { block: index, offset: 0 };
  if (pos === $from.end()) return { block: index, offset: (block.text as string).length };
  return null;
}

/** Map a block offset back to a document position (v4 `domPoint`). */
function docPoint(segments: Segment[], offset: number, edge: 'start' | 'end'): number | null {
  const segment = segments.find((s) =>
    edge === 'start'
      ? offset >= s.start && offset < s.start + s.length
      : offset > s.start && offset <= s.start + s.length,
  );
  return segment ? segment.pos + (offset - segment.start) : null;
}

/** A document range over a block's `[start, end)`, or null when it cannot be mapped. */
function rangeFor(
  blocks: LintBlock[],
  span: { block: number; start: number; end: number },
): { from: number; to: number } | null {
  const segments = blocks[span.block]?.segments ?? [];
  const from = docPoint(segments, span.start, 'start');
  const to = docPoint(segments, span.end, 'end');
  if (from === null || to === null || to <= from) return null;
  return { from, to };
}

// ---------------------------------------------------------------------------
// Engine → decorations
// ---------------------------------------------------------------------------

/** v4 `paint`, producing decorations where v4 produces DOM ranges. */
export function paint(
  state: EditorState,
  inputs: PunctuationLintInputs | null,
): PunctuationLintState {
  const warnings = !!inputs?.enabled;
  const proseMode = !!inputs?.proseMode;
  if (!warnings && !proseMode) return { decorations: DecorationSet.empty, explained: 0, inputs };

  const doc = state.doc;
  const blocks = collectBlocks(doc);
  const caret = caretFor(state, blocks);
  const texts = blocks.map((b) => b.text);

  // Built in ASCENDING priority: narration, ooc, no-dialogue, warning.
  const byName: Record<string, Decoration[]> = {
    [NARRATION_HIGHLIGHT_NAME]: [],
    [OOC_HIGHLIGHT_NAME]: [],
    [NO_DIALOGUE_HIGHLIGHT_NAME]: [],
    [PUNCTUATION_HIGHLIGHT_NAME]: [],
  };
  let explained = 0;

  if (warnings) {
    for (const issue of lintPunctuation(texts, { caret, proseMode })) {
      const range = rangeFor(blocks, issue);
      if (!range) continue;
      const name =
        issue.rule === 'no-dialogue' ? NO_DIALOGUE_HIGHLIGHT_NAME : PUNCTUATION_HIGHLIGHT_NAME;
      byName[name].push(
        Decoration.inline(
          range.from,
          range.to,
          {
            class: name,
            title: PUNCTUATION_RULE_MESSAGES[issue.rule],
          },
          { name },
        ),
      );
      explained++;
    }
  }

  if (proseMode) {
    for (const span of classifyProse(texts)) {
      const range = rangeFor(blocks, span);
      if (!range) continue;
      const name = span.kind === 'ooc' ? OOC_HIGHLIGHT_NAME : NARRATION_HIGHLIGHT_NAME;
      byName[name].push(Decoration.inline(range.from, range.to, { class: name }, { name }));
    }
  }

  const ordered = Object.keys(byName)
    .sort((a, b) => HIGHLIGHT_PRIORITY[a] - HIGHLIGHT_PRIORITY[b])
    .flatMap((name) => byName[name]);
  return { decorations: DecorationSet.create(doc, ordered), explained, inputs };
}

function sameInputs(a: PunctuationLintInputs | null, b: PunctuationLintInputs | null): boolean {
  if (a === null || b === null) return a === b;
  return a.enabled === b.enabled && a.proseMode === b.proseMode;
}

/**
 * The plugin. `inputs` is read on every repaint — so a host flips it without
 * rebuilding the editor, and dispatches `tr.setMeta(punctuationLintKey,
 * PUNCTUATION_LINT_REFRESH)` to repaint with no keystroke. `null` makes it
 * inert (paints nothing, logs nothing).
 */
export function punctuationLintPlugin(inputs: () => PunctuationLintInputs | null): Plugin {
  return new Plugin<PunctuationLintState>({
    key: punctuationLintKey,
    state: {
      init: (_config, state) => paint(state, inputs()),
      apply: (tr, prev, _old, state) =>
        tr.docChanged ||
        tr.selectionSet ||
        tr.getMeta(punctuationLintKey) === PUNCTUATION_LINT_REFRESH
          ? paint(state, inputs())
          : prev,
    },
    props: {
      decorations: (state) => punctuationLintKey.getState(state)?.decorations ?? null,
    },
    view: (view) => {
      let logged: PunctuationLintInputs | null = null;
      let lastCount = -1;
      const report = () => {
        const current = punctuationLintKey.getState(view.state);
        const active = current?.inputs ?? null;
        if (active === null) {
          logged = null;
          lastCount = -1;
          return;
        }
        if (!sameInputs(active, logged)) {
          // v4 logs on every effect run — its deps are the two inputs.
          console.debug('[punctuation-lint] mounted', {
            warnings: active.enabled,
            proseMode: active.proseMode,
          });
          logged = active;
          lastCount = -1;
        }
        // One line when the number of warnings changes — never per keystroke.
        if (current!.explained !== lastCount) {
          lastCount = current!.explained;
          console.debug(`[punctuation-lint] ${lastCount} warning(s)`);
        }
      };
      report();
      return { update: report };
    },
  });
}
