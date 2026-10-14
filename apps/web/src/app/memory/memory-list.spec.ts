import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { ToastService } from '../ui/toast.service';

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
  tier?: string;
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

function sourceSelectOf(fixture: ComponentFixture<MemoryList>): HTMLSelectElement {
  return Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('select')).find((s) =>
    Array.from(s.options).some((o) => o.value === 'MANUAL'),
  ) as HTMLSelectElement;
}

function tierSelectOf(fixture: ComponentFixture<MemoryList>): HTMLSelectElement {
  return (fixture.nativeElement as HTMLElement).querySelector(
    'select[aria-label="Filter by tier"]',
  ) as HTMLSelectElement;
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
    // Pinned to the source select BY ITS OPTIONS — since P4.D276 the tier
    // select follows it, so "the last select" would drive the wrong control
    // and `.value = 'AUTO'` would silently become '' (a vacuous pass).
    const sourceSelect = sourceSelectOf(fixture);
    sourceSelect.value = 'AUTO';
    expect(sourceSelect.value).toBe('AUTO');
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

/**
 * P4.D276 — v4 `d58548051`'s list (`components/memory/memory-list.tsx` at
 * `01a83539d`): the `Consolidated (digests)` source option, the `Filter by
 * tier` select, and the digest viewer over `memoryGet`.
 */
describe('MemoryList — tiers and digests (P4.D276, v4 d58548051)', () => {
  it('offers the four source options in v4 order (v4 :296-300)', async () => {
    const fixture = await render(stubClient([]));
    const options = Array.from(sourceSelectOf(fixture).options).map((o) => [o.value, o.textContent?.trim()]);
    expect(options).toEqual([
      ['ALL', 'All Sources'],
      ['AUTO', 'Auto-generated'],
      ['MANUAL', 'Manual'],
      ['CONSOLIDATED', 'Consolidated (digests)'],
    ]);
  });

  it('puts the tier select AFTER the source select, with v4\'s label and options (v4 :302-310)', async () => {
    const fixture = await render(stubClient([]));
    const selects = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('select'));
    const tier = tierSelectOf(fixture);
    expect(tier).toBeTruthy();
    expect(selects.indexOf(tier)).toBe(selects.indexOf(sourceSelectOf(fixture)) + 1);
    expect(tier.classList.contains('qt-select')).toBe(true);
    expect(tier.classList.contains('w-auto')).toBe(true);
    expect(Array.from(tier.options).map((o) => [o.value, o.textContent?.trim()])).toEqual([
      ['ALL', 'All Memories'],
      ['hot', 'Active'],
      ['cold', 'Archived'],
    ]);
    // v4 labels ONLY the tier select.
    expect(selects.filter((s) => s.hasAttribute('aria-label'))).toEqual([tier]);
  });

  it('sends source CONSOLIDATED when the digest option is chosen', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(stubClient([], (r) => seen.push(r)));
    const source = sourceSelectOf(fixture);
    source.value = 'CONSOLIDATED';
    expect(source.value).toBe('CONSOLIDATED');
    source.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(seen.some((r) => r.type === 'memoryList' && r.source === 'CONSOLIDATED')).toBe(true);
  });

  it('sends tier only when filtered, and a tier change re-queries from offset 0 (v4 :92, :131)', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(stubClient([mem({ id: 'a' })], (r) => seen.push(r)));
    const first = seen.filter((r) => r.type === 'memoryList');
    expect(first.length).toBeGreaterThan(0);
    expect(first.every((r) => !('tier' in r))).toBe(true);

    seen.length = 0;
    const tier = tierSelectOf(fixture);
    tier.value = 'cold';
    expect(tier.value).toBe('cold');
    tier.dispatchEvent(new Event('change'));
    await settle(fixture);
    const cold = seen.filter((r) => r.type === 'memoryList');
    expect(cold.length).toBeGreaterThan(0);
    expect(cold[0].tier).toBe('cold');
    expect(cold[0].offset).toBe(0);

    seen.length = 0;
    tier.value = 'ALL';
    tier.dispatchEvent(new Event('change'));
    await settle(fixture);
    // Back to All Memories: the cached first query answers, or a re-query carries no tier.
    expect(seen.filter((r) => r.type === 'memoryList').every((r) => !('tier' in r))).toBe(true);
  });

  function digestStub(
    reply: (req: DispatchReq) => Record<string, unknown> | Promise<Record<string, unknown>>,
    seen: DispatchReq[],
  ): Partial<CoreClient> {
    const base = stubClient(
      [mem({ id: 'cold-1', summary: 'Shelved note', tier: 'cold', supersededById: 'dig-1' })],
      (r) => seen.push(r),
    );
    return {
      dispatchData: (async (req: DispatchReq) => {
        if (req.type === 'memoryGet') {
          seen.push(req);
          return reply(req);
        }
        return base.dispatchData!(req as never);
      }) as CoreClient['dispatchData'],
    };
  }

  function supersededButton(fixture: ComponentFixture<MemoryList>): HTMLButtonElement {
    return Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Superseded by a digest',
    ) as HTMLButtonElement;
  }

  it('opens The Digest under document.body over memoryGet, handler-less, and closes it (v4 :195-205, :370-389)', async () => {
    const seen: DispatchReq[] = [];
    const digest = mem({
      id: 'dig-1',
      summary: 'The denser note',
      source: 'CONSOLIDATED',
      consolidatedFrom: ['cold-1'],
      chatId: 'chat-1',
      sourceMessageId: 'msg-1',
    });
    const fixture = await render(digestStub(() => ({ memory: digest }), seen));
    const btn = supersededButton(fixture);
    expect(btn).toBeTruthy();
    btn.click();
    await settle(fixture);
    expect(seen.find((r) => r.type === 'memoryGet')).toEqual({ type: 'memoryGet', memoryId: 'dig-1' });

    const viewer = document.body.querySelector('qt-memory-digest-viewer') as HTMLElement;
    expect(viewer).toBeTruthy();
    expect(viewer.parentElement).toBe(document.body);
    expect((fixture.nativeElement as HTMLElement).contains(viewer)).toBe(false);
    expect(viewer.querySelector('h2')?.textContent).toBe('The Digest');
    const card = viewer.querySelector('qt-memory-card') as HTMLElement;
    expect(card.textContent).toContain('The denser note');
    expect(card.textContent).toContain('Digest');
    const cardButtons = Array.from(card.querySelectorAll('button')).map((b) => b.textContent?.trim());
    expect(cardButtons).not.toContain('Edit');
    expect(cardButtons).not.toContain('Delete');
    expect(cardButtons.some((t) => t?.includes('Source'))).toBe(false);

    (Array.from(viewer.querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Close') as HTMLButtonElement).click();
    await settle(fixture);
    expect(document.body.querySelector('qt-memory-digest-viewer')).toBeNull();
  });

  it('toasts the server\'s message on a failed memoryGet (v4 :203)', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(
      digestStub(() => {
        throw new Error('Memory not found');
      }, seen),
    );
    supersededButton(fixture).click();
    await settle(fixture);
    expect(TestBed.inject(ToastService).toasts().map((t) => `${t.type}:${t.message}`)).toContain(
      'error:Memory not found',
    );
    expect(document.body.querySelector('qt-memory-digest-viewer')).toBeNull();
  });

  it('toasts `The digest could not be found` on a reply without memory (v4 :199-200)', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(digestStub(() => ({}), seen));
    supersededButton(fixture).click();
    await settle(fixture);
    expect(TestBed.inject(ToastService).toasts().map((t) => `${t.type}:${t.message}`)).toContain(
      'error:The digest could not be found',
    );
  });

  it('toasts `Failed to load the digest` on a non-Error throw that stringifies to nothing (v4 getErrorMessage)', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(
      digestStub(() => {
        throw {};
      }, seen),
    );
    supersededButton(fixture).click();
    await settle(fixture);
    expect(TestBed.inject(ToastService).toasts().map((t) => `${t.type}:${t.message}`)).toContain(
      'error:Failed to load the digest',
    );
  });
});

/**
 * P4.D300 — the Maintenance button (v4 `b045dd986`), transcribing v4's
 * `opens the maintenance dialog for this character`
 * (`__tests__/unit/components/memory/memory-list.test.tsx:142` at `83334b9a5`).
 */
describe('MemoryList — the Maintenance dialog (P4.D300)', () => {
  it('opens the maintenance dialog for this character (v4 memory-list.test.tsx:142)', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(stubClient([mem({ id: '1', summary: 'Alpha', content: 'Alpha', source: 'AUTO' })], (r) => seen.push(r)));
    const button = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Maintenance',
    ) as HTMLButtonElement;
    expect(button.getAttribute('title')).toBe('Fold repetitions, consolidate, and keep this character under the cap');
    button.click();
    await settle(fixture);
    const dialog = document.body.querySelector('qt-maintenance-dialog') as HTMLElement;
    expect(dialog.querySelector('h2')?.textContent).toBe('Memory Maintenance');
    // Fixed to this character: no picker, and the four pickable phases are offered.
    expect(dialog.querySelector('#maintenance-target')).toBeNull();
    expect(dialog.querySelectorAll('input[type="checkbox"]').length).toBe(4);
    expect(seen.some((r) => r.type === 'memoryCharacterCounts')).toBe(false);
    TestBed.resetTestingModule();
  });

  it('no Maintenance button with no memories', async () => {
    const fixture = await render(stubClient([]));
    const labels = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).map((b) => b.textContent?.trim());
    expect(labels).not.toContain('Maintenance');
    expect(labels).not.toContain('Cleanup');
  });

  it('a queued run re-reads the list from offset 0 and the dialog closes itself', async () => {
    const seen: DispatchReq[] = [];
    const client = stubClient([mem({ id: '1' })], (r) => seen.push(r));
    const base = client.dispatchData!;
    client.dispatchData = (async (req: DispatchReq) => {
      if (req.type !== 'memoryMaintain') return base(req as never);
      seen.push(req);
      return { success: true, dryRun: false, jobId: 'j', jobIds: ['j'] };
    }) as CoreClient['dispatchData'];
    const fixture = await render(client);
    (Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Maintenance') as HTMLButtonElement).click();
    await settle(fixture);
    const before = seen.filter((r) => r.type === 'memoryList').length;
    const dialog = document.body.querySelector('qt-maintenance-dialog') as HTMLElement;
    (Array.from(dialog.querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Run for real') as HTMLButtonElement).click();
    await settle(fixture, 10);
    expect(JSON.stringify(seen.find((r) => r.type === 'memoryMaintain'))).toBe(
      '{"type":"memoryMaintain","options":{"characterId":"c1","phases":["fold-near-duplicates","consolidate","demote","retire"],"dryRun":false}}',
    );
    const reads = seen.filter((r) => r.type === 'memoryList');
    expect(reads.length).toBeGreaterThan(before);
    expect(reads[reads.length - 1].offset).toBe(0);
    expect(document.body.querySelector('qt-maintenance-dialog')).toBeNull();
  });
});
