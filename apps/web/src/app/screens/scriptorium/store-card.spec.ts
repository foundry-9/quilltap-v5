import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { DocMountPointDto } from '../../core/core-contract';
import { StoreCard } from './store-card';

function store(over: Partial<DocMountPointDto> = {}): DocMountPointDto {
  return {
    id: 'm1',
    userId: 'u1',
    name: 'My Vault',
    basePath: '/docs/vault',
    mountType: 'filesystem',
    storeType: 'documents',
    includePatterns: ['*.md'],
    excludePatterns: ['.git'],
    enabled: true,
    lastScannedAt: null,
    scanStatus: 'idle',
    lastScanError: null,
    conversionStatus: 'idle',
    conversionError: null,
    fileCount: 0,
    chunkCount: 0,
    totalSizeBytes: 0,
    createdAt: '2024-01-01T00:00:00Z',
    updatedAt: '2024-01-01T00:00:00Z',
    embeddedChunkCount: 0,
    ...over,
  };
}

/**
 * P4.D358 — v4 `DocumentStoreCard` (`dff76ebef`): the store's name is the
 * card's keyboard handle; Enter reaches the card's own `(click)` → `open`.
 */
describe('StoreCard — the name is the keyboard handle', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('names the store in a role=button span inside the <h2>, and Enter emits open once', () => {
    TestBed.configureTestingModule({ imports: [StoreCard] });
    const fixture = TestBed.createComponent(StoreCard);
    fixture.componentRef.setInput('store', store({ name: 'Grimoire' }));
    fixture.detectChanges();
    const opens: number[] = [];
    fixture.componentInstance.open.subscribe(() => opens.push(1));

    const span = fixture.nativeElement.querySelector('h2 > span[role="button"]') as HTMLElement;
    expect(span).toBeTruthy();
    expect(span.getAttribute('tabindex')).toBe('0');
    expect(span.textContent?.trim()).toBe('Grimoire');

    span.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    expect(opens).toEqual([1]);
  });
});
