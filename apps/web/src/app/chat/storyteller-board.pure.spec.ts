/**
 * The storyteller board's pure helpers (P4.D344).
 *
 * The `arrangeBids` block is v4's ONE client test for Storyteller mode
 * (`__tests__/unit/components/chat/storyteller-board-arrange.test.ts` at the
 * pin `236c04e35`), transcribed NAME FOR NAME. Every other block is v5-authored
 * from the `StorytellerBoard.tsx` hunks with v4's strings byte for byte (v4
 * shipped no test for them).
 */
import { describe, expect, it } from 'vitest';

import type { StorytellerBid, StorytellerRound } from '../core/core-contract';
import {
  arrangeBids,
  cardClass,
  chosenInBoardOrder,
  moveId,
  ordinal,
  selectAllIds,
  statusLine,
  summaryText,
  TIER_BADGE,
  toggleSelection,
  turnModeOf,
  withBid,
} from './storyteller-board.pure';

function bid(participantId: string, over: Partial<StorytellerBid> = {}): StorytellerBid {
  return {
    participantId,
    characterId: `char-${participantId}`,
    characterName: participantId,
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

const ids = (bids: StorytellerBid[]) => bids.map((b) => b.participantId);

function round(over: Partial<StorytellerRound> = {}): StorytellerRound {
  return {
    id: 'r-1',
    chatId: 'chat-1',
    triggerMessageId: 'm-1',
    status: 'closed',
    bids: [],
    selection: null,
    closedReason: 'ran',
    createdAt: '2026-11-02T00:00:00.000Z',
    updatedAt: '2026-11-02T00:00:00.000Z',
    closedAt: null,
    ...over,
  };
}

// --- v4's test, transcribed name for name ---
describe('arrangeBids', () => {
  const bids = [bid('a'), bid('b'), bid('c')];

  it('keeps the round order when nothing has been arranged', () => {
    expect(ids(arrangeBids(bids, null))).toEqual(['a', 'b', 'c']);
  });

  it('follows the operator arrangement', () => {
    expect(ids(arrangeBids(bids, ['c', 'a', 'b']))).toEqual(['c', 'a', 'b']);
  });

  it('appends a late bid the arrangement does not name', () => {
    expect(ids(arrangeBids([...bids, bid('d')], ['b', 'a', 'c']))).toEqual(['b', 'a', 'c', 'd']);
  });

  it('drops an arranged seat that is no longer in the round', () => {
    expect(ids(arrangeBids(bids, ['x', 'c', 'b', 'a']))).toEqual(['c', 'b', 'a']);
  });
});

// --- v5-authored, from the hunks ---
describe('turnModeOf (R-4 — one read, absent is rotation)', () => {
  it('reads the wire value, and an absent key / no chat as rotation', () => {
    expect(turnModeOf({ turnMode: 'storyteller' })).toBe('storyteller');
    expect(turnModeOf({ turnMode: 'rotation' })).toBe('rotation');
    expect(turnModeOf({})).toBe('rotation');
    expect(turnModeOf(null)).toBe('rotation');
    expect(turnModeOf(undefined)).toBe('rotation');
  });
});

describe('summaryText — the five folded lines (v4 :275-289)', () => {
  it('names who is speaking while the round runs', () => {
    expect(
      summaryText(round({ status: 'running', selection: { participantIds: ['a', 'b'], order: 'sequential', chosenAt: 'x', names: ['Ada', 'Bo'] } })),
    ).toBe('Speaking now: Ada, Bo');
  });
  it('a round let lie', () => {
    expect(summaryText(round({ closedReason: 'let-lie' }))).toBe('The last round was let lie.');
  });
  it('a superseded or abandoned round closed unrun', () => {
    expect(summaryText(round({ closedReason: 'superseded' }))).toBe('The last round closed unrun.');
    expect(summaryText(round({ closedReason: 'abandoned' }))).toBe('The last round closed unrun.');
  });
  it('a run round names who went, with the all-at-once suffix', () => {
    const sel = { participantIds: ['a', 'b'], chosenAt: 'x', names: ['Ada', 'Bo'] };
    expect(summaryText(round({ selection: { ...sel, order: 'sequential' } }))).toBe('Last round: Ada, Bo');
    expect(summaryText(round({ selection: { ...sel, order: 'simultaneous' } }))).toBe(
      'Last round: Ada, Bo (all at once)',
    );
  });
  it('a run round with no names left', () => {
    expect(summaryText(round({ selection: null }))).toBe('The last round has run.');
  });
});

describe('statusLine (v4 :89-102)', () => {
  it('pending, kept quiet, and a live bid', () => {
    expect(statusLine(bid('a', { status: 'pending' }))).toBe('Considering…');
    expect(statusLine(bid('a', { status: 'kept-quiet' }))).toBe('Kept quiet this round');
    expect(statusLine(bid('a'))).toBeNull();
  });
  it('a pass prefers its plan, then its reason, then nothing', () => {
    expect(statusLine(bid('a', { status: 'pass', plan: 'I would rather listen.', reason: 'tired' }))).toBe(
      'Passes: I would rather listen.',
    );
    expect(statusLine(bid('a', { status: 'pass', reason: 'tired' }))).toBe('Passes — tired');
    expect(statusLine(bid('a', { status: 'pass' }))).toBe('Passes');
  });
  it('an error carries its reason or the fallback', () => {
    expect(statusLine(bid('a', { status: 'error', reason: 'timed out' }))).toBe('No bid — timed out');
    expect(statusLine(bid('a', { status: 'error' }))).toBe('No bid — the call failed');
  });
});

describe('ordinal (v4 :152-155)', () => {
  it('names one to ten and numbers the rest', () => {
    expect([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map(ordinal)).toEqual([
      'first', 'second', 'third', 'fourth', 'fifth', 'sixth', 'seventh', 'eighth', 'ninth', 'tenth', '#11',
    ]);
  });
});

describe('cardClass (v4 :78-87)', () => {
  it('the token order is v4\'s', () => {
    expect(cardClass(bid('a'), false, false, false)).toBe('qt-storyteller-card');
    expect(cardClass(bid('a'), true, false, false)).toBe('qt-storyteller-card qt-storyteller-card-selected');
    expect(cardClass(bid('a'), true, true, true)).toBe(
      'qt-storyteller-card qt-storyteller-card-selected qt-storyteller-card-draggable qt-storyteller-card-dragging',
    );
    expect(cardClass(bid('a', { tier: 'act' }), false, false, false)).toBe('qt-storyteller-card qt-storyteller-card-act');
    expect(cardClass(bid('a', { tier: 'destroy' }), true, false, false)).toBe(
      'qt-storyteller-card qt-storyteller-card-selected qt-storyteller-card-destroy',
    );
    expect(cardClass(bid('a', { status: 'pass', tier: 'destroy' }), false, false, false)).toBe(
      'qt-storyteller-card qt-storyteller-card-quiet',
    );
    expect(cardClass(bid('a', { status: 'pending' }), false, false, false)).toBe('qt-storyteller-card');
  });
});

describe('TIER_BADGE (v4 :71-76)', () => {
  it('maps each tier to its badge', () => {
    expect(TIER_BADGE).toEqual({
      simple: 'qt-badge-secondary',
      lookup: 'qt-badge-info',
      act: 'qt-badge-warning',
      destroy: 'qt-badge-destructive',
    });
  });
});

describe('the selection rules (v4 :241-259)', () => {
  const a = bid('a');
  const b = bid('b', { tier: 'lookup' });
  const d = bid('d', { tier: 'destroy' });
  const biddable = [a, b, d];

  it('ticking a Destroy bid leaves only it', () => {
    expect([...toggleSelection(new Set(['a', 'b']), d, biddable)]).toEqual(['d']);
  });
  it('ticking another with a Destroy ticked drops the Destroy', () => {
    expect([...toggleSelection(new Set(['d']), a, biddable)]).toEqual(['a']);
  });
  it('ticking a ticked card unticks it', () => {
    expect([...toggleSelection(new Set(['a', 'b']), a, biddable)]).toEqual(['b']);
  });
  it('Select all never includes a Destroy bid (D7)', () => {
    expect([...selectAllIds(biddable)]).toEqual(['a', 'b']);
  });
  it('the chosen bids come back in BOARD order, live bids only', () => {
    const cards = [b, bid('p', { status: 'pass' }), a];
    expect(ids(chosenInBoardOrder(cards, new Set(['a', 'b', 'p'])))).toEqual(['b', 'a']);
  });
});

describe('withBid (v4 :158-163)', () => {
  it('replaces the seat\'s card, or appends a new one', () => {
    const r = round({ status: 'open', bids: [bid('a', { status: 'pending' }), bid('b', { status: 'pending' })] });
    const patched = withBid(r, bid('b', { plan: 'Tea.' }));
    expect(patched.bids.map((x) => [x.participantId, x.status, x.plan])).toEqual([
      ['a', 'pending', null],
      ['b', 'bid', 'Tea.'],
    ]);
    expect(r.bids[1].status).toBe('pending');
    expect(ids(withBid(r, bid('c')).bids)).toEqual(['a', 'b', 'c']);
  });
});

describe('moveId — v4\'s arrayMove over the card ids (Tier 2)', () => {
  it('moves one id and leaves the rest in order', () => {
    expect(moveId(['a', 'b', 'c'], 2, 0)).toEqual(['c', 'a', 'b']);
    expect(moveId(['a', 'b', 'c'], 0, 1)).toEqual(['b', 'a', 'c']);
    expect(moveId(['a', 'b', 'c'], 0, 3)).toEqual(['a', 'b', 'c']);
    expect(moveId(['a', 'b', 'c'], -1, 0)).toEqual(['a', 'b', 'c']);
  });
});
