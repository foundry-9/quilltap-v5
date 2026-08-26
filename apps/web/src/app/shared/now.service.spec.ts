import { Injector, effect, runInInjectionContext, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  DAY_GRANULARITY_MS,
  NowService,
  __nowSubscriberCountForTests,
  __nowTimerArmedForTests,
  __resetNowTickersForTests,
} from './now.service';

/**
 * Parity specs for the shared clock, ported from v4's own
 * `__tests__/unit/hooks/useNow.test.tsx` (137 lines) at `f3892158d`: one shared
 * timer per granularity however many subscribers, boundary-aligned firing
 * (`granularity - (now % granularity) + 1`), local-midnight alignment at day
 * granularity, the hidden-tab pause below a minute, and an `enabled: false`
 * consumer that neither subscribes nor advances.
 */

const MINUTE = 60_000;

/** Drive `document.hidden` + fire `visibilitychange`, as v4's suite does. */
function setHidden(hidden: boolean): void {
  Object.defineProperty(document, 'hidden', { value: hidden, configurable: true });
  document.dispatchEvent(new Event('visibilitychange'));
}

describe('NowService (v4 hooks/useNow.ts)', () => {
  let injector: Injector;

  beforeEach(() => {
    vi.useFakeTimers();
    __resetNowTickersForTests();
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({});
    injector = TestBed.inject(Injector);
    setHidden(false);
  });

  afterEach(() => {
    __resetNowTickersForTests();
    vi.useRealTimers();
  });

  function nowSignal(granularityMs: number, enabled?: boolean | ReturnType<typeof signal<boolean>>) {
    return runInInjectionContext(injector, () =>
      TestBed.inject(NowService).now(granularityMs, enabled ?? true),
    );
  }

  it('fires just AFTER the boundary, not a granularity after mount', () => {
    // Start 20 s into a minute: the first tick is due in 40 s + 1 ms.
    vi.setSystemTime(new Date(3 * MINUTE + 20_000));
    const now = nowSignal(MINUTE);
    TestBed.tick();
    const first = now();

    vi.advanceTimersByTime(40_000);
    TestBed.tick();
    expect(now()).toBe(first);

    vi.advanceTimersByTime(1);
    TestBed.tick();
    expect(now()).toBe(4 * MINUTE + 1);
  });

  it('shares ONE timer and one subscriber set across consumers of a granularity', () => {
    vi.setSystemTime(new Date(0));
    const a = nowSignal(MINUTE);
    const b = nowSignal(MINUTE);
    const c = nowSignal(MINUTE);
    TestBed.tick();

    expect(__nowSubscriberCountForTests(MINUTE)).toBe(3);
    // One armed timer for all three (v4: "fifty chat cards cost one timer").
    expect(vi.getTimerCount()).toBe(1);

    vi.advanceTimersByTime(MINUTE + 1);
    TestBed.tick();
    expect(a()).toBe(b());
    expect(b()).toBe(c());
    expect(a()).toBe(MINUTE + 1);
  });

  it('keeps distinct granularities on distinct timers', () => {
    vi.setSystemTime(new Date(0));
    nowSignal(MINUTE);
    nowSignal(1_000);
    TestBed.tick();
    expect(vi.getTimerCount()).toBe(2);
    expect(__nowSubscriberCountForTests(MINUTE)).toBe(1);
    expect(__nowSubscriberCountForTests(1_000)).toBe(1);
  });

  it('day granularity aligns to LOCAL midnight, not 24 h from mount', () => {
    const noon = new Date(2026, 7, 26, 12, 0, 0, 0);
    vi.setSystemTime(noon);
    const now = nowSignal(DAY_GRANULARITY_MS);
    TestBed.tick();
    const before = now();

    const midnight = new Date(2026, 7, 27, 0, 0, 0, 0).getTime();
    vi.advanceTimersByTime(midnight - noon.getTime() - 1);
    TestBed.tick();
    expect(now()).toBe(before);

    vi.advanceTimersByTime(1);
    TestBed.tick();
    expect(now()).toBe(midnight);
  });

  it('a disabled consumer neither subscribes nor advances', () => {
    vi.setSystemTime(new Date(0));
    const frozen = nowSignal(1_000, false);
    TestBed.tick();
    expect(__nowSubscriberCountForTests(1_000)).toBe(0);
    expect(__nowTimerArmedForTests(1_000)).toBe(false);

    const seen = frozen();
    vi.advanceTimersByTime(10_000);
    TestBed.tick();
    expect(frozen()).toBe(seen);
  });

  it('flipping `enabled` on subscribes and resyncs; flipping it off freezes', () => {
    vi.setSystemTime(new Date(0));
    const enabled = signal(false);
    const now = nowSignal(1_000, enabled);
    TestBed.tick();
    const frozenAt = now();

    vi.setSystemTime(new Date(5_000));
    enabled.set(true);
    TestBed.tick();
    expect(__nowSubscriberCountForTests(1_000)).toBe(1);
    expect(now()).toBe(5_000);
    expect(now()).not.toBe(frozenAt);

    vi.advanceTimersByTime(1_001);
    TestBed.tick();
    const ticked = now();
    expect(ticked).toBeGreaterThan(5_000);

    enabled.set(false);
    TestBed.tick();
    expect(__nowSubscriberCountForTests(1_000)).toBe(0);
    vi.advanceTimersByTime(10_000);
    TestBed.tick();
    expect(now()).toBe(ticked);
  });

  it('a hidden tab parks sub-minute tickers and resyncs on the way back', () => {
    vi.setSystemTime(new Date(0));
    const now = nowSignal(1_000);
    TestBed.tick();
    expect(__nowTimerArmedForTests(1_000)).toBe(true);

    setHidden(true);
    expect(__nowTimerArmedForTests(1_000)).toBe(false);

    vi.setSystemTime(new Date(30_000));
    setHidden(false);
    TestBed.tick();
    // Resync makes the first visible frame already correct, and re-arms.
    expect(now()).toBe(30_000);
    expect(__nowTimerArmedForTests(1_000)).toBe(true);
  });

  it('a minute-or-coarser ticker keeps running while hidden (v4 leaves it armed)', () => {
    vi.setSystemTime(new Date(0));
    nowSignal(MINUTE);
    TestBed.tick();
    setHidden(true);
    expect(__nowTimerArmedForTests(MINUTE)).toBe(true);
  });

  it('the last consumer leaving clears the shared timer', () => {
    vi.setSystemTime(new Date(0));
    const child = Injector.create({ providers: [], parent: injector });
    const scoped = TestBed.runInInjectionContext(() => {
      const local = TestBed.inject(NowService);
      return runInInjectionContext(child, () => local.now(1_000));
    });
    TestBed.tick();
    expect(scoped()).toBeTypeOf('number');
    expect(__nowSubscriberCountForTests(1_000)).toBe(1);

    (child as unknown as { destroy(): void }).destroy();
    expect(__nowSubscriberCountForTests(1_000)).toBe(0);
    expect(__nowTimerArmedForTests(1_000)).toBe(false);
  });

  it('the returned value is a signal consumers can react to', () => {
    vi.setSystemTime(new Date(0));
    const now = nowSignal(1_000);
    const seen: number[] = [];
    runInInjectionContext(injector, () => effect(() => seen.push(now())));
    TestBed.tick();
    vi.advanceTimersByTime(1_001);
    TestBed.tick();
    expect(seen.length).toBeGreaterThanOrEqual(2);
    expect(seen[seen.length - 1]).toBe(1_001);
  });
});
