import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  AutoScrollController,
  SCROLL_CHECK_DEBOUNCE_MS,
  SETTLE_DELAY_MS,
  type ScrollVirtualizer,
} from './auto-scroll';

/** A layout-free stand-in for the scroll container the controller drives. */
class FakeScrollElement {
  scrollTop = 0;
  scrollHeight = 1000;
  clientHeight = 400;
  scrollTo = vi.fn((opts: { top: number }) => {
    this.scrollTop = opts.top;
  });
  scrollIntoView = vi.fn();
  private listener: (() => void) | null = null;

  addEventListener(_type: string, cb: () => void): void {
    this.listener = cb;
  }
  removeEventListener(): void {
    this.listener = null;
  }
  /** Simulate the user scrolling to a given distance from the bottom. */
  scrollToDistanceFromBottom(distance: number): void {
    this.scrollTop = this.scrollHeight - this.clientHeight - distance;
    this.listener?.();
  }
}

/** Advance past the settle delay AND the 300ms multi-strategy scroll it kicks off. */
function settleFully(): void {
  vi.advanceTimersByTime(SETTLE_DELAY_MS + 350);
}

function makeController(over: { autoScrollOnComplete?: boolean; itemCount?: number } = {}) {
  const el = new FakeScrollElement();
  const end = { scrollIntoView: vi.fn() } as unknown as HTMLElement;
  const virtualizer: ScrollVirtualizer = { scrollToIndex: vi.fn() };
  const controller = new AutoScrollController();
  controller.bind(
    {
      container: el as unknown as HTMLElement,
      end,
      virtualizer,
      itemCount: () => over.itemCount ?? 300,
      autoScrollOnComplete: () => over.autoScrollOnComplete ?? false,
    },
    /* messageCount */ 10,
  );
  return { controller, el, end, virtualizer };
}

describe('AutoScrollController', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  describe('the settle gate', () => {
    it('stays unsettled until SETTLE_DELAY_MS, then does one instant scroll', () => {
      const { controller, virtualizer } = makeController({ itemCount: 300 });
      expect(controller.isSettled()).toBe(false);

      vi.advanceTimersByTime(SETTLE_DELAY_MS - 1);
      expect(controller.isSettled()).toBe(false);
      expect(virtualizer.scrollToIndex).not.toHaveBeenCalled();

      vi.advanceTimersByTime(1);
      expect(controller.isSettled()).toBe(true);
      // Strategy 1: virtualizer lands on the last item, instant.
      expect(virtualizer.scrollToIndex).toHaveBeenCalledWith(299, { align: 'end', behavior: 'auto' });
    });

    it('settles immediately with no messages and never scrolls', () => {
      const controller = new AutoScrollController();
      const virtualizer: ScrollVirtualizer = { scrollToIndex: vi.fn() };
      controller.bind(
        {
          container: new FakeScrollElement() as unknown as HTMLElement,
          end: null,
          virtualizer,
          itemCount: () => 0,
          autoScrollOnComplete: () => false,
        },
        0,
      );
      expect(controller.isSettled()).toBe(true);
      vi.advanceTimersByTime(SETTLE_DELAY_MS + 500);
      expect(virtualizer.scrollToIndex).not.toHaveBeenCalled();
    });
  });

  describe('the 100px threshold', () => {
    it('suppresses auto-scroll when scrolled up past the threshold and re-enables at the bottom', () => {
      const { controller, el } = makeController();
      settleFully();
      expect(controller.isAtBottom()).toBe(true);
      expect(controller.showScrollToBottom()).toBe(false);

      // Scrolled up 300px (> 100) → suppressed after the debounce.
      el.scrollToDistanceFromBottom(300);
      vi.advanceTimersByTime(SCROLL_CHECK_DEBOUNCE_MS);
      expect(controller.isAtBottom()).toBe(false);
      expect(controller.showScrollToBottom()).toBe(true);

      // Back within 100px → re-enabled, button hidden.
      el.scrollToDistanceFromBottom(40);
      vi.advanceTimersByTime(SCROLL_CHECK_DEBOUNCE_MS);
      expect(controller.isAtBottom()).toBe(true);
      expect(controller.showScrollToBottom()).toBe(false);
    });

    it('treats exactly 100px from the bottom as "at bottom"', () => {
      const { controller, el } = makeController();
      settleFully();
      el.scrollToDistanceFromBottom(100);
      vi.advanceTimersByTime(SCROLL_CHECK_DEBOUNCE_MS);
      expect(controller.isAtBottom()).toBe(true);
    });
  });

  describe('user send + jump button', () => {
    it('scrollOnUserMessage re-enables and scrolls even after scrolling up', () => {
      const { controller, el } = makeController();
      settleFully();
      el.scrollToDistanceFromBottom(500);
      vi.advanceTimersByTime(SCROLL_CHECK_DEBOUNCE_MS);
      expect(controller.isAtBottom()).toBe(false);

      controller.scrollOnUserMessage();
      expect(controller.isAtBottom()).toBe(true);
      vi.advanceTimersByTime(300);
      expect(el.scrollTo).toHaveBeenCalled();
    });
  });

  describe('completion-gated auto-scroll', () => {
    it('does NOT scroll on stream completion when the setting is off', () => {
      const { controller, el } = makeController({ autoScrollOnComplete: false });
      settleFully();
      el.scrollTo.mockClear();

      controller.notifyStreaming(true, false); // streaming
      controller.notifyStreaming(false, false); // completed
      vi.advanceTimersByTime(500);
      expect(el.scrollTo).not.toHaveBeenCalled();
    });

    it('scrolls on stream completion when the setting is on and stuck to bottom', () => {
      const { controller, el } = makeController({ autoScrollOnComplete: true });
      settleFully();
      el.scrollTo.mockClear();

      controller.notifyStreaming(true, false);
      controller.notifyStreaming(false, false);
      vi.advanceTimersByTime(500);
      expect(el.scrollTo).toHaveBeenCalled();
    });

    it('does NOT scroll on completion when the reader has scrolled up, even with the setting on', () => {
      const { controller, el } = makeController({ autoScrollOnComplete: true });
      settleFully();
      el.scrollToDistanceFromBottom(400);
      vi.advanceTimersByTime(SCROLL_CHECK_DEBOUNCE_MS);
      el.scrollTo.mockClear();

      controller.notifyStreaming(true, false);
      controller.notifyStreaming(false, false);
      vi.advanceTimersByTime(500);
      expect(el.scrollTo).not.toHaveBeenCalled();
    });
  });

  it('dispose clears timers and the scroll listener', () => {
    const { controller, el } = makeController();
    settleFully();
    controller.dispose();
    el.scrollTo.mockClear();
    // A scroll after disposal must not flip state (listener removed).
    el.scrollToDistanceFromBottom(500);
    vi.advanceTimersByTime(SCROLL_CHECK_DEBOUNCE_MS);
    expect(controller.isAtBottom()).toBe(true);
  });

  // === P4.D384 ===
  describe('isInitialScrollDone (v4 useAutoScroll, 4f5047241)', () => {
    it('is false until the initial scroll-to-bottom finishes its 300ms correction', () => {
      const { controller } = makeController();
      expect(controller.isInitialScrollDone()).toBe(false);

      vi.advanceTimersByTime(SETTLE_DELAY_MS);
      // Settled, but the initial scroll is still nudging.
      expect(controller.isSettled()).toBe(true);
      expect(controller.isInitialScrollDone()).toBe(false);

      vi.advanceTimersByTime(299);
      expect(controller.isInitialScrollDone()).toBe(false);
      vi.advanceTimersByTime(1);
      expect(controller.isInitialScrollDone()).toBe(true);
    });

    it('is true at once when no initial scroll is owed (no messages)', () => {
      const controller = new AutoScrollController();
      controller.bind(
        {
          container: new FakeScrollElement() as unknown as HTMLElement,
          end: null,
          virtualizer: null,
          itemCount: () => 0,
          autoScrollOnComplete: () => false,
        },
        0,
      );
      expect(controller.isInitialScrollDone()).toBe(true);
    });

    it('a later user-send or jump scroll does not touch it', () => {
      const { controller } = makeController();
      controller.scrollToBottom();
      vi.advanceTimersByTime(300);
      // The jump's own 300ms correction is not the initial scroll.
      expect(controller.isInitialScrollDone()).toBe(false);
      settleFully();
      expect(controller.isInitialScrollDone()).toBe(true);
    });

    it('resets to false on a rebind (v4: loading started again)', () => {
      const { controller } = makeController();
      settleFully();
      expect(controller.isInitialScrollDone()).toBe(true);
      controller.dispose();
      controller.bind(
        {
          container: new FakeScrollElement() as unknown as HTMLElement,
          end: null,
          virtualizer: null,
          itemCount: () => 3,
          autoScrollOnComplete: () => false,
        },
        3,
      );
      expect(controller.isInitialScrollDone()).toBe(false);
      settleFully();
      expect(controller.isInitialScrollDone()).toBe(true);
    });

    it('never fires once disposed mid-scroll', () => {
      const { controller } = makeController();
      vi.advanceTimersByTime(SETTLE_DELAY_MS + 100);
      controller.dispose();
      vi.advanceTimersByTime(1000);
      expect(controller.isInitialScrollDone()).toBe(false);
    });
  });
  // === end P4.D384 ===
});
