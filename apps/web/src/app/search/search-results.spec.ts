import { provideRouter } from '@angular/router';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { OpenDocumentFromSearch } from '../documents/open-document-from-search';
import { SearchResults } from './search-results';
import type { DocumentSearchResultItem, MemorySearchResult } from './search.types';

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

/**
 * The P4.D122 documents row (v4 `search-results.tsx` `DocumentResultCard`,
 * `b220999d`). Two things are load-bearing and easy to lose: the subtitle's
 * exact `name · path` bytes, and the modified-click PASSTHROUGH — v4's card is
 * a plain `<a href>` whose handler returns without `preventDefault` on a
 * modified click, so the browser follows the silent standalone deep link and
 * the search UI stays open.
 */
function documentResult(
  over: Partial<DocumentSearchResultItem> = {},
): DocumentSearchResultItem {
  return {
    id: 'link-1',
    type: 'documents',
    name: 'manifesto.md',
    matchedField: 'fileName',
    matchedValue: 'manifesto.md',
    snippet: 'Notes/manifesto.md',
    url: '/workspace?open=document-standalone&scope=document_store&mountPoint=Papers&filePath=Notes%2Fmanifesto.md',
    matchPriority: 1,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    mountPointId: 'mp-1',
    mountPointName: 'Papers',
    mountPointRef: 'Papers',
    storeType: 'documents',
    relativePath: 'Notes/manifesto.md',
    ...over,
  };
}

function renderDocuments(
  results: DocumentSearchResultItem[],
  open: (r: DocumentSearchResultItem, e: MouseEvent) => void,
): { host: HTMLElement; clicks: number } {
  const state = { clicks: 0 };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [SearchResults],
    providers: [
      provideRouter([]),
      { provide: OpenDocumentFromSearch, useValue: { open } },
    ],
  });
  const fixture = TestBed.createComponent(SearchResults);
  fixture.componentRef.setInput('results', results);
  fixture.componentRef.setInput('query', 'manifesto');
  fixture.componentInstance.resultClick.subscribe(() => (state.clicks += 1));
  fixture.detectChanges();
  return { host: fixture.nativeElement as HTMLElement, get clicks() { return state.clicks; } };
}

describe('SearchResults — the documents row (v4 DocumentResultCard)', () => {
  it('renders the store · path subtitle, the Document badge, and the standalone href', () => {
    const { host } = renderDocuments([documentResult()], () => {});
    const anchor = host.querySelector('a') as HTMLAnchorElement;
    // `mapResultUrl` passes a `/workspace?…` URL straight through — the href IS
    // the standalone deep link, which is what a middle-click follows.
    expect(anchor.getAttribute('href')).toBe(
      '/workspace?open=document-standalone&scope=document_store&mountPoint=Papers&filePath=Notes%2Fmanifesto.md',
    );
    expect(host.textContent).toContain('Papers · Notes/manifesto.md');
    expect(host.textContent).toContain('Document');
    expect(host.textContent).not.toContain('Vault');
  });

  it('adds the Vault badge only for a character-vault document', () => {
    const { host } = renderDocuments([documentResult({ storeType: 'character' })], () => {});
    expect(host.textContent).toContain('Vault');
  });

  it('routes the click through the open service and emits only when it handled it', () => {
    // A handled click: the service prevents default, so the dialog closes.
    const handled = renderDocuments([documentResult()], (_r, e) => e.preventDefault());
    (handled.host.querySelector('a') as HTMLAnchorElement).dispatchEvent(
      new MouseEvent('click', { bubbles: true, cancelable: true }),
    );
    expect(handled.clicks).toBe(1);

    // A MODIFIED click: the service returns without preventing default, so the
    // browser follows the href AND the search UI stays open.
    const passed = renderDocuments([documentResult()], () => {});
    const event = new MouseEvent('click', { bubbles: true, cancelable: true });
    (passed.host.querySelector('a') as HTMLAnchorElement).dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
    expect(passed.clicks).toBe(0);
  });
});
