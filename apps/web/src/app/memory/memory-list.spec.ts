import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { MemoryDto } from '../core/core-contract';
import { MemoryList } from './memory-list';

function mem(over: Partial<MemoryDto>): MemoryDto {
  return {
    id: 'm1',
    userId: 'u1',
    characterId: 'c1',
    content: 'content',
    summary: 'summary',
    keywords: [],
    tags: [],
    importance: 0.5,
    source: 'MANUAL',
    chatId: null,
    projectId: null,
    aboutCharacterId: null,
    sourceMessageId: null,
    witnessedContext: null,
    lastAccessedAt: null,
    reinforcementCount: 0,
    lastReinforcedAt: null,
    relatedMemoryIds: [],
    reinforcedImportance: 0.5,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  };
}

interface DispatchReq {
  type: string;
  characterId?: string;
  search?: string;
  source?: string;
  limit?: number;
  offset?: number;
  memoryId?: string;
  [k: string]: unknown;
}

function stubClient(
  all: MemoryDto[],
  onDispatch?: (req: DispatchReq) => void,
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: DispatchReq) => {
      onDispatch?.(req);
      if (req.type === 'memoryList') {
        let filtered = all;
        if (req.search) {
          const q = req.search.toLowerCase();
          filtered = filtered.filter(
            (m) => m.summary.toLowerCase().includes(q) || m.content.toLowerCase().includes(q),
          );
        }
        if (req.source) {
          filtered = filtered.filter((m) => m.source === req.source);
        }
        const offset = req.offset ?? 0;
        const limit = req.limit ?? 30;
        return {
          memories: filtered.slice(offset, offset + limit),
          totalCount: filtered.length,
          count: filtered.length,
        };
      }
      if (req.type === 'memoryDelete') {
        return { success: true };
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 6): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<MemoryList>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [MemoryList],
    providers: [
      provideRouter([]),
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
    ],
  });
  const fixture = TestBed.createComponent(MemoryList);
  fixture.componentRef.setInput('characterId', 'c1');
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

describe('MemoryList', () => {
  it('shows the empty state when there are no memories', async () => {
    const fixture = await render(stubClient([]));
    expect(fixture.nativeElement.textContent).toContain('No memories yet');
  });

  it('renders a card per memory and the total count in the header', async () => {
    const fixture = await render(
      stubClient([mem({ id: 'a', summary: 'Alpha' }), mem({ id: 'b', summary: 'Beta' })]),
    );
    const cards = fixture.nativeElement.querySelectorAll('qt-memory-card');
    expect(cards.length).toBe(2);
    expect(fixture.nativeElement.textContent).toContain('Memories (2)');
  });

  it('paginates 30-per-page, deduping ids across pages', async () => {
    // 45 memories with unique ids → first page 30, second page 15.
    const many = Array.from({ length: 45 }, (_, i) => mem({ id: `m${i}`, summary: `Mem ${i}` }));
    const fixture = await render(stubClient(many));
    expect(fixture.nativeElement.querySelectorAll('qt-memory-card').length).toBe(30);
    fixture.componentInstance['loadNext']();
    await settle(fixture);
    // Second page (15) appended; all unique ids present, count stable.
    expect(fixture.nativeElement.querySelectorAll('qt-memory-card').length).toBe(45);
    expect(fixture.componentInstance['memoriesQuery'].hasNextPage()).toBe(false);
    // v4 shows "All memories loaded" ONLY when length < totalCount — here it's
    // equal, so the banner stays hidden (faithful port).
    expect(fixture.nativeElement.textContent).not.toContain('Loading more memories');
  });

  it('debounces the search box and re-queries with the term', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(
      stubClient(
        [mem({ id: 'a', summary: 'Tea party' }), mem({ id: 'b', summary: 'War council' })],
        (r) => seen.push(r),
      ),
    );
    expect(fixture.nativeElement.querySelectorAll('qt-memory-card').length).toBe(2);
    const input = fixture.nativeElement.querySelector('input[type="text"]') as HTMLInputElement;
    input.value = 'war';
    input.dispatchEvent(new Event('input'));
    await new Promise((r) => setTimeout(r, 350));
    await settle(fixture);
    expect(seen.some((r) => r.type === 'memoryList' && r.search === 'war')).toBe(true);
    expect(fixture.nativeElement.querySelectorAll('qt-memory-card').length).toBe(1);
    expect(fixture.nativeElement.textContent).toContain('War council');
  });

  it('filters by source via the source select', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(
      stubClient(
        [mem({ id: 'a', source: 'AUTO', summary: 'Auto one' }), mem({ id: 'b', source: 'MANUAL', summary: 'Manual one' })],
        (r) => seen.push(r),
      ),
    );
    const selects = fixture.nativeElement.querySelectorAll('select');
    const sourceSelect = selects[selects.length - 1] as HTMLSelectElement;
    sourceSelect.value = 'AUTO';
    sourceSelect.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(seen.some((r) => r.type === 'memoryList' && r.source === 'AUTO')).toBe(true);
    expect(fixture.nativeElement.querySelectorAll('qt-memory-card').length).toBe(1);
    expect(fixture.nativeElement.textContent).toContain('Auto one');
  });

  it('opens the editor via Add Memory', async () => {
    const fixture = await render(stubClient([]));
    const addBtn = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((b) => b.textContent?.trim() === 'Add Memory')!;
    addBtn.click();
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('qt-memory-editor')).toBeTruthy();
    expect(fixture.nativeElement.textContent).toContain('Add Memory');
  });

  it('confirms then deletes a memory', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(stubClient([mem({ id: 'del-me', summary: 'Doomed' })], (r) => seen.push(r)));
    // Trigger the card's delete → confirm modal.
    fixture.componentInstance['confirmDelete']('del-me');
    fixture.detectChanges();
    const dialog = fixture.nativeElement.querySelector('[role="dialog"]') as HTMLElement;
    expect(dialog.textContent).toContain('Are you sure you want to delete this memory?');
    // Confirm.
    const confirmBtn = Array.from(
      dialog.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((b) => b.textContent?.trim() === 'Delete')!;
    confirmBtn.click();
    await settle(fixture);
    expect(seen.some((r) => r.type === 'memoryDelete' && r.memoryId === 'del-me')).toBe(true);
  });
});
