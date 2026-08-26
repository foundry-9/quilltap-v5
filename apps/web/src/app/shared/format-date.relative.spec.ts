import { describe, expect, it } from 'vitest';

import { formatChatListDate, formatRelativeAge, formatRelativeDate } from './format-date';

/**
 * Parity specs for the three relative formatters against v4
 * `lib/format-time.ts` at `f3892158d` (P4.D125). The oracle is v4's client
 * source: the branch boundaries are transcribed from its `if` ladder, and the
 * `nowMs` parameterization the drift commit adds is exercised by driving each
 * boundary from a FIXED instant — which is the whole point of the parameter
 * (before it, these functions read `Date.now()` and no boundary was testable).
 *
 * TZ: the day-boundary arms use `setHours(0,0,0,0)` arithmetic on the local
 * zone, exactly as v4's `new Date(...)` math does; every case is expressed as a
 * millisecond OFFSET from a chosen `now`, so the suite is zone-independent
 * except for the one local-midnight case, which computes midnight itself.
 */

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

describe('formatRelativeDate (v4 lib/format-time.ts:93-116)', () => {
  const now = Date.parse('2026-08-26T15:30:00.000Z');
  const at = (offsetMs: number) => new Date(now - offsetMs).toISOString();

  it('returns the empty string for null/undefined/empty (v4 `if (!dateString)`)', () => {
    expect(formatRelativeDate(null, now)).toBe('');
    expect(formatRelativeDate(undefined, now)).toBe('');
    expect(formatRelativeDate('', now)).toBe('');
  });

  it('"Just now" strictly below one minute; the 60 s boundary flips to "1m ago"', () => {
    expect(formatRelativeDate(at(0), now)).toBe('Just now');
    expect(formatRelativeDate(at(59_999), now)).toBe('Just now');
    expect(formatRelativeDate(at(MIN), now)).toBe('1m ago');
  });

  it('minutes below 60, hours below 1440 (v4\'s two floors)', () => {
    expect(formatRelativeDate(at(59 * MIN), now)).toBe('59m ago');
    expect(formatRelativeDate(at(HOUR), now)).toBe('1h ago');
    expect(formatRelativeDate(at(23 * HOUR + 59 * MIN), now)).toBe('23h ago');
  });

  it('past a day it is the absolute short date+time (the ladder\'s tail)', () => {
    const out = formatRelativeDate(at(DAY), now);
    expect(out).not.toMatch(/ago$/);
    expect(out).not.toBe('Just now');
  });

  it('nowMs is what moves the answer — the same input reads differently later', () => {
    const ts = at(0);
    expect(formatRelativeDate(ts, now)).toBe('Just now');
    expect(formatRelativeDate(ts, now + 5 * MIN)).toBe('5m ago');
  });

  it('an unparseable date falls through the ladder rather than throwing', () => {
    expect(() => formatRelativeDate('not-a-date', now)).not.toThrow();
  });
});

describe('formatChatListDate (v4 lib/format-time.ts:121-149)', () => {
  it('day 0 → a time, day 1 → "Yesterday", <7 → a weekday', () => {
    const now = Date.parse('2026-08-26T15:30:00.000Z');
    expect(formatChatListDate(new Date(now - HOUR).toISOString(), now)).toMatch(/\d/);
    expect(formatChatListDate(new Date(now - DAY).toISOString(), now)).toBe('Yesterday');
    const weekday = formatChatListDate(new Date(now - 3 * DAY).toISOString(), now);
    expect(weekday).not.toBe('Yesterday');
    expect(weekday.length).toBeLessThanOrEqual(4);
  });

  it('the diffDays ladder is a FLOOR of elapsed ms, not a calendar-day count', () => {
    // v4 computes `Math.floor(diffMs / 86_400_000)`, so 23 h 59 m is still "day 0"
    // however many midnights it crossed. Transcribed quirk and all.
    const now = Date.parse('2026-08-26T00:30:00.000Z');
    expect(formatChatListDate(new Date(now - 23 * HOUR).toISOString(), now)).toMatch(/\d/);
  });

  it('the day rollover is what a DAY-granularity tick buys: same input, later now', () => {
    const now = Date.parse('2026-08-26T15:30:00.000Z');
    const ts = new Date(now - 2 * HOUR).toISOString();
    expect(formatChatListDate(ts, now)).toMatch(/\d/);
    // One day later the very same message reads "Yesterday" — the transition the
    // local-midnight ticker exists to deliver.
    expect(formatChatListDate(ts, now + DAY)).toBe('Yesterday');
  });

  it('an unparseable date returns the raw string (v5\'s NaN guard)', () => {
    expect(formatChatListDate('not-a-date', Date.now())).toBe('not-a-date');
  });
});

describe('formatRelativeAge (v4 lib/format-time.ts:161-167)', () => {
  const now = 1_700_000_000_000;

  it('"just now" strictly below 2 s, seconds below 60, minutes after', () => {
    expect(formatRelativeAge(now, now)).toBe('just now');
    expect(formatRelativeAge(now - 1_499, now)).toBe('just now');
    // v4 ROUNDS (not floors) the seconds — 1500 ms rounds to 2.
    expect(formatRelativeAge(now - 1_500, now)).toBe('2s ago');
    expect(formatRelativeAge(now - 59_000, now)).toBe('59s ago');
    expect(formatRelativeAge(now - 60_000, now)).toBe('1m ago');
    expect(formatRelativeAge(now - 125_000, now)).toBe('2m ago');
  });

  it('a future timestamp floors at 0 rather than going negative (v4 `Math.max(0, …)`)', () => {
    expect(formatRelativeAge(now + 10_000, now)).toBe('just now');
  });
});
