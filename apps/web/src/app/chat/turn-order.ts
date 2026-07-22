/**
 * The client-side turn-order display core — v4 `lib/chat/turn-manager`'s
 * browser-reachable slice, ported for the Salon chat sidebar (P4.9H1):
 *
 *   - `turn-order.ts` — {@link computePredictedTurnOrder} (the whole module)
 *   - `state.ts` — {@link createInitialTurnState}
 *   - `queue.ts` — {@link addToQueue} / {@link removeFromQueue} /
 *     {@link nudgeParticipant} (the three the sidebar's optimistic queue
 *     updates call; `popFromQueue`/`resetCycleForUserSkip` are server-side)
 *   - `utils.ts` — {@link getQueuePosition}
 *   - `types.ts` — {@link TurnState} / {@link TurnSelectionResult}
 *
 * This is DISPLAY-ONLY logic: it never picks a speaker, it only predicts the
 * order the sidebar draws badges for. The authoritative selection is the
 * server's (`chatTurnAction`, the ported `turn_orchestrator`); v4 keeps this
 * client copy purely so the sidebar can number the cast without a round-trip.
 *
 * v4's client `turnState` starts EMPTY (`createInitialTurnState()` in
 * `SalonView.tsx:144`) and only ever takes `queue` back from the server
 * (`useTurnManagement.applyServerResponse`) — `spokenSinceUserTurn` and
 * `lastSpeakerId` stay at their initial values in the browser. The port keeps
 * that shape rather than inventing a richer client state.
 *
 * @module chat/turn-order
 */

import { isParticipantPresent } from './skip-signal-helpers';

/**
 * Session-only turn state (v4 `TurnState`). Recalculated from history
 * server-side; in the browser only `queue` is ever refreshed.
 */
export interface TurnState {
  /** Participants who have spoken since the user last spoke. */
  spokenSinceUserTurn: string[];
  /** The participant whose turn it is (null = user's turn). */
  currentTurnParticipantId: string | null;
  /** Manually queued participants (in order, first = next). */
  queue: string[];
  /** Last speaker (cannot speak again unless nudged/queued, except if only character). */
  lastSpeakerId: string | null;
}

/** Result of the turn-selection algorithm (v4 `TurnSelectionResult`). */
export interface TurnSelectionResult {
  /** The selected participant ID, or null if it's the user's turn. */
  nextSpeakerId: string | null;
  /** Reason for the selection (for debugging). */
  reason: 'queue' | 'weighted_selection' | 'only_character' | 'user_turn' | 'cycle_complete';
  /** Whether the cycle is complete (all characters have spoken). */
  cycleComplete: boolean;
  /** Debug info about the selection process. */
  debug?: {
    eligibleSpeakers: string[];
    weights?: Record<string, number>;
    randomValue?: number;
    allLLMNewCycle?: boolean;
  };
}

/** Status values for turn-order entries (v4 `TurnOrderStatus`). */
export type TurnOrderStatus =
  | 'generating' // Currently generating a response (#1 during generation)
  | 'next' // Next speaker from turn selection result
  | 'queued' // In the manual queue
  | 'eligible' // Eligible to speak (hasn't spoken this cycle, not last speaker)
  | 'user-turn' // User's slot in the cycle
  | 'spoken' // Already spoke this cycle
  | 'silent' // Participant is silent (present but not speaking)
  | 'absent' // Participant is absent
  | 'inactive'; // Participant is inactive

/** A single entry in the predicted turn order (v4 `TurnOrderEntry`). */
export interface TurnOrderEntry {
  /** The participant ID. */
  participantId: string;
  /** Display position (1-based), or null for inactive participants. */
  position: number | null;
  /** The status category for styling. */
  status: TurnOrderStatus;
}

/**
 * The participant shape {@link computePredictedTurnOrder} reads (v4 types it as
 * `ParticipantData`, the ParticipantCard prop). Deliberately structural so the
 * `ChatDetail.participants` DTO satisfies it without a cast.
 */
export interface TurnOrderParticipant {
  id: string;
  controlledBy?: 'llm' | 'user' | string;
  status?: string | null;
  character?: { talkativeness?: number | null } | null;
}

/** Options for computing the predicted turn order (v4 `ComputeTurnOrderOptions`). */
export interface ComputeTurnOrderOptions {
  /** All participants in the chat (including inactive). */
  participants: TurnOrderParticipant[];
  /** Current turn state. */
  turnState: TurnState;
  /** Current turn selection result (may be null before first selection). */
  turnSelectionResult: TurnSelectionResult | null;
  /** Whether a response is currently being generated. */
  isGenerating: boolean;
  /** The participant currently generating a response. */
  respondingParticipantId?: string | null;
  /** The user's participant ID (user-controlled character). */
  userParticipantId: string | null;
}

/** A fresh turn state (v4 `createInitialTurnState`). Side-effect free by contract. */
export function createInitialTurnState(): TurnState {
  return {
    spokenSinceUserTurn: [],
    currentTurnParticipantId: null,
    queue: [],
    lastSpeakerId: null,
  };
}

/** The queue position for a participant (1-indexed), or 0 if not queued (v4 `getQueuePosition`). */
export function getQueuePosition(state: TurnState, participantId: string): number {
  const index = state.queue.indexOf(participantId);
  return index === -1 ? 0 : index + 1;
}

/** Append to the turn queue, ignoring duplicates (v4 `addToQueue`). */
export function addToQueue(currentState: TurnState, participantId: string): TurnState {
  if (currentState.queue.includes(participantId)) {
    return currentState;
  }
  return { ...currentState, queue: [...currentState.queue, participantId] };
}

/** Drop a participant from the turn queue (v4 `removeFromQueue`). */
export function removeFromQueue(currentState: TurnState, participantId: string): TurnState {
  return {
    ...currentState,
    queue: currentState.queue.filter((id) => id !== participantId),
  };
}

/** Move (or add) a participant to the FRONT of the queue (v4 `nudgeParticipant`). */
export function nudgeParticipant(currentState: TurnState, participantId: string): TurnState {
  const filteredQueue = currentState.queue.filter((id) => id !== participantId);
  return { ...currentState, queue: [participantId, ...filteredQueue] };
}

/**
 * Computes the predicted turn order for display (v4 `computePredictedTurnOrder`).
 *
 * Ordering priority:
 *   1. Currently generating participant (#1 if generating)
 *   2. Next speaker from turnSelectionResult (#2 if generating, #1 if not)
 *   3. Queue entries (in order)
 *   4. Eligible participants (not spoken this cycle, not last speaker) by talkativeness desc
 *   5. User character (at their cycle position)
 *   6. Already-spoken participants
 *   7. Inactive participants (position = null)
 */
export function computePredictedTurnOrder(options: ComputeTurnOrderOptions): TurnOrderEntry[] {
  const {
    participants,
    turnState,
    turnSelectionResult,
    isGenerating,
    respondingParticipantId,
    userParticipantId,
  } = options;

  const entries: TurnOrderEntry[] = [];
  const placed = new Set<string>();

  // Track which IDs we've already assigned a position to.
  const addEntry = (participantId: string, status: TurnOrderStatus) => {
    if (placed.has(participantId)) return;
    // Verify participant exists.
    if (!participants.some((p) => p.id === participantId)) return;
    placed.add(participantId);
    entries.push({
      participantId,
      position:
        status === 'inactive' ? null : entries.filter((e) => e.status !== 'inactive').length + 1,
      status,
    });
  };

  // 1. Currently generating participant.
  if (isGenerating && respondingParticipantId) {
    addEntry(respondingParticipantId, 'generating');
  }

  // 2. Next speaker from turn selection result.
  if (turnSelectionResult?.nextSpeakerId) {
    // Only add as 'next' if not already placed as generating.
    if (!placed.has(turnSelectionResult.nextSpeakerId)) {
      addEntry(turnSelectionResult.nextSpeakerId, 'next');
    }
  }

  // 3. Queue entries (in order).
  for (const queuedId of turnState.queue) {
    addEntry(queuedId, 'queued');
  }

  // Separate active vs inactive participants.
  const activeParticipants = participants.filter((p) => isParticipantPresent(p.status || 'active'));
  const inactiveParticipants = participants.filter(
    (p) => !isParticipantPresent(p.status || 'active'),
  );

  // 4. Eligible participants (active, not spoken this cycle, not last speaker, not user),
  //    sorted by talkativeness descending.
  const eligible = activeParticipants
    .filter((p) => {
      if (placed.has(p.id)) return false;
      if (p.id === userParticipantId) return false; // User handled separately.
      if (turnState.spokenSinceUserTurn.includes(p.id)) return false;
      if (p.id === turnState.lastSpeakerId) return false;
      // Must be LLM-controlled (or undefined type CHARACTER).
      if (p.controlledBy === 'user') return false;
      return true;
    })
    .sort((a, b) => {
      const talkA = a.character?.talkativeness ?? 0.5;
      const talkB = b.character?.talkativeness ?? 0.5;
      return talkB - talkA; // Descending.
    });

  for (const p of eligible) {
    addEntry(p.id, 'eligible');
  }

  // 5. User character at their cycle position. (v4 computes `isUserTurn` here and
  //    then passes 'user-turn' on BOTH branches — kept verbatim, quirk included.)
  if (userParticipantId && !placed.has(userParticipantId)) {
    const userP = participants.find((p) => p.id === userParticipantId);
    if (userP && isParticipantPresent(userP.status || 'active')) {
      addEntry(userParticipantId, 'user-turn');
    }
  }

  // 6. Already-spoken participants (active but already spoke this cycle).
  const spoken = activeParticipants
    .filter((p) => !placed.has(p.id))
    .sort((a, b) => {
      const talkA = a.character?.talkativeness ?? 0.5;
      const talkB = b.character?.talkativeness ?? 0.5;
      return talkB - talkA;
    });

  for (const p of spoken) {
    addEntry(p.id, 'spoken');
  }

  // 7. Inactive participants (no position).
  for (const p of inactiveParticipants) {
    if (!placed.has(p.id)) {
      placed.add(p.id);
      const pStatus = p.status;
      let turnOrderStatus: TurnOrderStatus = 'inactive';
      if (pStatus === 'absent') {
        turnOrderStatus = 'absent';
      } else if (pStatus === 'silent') {
        turnOrderStatus = 'silent';
      }
      entries.push({ participantId: p.id, position: null, status: turnOrderStatus });
    }
  }

  return entries;
}
