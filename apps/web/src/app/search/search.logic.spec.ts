import { describe, expect, it } from 'vitest';

import { appendDeduped, groupResultsByType, highlightSegments, mapResultUrl } from './search.logic';
import type { SearchResult } from './search.types';

function result(type: SearchResult['type'], id: string): SearchResult {
  return {
    id,
    type,
    name: id,
    matchedField: 'name',
    matchedValue: id,
    snippet: id,
    url: `/x/${id}`,
    matchPriority: 1,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
  } as SearchResult;
}

describe('mapResultUrl (the documented v5-route divergences)', () => {
  it('maps /aurora/{id} to /characters/{id}, preserving ?tab=', () => {
    expect(mapResultUrl('/aurora/abc-123')).toBe('/characters/abc-123');
    expect(mapResultUrl('/aurora/abc-123?tab=memories')).toBe('/characters/abc-123?tab=memories');
  });

  it('maps /gallery?tag= to /photos?tag= (the tag filter is a named deferral)', () => {
    expect(mapResultUrl('/gallery?tag=brass%20%26%20goggles')).toBe(
      '/photos?tag=brass%20%26%20goggles',
    );
  });

  it('leaves salon URLs untouched, ?msg= included (anchor is a named deferral)', () => {
    expect(mapResultUrl('/salon/c1')).toBe('/salon/c1');
    expect(mapResultUrl('/salon/c1?msg=m1')).toBe('/salon/c1?msg=m1');
  });
});

describe('highlightSegments (v4 HighlightedText)', () => {
  it('marks case-insensitive matches and keeps the rest', () => {
    const segs = highlightSegments('The Airship rises', 'airship');
    expect(segs).toEqual([
      { text: 'The ', match: false },
      { text: 'Airship', match: true },
      { text: ' rises', match: false },
    ]);
  });

  it('escapes regex metacharacters in the query', () => {
    const segs = highlightSegments('a+b and a+b', 'a+b');
    expect(segs.filter((s) => s.match)).toHaveLength(2);
  });

  it('empty query or text yields one unmarked segment', () => {
    expect(highlightSegments('text', '')).toEqual([{ text: 'text', match: false }]);
  });
});

describe('groupResultsByType (first-encounter order)', () => {
  it('buckets in the order the sorted results introduce each type', () => {
    const groups = groupResultsByType([
      result('tags', 't1'),
      result('chats', 'c1'),
      result('tags', 't2'),
      result('messages', 'm1'),
    ]);
    expect(groups.map((g) => g.type)).toEqual(['tags', 'chats', 'messages']);
    expect(groups[0].results.map((r) => r.id)).toEqual(['t1', 't2']);
  });
});

describe('appendDeduped (v4 search-dialog.tsx:83-89)', () => {
  it('drops rows whose type-id key is already present; existing rows win', () => {
    const prev = [result('chats', 'a'), result('tags', 'a')];
    const next = [result('chats', 'a'), result('chats', 'b')];
    const merged = appendDeduped(prev, next);
    expect(merged.map((r) => `${r.type}-${r.id}`)).toEqual(['chats-a', 'tags-a', 'chats-b']);
  });
});
