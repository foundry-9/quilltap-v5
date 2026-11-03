/**
 * Marking where a name chip goes in a rendered message — a TS twin of v4
 * `lib/chat/mention-markers.ts` (`9a88f862f`, at `236c04e35`), pinned row for
 * row against v4's real module by `mention-markers.corpus.spec.ts`, plus the
 * v5 production entry {@link markMentionsInElement}.
 *
 * The Salon puts a chip on an absent character's name at its first mention in
 * the chat (the server's `chatAbsentMentions` read says which message and
 * which words). The chip has to land inside the roleplay styling, not split
 * it: a name in the middle of a quoted line must not break the line's
 * dialogue colouring. v4 wraps the name in three private-use characters inside
 * the same text node of the markdown tree, BEFORE its inline roleplay
 * tokenizer runs, and its renderer turns each marked run into a chip.
 *
 * **Recorded divergence `MENTION_MARKERS_POST_RENDER` (ruling R-2).** v5
 * renders a message to an HTML STRING (P4.D352's renderer; there is no hast
 * tree to hook), so {@link markMentionsInElement} walks the RENDERED DOM's text
 * nodes instead and sees THROUGH the roleplay tokenizer's `<span>`s: a text
 * node is eligible when its nearest block ancestor is a paragraph, heading,
 * list item or blockquote and every element between is a plain `<span>` (no
 * `katex*` / `hljs*` class). Code, links, emphasis, math and diagram blocks are
 * skipped, and the next eligible occurrence takes the chip — v4's rule, read
 * over a tree where the tokenizer has already run. {@link rehypeMentionMarkers}
 * is kept as v4's exact plugin over a minimal hast shape so the recorded
 * corpus replays byte for byte.
 *
 * Pure and client-safe.
 *
 * @module chat/absent-mentions/mention-markers
 */

import { buildNameMatcher } from './name-mentions';

/** U+E000 / U+E001 / U+E002 — v4's three private-use markers (ruling R-12). */
export const MENTION_OPEN = '';
export const MENTION_MID = '';
export const MENTION_CLOSE = '';

const ELIGIBLE_PARENTS = new Set(['p', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'li', 'blockquote']);

/** The minimal hast node shape v4's plugin walks. */
export interface HastNode {
  type: string;
  tagName?: string;
  value?: string;
  children?: HastNode[];
}

type Pending = Array<{ index: number; matcher: RegExp }>;

function pendingFor(mentions: ReadonlyArray<{ text: string }>): Pending {
  return mentions
    .map((m, index) => ({ index, matcher: buildNameMatcher([m.text]) }))
    .filter((entry): entry is { index: number; matcher: RegExp } => !!entry.matcher);
}

/**
 * v4's rehype plugin: the first eligible occurrence of each `mentions[i].text`
 * becomes `OPEN i MID text CLOSE`. Each mention is marked at most once.
 */
export function rehypeMentionMarkers(mentions: ReadonlyArray<{ text: string }>) {
  return (tree: HastNode): void => {
    const pending = pendingFor(mentions);
    if (pending.length === 0) return;

    const walk = (node: HastNode): void => {
      if (pending.length === 0 || !node.children) return;
      const eligible = node.type === 'element' && ELIGIBLE_PARENTS.has(node.tagName ?? '');
      for (const child of node.children) {
        if (pending.length === 0) return;
        if (child.type === 'text' && eligible && child.value) {
          child.value = markText(child.value, pending);
        } else if (
          child.type === 'element' &&
          child.tagName !== 'code' &&
          child.tagName !== 'pre'
        ) {
          walk(child);
        }
      }
    };
    walk(tree);
  };
}

/** Mark every pending mention found in `text`, removing each from `pending` (v4 `:63-82`). */
function markText(text: string, pending: Pending): string {
  let out = text;
  for (let i = 0; i < pending.length;) {
    const { index, matcher } = pending[i];
    matcher.lastIndex = 0;
    let hit: RegExpExecArray | null;
    let placed = false;
    while ((hit = matcher.exec(out)) !== null) {
      // Never inside a run another mention already marked.
      const before = out.slice(0, hit.index);
      if (before.lastIndexOf(MENTION_OPEN) > before.lastIndexOf(MENTION_CLOSE)) continue;
      out = `${before}${MENTION_OPEN}${index}${MENTION_MID}${hit[0]}${MENTION_CLOSE}${out.slice(hit.index + hit[0].length)}`;
      placed = true;
      break;
    }
    if (placed) pending.splice(i, 1);
    else i++;
  }
  return out;
}

/** One run of rendered text: plain, or a marked mention by its index. */
export type MentionRun = string | { index: number; text: string };

/** Split marked text into plain runs and mention runs (v4 `:88-101`). */
export function splitMentionMarkers(text: string): MentionRun[] {
  if (!text.includes(MENTION_OPEN)) return [text];
  const runs: MentionRun[] = [];
  const pattern = new RegExp(
    `${MENTION_OPEN}(\\d+)${MENTION_MID}([^${MENTION_CLOSE}]*)${MENTION_CLOSE}`,
    'g',
  );
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = pattern.exec(text)) !== null) {
    if (m.index > last) runs.push(text.slice(last, m.index));
    runs.push({ index: Number(m[1]), text: m[2] });
    last = m.index + m[0].length;
  }
  if (last < text.length) runs.push(text.slice(last));
  return runs;
}

/**
 * When a message renders in several runs (prose split around tool calls), give
 * each mention to the first run that names it. `runs[i]` is null for a run
 * that is not prose. Returns the mentions for each run, by index (v4 `:108-124`).
 */
export function assignMentionsToRuns<M extends { text: string }>(
  runs: ReadonlyArray<string | null>,
  mentions: ReadonlyArray<M>,
): Array<M[] | undefined> {
  const out: Array<M[] | undefined> = runs.map(() => undefined);
  for (const mention of mentions) {
    const matcher = buildNameMatcher([mention.text]);
    if (!matcher) continue;
    const at = runs.findIndex((run) => {
      if (run === null) return false;
      matcher.lastIndex = 0;
      return matcher.test(run);
    });
    if (at >= 0) (out[at] ??= []).push(mention);
  }
  return out;
}

// ---------------------------------------------------------------------------
// v5: the post-render pass (MENTION_MARKERS_POST_RENDER)
// ---------------------------------------------------------------------------

/** What a chip needs of a mention: the words, and the card's name for its label. */
export interface ChipMention {
  text: string;
  card: { name: string };
}

/** The chip class — the CSS hook and the host directive's delegation target. */
export const MENTION_CHIP_CLASS = 'qt-mention-chip';

/** Elements the walk never enters (v4 skips `code` / `pre`; the rest are v5's rendered shapes). */
const OPAQUE = new Set(['CODE', 'PRE', 'SCRIPT', 'STYLE', 'SVG', 'QT-DIAGRAM-BLOCK']);

/** A roleplay tokenizer span — transparent to eligibility. Math / highlight spans are not. */
function isTransparentSpan(el: Element): boolean {
  if (el.tagName !== 'SPAN' || el.classList.contains(MENTION_CHIP_CLASS)) return false;
  for (const cls of Array.from(el.classList)) {
    if (cls.startsWith('katex') || cls.startsWith('hljs')) return false;
  }
  return true;
}

/** Put any chips a previous pass left back to plain text, so a re-mark starts clean. */
function unwrapChips(root: Element): void {
  const chips = Array.from(root.querySelectorAll(`span.${MENTION_CHIP_CLASS}`));
  for (const chip of chips) {
    const parent = chip.parentNode;
    if (!parent) continue;
    parent.replaceChild(chip.ownerDocument.createTextNode(chip.textContent ?? ''), chip);
    parent.normalize();
  }
}

/** The chip for one marked run (`MentionChip.tsx:62-75`'s span, as markup). */
function chipFor(doc: Document, index: number, text: string, name: string): HTMLSpanElement {
  const span = doc.createElement('span');
  span.className = MENTION_CHIP_CLASS;
  span.setAttribute('tabindex', '0');
  span.setAttribute('role', 'button');
  span.setAttribute('aria-label', `${name}, not in this conversation`);
  span.setAttribute('data-mention', String(index));
  span.textContent = text;
  return span;
}

/**
 * Mark the first eligible occurrence of each mention in an already-rendered
 * message, turning each into a chip (`MENTION_MARKERS_POST_RENDER`). Idempotent:
 * chips a previous pass placed are unwrapped first. No mentions → nothing
 * touched. The private-use markers never reach the DOM.
 */
export function markMentionsInElement(root: Element, mentions: ReadonlyArray<ChipMention>): void {
  unwrapChips(root);
  const pending = pendingFor(mentions);
  if (pending.length === 0) return;
  const doc = root.ownerDocument;

  const visitText = (node: Text): void => {
    const value = node.data;
    if (!value) return;
    const marked = markText(value, pending);
    if (marked === value) return;
    const frag = doc.createDocumentFragment();
    for (const run of splitMentionMarkers(marked)) {
      if (typeof run === 'string') {
        frag.appendChild(doc.createTextNode(run));
      } else {
        const mention = mentions[run.index];
        // v4 `MentionText`: an index with no chip renders the bare text.
        frag.appendChild(
          mention
            ? chipFor(doc, run.index, run.text, mention.card.name)
            : doc.createTextNode(run.text),
        );
      }
    }
    node.replaceWith(frag);
  };

  const walk = (el: Element, eligible: boolean): void => {
    for (const child of Array.from(el.childNodes)) {
      if (pending.length === 0) return;
      if (child.nodeType === 3 /* TEXT_NODE */) {
        if (eligible) visitText(child as Text);
      } else if (child.nodeType === 1 /* ELEMENT_NODE */) {
        const sub = child as Element;
        if (OPAQUE.has(sub.tagName.toUpperCase())) continue;
        const tag = sub.tagName.toLowerCase();
        walk(sub, ELIGIBLE_PARENTS.has(tag) || (eligible && isTransparentSpan(sub)));
      }
    }
  };
  walk(root, false);
}

/**
 * {@link markMentionsInElement} over an HTML string (the specs' entry, and any
 * caller holding markup rather than a live element). With no mentions the
 * input string itself is returned — no parse.
 */
export function markMentionsInHtml(html: string, mentions: ReadonlyArray<ChipMention>): string {
  if (mentions.length === 0) return html;
  const template = document.createElement('template');
  template.innerHTML = html;
  const holder = document.createElement('div');
  holder.appendChild(template.content);
  markMentionsInElement(holder, mentions);
  return holder.innerHTML;
}
