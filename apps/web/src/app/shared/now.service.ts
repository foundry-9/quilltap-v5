/**
 * The shared clock — v5's twin of v4 `hooks/useNow.ts`.
 *
 * Relative timestamps ("4m ago", "Yesterday") go stale for a reason that has
 * nothing to do with the server: the *client's* clock advanced. Before this
 * service, nothing in the interface ticked — a "3m ago" only ever changed when
 * some unrelated state happened to re-render the component around it, so a chat
 * card could sit at "Just now" for an hour.
 *
 * v4's design notes carry over verbatim, because the mechanism is the same:
 *   - **One timer per granularity, not one per component.** Every consumer
 *     asking for 60 000 ms shares a single `setTimeout` chain and a single
 *     subscriber set. Fifty chat cards cost one timer.
 *   - **Boundary-aligned ticks.** The timer fires just *after* each minute (or
 *     second, or local midnight) boundary rather than 60 s after whenever the
 *     first subscriber mounted, so every "4m ago" on screen flips to "5m ago"
 *     together instead of drifting apart.
 *   - **Hidden tabs don't get fine-grained ticks.** Anything finer than a
 *     minute pauses while `document.hidden`, and resyncs on the way back so the
 *     first visible frame is already correct.
 *
 * **Mechanism divergence from v4, recorded:** v4 backs the hook with
 * `useSyncExternalStore`, whose `subscribe` never runs during SSR — the SSR
 * inertness leg has no v5 analogue (the SPA never renders on a server). What
 * replaces it here is Angular's own lifecycle: {@link NowService.now} must be
 * called in an injection context, and the subscription is released by that
 * context's `DestroyRef`.
 *
 * @module shared/now.service
 */

import {
  DestroyRef,
  Injectable,
  type Signal,
  computed,
  effect,
  inject,
  isSignal,
  signal,
} from '@angular/core';

/**
 * Granularity for anything that only changes at a day boundary — chat-list
 * dates rolling "today" → "Yesterday" → weekday. Ticks at *local* midnight,
 * not every 24 h from mount (v4 `DAY_GRANULARITY_MS`).
 */
export const DAY_GRANULARITY_MS = 86_400_000;

/** Below this, ticking is suspended while the tab is hidden (v4's constant). */
const HIDDEN_TAB_PAUSE_BELOW_MS = 60_000;

interface Ticker {
  subscribers: Set<() => void>;
  timer: ReturnType<typeof setTimeout> | null;
  now: number;
}

const tickers = new Map<number, Ticker>();
let visibilityWired = false;

/**
 * Delay until just after the next boundary of `granularityMs`. Day granularity
 * aligns to local midnight; everything else to a multiple of the granularity
 * since the epoch. The extra millisecond keeps us on the far side of the
 * boundary, so `Math.floor((now - then) / 60000)` has actually advanced.
 * (v4 `nextBoundaryDelay`.)
 */
function nextBoundaryDelay(granularityMs: number, from: number): number {
  if (granularityMs === DAY_GRANULARITY_MS) {
    const midnight = new Date(from);
    midnight.setHours(24, 0, 0, 0);
    return Math.max(1, midnight.getTime() - from);
  }
  return granularityMs - (from % granularityMs) + 1;
}

function isPaused(granularityMs: number): boolean {
  return (
    granularityMs < HIDDEN_TAB_PAUSE_BELOW_MS &&
    typeof document !== 'undefined' &&
    document.hidden
  );
}

/**
 * Arm the next tick, if one isn't already armed. Idempotent by design: fifty
 * chat cards mounting at once must not each tear down and re-create the shared
 * timer. (v4 `schedule`.)
 */
function schedule(granularityMs: number, ticker: Ticker): void {
  if (ticker.timer) return;
  if (ticker.subscribers.size === 0) return;
  if (isPaused(granularityMs)) return;

  const now = Date.now();
  ticker.timer = setTimeout(() => {
    ticker.timer = null;
    ticker.now = Date.now();
    for (const notify of [...ticker.subscribers]) notify();
    schedule(granularityMs, ticker);
  }, nextBoundaryDelay(granularityMs, now));
}

/** Drop any armed tick and arm a fresh one from the current instant. */
function reschedule(granularityMs: number, ticker: Ticker): void {
  if (ticker.timer) {
    clearTimeout(ticker.timer);
    ticker.timer = null;
  }
  schedule(granularityMs, ticker);
}

/**
 * Bring every ticker back in step after the tab was hidden. Fine-grained
 * tickers were parked; coarse ones kept running but may have been throttled by
 * the browser, so both get a fresh reading and a re-armed timer.
 */
function resyncAll(): void {
  const now = Date.now();
  for (const [granularityMs, ticker] of tickers) {
    if (ticker.subscribers.size === 0) continue;
    if (ticker.now !== now) {
      ticker.now = now;
      for (const notify of [...ticker.subscribers]) notify();
    }
    reschedule(granularityMs, ticker);
  }
}

function wireVisibility(): void {
  if (visibilityWired || typeof document === 'undefined') return;
  visibilityWired = true;
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden) resyncAll();
    else {
      // Park the fine-grained tickers; coarse ones are cheap enough to leave.
      for (const [granularityMs, ticker] of tickers) {
        if (granularityMs < HIDDEN_TAB_PAUSE_BELOW_MS && ticker.timer) {
          clearTimeout(ticker.timer);
          ticker.timer = null;
        }
      }
    }
  });
}

function getTicker(granularityMs: number): Ticker {
  let ticker = tickers.get(granularityMs);
  if (!ticker) {
    ticker = { subscribers: new Set(), timer: null, now: Date.now() };
    tickers.set(granularityMs, ticker);
  }
  return ticker;
}

/** The shared, boundary-aligned clock every relative timestamp reads. */
@Injectable({ providedIn: 'root' })
export class NowService {
  /**
   * Subscribe to a shared, boundary-aligned clock.
   *
   * **Must be called in an injection context** (a field initializer or a
   * constructor) — the subscription is released by the caller's `DestroyRef`,
   * which is what makes "one timer per granularity" survivable.
   *
   * @param granularityMs How often the value may change — `60_000` for "m ago"
   *   readouts, `1_000` for second-resolution ones, {@link DAY_GRANULARITY_MS}
   *   for calendar-day rollovers.
   * @param enabled Pass `false` (or a signal that reads `false`) to freeze the
   *   value and stop subscribing at all. v4 needs this because hooks can't be
   *   called conditionally; v5 keeps it for the same reason a component only
   *   *sometimes* wants a fast tick (a badge with a time budget), and to keep
   *   the two twins' semantics identical.
   * @returns The current epoch milliseconds, changing only on a tick.
   */
  now(granularityMs = 60_000, enabled: Signal<boolean> | boolean = true): Signal<number> {
    const destroyRef = inject(DestroyRef);
    const enabledSignal = isSignal(enabled) ? enabled : signal(enabled).asReadonly();

    // A local mirror rather than the shared ticker's own value: a DISABLED
    // consumer must freeze where it stands, not follow ticks another consumer
    // is driving. (v4 gets that for free — a component that doesn't subscribe
    // simply doesn't re-render; a signal read would propagate regardless.)
    const value = signal(getTicker(granularityMs).now);

    let release: (() => void) | null = null;
    const unsubscribe = () => {
      release?.();
      release = null;
    };

    effect((onCleanup) => {
      if (!enabledSignal()) {
        unsubscribe();
        return;
      }
      wireVisibility();
      const ticker = getTicker(granularityMs);
      const firstSubscriber = ticker.subscribers.size === 0;
      const notify = () => value.set(ticker.now);
      ticker.subscribers.add(notify);
      // A ticker nobody was watching holds a stale reading; refresh it before
      // arming, so the first subscriber back isn't a minute behind. While
      // others are already subscribed the shared reading is current and must
      // not be disturbed — they would render a different instant than they
      // last committed. (v4's `firstSubscriber` branch, verbatim.)
      if (firstSubscriber) ticker.now = Date.now();
      value.set(ticker.now);
      schedule(granularityMs, ticker);

      release = () => {
        ticker.subscribers.delete(notify);
        if (ticker.subscribers.size === 0 && ticker.timer) {
          clearTimeout(ticker.timer);
          ticker.timer = null;
        }
      };
      onCleanup(unsubscribe);
    });

    destroyRef.onDestroy(unsubscribe);

    return computed(() => value());
  }
}

/** Test seam: drop every ticker and its timer (v4 `__resetNowTickersForTests`). */
export function __resetNowTickersForTests(): void {
  for (const ticker of tickers.values()) {
    if (ticker.timer) clearTimeout(ticker.timer);
  }
  tickers.clear();
}

/** Test seam: how many consumers a granularity currently has (spec-only). */
export function __nowSubscriberCountForTests(granularityMs: number): number {
  return tickers.get(granularityMs)?.subscribers.size ?? 0;
}

/** Test seam: whether a granularity currently has an armed timer (spec-only). */
export function __nowTimerArmedForTests(granularityMs: number): boolean {
  return tickers.get(granularityMs)?.timer != null;
}
