/**
 * The Wardrobe dialog's History tab (P4.D318 — v4 `8d8618819` + `4ae20977f`,
 * `components/wardrobe/__tests__/outfit-history-tab.test.tsx`): newest first,
 * the current entry marked "Wearing now" with no button, Wear again posting the
 * rollback, a stale-item warning on the button, and — when the Live tab holds
 * staged edits for this character — a confirmation, then the staging discarded
 * before the rollback runs, so Done can never flush it over the restored outfit.
 *
 * v4's six cases are transcribed NAME FOR NAME over v4's fixture (verbatim,
 * `actorToolName: null` in every entry — `4ae20977f`); the six v5-added cases
 * pin the strings v4's test does not assert.
 */

import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import {
  CoreDispatchError,
  type CoreRequest,
  type OutfitHistoryEntry,
  type OutfitHistoryResponse,
} from '../core/core-contract';
import { formatRelativeDate } from '../shared/format-date';
import { NowService } from '../shared/now.service';
import { ToastService } from '../ui/toast.service';
import { OutfitHistoryTab, staleCount } from './outfit-history-tab';

type AnyRequest = CoreRequest & Record<string, unknown>;

const EMPTY = { top: [], bottom: [], footwear: [], accessories: [], hair: [] };

/** v4 `outfit-history-tab.test.tsx:15-37`, verbatim. */
const HISTORY: OutfitHistoryResponse = {
  currentIndex: 2,
  entries: [
    {
      index: 0, at: '2026-10-16T10:00:00.000Z', source: 'chat-start', actorCharacterId: null, actorName: null, actorToolName: null,
      changedBy: 'chat opened', isCurrent: false, summary: 'Green Dress',
      slots: { ...EMPTY, top: [{ itemId: 'dress', title: 'Green Dress', archived: false, missing: false }] },
    },
    {
      index: 1, at: '2026-10-16T11:00:00.000Z', source: 'character', actorCharacterId: 'char-1', actorName: 'Bertie', actorToolName: null,
      changedBy: 'Bertie', isCurrent: false, summary: 'Greatcoat (archived)',
      slots: { ...EMPTY, top: [{ itemId: 'coat', title: 'Greatcoat', archived: true, missing: false }] },
    },
    {
      index: 2, at: '2026-10-16T12:00:00.000Z', source: 'operator', actorCharacterId: null, actorName: null, actorToolName: null,
      changedBy: 'the operator', isCurrent: true, summary: 'Nothing',
      slots: EMPTY,
    },
  ],
};

/** A fixed clock (v4 `useNow(60_000)` → the stubbed `NowService`). */
const NOW = Date.parse('2026-10-16T13:30:00.000Z');

type Route = (req: AnyRequest) => unknown;

/** v4's fetch spy, as a dispatch route: the read answers HISTORY, the rollback v4's body. */
const defaultRoute: Route = (req) =>
  (req.type as string) === 'chatWardrobeRollback'
    ? { equippedSlots: EMPTY, previousIndex: 2, restoredIndex: req['index'], skipped: [] }
    : HISTORY;

interface Inputs {
  chatId: string;
  characterId: string;
  characterName: string;
  hasStagedEdits: boolean;
  discardStagedEdits: ReturnType<typeof vi.fn>;
  requestConfirmation: ReturnType<typeof vi.fn>;
}

interface Handle {
  fixture: ComponentFixture<OutfitHistoryTab>;
  el: HTMLElement;
  seen: AnyRequest[];
  props: Inputs;
  toasts: ToastService;
  queryClient: QueryClient;
}

async function settle(fixture: ComponentFixture<unknown>, rounds = 8): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function renderTab(overrides: Partial<Inputs> = {}, route: Route = defaultRoute): Promise<Handle> {
  const seen: AnyRequest[] = [];
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      const r = req as AnyRequest;
      seen.push(r);
      return (await route(r)) as Record<string, unknown>;
    }),
  } as unknown as CoreClient;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [OutfitHistoryTab],
    providers: [
      provideTanStackQuery(queryClient),
      { provide: CoreClient, useValue: core },
      { provide: NowService, useValue: { now: () => signal(NOW).asReadonly() } },
    ],
  });
  const toasts = TestBed.inject(ToastService);
  vi.spyOn(toasts, 'showSuccess');
  vi.spyOn(toasts, 'showError');
  const props: Inputs = {
    chatId: 'chat-1',
    characterId: 'char-1',
    characterName: 'Bertie',
    hasStagedEdits: false,
    discardStagedEdits: vi.fn(),
    requestConfirmation: vi.fn().mockResolvedValue(true),
    ...overrides,
  };
  const fixture = TestBed.createComponent(OutfitHistoryTab);
  for (const [k, v] of Object.entries(props)) fixture.componentRef.setInput(k, v);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, el: fixture.nativeElement as HTMLElement, seen, props, toasts, queryClient };
}

const entryEl = (el: HTMLElement, index: number): HTMLElement =>
  el.querySelector(`[data-testid="outfit-history-entry-${index}"]`) as HTMLElement;

const wearAgainButton = (el: HTMLElement, index: number): HTMLButtonElement | null =>
  ([...entryEl(el, index).querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === 'Wear again',
  ) as HTMLButtonElement | undefined) ?? null;

const rollbackCalls = (seen: AnyRequest[]): AnyRequest[] =>
  seen.filter((r) => (r.type as string) === 'chatWardrobeRollback');

describe('OutfitHistoryTab', () => {
  afterEach(() => vi.restoreAllMocks());

  it('lists newest first, marks the current entry, and offers Wear again on the rest', async () => {
    const { el } = await renderTab();
    const entries = [...el.querySelectorAll('[data-testid^="outfit-history-entry-"]')];
    expect(entries.map((e) => e.getAttribute('data-testid'))).toEqual([
      'outfit-history-entry-2',
      'outfit-history-entry-1',
      'outfit-history-entry-0',
    ]);
    const current = entryEl(el, 2);
    expect(
      [...current.querySelectorAll('span')].some((s) => s.textContent?.trim() === 'Wearing now'),
    ).toBe(true);
    expect(wearAgainButton(el, 2)).toBeNull();
    expect(wearAgainButton(el, 0)).not.toBeNull();
  });

  it('warns ahead of time when an entry holds items that will be left off', async () => {
    const { el } = await renderTab();
    const button = wearAgainButton(el, 1)!;
    expect(button.getAttribute('title')).toContain('1 item in this outfit is no longer available');
    expect(entryEl(el, 1).querySelector('[title="Top — archived"]')).not.toBeNull();
  });

  it('Wear again posts the rollback for that index', async () => {
    const { el, fixture, seen, props } = await renderTab();
    wearAgainButton(el, 0)!.click();
    await settle(fixture);
    expect(rollbackCalls(seen)).toHaveLength(1);
    expect(rollbackCalls(seen)[0]).toEqual({
      type: 'chatWardrobeRollback',
      chatId: 'chat-1',
      characterId: 'char-1',
      index: 0,
    });
    expect(props.requestConfirmation).not.toHaveBeenCalled();
  });

  it('confirms and discards staged Live-tab edits before rolling back', async () => {
    const order: string[] = [];
    const { el, fixture, props } = await renderTab(
      {
        hasStagedEdits: true,
        discardStagedEdits: vi.fn((options?: { reseed?: boolean }) => {
          order.push(options?.reseed ? 'reseed' : 'discard');
        }),
      },
      (req) => {
        if ((req.type as string) === 'chatWardrobeRollback') {
          order.push('rollback');
          return { equippedSlots: EMPTY, previousIndex: 2, restoredIndex: 0, skipped: [] };
        }
        return HISTORY;
      },
    );
    wearAgainButton(el, 0)!.click();
    await settle(fixture);
    expect(props.requestConfirmation).toHaveBeenCalledWith(
      "Discard the staged changes to Bertie's outfit and return to outfit 0?",
    );
    expect(order).toEqual(['discard', 'rollback', 'reseed']);
  });

  it('does nothing when the operator keeps the staged edits', async () => {
    const { el, fixture, seen, props } = await renderTab({
      hasStagedEdits: true,
      requestConfirmation: vi.fn().mockResolvedValue(false),
    });
    wearAgainButton(el, 0)!.click();
    await settle(fixture);
    expect(props.requestConfirmation).toHaveBeenCalled();
    expect(props.discardStagedEdits).not.toHaveBeenCalled();
    expect(rollbackCalls(seen)).toHaveLength(0);
  });

  it('says so when nothing is recorded', async () => {
    const { el } = await renderTab({}, () => ({ currentIndex: null, entries: [] }));
    expect(el.textContent).toMatch(/No outfits recorded yet/);
    const p = [...el.querySelectorAll('p')].find((x) => /No outfits recorded yet/.test(x.textContent ?? ''))!;
    expect(p.textContent).toBe(
      'No outfits recorded yet. The first change of clothes starts the record.',
    );
    expect(p.getAttribute('class')).toBe('qt-text-xs qt-text-muted px-1');
  });

  // ---- v5-added: the strings v4's test does not assert ----

  it('renders changedBy verbatim, the relative time with the full date as its title, and the summary', async () => {
    const pascal: OutfitHistoryEntry = {
      ...HISTORY.entries[2],
      source: 'pascal',
      actorCharacterId: 'char-9',
      actorName: 'Jeeves',
      actorToolName: 'dress_for_dinner',
      changedBy: "Pascal's dress_for_dinner (rolled by Jeeves)",
    };
    const { el } = await renderTab({}, (req) =>
      (req.type as string) === 'chatWardrobeHistory'
        ? { currentIndex: 2, entries: [HISTORY.entries[0], HISTORY.entries[1], pascal] }
        : defaultRoute(req),
    );
    const entry = entryEl(el, 2);
    const spans = [...entry.querySelectorAll('span')];
    expect(spans.some((s) => s.textContent === "· Pascal's dress_for_dinner (rolled by Jeeves)")).toBe(true);
    const at = HISTORY.entries[2].at;
    const time = spans.find((s) => s.getAttribute('title') === new Date(at).toLocaleString())!;
    expect(time).toBeTruthy();
    expect(time.textContent).toBe(formatRelativeDate(at, NOW));
    expect(spans.some((s) => s.textContent === '#2' && s.getAttribute('class') === 'qt-badge text-xs')).toBe(true);
    const summary = [...entry.querySelectorAll('p')].find((p) => p.getAttribute('class') === 'qt-text-xs mb-1')!;
    expect(summary.textContent).toBe('Nothing');
    const description = el.querySelector('p') as HTMLElement;
    expect(description.getAttribute('class')).toBe('qt-text-xs qt-text-secondary px-1');
    expect(description.textContent).toBe(
      'Every change of clothes in this chat, numbered from the outfit they arrived in. Wear again returns to one — Aurora will say so.',
    );
    // An unstaged entry's button title names the character and the index.
    expect(wearAgainButton(el, 0)!.getAttribute('title')).toBe('Return Bertie to outfit 0');
    expect(wearAgainButton(el, 0)!.getAttribute('class')).toBe('qt-button-secondary qt-button-sm');
  });

  it('toasts "Back in outfit N" and the skipped-item sentence, singular and plural', async () => {
    let skipped: Array<{ itemId: string; title: string | null; reason: string }> = [];
    const { el, fixture, toasts } = await renderTab({}, (req) =>
      (req.type as string) === 'chatWardrobeRollback'
        ? { equippedSlots: EMPTY, previousIndex: 2, restoredIndex: 0, skipped }
        : HISTORY,
    );
    wearAgainButton(el, 0)!.click();
    await settle(fixture);
    expect(toasts.showSuccess).toHaveBeenLastCalledWith('Back in outfit 0');

    skipped = [{ itemId: 'coat', title: 'Greatcoat', reason: 'archived' }];
    wearAgainButton(el, 0)!.click();
    await settle(fixture);
    expect(toasts.showSuccess).toHaveBeenLastCalledWith(
      'Back in outfit 0. 1 item could not be put back on: Greatcoat',
    );

    skipped = [
      { itemId: 'coat', title: 'Greatcoat', reason: 'archived' },
      { itemId: 'gone-id', title: null, reason: 'missing' },
    ];
    wearAgainButton(el, 0)!.click();
    await settle(fixture);
    expect(toasts.showSuccess).toHaveBeenLastCalledWith(
      'Back in outfit 0. 2 items could not be put back on: Greatcoat, gone-id',
    );

    // Tier 2 item 17: every skipped item untitled → the ids.
    skipped = [
      { itemId: 'id-a', title: null, reason: 'missing' },
      { itemId: 'id-b', title: null, reason: 'missing' },
    ];
    wearAgainButton(el, 0)!.click();
    await settle(fixture);
    expect(toasts.showSuccess).toHaveBeenLastCalledWith(
      'Back in outfit 0. 2 items could not be put back on: id-a, id-b',
    );
    expect(toasts.showError).not.toHaveBeenCalled();
  });

  it("a refused rollback toasts the server's sentence and still re-seeds", async () => {
    let refusal: unknown = new CoreDispatchError({
      kind: 'bad-request',
      message: 'Bertie is already wearing outfit 0.',
    });
    const { el, fixture, toasts, props, queryClient } = await renderTab({}, (req) => {
      if ((req.type as string) === 'chatWardrobeRollback') throw refusal;
      return HISTORY;
    });
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
    wearAgainButton(el, 0)!.click();
    await settle(fixture);
    expect(toasts.showError).toHaveBeenLastCalledWith('Bertie is already wearing outfit 0.');
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['chat', 'chat-1', 'wardrobe-history'] });
    expect(props.discardStagedEdits).toHaveBeenCalledWith({ reseed: true });
    expect(toasts.showSuccess).not.toHaveBeenCalled();

    refusal = 'not an Error';
    wearAgainButton(el, 0)!.click();
    await settle(fixture);
    expect(toasts.showError).toHaveBeenLastCalledWith('Could not return to that outfit');
  });

  it('a stale badge reads "<title> · gone" / "· archived" with the slot-label title; an empty entry reads "Nothing at all."', async () => {
    const stale: OutfitHistoryEntry = {
      ...HISTORY.entries[0],
      index: 3,
      isCurrent: false,
      slots: {
        ...EMPTY,
        top: [
          { itemId: 'coat', title: 'Greatcoat', archived: false, missing: true },
          { itemId: 'shirt', title: 'Shirt', archived: false, missing: false },
        ],
        accessories: [
          { itemId: 'pin', title: 'Tie Pin', archived: true, missing: false },
          { itemId: 'untitled', title: null, archived: false, missing: false },
        ],
      },
    };
    const twice: OutfitHistoryEntry = {
      ...HISTORY.entries[0],
      index: 4,
      isCurrent: false,
      slots: {
        ...EMPTY,
        top: [{ itemId: 'cloak', title: 'Cloak', archived: true, missing: false }],
        bottom: [{ itemId: 'cloak', title: 'Cloak', archived: true, missing: false }],
      },
    };
    const { el } = await renderTab({}, (req) =>
      (req.type as string) === 'chatWardrobeHistory'
        ? { currentIndex: 2, entries: [...HISTORY.entries, stale, twice] }
        : defaultRoute(req),
    );
    const badges = [...entryEl(el, 3).querySelectorAll('div.flex-wrap > span')];
    const byText = (t: string) => badges.find((b) => b.textContent === t)!;
    const gone = byText('Greatcoat · gone');
    expect(gone.getAttribute('class')).toBe('qt-badge qt-badge-secondary text-xs');
    expect(gone.getAttribute('title')).toBe('Top — no longer exists');
    const live = byText('Shirt');
    expect(live.getAttribute('class')).toBe('qt-badge qt-badge-wardrobe-top text-xs');
    expect(live.getAttribute('title')).toBe('Top');
    const archived = byText('Tie Pin · archived');
    expect(archived.getAttribute('title')).toBe('Accessories — archived');
    expect(byText('untitled')).toBeTruthy();
    // Canonical slot order: top's two, then accessories' two.
    expect(badges.map((b) => b.textContent)).toEqual([
      'Greatcoat · gone',
      'Shirt',
      'Tie Pin · archived',
      'untitled',
    ]);
    expect(wearAgainButton(el, 3)!.getAttribute('title')).toBe(
      '2 items in this outfit are no longer available and will be left off.',
    );
    // An id stale in two slots counts once.
    expect(wearAgainButton(el, 4)!.getAttribute('title')).toBe(
      '1 item in this outfit is no longer available and will be left off.',
    );
    const nothing = [...entryEl(el, 2).querySelectorAll('p')].find(
      (p) => p.textContent === 'Nothing at all.',
    )!;
    expect(nothing.getAttribute('class')).toBe('qt-text-xs qt-text-muted italic');
  });

  it('every Wear again is disabled while a rollback is in flight', async () => {
    let release!: () => void;
    const held = new Promise<void>((r) => (release = r));
    const { el, fixture } = await renderTab({}, async (req) => {
      if ((req.type as string) === 'chatWardrobeRollback') {
        await held;
        return { equippedSlots: EMPTY, previousIndex: 2, restoredIndex: 0, skipped: [] };
      }
      return HISTORY;
    });
    wearAgainButton(el, 0)!.click();
    await settle(fixture);
    expect(wearAgainButton(el, 0)!.disabled).toBe(true);
    expect(wearAgainButton(el, 1)!.disabled).toBe(true);
    release();
    await settle(fixture);
    expect(wearAgainButton(el, 0)!.disabled).toBe(false);
    expect(wearAgainButton(el, 1)!.disabled).toBe(false);
  });

  it("a read failure shows the server's sentence", async () => {
    const danger = (el: HTMLElement) =>
      [...el.querySelectorAll('p')].find((p) => p.getAttribute('class') === 'qt-text-xs qt-text-danger px-1');

    const refused = await renderTab({}, () => {
      throw new CoreDispatchError({ kind: 'not-found', message: 'Chat not found' });
    });
    expect(danger(refused.el)?.textContent).toBe('Chat not found');

    const odd = await renderTab({}, () => {
      throw 'not an Error';
    });
    expect(danger(odd.el)?.textContent).toBe('The outfit record could not be read.');

    const pending = await renderTab({}, () => new Promise(() => undefined));
    const loading = [...pending.el.querySelectorAll('p')].find(
      (p) => p.textContent === 'Consulting the record…',
    );
    expect(loading?.getAttribute('class')).toBe('qt-text-xs qt-text-muted px-1');
  });
});

describe('staleCount (v4 outfit-history-tab.tsx:77-86)', () => {
  const item = (itemId: string, archived = false, missing = false) => ({ itemId, title: itemId, archived, missing });
  const base: OutfitHistoryEntry = { ...HISTORY.entries[0], slots: { ...EMPTY } };

  it('counts DISTINCT stale ids across the five slots, either flag', () => {
    expect(staleCount(base)).toBe(0);
    expect(
      staleCount({
        ...base,
        slots: { ...EMPTY, top: [item('a', true), item('b')], hair: [item('c', false, true)] },
      }),
    ).toBe(2);
    expect(
      staleCount({ ...base, slots: { ...EMPTY, top: [item('a', true)], bottom: [item('a', true)] } }),
    ).toBe(1);
  });

  it('reads an absent slot key as empty', () => {
    const partial = { ...base, slots: { top: [item('a', true)] } } as unknown as OutfitHistoryEntry;
    expect(staleCount(partial)).toBe(1);
  });
});
