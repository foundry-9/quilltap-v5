import { conversationQueryPattern } from './conversation-search';

/**
 * The CSS highlight name painted by in-conversation search (v4
 * `app/salon/[id]/hooks/useSearchHighlight.ts`, `4f5047241`). Styled with
 * `::highlight(quilltap-conversation-search)`.
 */
export const CONVERSATION_SEARCH_HIGHLIGHT = 'quilltap-conversation-search';

/** Text that is not transcript: the composer, inline editors, and the like. */
export const SKIP_SELECTOR = 'input, textarea, [contenteditable="true"], script, style';

/**
 * One shared `Highlight` per document. Each Salon instance adds and removes only
 * its own ranges, so kept-alive tabs never wipe each other's paint.
 */
function sharedHighlight(): Highlight | null {
  if (
    typeof CSS === 'undefined' ||
    !('highlights' in CSS) ||
    typeof Highlight === 'undefined'
  ) {
    return null;
  }
  let highlight = CSS.highlights.get(CONVERSATION_SEARCH_HIGHLIGHT);
  if (!highlight) {
    highlight = new Highlight();
    CSS.highlights.set(CONVERSATION_SEARCH_HIGHLIGHT, highlight);
  }
  return highlight;
}

function collectRanges(container: HTMLElement, pattern: RegExp): Range[] {
  const ranges: Range[] = [];
  const doc = container.ownerDocument;
  const walker = doc.createTreeWalker(container, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) =>
      node.parentElement?.closest(SKIP_SELECTOR) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT,
  });
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const text = node.nodeValue ?? '';
    pattern.lastIndex = 0;
    for (let m = pattern.exec(text); m; m = pattern.exec(text)) {
      const range = doc.createRange();
      range.setStart(node, m.index);
      range.setEnd(node, m.index + m[0].length);
      ranges.push(range);
    }
  }
  return ranges;
}

/**
 * Paint every occurrence of a query inside the transcript container, without
 * touching the DOM Angular owns — v4's `useSearchHighlight` hook as a class the
 * Salon holds for its lifetime. `attach(container, query)` replaces whatever
 * this instance painted before (v4's effect re-run on a new query);
 * `dispose()` is the effect's cleanup.
 *
 * The list is virtualized, so rows mount and unmount as the reader scrolls (and
 * stream in during a turn); a MutationObserver re-paints, at most once a frame.
 * A match split across elements (half of it bold) is not painted — the in-chat
 * result list still finds it, because that matches on the markdown-stripped
 * text.
 *
 * A browser without the Custom Highlight API simply gets no paint.
 */
export class SearchHighlight {
  private owned: Range[] = [];
  private frame = 0;
  private observer: MutationObserver | null = null;
  private highlight: Highlight | null = null;

  /** Paint `query` inside `container` (null / too short clears this instance's paint). */
  attach(container: HTMLElement | null, query: string | null): void {
    this.dispose();
    const pattern = query ? conversationQueryPattern(query) : null;
    const highlight = sharedHighlight();
    if (!container || !pattern || !highlight) return;

    this.highlight = highlight;
    const paint = () => {
      this.frame = 0;
      for (const range of this.owned) highlight.delete(range);
      this.owned = collectRanges(container, pattern);
      for (const range of this.owned) highlight.add(range);
    };
    const schedule = () => {
      if (!this.frame) this.frame = requestAnimationFrame(paint);
    };

    paint();
    this.observer = new MutationObserver(schedule);
    this.observer.observe(container, { childList: true, subtree: true, characterData: true });
  }

  /** Stop observing and remove only this instance's ranges from the shared highlight. */
  dispose(): void {
    this.observer?.disconnect();
    this.observer = null;
    if (this.frame) cancelAnimationFrame(this.frame);
    this.frame = 0;
    if (this.highlight) {
      for (const range of this.owned) this.highlight.delete(range);
    }
    this.owned = [];
    this.highlight = null;
  }
}
