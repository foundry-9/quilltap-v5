import { provideRouter } from '@angular/router';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { SearchResults } from './search-results';
import type { MemorySearchResult } from './search.types';

/**
 * The memory row's importance meter, migrated onto the shared `qt-progress`
 * family by P4.38 (v4 `search-results.tsx:226-230`). v4's fill was a hardcoded
 * `bg-destructive`; routing it through `qt-progress-fill-danger` is what makes
 * it themeable, and that is the whole point of the migration — so it is pinned.
 */

function memoryResult(importance: number): MemorySearchResult {
  return {
    id: 'mem-1',
    type: 'memories',
    name: 'A remembered thing',
    matchedField: 'content',
    matchedValue: 'remembered',
    snippet: 'A remembered thing',
    url: '/characters/c1',
    matchPriority: 0,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    characterId: 'c1',
    characterName: 'Lorian',
    importance,
    source: 'AUTO',
  };
}

describe('SearchResults — the importance meter', () => {
  it('renders on the shared qt-progress family, with no raw colour utility', () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [SearchResults],
      providers: [provideRouter([])],
    });
    const fixture = TestBed.createComponent(SearchResults);
    fixture.componentRef.setInput('results', [memoryResult(0.75)]);
    fixture.componentRef.setInput('query', 'remembered');
    fixture.detectChanges();

    const host = fixture.nativeElement as HTMLElement;
    const track = host.querySelector('.qt-progress.qt-progress-sm.w-16');
    expect(track).not.toBeNull();

    const fill = track!.querySelector('.qt-progress-fill') as HTMLElement;
    expect(fill.className).toContain('qt-progress-fill-danger');
    expect(fill.className).not.toContain('bg-destructive');
    expect(fill.style.width).toBe('75%');
  });
});
