/**
 * The storyteller board's pure helpers — v4 `components/chat/StorytellerBoard.tsx`
 * at the pin `236c04e35` (`b15fe9404` + `946a50a04`), lifted out of the React
 * component so each rule is a function the spec can drive alone.
 *
 * Every user-facing string here is v4's byte for byte. The drag that v4 layers
 * over the arrangement (`@dnd-kit`) is a NO-PORT under round ruling R-L; the
 * arrangement STATE it feeds — {@link arrangeBids} — is ported whole.
 *
 * @module chat/storyteller-board.pure
 */

import type {
  ChatDetail,
  StorytellerBid,
  StorytellerRound,
} from '../core/core-contract';

/** v4 `TIER_BADGE` (`StorytellerBoard.tsx:71-76`) — the tier badge's colour class. */
export const TIER_BADGE: Record<string, string> = {
  simple: 'qt-badge-secondary',
  lookup: 'qt-badge-info',
  act: 'qt-badge-warning',
  destroy: 'qt-badge-destructive',
};

/**
 * The chat's turn mode, read ONCE (P4.D344 R-4). The wire never sends null (v4
 * `handlers/get.ts:378 ?? 'rotation'`), but an older server omits the key, and
 * an absent key must read as rotation — the board hidden, the radiogroup on
 * Normal. Every site reads it through here, never a scattered default.
 */
export function turnModeOf(
  chat: Pick<ChatDetail, 'turnMode'> | null | undefined,
): 'rotation' | 'storyteller' {
  return chat?.turnMode ?? 'rotation';
}

/** v4 `cardClass` (`:78-87`), kept whole — `draggable` / `dragging` are never true in v5 (R-L). */
export function cardClass(
  bid: StorytellerBid,
  selected: boolean,
  draggable: boolean,
  dragging: boolean,
): string {
  const classes = ['qt-storyteller-card'];
  if (selected) classes.push('qt-storyteller-card-selected');
  if (draggable) classes.push('qt-storyteller-card-draggable');
  if (dragging) classes.push('qt-storyteller-card-dragging');
  if (bid.status === 'bid' && bid.tier === 'act') classes.push('qt-storyteller-card-act');
  if (bid.status === 'bid' && bid.tier === 'destroy') classes.push('qt-storyteller-card-destroy');
  if (bid.status !== 'bid' && bid.status !== 'pending') classes.push('qt-storyteller-card-quiet');
  return classes.join(' ');
}

/** v4 `statusLine` (`:89-102`) — the card's one-line state, or null for a live bid. */
export function statusLine(bid: StorytellerBid): string | null {
  switch (bid.status) {
    case 'pending':
      return 'Considering…';
    case 'pass':
      return bid.plan ? `Passes: ${bid.plan}` : `Passes${bid.reason ? ` — ${bid.reason}` : ''}`;
    case 'error':
      return `No bid — ${bid.reason ?? 'the call failed'}`;
    case 'kept-quiet':
      return 'Kept quiet this round';
    default:
      return null;
  }
}

/**
 * The board's card order: the operator's arrangement where there is one, with
 * any card it does not name (a late bid) following in the round's own order.
 * (v4 `arrangeBids`, `:108-117`.)
 */
export function arrangeBids(
  bids: StorytellerBid[],
  arrangement: string[] | null,
): StorytellerBid[] {
  if (!arrangement) return bids;
  const byId = new Map(bids.map((b) => [b.participantId, b]));
  const placed = arrangement.flatMap((id) => {
    const bid = byId.get(id);
    return bid ? [bid] : [];
  });
  const named = new Set(arrangement);
  return [...placed, ...bids.filter((b) => !named.has(b.participantId))];
}

/** v4 `ordinal` (`:152-155`) — `Speaks first` … `tenth`, then `#n`. */
export function ordinal(n: number): string {
  const words = ['first', 'second', 'third', 'fourth', 'fifth', 'sixth', 'seventh', 'eighth', 'ninth', 'tenth'];
  return words[n - 1] ?? `#${n}`;
}

/** Patch one card into a cached round (the stream's `storytellerBid` frames). v4 `withBid`, `:158-163`. */
export function withBid(round: StorytellerRound, bid: StorytellerBid): StorytellerRound {
  const bids = round.bids.some((b) => b.participantId === bid.participantId)
    ? round.bids.map((b) => (b.participantId === bid.participantId ? bid : b))
    : [...round.bids, bid];
  return { ...round, bids };
}

/**
 * A closed (or running) round folds to one line, until the next round opens —
 * v4's five texts (`:275-289`) as one function.
 */
export function summaryText(round: StorytellerRound): string {
  const names = round.selection?.names ?? [];
  if (round.status === 'running') return `Speaking now: ${names.join(', ')}`;
  if (round.closedReason === 'let-lie') return 'The last round was let lie.';
  if (round.closedReason === 'superseded' || round.closedReason === 'abandoned') {
    return 'The last round closed unrun.';
  }
  return names.length > 0
    ? `Last round: ${names.join(', ')}${round.selection?.order === 'simultaneous' ? ' (all at once)' : ''}`
    : 'The last round has run.';
}

/** The chosen bids IN BOARD ORDER — the run order once the operator has arranged them (v4 `:225-226`). */
export function chosenInBoardOrder(
  cards: StorytellerBid[],
  selected: ReadonlySet<string>,
): StorytellerBid[] {
  return cards.filter((b) => b.status === 'bid' && selected.has(b.participantId));
}

/**
 * v4 `toggle` (`:241-254`): an unticked card is ticked; a Destroy bid goes ahead
 * ALONE (the selection becomes only it); ticking anything else drops every
 * ticked Destroy bid.
 */
export function toggleSelection(
  selected: ReadonlySet<string>,
  bid: StorytellerBid,
  biddable: StorytellerBid[],
): Set<string> {
  const next = new Set(selected);
  if (next.has(bid.participantId)) {
    next.delete(bid.participantId);
  } else if (bid.tier === 'destroy') {
    // A Destroy bid goes ahead alone.
    return new Set([bid.participantId]);
  } else {
    for (const other of biddable) if (other.tier === 'destroy') next.delete(other.participantId);
    next.add(bid.participantId);
  }
  return next;
}

/** v4 `selectAll` (`:257-259`) — every live bid but a Destroy one (D7). */
export function selectAllIds(biddable: StorytellerBid[]): Set<string> {
  return new Set(biddable.filter((b) => b.tier !== 'destroy').map((b) => b.participantId));
}

/**
 * v4's `arrayMove` (`@dnd-kit/sortable`) over the card ids — the one
 * arrangement step the board's keyboard / button moves take (Tier 2, R-1).
 * Out-of-range indices leave the list as it was.
 */
export function moveId(ids: string[], from: number, to: number): string[] {
  if (from < 0 || from >= ids.length || to < 0 || to >= ids.length) return ids;
  const next = ids.slice();
  next.splice(to, 0, next.splice(from, 1)[0]);
  return next;
}
