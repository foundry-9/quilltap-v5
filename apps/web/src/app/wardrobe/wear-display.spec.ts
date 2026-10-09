import { describe, expect, it } from 'vitest';

import type { WardrobeWearSummary } from '../core/core-contract';
import {
  formatWearLine,
  formatWornRelative,
  formatWornWhen,
  isNeverWorn,
  sortAndFilterWardrobeItems,
  sortWardrobeItems,
  WARDROBE_LIST_SORTS,
  wearOf,
  type SortableWardrobeItem,
} from './wear-display';

/**
 * P4.D261 — v4 `3ee3b1342` `__tests__/unit/lib/wardrobe/wear-display.test.ts`
 * at the pin `f5e953a3f`, transcribed case for case (11), against the
 * transcription of `lib/wardrobe/wear-display.ts`. Plus the two arms the v4
 * file leaves to its callers: the sort options' labels and `formatWornWhen`'s
 * empty answers.
 */
const NOW = Date.parse('2026-10-07T12:00:00.000Z');

const worn = (wearCount: number, lastWornAt: string | null): WardrobeWearSummary => ({
  wearCount,
  firstWornAt: lastWornAt,
  lastWornAt,
  lastWornChatId: null,
});

describe('wearOf / isNeverWorn (v4 wear-display.ts:27-35)', () => {
  it('reads a missing annotation as the canonical never-worn summary', () => {
    expect(wearOf({})).toEqual({
      wearCount: 0,
      firstWornAt: null,
      lastWornAt: null,
      lastWornChatId: null,
    });
    expect(wearOf({ wear: null }).wearCount).toBe(0);
    expect(isNeverWorn({})).toBe(true);
    expect(isNeverWorn({ wear: worn(2, '2026-10-01T00:00:00.000Z') })).toBe(false);
  });
});

describe('formatWearLine (v4 :37-59)', () => {
  it('says "Never worn" for a zero or absent tally', () => {
    expect(formatWearLine(undefined, NOW)).toBe('Never worn');
    expect(formatWearLine(null, NOW)).toBe('Never worn');
    expect(formatWearLine(worn(0, null), NOW)).toBe('Never worn');
  });

  it('says "once" for a single wear and uses the relative date', () => {
    const at = new Date(NOW - 21 * 86400_000).toISOString();
    expect(formatWearLine(worn(1, at), NOW)).toBe('Worn once · last 3 weeks ago');
  });

  it('prints the count with × for several wears', () => {
    const at = new Date(NOW - 3 * 86400_000).toISOString();
    expect(formatWearLine(worn(4, at), NOW)).toBe('Worn 4× · last 3 days ago');
  });

  it('says "today" for a wear within the day', () => {
    const at = new Date(NOW - 10_000).toISOString();
    expect(formatWearLine(worn(2, at), NOW)).toBe('Worn 2× · last today');
  });

  it('drops the date part when a counted wear has no date', () => {
    expect(formatWearLine(worn(3, null), NOW)).toBe('Worn 3×');
  });
});

describe('formatWearLine — bug 183 (v4 ed8b15b50, wear-display.test.ts:66-77 at the pin 01a83539d)', () => {
  // Bug 183: the shared ladder's "last week" / "last month" rungs read
  // "last last week" after the line's own "last".
  it.each([
    [9, 'Worn 8× · last a week ago'],
    [13, 'Worn 8× · last a week ago'],
    [30, 'Worn 8× · last a month ago'],
    [59, 'Worn 8× · last a month ago'],
  ])('does not double "last" %i days back', (days, line) => {
    const at = new Date(NOW - days * 86_400_000).toISOString();
    expect(formatWearLine(worn(8, at), NOW)).toBe(line);
  });
});

describe('formatWornRelative (v4 wear-display.ts:79-84; test.ts:79-90 at the pin 01a83539d)', () => {
  it('rewords only the two rungs that begin with "last"', () => {
    const ago = (days: number) => NOW - days * 86_400_000;
    expect(formatWornRelative(ago(0), NOW)).toBe('today');
    expect(formatWornRelative(ago(1), NOW)).toBe('yesterday');
    expect(formatWornRelative(ago(3), NOW)).toBe('3 days ago');
    expect(formatWornRelative(ago(8), NOW)).toBe('a week ago');
    expect(formatWornRelative(ago(15), NOW)).toBe('2 weeks ago');
    expect(formatWornRelative(ago(45), NOW)).toBe('a month ago');
    expect(formatWornRelative(ago(120), NOW)).toBe('4 months ago');
  });
});

describe('formatWornWhen (v4 :61-70)', () => {
  it('is empty for a missing or unparseable date, never a weekday (R-E)', () => {
    expect(formatWornWhen(null, NOW)).toBe('');
    expect(formatWornWhen(undefined, NOW)).toBe('');
    expect(formatWornWhen('not a date', NOW)).toBe('');
    expect(formatWornWhen(new Date(NOW - 86400_000 * 1.5).toISOString(), NOW)).toBe('yesterday');
  });

  it("reads the two reworded rungs too — the item editor's Wear-history path (v4 :65-69, ed8b15b50)", () => {
    expect(formatWornWhen(new Date(NOW - 9 * 86_400_000).toISOString(), NOW)).toBe('a week ago');
    expect(formatWornWhen(new Date(NOW - 45 * 86_400_000).toISOString(), NOW)).toBe('a month ago');
  });
});

describe('sortWardrobeItems (v4 :72-136)', () => {
  const items: SortableWardrobeItem[] = [
    { title: 'Coat', createdAt: '2026-01-01T00:00:00.000Z', wear: worn(2, '2026-10-01T00:00:00.000Z') },
    { title: 'Apron', createdAt: '2026-05-01T00:00:00.000Z' },
    { title: 'Boots', createdAt: '2026-03-01T00:00:00.000Z', wear: worn(5, '2026-09-01T00:00:00.000Z') },
    { title: 'Dress', createdAt: '2026-05-01T00:00:00.000Z', wear: worn(2, '2026-10-01T00:00:00.000Z') },
    { title: 'Bonnet', createdAt: '2026-02-01T00:00:00.000Z', wear: worn(0, null) },
  ];
  const titles = (list: SortableWardrobeItem[]): string[] => list.map((i) => i.title);

  it("Title: A→Z (today's order), without mutating the input", () => {
    const before = titles(items);
    expect(titles(sortWardrobeItems(items, 'title'))).toEqual([
      'Apron',
      'Bonnet',
      'Boots',
      'Coat',
      'Dress',
    ]);
    expect(titles(items)).toEqual(before);
  });

  it('Recently worn: latest first, ties by title, never-worn last (alphabetically)', () => {
    expect(titles(sortWardrobeItems(items, 'recently-worn'))).toEqual([
      'Coat',
      'Dress',
      'Boots',
      'Apron',
      'Bonnet',
    ]);
  });

  it('Most worn: highest count first, ties by title, never-worn last', () => {
    expect(titles(sortWardrobeItems(items, 'most-worn'))).toEqual([
      'Boots',
      'Coat',
      'Dress',
      'Apron',
      'Bonnet',
    ]);
  });

  it('Newest: latest createdAt first, ties by title', () => {
    expect(titles(sortWardrobeItems(items, 'newest'))).toEqual([
      'Apron',
      'Dress',
      'Boots',
      'Bonnet',
      'Coat',
    ]);
  });

  it('"Never worn" filters and composes with any sort', () => {
    expect(
      titles(sortAndFilterWardrobeItems(items, { sort: 'newest', neverWornOnly: true })),
    ).toEqual(['Apron', 'Bonnet']);
    expect(
      titles(sortAndFilterWardrobeItems(items, { sort: 'title', neverWornOnly: false })),
    ).toHaveLength(5);
  });
});

describe('WARDROBE_LIST_SORTS (v4 :96-103)', () => {
  it('offers the four orderings, Title first (the default)', () => {
    expect(WARDROBE_LIST_SORTS).toEqual([
      { value: 'title', label: 'Title' },
      { value: 'recently-worn', label: 'Recently worn' },
      { value: 'most-worn', label: 'Most worn' },
      { value: 'newest', label: 'Newest' },
    ]);
  });
});
