import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub } from '../core/core-client.testing';
import type {
  CoreRequest,
  CoreResponse,
  StorytellerBid,
  StorytellerRound,
} from '../core/core-contract';
import { RealtimeService } from '../core/realtime.service';
import { ToastService } from '../ui/toast.service';
import { chatKeys } from './chat-keys';
import { StorytellerBoard, type StorytellerRunRequest } from './storyteller-board';
import { withBid } from './storyteller-board.pure';

/**
 * The storyteller board (P4.D344) — v5-authored from v4
 * `components/chat/StorytellerBoard.tsx` at the pin `236c04e35`, every string
 * byte for byte (v4 ships no component test for it; its one client test, the
 * arrange cases, lives in `storyteller-board.pure.spec.ts`). The dispatch is
 * stubbed against wire contract W1 (P4.D343 serves it).
 */

function bid(participantId: string, over: Partial<StorytellerBid> = {}): StorytellerBid {
  return {
    participantId,
    characterId: `char-${participantId}`,
    characterName: participantId.toUpperCase(),
    status: 'bid',
    statedTier: 'simple',
    tier: 'simple',
    tierRaised: false,
    tools: [],
    plan: null,
    reason: null,
    talkativeness: 0.5,
    silent: false,
    askedAt: null,
    answeredAt: null,
    ...over,
  };
}

function openRound(bids: StorytellerBid[], id = 'round-1'): StorytellerRound {
  return {
    id,
    chatId: 'chat-1',
    triggerMessageId: 'm-1',
    status: 'open',
    bids,
    selection: null,
    closedReason: null,
    createdAt: '2026-11-02T00:00:00.000Z',
    updatedAt: '2026-11-02T00:00:00.000Z',
    closedAt: null,
  };
}

interface Stub {
  calls: CoreRequest[];
  client: Partial<CoreClient>;
  answer: { round: StorytellerRound | null };
}

function stub(
  round: StorytellerRound | null,
  fail: Partial<Record<string, { kind: string; message: string }>> = {},
): Stub {
  const calls: CoreRequest[] = [];
  const stream = coreStreamStub();
  const answer = { round };
  const dispatch = vi.fn(async (req: CoreRequest): Promise<CoreResponse> => {
    calls.push(req);
    const refusal = fail[req.type];
    if (refusal) return { type: 'error', data: refusal } as CoreResponse;
    switch (req.type) {
      case 'chatStorytellerRound':
        return { type: 'storytellerRound', data: { round: answer.round } };
      case 'chatStorytellerAsk':
        return { type: 'storytellerAsk', data: { bid: bid(req.participantId) } };
      case 'chatStorytellerLie':
        return { type: 'storytellerLie', data: { closed: true } };
      default:
        throw new Error(`unexpected ${req.type}`);
    }
  });
  const client: Partial<CoreClient> = {
    ...stream,
    dispatch: dispatch as unknown as CoreClient['dispatch'],
  };
  client.dispatchExpect = CoreClient.prototype.dispatchExpect.bind(client as CoreClient);
  return { calls, client, answer };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

interface Mounted {
  fixture: ComponentFixture<StorytellerBoard>;
  toasts: ToastService;
  queryClient: QueryClient;
  runs: StorytellerRunRequest[];
  el: HTMLElement;
}

async function mount(s: Stub, busy = false): Promise<Mounted> {
  TestBed.resetTestingModule();
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  TestBed.configureTestingModule({
    imports: [StorytellerBoard],
    providers: [provideTanStackQuery(queryClient), { provide: CoreClient, useValue: s.client }],
  });
  TestBed.inject(RealtimeService);
  const toasts = TestBed.inject(ToastService);
  const fixture = TestBed.createComponent(StorytellerBoard);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.componentRef.setInput('busy', busy);
  fixture.componentRef.setInput('participants', [
    { id: 'a', character: { avatarUrl: 'a.png' } },
  ] as never);
  const runs: StorytellerRunRequest[] = [];
  fixture.componentInstance.run.subscribe((r) => runs.push(r));
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, toasts, queryClient, runs, el: fixture.nativeElement as HTMLElement };
}

const text = (el: Element | null) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

function button(el: HTMLElement, label: string): HTMLButtonElement {
  const found = [...el.querySelectorAll('button')].find((b) => text(b) === label);
  if (!found) throw new Error(`no button ${label}`);
  return found as HTMLButtonElement;
}

function tick(el: HTMLElement, name: string): HTMLInputElement {
  const box = el.querySelector(`input[aria-label="Let ${name} go ahead"]`) as HTMLInputElement | null;
  if (!box) throw new Error(`no checkbox for ${name}`);
  return box;
}

async function click(m: Mounted, target: HTMLElement): Promise<void> {
  if (target instanceof HTMLInputElement) {
    target.checked = !target.checked;
    target.dispatchEvent(new Event('change'));
  } else {
    target.click();
  }
  await settle(m.fixture);
}

describe('StorytellerBoard (v4 components/chat/StorytellerBoard.tsx @ 236c04e35)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('reads the round through chatStorytellerRound and renders nothing without one', async () => {
    const s = stub(null);
    const m = await mount(s);
    expect(s.calls).toEqual([{ type: 'chatStorytellerRound', chatId: 'chat-1' }]);
    expect(m.el.children.length).toBe(0);
    expect(m.el.classList.contains('block')).toBe(true);
  });

  it('folds a closed round to its summary line', async () => {
    const r: StorytellerRound = {
      ...openRound([]),
      status: 'closed',
      closedReason: 'ran',
      selection: { participantIds: ['a', 'b'], order: 'simultaneous', chosenAt: 'x', names: ['Ada', 'Bo'] },
    };
    const m = await mount(stub(r));
    const line = m.el.querySelector('.qt-storyteller-summary');
    expect(line?.getAttribute('role')).toBe('status');
    expect(text(line)).toBe('Last round: Ada, Bo (all at once)');
    expect(line?.querySelector('qt-icon')?.getAttribute('class')).toBe('w-3.5 h-3.5');
    expect(m.el.querySelector('section')).toBeNull();
  });

  it('the five summary variants', async () => {
    const sel = { participantIds: ['a'], order: 'sequential' as const, chosenAt: 'x', names: ['Ada'] };
    const cases: Array<[Partial<StorytellerRound>, string]> = [
      [{ status: 'running', selection: sel }, 'Speaking now: Ada'],
      [{ status: 'closed', closedReason: 'let-lie' }, 'The last round was let lie.'],
      [{ status: 'closed', closedReason: 'abandoned' }, 'The last round closed unrun.'],
      [{ status: 'closed', closedReason: 'ran', selection: sel }, 'Last round: Ada'],
      [{ status: 'closed', closedReason: 'ran' }, 'The last round has run.'],
    ];
    for (const [over, want] of cases) {
      const m = await mount(stub({ ...openRound([]), ...over }));
      expect(text(m.el.querySelector('.qt-storyteller-summary'))).toBe(want);
    }
  });

  it('an open board: header, pending count, every card kind and every line', async () => {
    const r = openRound([
      bid('p', { status: 'pending', tier: null, statedTier: null }),
      bid('a', { plan: 'I shall put the kettle on.', tools: ['search', 'run_custom:dice'], reason: 'It is cold.' }),
      bid('r', { tier: 'act', statedTier: 'simple', tierRaised: true, plan: 'Fetch the ledger.' }),
      bid('l', { tier: 'lookup', statedTier: 'simple', tierRaised: true }),
      bid('x', { status: 'pass', reason: 'tired', talkativeness: 0.1, silent: true }),
      bid('e', { status: 'error', reason: null }),
      bid('k', { status: 'kept-quiet' }),
    ]);
    const m = await mount(stub(r));
    const section = m.el.querySelector('section.qt-storyteller-board');
    expect(section?.getAttribute('aria-label')).toBe("The storyteller's board");
    expect(text(m.el.querySelector('.qt-storyteller-board-header > span'))).toBe(
      'The company bids — 1 still considering…',
    );
    const cards = [...m.el.querySelectorAll('.qt-storyteller-cards > div')];
    expect(cards.map((c) => c.getAttribute('class'))).toEqual([
      'qt-storyteller-card',
      'qt-storyteller-card',
      'qt-storyteller-card qt-storyteller-card-act',
      'qt-storyteller-card',
      'qt-storyteller-card qt-storyteller-card-quiet',
      'qt-storyteller-card qt-storyteller-card-quiet',
      'qt-storyteller-card qt-storyteller-card-quiet',
    ]);
    const lines = (c: Element) => [...c.children].slice(1).map(text);
    expect(text(cards[0].querySelector('.qt-storyteller-card-name'))).toBe('P');
    expect(cards[0].querySelector('input')).toBeNull();
    expect(lines(cards[0])).toEqual(['Considering…', 'Talkativeness 0.50']);
    expect(text(cards[1].querySelector('.qt-badge'))).toBe('Simple');
    expect(cards[1].querySelector('.qt-badge')?.getAttribute('class')).toBe('qt-badge qt-badge-secondary');
    expect(cards[1].querySelector('qt-avatar')).not.toBeNull();
    expect(cards[1].querySelector('.qt-storyteller-card-name')?.getAttribute('title')).toBe('A');
    expect(lines(cards[1])).toEqual([
      'I shall put the kettle on.',
      'Tools: search, run_custom:dice',
      'It is cold.',
      'Talkativeness 0.50',
    ]);
    expect(cards[2].querySelector('.qt-badge')?.getAttribute('class')).toBe('qt-badge qt-badge-warning');
    expect(lines(cards[2])).toEqual(['Fetch the ledger.', 'Asked for Simple, named an Act tool.', 'Talkativeness 0.50']);
    expect(lines(cards[3])).toEqual(['Asked for Simple, named a Lookup tool.', 'Talkativeness 0.50']);
    expect(lines(cards[4])).toEqual(['Passes — tired', 'Talkativeness 0.10 · silent']);
    expect(lines(cards[5])).toEqual(['No bid — the call failed', 'Talkativeness 0.50', 'Ask anyway']);
    expect(lines(cards[6])).toEqual(['Kept quiet this round', 'Talkativeness 0.50', 'Ask anyway']);
    // Unarranged: no order badge, no Reset order, and v5's arrows hint
    // (BOARD_ARRANGE_HINT_WITHOUT_DRAG — v4's promises dragging).
    expect(m.el.querySelector('.qt-storyteller-card-order')).toBeNull();
    expect([...m.el.querySelectorAll('button')].map(text)).not.toContain('Reset order');
    expect(text(m.el.querySelector('section > .qt-storyteller-meta'))).toBe(
      'Use a card’s arrows to set the running order; left alone, the room’s rotation decides.',
    );
    expect(button(m.el, 'All at once').title).toBe(
      'Simple and Lookup bids speak at once; Act and Destroy bids follow one at a time',
    );
  });

  it('a Destroy bid wears its class and warning, goes ahead alone, and Select all leaves it out', async () => {
    const r = openRound([bid('a'), bid('b'), bid('d', { tier: 'destroy', statedTier: 'destroy' })]);
    const m = await mount(stub(r));
    const destroyCard = m.el.querySelectorAll('.qt-storyteller-cards > div')[2];
    expect(destroyCard.getAttribute('class')).toBe('qt-storyteller-card qt-storyteller-card-destroy');
    expect(destroyCard.querySelector('.qt-badge')?.getAttribute('class')).toBe('qt-badge qt-badge-destructive');
    expect(text(destroyCard.querySelector('.qt-storyteller-warning'))).toBe(
      'This bid means to delete something, and cannot be undone. It goes ahead alone.',
    );

    await click(m, button(m.el, 'Select all'));
    expect(tick(m.el, 'A').checked).toBe(true);
    expect(tick(m.el, 'B').checked).toBe(true);
    expect(tick(m.el, 'D').checked).toBe(false);

    await click(m, tick(m.el, 'D'));
    expect([tick(m.el, 'A').checked, tick(m.el, 'B').checked, tick(m.el, 'D').checked]).toEqual([false, false, true]);
    expect(m.el.querySelectorAll('.qt-storyteller-cards > div')[2].getAttribute('class')).toBe(
      'qt-storyteller-card qt-storyteller-card-selected qt-storyteller-card-destroy',
    );
    expect(button(m.el, 'Run in order').disabled).toBe(false);

    await click(m, tick(m.el, 'A'));
    expect([tick(m.el, 'A').checked, tick(m.el, 'D').checked]).toEqual([true, false]);
  });

  it('a Destroy bid in a mixed selection warns and disables Run in order / All at once (a ticked card re-tiered by a patch)', async () => {
    const r = openRound([bid('a'), bid('b')]);
    const m = await mount(stub(r));
    await click(m, button(m.el, 'Select all'));
    expect(text(m.el.querySelector('section > .qt-storyteller-warning'))).toBe('');
    // The toggle never builds a mixed set; a bid re-tiered under a live
    // selection does (the patch keeps the ticks — same round id).
    m.queryClient.setQueryData(chatKeys.storytellerRound('chat-1'), {
      round: withBid(r, bid('a', { tier: 'destroy' })),
    });
    await settle(m.fixture);
    expect(text(m.el.querySelector('section > .qt-storyteller-warning'))).toBe(
      'A Destroy bid must be chosen on its own.',
    );
    expect(button(m.el, 'Run in order').disabled).toBe(true);
    expect(button(m.el, 'All at once').disabled).toBe(true);
    expect(button(m.el, 'Let it lie').disabled).toBe(false);
  });

  it('Run in order is disabled with nothing chosen and while busy; All at once below two', async () => {
    const r = openRound([bid('a'), bid('b')]);
    const m = await mount(stub(r));
    expect(button(m.el, 'Run in order').disabled).toBe(true);
    expect(button(m.el, 'All at once').disabled).toBe(true);
    await click(m, tick(m.el, 'A'));
    expect(button(m.el, 'Run in order').disabled).toBe(false);
    expect(button(m.el, 'All at once').disabled).toBe(true);
    await click(m, tick(m.el, 'B'));
    expect(button(m.el, 'All at once').disabled).toBe(false);
    m.fixture.componentRef.setInput('busy', true);
    await settle(m.fixture);
    for (const label of ['Run in order', 'All at once', 'Let it lie', 'Select all']) {
      expect(button(m.el, label).disabled).toBe(true);
    }
    expect(tick(m.el, 'A').disabled).toBe(true);
  });

  it('run emits the chosen seats in BOARD order with keepOrder false (Tier 1 — no arrangement)', async () => {
    const r = openRound([bid('a'), bid('b'), bid('c')]);
    const m = await mount(stub(r));
    await click(m, tick(m.el, 'C'));
    await click(m, tick(m.el, 'A'));
    await click(m, button(m.el, 'Run in order'));
    await click(m, button(m.el, 'All at once'));
    expect(m.runs).toEqual([
      { roundId: 'round-1', participantIds: ['a', 'c'], order: 'sequential', keepOrder: false },
      { roundId: 'round-1', participantIds: ['a', 'c'], order: 'simultaneous', keepOrder: false },
    ]);
  });

  it('Ask anyway dispatches chatStorytellerAsk and refreshes the round', async () => {
    const s = stub(openRound([bid('k', { status: 'kept-quiet' })]));
    const m = await mount(s);
    const spy = vi.spyOn(m.queryClient, 'invalidateQueries');
    await click(m, button(m.el, 'Ask anyway'));
    expect(s.calls).toContainEqual({
      type: 'chatStorytellerAsk',
      chatId: 'chat-1',
      roundId: 'round-1',
      participantId: 'k',
    });
    expect(spy).toHaveBeenCalledWith({ queryKey: ['chat', 'chat-1', 'storytellerRound'] });
  });

  it("Ask anyway toasts the server's sentence, else v4's fallback", async () => {
    const s = stub(openRound([bid('k', { status: 'kept-quiet' })]), {
      chatStorytellerAsk: { kind: 'conflict', message: 'That round is no longer open' },
    });
    const m = await mount(s);
    const err = vi.spyOn(m.toasts, 'showError');
    await click(m, button(m.el, 'Ask anyway'));
    expect(err).toHaveBeenCalledWith('That round is no longer open');

    const s2 = stub(openRound([bid('k', { status: 'error' })]), {
      chatStorytellerAsk: { kind: 'internal', message: '' },
    });
    const m2 = await mount(s2);
    const err2 = vi.spyOn(m2.toasts, 'showError');
    await click(m2, button(m2.el, 'Ask anyway'));
    expect(err2).toHaveBeenCalledWith('Could not ask for that bid');
  });

  it('Let it lie dispatches chatStorytellerLie, invalidates, and toasts its own fallback', async () => {
    const s = stub(openRound([bid('a')]));
    const m = await mount(s);
    const spy = vi.spyOn(m.queryClient, 'invalidateQueries');
    await click(m, button(m.el, 'Let it lie'));
    expect(s.calls).toContainEqual({ type: 'chatStorytellerLie', chatId: 'chat-1', roundId: 'round-1' });
    expect(spy).toHaveBeenCalledWith({ queryKey: ['chat', 'chat-1', 'storytellerRound'] });

    const s2 = stub(openRound([bid('a')]), { chatStorytellerLie: { kind: 'internal', message: '' } });
    const m2 = await mount(s2);
    const err = vi.spyOn(m2.toasts, 'showError');
    await click(m2, button(m2.el, 'Let it lie'));
    expect(err).toHaveBeenCalledWith('Could not close the round');
  });

  it('the selection SURVIVES a bid patch and RESETS on a new round', async () => {
    const r = openRound([bid('a'), bid('b', { status: 'pending' })]);
    const m = await mount(stub(r));
    await click(m, tick(m.el, 'A'));
    m.queryClient.setQueryData(chatKeys.storytellerRound('chat-1'), { round: withBid(r, bid('b')) });
    await settle(m.fixture);
    expect(tick(m.el, 'A').checked).toBe(true);
    expect(tick(m.el, 'B').checked).toBe(false);

    m.queryClient.setQueryData(chatKeys.storytellerRound('chat-1'), {
      round: openRound([bid('a'), bid('b')], 'round-2'),
    });
    await settle(m.fixture);
    expect(tick(m.el, 'A').checked).toBe(false);
  });

  // --- Tier 2 (R-1): arrangement by buttons and keys, no drag dependency ---

  const order = (el: HTMLElement) =>
    [...el.querySelectorAll('.qt-storyteller-card-name')].map((n) => n.textContent?.trim());
  const arrow = (el: HTMLElement, name: string, dir: 'earlier' | 'later') =>
    el.querySelector(`button[aria-label="Move ${name} ${dir}"]`) as HTMLButtonElement | null;

  it('moving a card arranges the board: badges, Reset order, the arranged hint, keepOrder true in board order', async () => {
    const m = await mount(stub(openRound([bid('a'), bid('b'), bid('c')])));
    expect(m.el.querySelector('.qt-storyteller-cards > div')?.getAttribute('aria-roledescription')).toBeNull();
    await click(m, tick(m.el, 'A'));
    await click(m, tick(m.el, 'C'));
    await click(m, arrow(m.el, 'C', 'earlier')!);
    await click(m, arrow(m.el, 'C', 'earlier')!);
    expect(order(m.el)).toEqual(['C', 'A', 'B']);
    const badges = [...m.el.querySelectorAll('.qt-storyteller-card-order')];
    expect(badges.map((b) => [b.textContent?.trim(), b.getAttribute('title')])).toEqual([
      ['1', 'Speaks first'],
      ['2', 'Speaks second'],
    ]);
    expect(text(m.el.querySelector('section > .qt-storyteller-meta'))).toBe('Chosen bids run in the order shown.');
    const reset = button(m.el, 'Reset order');
    expect(reset.title).toBe("Put the cards back and let the room's rotation set the order");
    await click(m, button(m.el, 'Run in order'));
    expect(m.runs.at(-1)).toEqual({ roundId: 'round-1', participantIds: ['c', 'a'], order: 'sequential', keepOrder: true });
    // No drag tokens are ever emitted.
    expect(m.el.innerHTML).not.toContain('qt-storyteller-card-draggable');

    await click(m, reset);
    expect(order(m.el)).toEqual(['A', 'B', 'C']);
    expect([...m.el.querySelectorAll('button')].map(text)).not.toContain('Reset order');
    await click(m, button(m.el, 'Run in order'));
    expect(m.runs.at(-1)).toEqual({ roundId: 'round-1', participantIds: ['a', 'c'], order: 'sequential', keepOrder: false });
  });

  it('the keyboard twin: ArrowLeft / ArrowUp earlier, ArrowRight / ArrowDown later, only on the card itself', async () => {
    const m = await mount(stub(openRound([bid('a'), bid('b'), bid('c')])));
    const card = (name: string) =>
      [...m.el.querySelectorAll('.qt-storyteller-cards > div')].find(
        (c) => c.querySelector('.qt-storyteller-card-name')?.textContent?.trim() === name,
      ) as HTMLElement;
    expect(card('A').getAttribute('tabindex')).toBe('0');
    const key = async (target: HTMLElement, k: string) => {
      target.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true }));
      await settle(m.fixture);
    };
    await key(card('A'), 'ArrowRight');
    expect(order(m.el)).toEqual(['B', 'A', 'C']);
    await key(card('A'), 'ArrowDown');
    expect(order(m.el)).toEqual(['B', 'C', 'A']);
    await key(card('A'), 'ArrowUp');
    await key(card('A'), 'ArrowLeft');
    expect(order(m.el)).toEqual(['A', 'B', 'C']);
    // A key on the checkbox inside stays the checkbox's.
    await key(tick(m.el, 'A'), 'ArrowRight');
    expect(order(m.el)).toEqual(['A', 'B', 'C']);
  });

  it('a late bid lands last and a seat gone from the round is dropped (v4 arrangeBids, through the board)', async () => {
    const r = openRound([bid('a'), bid('b'), bid('c')]);
    const m = await mount(stub(r));
    await click(m, arrow(m.el, 'C', 'earlier')!);
    expect(order(m.el)).toEqual(['A', 'C', 'B']);
    m.queryClient.setQueryData(chatKeys.storytellerRound('chat-1'), { round: withBid(r, bid('d')) });
    await settle(m.fixture);
    expect(order(m.el)).toEqual(['A', 'C', 'B', 'D']);
    m.queryClient.setQueryData(chatKeys.storytellerRound('chat-1'), {
      round: { ...r, bids: [bid('a'), bid('b'), bid('d')] },
    });
    await settle(m.fixture);
    expect(order(m.el)).toEqual(['A', 'B', 'D']);
  });

  it('a card that is not a live bid has no arrows and no tab stop; busy hides every arrow', async () => {
    const m = await mount(stub(openRound([bid('a'), bid('b'), bid('p', { status: 'pending' }), bid('k', { status: 'kept-quiet' })])));
    expect(arrow(m.el, 'P', 'earlier')).toBeNull();
    expect(arrow(m.el, 'K', 'later')).toBeNull();
    expect(arrow(m.el, 'A', 'later')).not.toBeNull();
    m.fixture.componentRef.setInput('busy', true);
    await settle(m.fixture);
    expect(m.el.querySelector('button[aria-label^="Move "]')).toBeNull();
    expect(m.el.querySelector('.qt-storyteller-cards > div[tabindex]')).toBeNull();
  });
});

