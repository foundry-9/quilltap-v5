import { computed, signal, type Signal } from '@angular/core';

/**
 * The Salon auto-scroll state machine — a faithful port of v4
 * `app/salon/[id]/hooks/useAutoScroll.ts` (HEAD a7b1398d), reshaped from a React
 * hook (effects + refs) into a framework-light controller the zoneless
 * {@link import('./message-list').MessageList} drives via explicit calls.
 *
 * The semantics are v4's, verbatim:
 * - **Initial load** settles ({@link SETTLE_DELAY_MS} after the list mounts with
 *   content) then does ONE instant multi-strategy scroll to the bottom. Never a
 *   smooth scroll on first paint — smooth + dynamic row sizes janks the
 *   virtualizer.
 * - **Stick-to-bottom** intent is tracked off a debounced scroll listener: within
 *   {@link SCROLL_THRESHOLD}px of the bottom ⇒ "at bottom" (auto-scroll enabled);
 *   scrolled up past it ⇒ suppressed.
 * - **Streaming** never scrolls per chunk. On completion it chases the bottom only
 *   when the `autoScrollOnResponseComplete` chat setting is on AND the reader was
 *   already stuck to the bottom.
 * - A **user send** always scrolls and re-enables. The **jump-to-bottom** button
 *   ({@link showScrollToBottom}) shows when settled and not at bottom; clicking it
 *   always scrolls and re-enables regardless of the setting.
 */

/** Distance-from-bottom (px) within which the view counts as "at the bottom". */
export const SCROLL_THRESHOLD = 100;

/** Delay after the list first has content before the initial scroll fires. */
export const SETTLE_DELAY_MS = 400;

/** Debounce for the scroll-position check that tracks stick-to-bottom intent. */
export const SCROLL_CHECK_DEBOUNCE_MS = 100;

/** The slice of the virtualizer the controller needs (kept minimal for testing). */
export interface ScrollVirtualizer {
  scrollToIndex(index: number, options: { align: 'end'; behavior: 'auto' | 'smooth' }): void;
}

export interface AutoScrollBindings {
  /** The bounded-height scroll container (v4 `messagesContainerRef`). */
  container: HTMLElement;
  /** The zero-height anchor at the end of the list (v4 `messagesEndRef`). */
  end: HTMLElement | null;
  /** The virtualizer, for its scroll-to-index strategy (may be null in tests). */
  virtualizer: ScrollVirtualizer | null;
  /** Current render-item count — the valid upper bound for `scrollToIndex`. */
  itemCount: () => number;
  /** Backed by the `autoScrollOnResponseComplete` chat setting (default false). */
  autoScrollOnComplete: () => boolean;
}

export class AutoScrollController {
  // --- reactive state (read by the template) ---
  private readonly _enabled = signal(true);
  private readonly _settled = signal(false);

  /** Whether the view is at/near the bottom (v4 `isAtBottom` / `isAutoScrollEnabled`). */
  readonly isAtBottom: Signal<boolean> = this._enabled.asReadonly();
  /** Whether the initial-load settle has completed. */
  readonly isSettled: Signal<boolean> = this._settled.asReadonly();
  /** v4 `showScrollToBottom={isSettled && !isAtBottom}`. */
  readonly showScrollToBottom = computed(() => this._settled() && !this._enabled());

  private bindings: AutoScrollBindings | null = null;
  private removeScrollListener: (() => void) | null = null;

  private scrollCheckTimer: ReturnType<typeof setTimeout> | null = null;
  private settleTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly pendingScrollTimers = new Set<ReturnType<typeof setTimeout>>();

  private hasInitialScrolled = false;
  private prevMessageCount = 0;
  private wasStreaming = false;

  /**
   * Attach to the DOM: wire the debounced scroll listener and begin the settle
   * timer. Called once the list exists (v4's effect that waits for `!isLoading`).
   */
  bind(bindings: AutoScrollBindings, messageCount: number): void {
    this.bindings = bindings;
    this.prevMessageCount = messageCount;

    const handleScroll = () => {
      if (this.scrollCheckTimer) clearTimeout(this.scrollCheckTimer);
      this.scrollCheckTimer = setTimeout(() => {
        this._enabled.set(this.isNearBottom());
      }, SCROLL_CHECK_DEBOUNCE_MS);
    };
    bindings.container.addEventListener('scroll', handleScroll, { passive: true });
    this.removeScrollListener = () => bindings.container.removeEventListener('scroll', handleScroll);

    // Loading is already complete by the time the list mounts, so start the
    // settle timer immediately (v4's "loading complete → settle" branch).
    if (messageCount > 0) {
      this.settleTimer = setTimeout(() => {
        this._settled.set(true);
        if (!this.hasInitialScrolled) {
          this.hasInitialScrolled = true;
          this.performScrollToBottom('auto');
        }
      }, SETTLE_DELAY_MS);
    } else {
      this._settled.set(true);
    }
  }

  /**
   * React to the flat message count changing. Detects a new (non-streaming)
   * message and, when the reader opted in and is stuck to the bottom, chases it
   * (v4's "new messages added" effect).
   */
  notifyMessageCount(messageCount: number, isStreaming: boolean, isWaitingForResponse: boolean): void {
    const prev = this.prevMessageCount;
    this.prevMessageCount = messageCount;
    if (
      this.bindings &&
      this.bindings.autoScrollOnComplete() &&
      this._settled() &&
      messageCount > prev &&
      !isStreaming &&
      !isWaitingForResponse &&
      this._enabled()
    ) {
      this.schedule(() => this.performScrollToBottom('smooth'), 50);
    }
  }

  /**
   * React to the streaming flags. On the streaming→idle transition, chase the
   * bottom only when the reader opted in AND was already stuck (v4's
   * "streaming completion" effect).
   */
  notifyStreaming(isStreaming: boolean, isWaitingForResponse: boolean): void {
    const nowStreaming = isStreaming || isWaitingForResponse;
    if (
      this.wasStreaming &&
      !nowStreaming &&
      this.bindings?.autoScrollOnComplete() &&
      this._enabled() &&
      this._settled()
    ) {
      this.schedule(() => this.performScrollToBottom('smooth'), 100);
    }
    this.wasStreaming = nowStreaming;
  }

  /** The user sent a message — always scroll to the bottom and re-enable. */
  scrollOnUserMessage(): void {
    this._enabled.set(true);
    this.performScrollToBottom('smooth');
  }

  /** The jump-to-bottom button — always scroll and re-enable, setting-agnostic. */
  scrollToBottom(): void {
    this._enabled.set(true);
    this.performScrollToBottom('smooth');
  }

  /** Tear down listeners and pending timers. */
  dispose(): void {
    this.removeScrollListener?.();
    this.removeScrollListener = null;
    if (this.scrollCheckTimer) clearTimeout(this.scrollCheckTimer);
    if (this.settleTimer) clearTimeout(this.settleTimer);
    for (const t of this.pendingScrollTimers) clearTimeout(t);
    this.pendingScrollTimers.clear();
    this.bindings = null;
  }

  // --- internals ---

  private isNearBottom(): boolean {
    const container = this.bindings?.container;
    if (!container) return true;
    const { scrollTop, scrollHeight, clientHeight } = container;
    return scrollHeight - scrollTop - clientHeight <= SCROLL_THRESHOLD;
  }

  private schedule(fn: () => void, delay: number): void {
    const timer = setTimeout(() => {
      this.pendingScrollTimers.delete(timer);
      fn();
    }, delay);
    this.pendingScrollTimers.add(timer);
  }

  /**
   * The four-strategy scroll-to-bottom (v4 `performScrollToBottom`): tell the
   * virtualizer to land on the last item (always instant — smooth + dynamic
   * sizing warns and janks), then a 50ms direct scrollTop correction, a 150ms
   * `endRef.scrollIntoView`, and a 300ms final safety correction.
   */
  private performScrollToBottom(behavior: 'auto' | 'smooth' = 'smooth'): void {
    const b = this.bindings;
    if (!b) return;

    if (b.virtualizer && b.itemCount() > 0) {
      b.virtualizer.scrollToIndex(b.itemCount() - 1, { align: 'end', behavior: 'auto' });
    }

    this.schedule(() => {
      const maxScrollTop = b.container.scrollHeight - b.container.clientHeight;
      b.container.scrollTo({ top: maxScrollTop, behavior });
    }, 50);

    this.schedule(() => {
      b.end?.scrollIntoView({ behavior });
    }, 150);

    this.schedule(() => {
      const maxScrollTop = b.container.scrollHeight - b.container.clientHeight;
      if (b.container.scrollTop < maxScrollTop - 10) {
        b.container.scrollTo({ top: maxScrollTop, behavior: 'auto' });
      }
    }, 300);
  }
}
