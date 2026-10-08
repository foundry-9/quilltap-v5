import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type {
  CoreRequest,
  WardrobeWearHistoryResponse,
} from '../../core/core-contract';
import { WORKSPACE_HANDLE } from '../../workspace/workspace-contract';
import type { WardrobeContainer } from '../wardrobe-container';
import { WardrobeWearHistorySection } from './wear-history-section';

/**
 * P4.D261 — v4 `3ee3b1342` `wardrobe-wear-ledger-ui.test.tsx` (the history ×3)
 * at the pin `f5e953a3f`, transcribed onto the dispatch verb
 * `wardrobeItemWearHistory` (C2 §3) — v4 fetches `${itemUrl}?action=wear-
 * history`; v5 names the container instead. Markup:
 * `WardrobeWearHistorySection.tsx:57-154`.
 */
const CHAT = '33333333-3333-4333-8333-333333333333';
const ALICE = '11111111-1111-4111-8111-111111111111';
const GONE = '22222222-2222-4222-8222-222222222222';

function historyResponse(
  overrides: Partial<WardrobeWearHistoryResponse> = {},
): WardrobeWearHistoryResponse {
  return {
    history: {
      wearCount: 4,
      firstWornAt: '2026-03-14T00:00:00.000Z',
      lastWornAt: '2026-10-06T00:00:00.000Z',
      lastWornChatId: CHAT,
      wearers: [
        {
          characterId: ALICE,
          wearCount: 3,
          firstWornAt: '2026-03-14T00:00:00.000Z',
          lastWornAt: '2026-10-06T00:00:00.000Z',
          lastWornChatId: CHAT,
        },
        {
          characterId: GONE,
          wearCount: 1,
          firstWornAt: '2026-04-01T00:00:00.000Z',
          lastWornAt: '2026-04-01T00:00:00.000Z',
          lastWornChatId: null,
        },
      ],
    },
    wearers: [{ characterId: ALICE, name: 'Vivienne', avatarUrl: '/avatars/v.webp' }],
    lastWornChat: { id: CHAT, title: 'The Thornfield Dinner' },
    ...overrides,
  };
}

interface Rendered {
  el: HTMLElement;
  seen: Array<Record<string, unknown>>;
  openTab: ReturnType<typeof vi.fn>;
  navigate: ReturnType<typeof vi.fn>;
}

async function render(
  inputs: { itemId: string; container: WardrobeContainer; createdAt?: string | null; isComposite: boolean },
  answer: () => Promise<unknown>,
  opts: { workspace?: boolean } = {},
): Promise<Rendered> {
  const seen: Array<Record<string, unknown>> = [];
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      seen.push(req as unknown as Record<string, unknown>);
      return answer();
    }),
  } as unknown as CoreClient;
  const openTab = vi.fn(() => 'tab-1');
  const navigate = vi.fn(async () => true);
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WardrobeWearHistorySection],
    providers: [
      { provide: CoreClient, useValue: core },
      { provide: Router, useValue: { navigate } },
      ...(opts.workspace === false ? [] : [{ provide: WORKSPACE_HANDLE, useValue: { openTab } }]),
    ],
  });
  const fixture = TestBed.createComponent(WardrobeWearHistorySection);
  fixture.componentRef.setInput('itemId', inputs.itemId);
  fixture.componentRef.setInput('container', inputs.container);
  if (inputs.createdAt !== undefined) fixture.componentRef.setInput('createdAt', inputs.createdAt);
  fixture.componentRef.setInput('isComposite', inputs.isComposite);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
  return { el: fixture.nativeElement as HTMLElement, seen, openTab, navigate };
}

const dt = (el: HTMLElement, label: string): HTMLElement | undefined =>
  [...el.querySelectorAll<HTMLElement>('dt')].find((d) => d.textContent!.trim() === label);

afterEach(() => vi.restoreAllMocks());

describe('WardrobeWearHistorySection (v4 wardrobe-wear-ledger-ui.test.tsx)', () => {
  it('reads wardrobeItemWearHistory through the item’s container and lays out the breakdown', async () => {
    const { el, seen } = await render(
      {
        itemId: 'item-1',
        container: { scope: 'character', id: 'char-1' },
        createdAt: '2026-03-12T00:00:00.000Z',
        isComposite: false,
      },
      async () => historyResponse(),
    );
    expect(seen).toEqual([
      { type: 'wardrobeItemWearHistory', scope: 'character', containerId: 'char-1', itemId: 'item-1' },
    ]);
    const section = el.querySelector('section')!;
    expect(section.getAttribute('aria-labelledby')).toBe('wear-history-item-1');
    expect(section.className).toBe('border-t qt-border-default pt-4');
    const h3 = section.querySelector('h3')!;
    expect(h3.id).toBe('wear-history-item-1');
    expect(h3.textContent!.trim()).toBe('Wear history');

    const link = el.querySelector<HTMLAnchorElement>('a.qt-link')!;
    expect(link.textContent!.trim()).toBe('“The Thornfield Dinner”');
    expect(link.getAttribute('href')).toBe(`/salon/${CHAT}`);
    expect(dt(el, 'Times worn')!.nextElementSibling!.textContent!.trim()).toBe('4');
    expect(dt(el, 'Created')).toBeDefined();
    expect(dt(el, 'First worn')).toBeDefined();
    expect(dt(el, 'Last worn')!.nextElementSibling!.textContent).toContain(', in');
    expect(el.textContent).toContain('Vivienne');
    const counts = [...el.querySelectorAll('li span.qt-text-xs')].map((s) => s.textContent!.trim());
    expect(counts[0]).toMatch(/^3×, last /);
    // A wearer the server could not name is labelled, not dropped.
    expect(el.textContent).toContain('A departed character');
    expect(el.querySelector('img')!.getAttribute('src')).toBe('/avatars/v.webp');
    expect(el.textContent).not.toMatch(/also counts a wear/);
  });

  it('says "a chat since deleted" when the last chat is gone', async () => {
    const { el } = await render(
      { itemId: 'item-2', container: { scope: 'general', id: null }, isComposite: true },
      async () => historyResponse({ lastWornChat: null }),
    );
    expect(el.textContent).toMatch(/, in a chat since deleted/);
    expect(el.querySelector('a')).toBeNull();
    expect(dt(el, 'Created')).toBeUndefined();
    const foot = [...el.querySelectorAll('p')].map((p) => p.textContent!.trim());
    expect(foot).toContain('Wearing this outfit also counts a wear for each garment it put on.');
  });

  it('shows a never-worn item as zero wears with no dates', async () => {
    const { el } = await render(
      { itemId: 'item-3', container: { scope: 'general', id: null }, isComposite: false },
      async () =>
        historyResponse({
          history: {
            wearCount: 0,
            firstWornAt: null,
            lastWornAt: null,
            lastWornChatId: null,
            wearers: [],
          },
          wearers: [],
          lastWornChat: null,
        }),
    );
    expect(dt(el, 'Times worn')!.nextElementSibling!.textContent!.trim()).toBe('0');
    expect(dt(el, 'Last worn')).toBeUndefined();
    expect(dt(el, 'Worn by')).toBeUndefined();
  });
});

describe('WardrobeWearHistorySection — the verb, the states and the chat link (v5)', () => {
  it('names the container per scope; General carries NO containerId (C2 §3)', async () => {
    const cases: Array<[WardrobeContainer, Record<string, unknown>]> = [
      [{ scope: 'general', id: null }, { scope: 'general' }],
      [{ scope: 'project', id: 'p1' }, { scope: 'project', containerId: 'p1' }],
      [{ scope: 'group', id: 'g1' }, { scope: 'group', containerId: 'g1' }],
    ];
    for (const [container, expected] of cases) {
      const { seen } = await render(
        { itemId: 'w', container, isComposite: false },
        async () => historyResponse(),
      );
      expect(seen).toEqual([{ type: 'wardrobeItemWearHistory', ...expected, itemId: 'w' }]);
    }
  });

  it('says it is consulting the ledger while the read is in flight, and v4’s sentence on failure', async () => {
    const pending = await render(
      { itemId: 'w', container: { scope: 'general', id: null }, isComposite: false },
      () => new Promise(() => {}),
    );
    expect(pending.el.querySelector('p')!.textContent!.trim()).toBe('Consulting the ledger…');
    const failed = await render(
      { itemId: 'w', container: { scope: 'general', id: null }, isComposite: false },
      async () => {
        throw new Error('not_available');
      },
    );
    expect(failed.el.querySelector('p')!.textContent!.trim()).toBe(
      'The wear ledger could not be read just now.',
    );
  });

  it('the chat link opens the Salon TAB in the workspace, else routes to /salon/:id', async () => {
    const hosted = await render(
      { itemId: 'w', container: { scope: 'general', id: null }, isComposite: false },
      async () => historyResponse(),
    );
    hosted.el.querySelector<HTMLAnchorElement>('a.qt-link')!.click();
    expect(hosted.openTab).toHaveBeenCalledWith('salon', { chatId: CHAT });
    expect(hosted.navigate).not.toHaveBeenCalled();

    const bare = await render(
      { itemId: 'w', container: { scope: 'general', id: null }, isComposite: false },
      async () => historyResponse(),
      { workspace: false },
    );
    bare.el.querySelector<HTMLAnchorElement>('a.qt-link')!.click();
    expect(bare.navigate).toHaveBeenCalledWith(['/salon', CHAT]);
  });
});
