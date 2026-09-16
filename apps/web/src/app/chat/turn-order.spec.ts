import { describe, expect, it } from 'vitest';

import {
  addToQueue,
  computePredictedTurnOrder,
  createInitialTurnState,
  findActiveUserParticipant,
  findUserParticipant,
  getQueuePosition,
  isUserDrivenSeat,
  nudgeParticipant,
  removeFromQueue,
  resolveFloorSeatId,
  type SeatView,
  type TurnOrderParticipant,
  type TurnState,
} from './turn-order';

/**
 * Case-for-case from v4
 * `__tests__/unit/lib/chat/turn-manager/turn-order.test.ts` (all 15 cases, the
 * helper factories included), plus a small block for the queue helpers and
 * `getQueuePosition` — which v4 exercises only through the orchestrator.
 */

/** v4's factories carry a display `name` the computation never reads — kept for parity. */
type Fixture = TurnOrderParticipant & { name: string };

function createCharacter(
  id: string,
  name: string,
  talkativeness = 0.5,
  isActive = true,
  controlledBy: 'llm' | 'user' = 'llm',
): Fixture {
  return {
    id,
    name,
    controlledBy,
    status: isActive ? 'active' : 'absent',
    character: { talkativeness },
  };
}

function createPersona(id: string, name: string, isActive = true): Fixture {
  return {
    id,
    name,
    controlledBy: 'user',
    status: isActive ? 'active' : 'absent',
    character: {},
  };
}

function createTurnState(overrides: Partial<TurnState> = {}): TurnState {
  return { ...createInitialTurnState(), ...overrides };
}

describe('computePredictedTurnOrder', () => {
  describe('basic ordering', () => {
    it('places generating participant first with generating status', () => {
      const participants = [createCharacter('alice', 'Alice'), createCharacter('bob', 'Bob')];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState(),
        turnSelectionResult: {
          nextSpeakerId: 'bob',
          reason: 'weighted_selection',
          cycleComplete: false,
        },
        isGenerating: true,
        respondingParticipantId: 'alice',
        userParticipantId: null,
      });

      expect(result[0]).toEqual({ participantId: 'alice', position: 1, status: 'generating' });
    });

    it('places next speaker from selection result with next status', () => {
      const participants = [createCharacter('alice', 'Alice'), createCharacter('bob', 'Bob')];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState(),
        turnSelectionResult: {
          nextSpeakerId: 'bob',
          reason: 'weighted_selection',
          cycleComplete: false,
        },
        isGenerating: false,
        respondingParticipantId: null,
        userParticipantId: null,
      });

      expect(result[0]).toEqual({ participantId: 'bob', position: 1, status: 'next' });
    });

    it('places generating first, then next speaker second', () => {
      const participants = [
        createCharacter('alice', 'Alice'),
        createCharacter('bob', 'Bob'),
        createCharacter('carol', 'Carol'),
      ];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState(),
        turnSelectionResult: {
          nextSpeakerId: 'bob',
          reason: 'weighted_selection',
          cycleComplete: false,
        },
        isGenerating: true,
        respondingParticipantId: 'alice',
        userParticipantId: null,
      });

      expect(result[0].participantId).toBe('alice');
      expect(result[0].status).toBe('generating');
      expect(result[0].position).toBe(1);

      expect(result[1].participantId).toBe('bob');
      expect(result[1].status).toBe('next');
      expect(result[1].position).toBe(2);
    });

    it('does not duplicate generating participant if they are also next speaker', () => {
      const participants = [createCharacter('alice', 'Alice'), createCharacter('bob', 'Bob')];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState(),
        turnSelectionResult: {
          nextSpeakerId: 'alice',
          reason: 'weighted_selection',
          cycleComplete: false,
        },
        isGenerating: true,
        respondingParticipantId: 'alice',
        userParticipantId: null,
      });

      const aliceEntries = result.filter((e) => e.participantId === 'alice');
      expect(aliceEntries).toHaveLength(1);
      expect(aliceEntries[0].status).toBe('generating');
    });
  });

  describe('queue handling', () => {
    it('places queued participants after next speaker with queued status', () => {
      const participants = [
        createCharacter('alice', 'Alice'),
        createCharacter('bob', 'Bob'),
        createCharacter('carol', 'Carol'),
      ];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState({ queue: ['carol', 'bob'] }),
        turnSelectionResult: { nextSpeakerId: 'alice', reason: 'queue', cycleComplete: false },
        isGenerating: false,
        respondingParticipantId: null,
        userParticipantId: null,
      });

      expect(result[0].participantId).toBe('alice');

      const carolEntry = result.find((e) => e.participantId === 'carol');
      const bobEntry = result.find((e) => e.participantId === 'bob');
      expect(carolEntry?.status).toBe('queued');
      expect(bobEntry?.status).toBe('queued');
    });
  });

  describe('eligible participants', () => {
    it('sorts eligible participants by talkativeness descending', () => {
      const participants = [
        createCharacter('alice', 'Alice', 0.3),
        createCharacter('bob', 'Bob', 0.9),
        createCharacter('carol', 'Carol', 0.6),
      ];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState(),
        turnSelectionResult: null,
        isGenerating: false,
        respondingParticipantId: null,
        userParticipantId: null,
      });

      const eligible = result.filter((e) => e.status === 'eligible');
      expect(eligible[0].participantId).toBe('bob'); // 0.9
      expect(eligible[1].participantId).toBe('carol'); // 0.6
      expect(eligible[2].participantId).toBe('alice'); // 0.3
    });

    it('excludes participants who spoke this cycle from eligible', () => {
      const participants = [
        createCharacter('alice', 'Alice', 0.5),
        createCharacter('bob', 'Bob', 0.5),
        createCharacter('carol', 'Carol', 0.5),
      ];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState({ spokenSinceUserTurn: ['alice'], lastSpeakerId: 'bob' }),
        turnSelectionResult: null,
        isGenerating: false,
        respondingParticipantId: null,
        userParticipantId: null,
      });

      const eligible = result.filter((e) => e.status === 'eligible');
      expect(eligible).toHaveLength(1);
      expect(eligible[0].participantId).toBe('carol');

      const spoken = result.filter((e) => e.status === 'spoken');
      expect(spoken).toHaveLength(2);
    });

    it('excludes user-controlled characters from eligible', () => {
      const participants = [
        createCharacter('alice', 'Alice', 0.5, true, 'llm'),
        createCharacter('bob', 'Bob', 0.5, true, 'user'),
      ];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState(),
        turnSelectionResult: null,
        isGenerating: false,
        respondingParticipantId: null,
        userParticipantId: null,
      });

      const eligible = result.filter((e) => e.status === 'eligible');
      expect(eligible).toHaveLength(1);
      expect(eligible[0].participantId).toBe('alice');
    });
  });

  describe('user persona handling', () => {
    it('places user persona with user-turn status', () => {
      const participants = [createPersona('user', 'User'), createCharacter('alice', 'Alice')];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState(),
        turnSelectionResult: { nextSpeakerId: null, reason: 'user_turn', cycleComplete: false },
        isGenerating: false,
        respondingParticipantId: null,
        userParticipantId: 'user',
      });

      const userEntry = result.find((e) => e.participantId === 'user');
      expect(userEntry?.status).toBe('user-turn');
      expect(userEntry?.position).not.toBeNull();
    });
  });

  describe('inactive participants', () => {
    it('places inactive participants at end with null position', () => {
      const participants = [
        createCharacter('alice', 'Alice', 0.5, true),
        createCharacter('bob', 'Bob', 0.5, false),
        createCharacter('carol', 'Carol', 0.5, true),
      ];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState(),
        turnSelectionResult: null,
        isGenerating: false,
        respondingParticipantId: null,
        userParticipantId: null,
      });

      const bobEntry = result.find((e) => e.participantId === 'bob');
      expect(bobEntry?.status).toBe('absent');
      expect(bobEntry?.position).toBeNull();

      expect(result[result.length - 1].participantId).toBe('bob');
    });

    it('includes all inactive participants', () => {
      const participants = [
        createCharacter('alice', 'Alice', 0.5, false),
        createCharacter('bob', 'Bob', 0.5, false),
      ];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState(),
        turnSelectionResult: null,
        isGenerating: false,
        respondingParticipantId: null,
        userParticipantId: null,
      });

      expect(result).toHaveLength(2);
      expect(result.every((e) => e.status === 'absent')).toBe(true);
      expect(result.every((e) => e.position === null)).toBe(true);
    });
  });

  describe('complete ordering', () => {
    it('produces correct order: generating, next, queued, eligible, user, spoken, inactive', () => {
      const participants = [
        createPersona('user', 'User'),
        createCharacter('gen', 'Generating', 0.5, true),
        createCharacter('next', 'Next', 0.5, true),
        createCharacter('queued', 'Queued', 0.5, true),
        createCharacter('eligible', 'Eligible', 0.8, true),
        createCharacter('spoken', 'Spoken', 0.5, true),
        createCharacter('inactive', 'Inactive', 0.5, false),
      ];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState({
          queue: ['queued'],
          spokenSinceUserTurn: ['spoken'],
          lastSpeakerId: 'gen',
        }),
        turnSelectionResult: {
          nextSpeakerId: 'next',
          reason: 'weighted_selection',
          cycleComplete: false,
        },
        isGenerating: true,
        respondingParticipantId: 'gen',
        userParticipantId: 'user',
      });

      const statuses = result.map((e) => e.status);
      expect(statuses).toEqual([
        'generating', // gen
        'next', // next
        'queued', // queued
        'eligible', // eligible
        'user-turn', // user
        'spoken', // spoken
        'absent', // inactive (status: absent)
      ]);

      expect(result.find((e) => e.status === 'generating')?.position).toBe(1);
      expect(result.find((e) => e.status === 'next')?.position).toBe(2);
      expect(result.find((e) => e.status === 'queued')?.position).toBe(3);
      expect(result.find((e) => e.status === 'eligible')?.position).toBe(4);
      expect(result.find((e) => e.status === 'user-turn')?.position).toBe(5);
      expect(result.find((e) => e.status === 'spoken')?.position).toBe(6);
      expect(result.find((e) => e.status === 'absent')?.position).toBeNull();
    });

    it('handles empty participants list', () => {
      const result = computePredictedTurnOrder({
        participants: [],
        turnState: createTurnState(),
        turnSelectionResult: null,
        isGenerating: false,
        respondingParticipantId: null,
        userParticipantId: null,
      });

      expect(result).toHaveLength(0);
    });

    it('handles no selection result gracefully', () => {
      const participants = [createCharacter('alice', 'Alice'), createCharacter('bob', 'Bob')];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState(),
        turnSelectionResult: null,
        isGenerating: false,
        respondingParticipantId: null,
        userParticipantId: null,
      });

      expect(result).toHaveLength(2);
      expect(result.every((e) => e.status === 'eligible')).toBe(true);
    });

    it('ignores respondingParticipantId that does not exist in participants', () => {
      const participants = [createCharacter('alice', 'Alice')];

      const result = computePredictedTurnOrder({
        participants,
        turnState: createTurnState(),
        turnSelectionResult: null,
        isGenerating: true,
        respondingParticipantId: 'nonexistent',
        userParticipantId: null,
      });

      expect(result).toHaveLength(1);
      expect(result[0].participantId).toBe('alice');
    });
  });
});

describe('computePredictedTurnOrder — the drawn rotation (P4.D177, v4 2aca73ad6)', () => {
  it('orders the rest of the cycle by the rotation, not by talkativeness', () => {
    // Talkativeness would sort these loudest-first: alice, bob, carol. The
    // rotation says otherwise, and the rotation is what will actually happen.
    const participants = [
      createCharacter('alice', 'Alice', 0.9),
      createCharacter('bob', 'Bob', 0.6),
      createCharacter('carol', 'Carol', 0.2),
    ];

    const result = computePredictedTurnOrder({
      participants,
      turnState: createTurnState({ cycleOrder: ['carol', 'alice', 'bob'] }),
      turnSelectionResult: null,
      isGenerating: false,
      respondingParticipantId: null,
      userParticipantId: null,
    });

    expect(result.map((e) => e.participantId)).toEqual(['carol', 'alice', 'bob']);
    expect(result.map((e) => e.position)).toEqual([1, 2, 3]);
  });

  it('keeps the seat now generating at the head, with the rotation behind it', () => {
    const participants = [
      createCharacter('alice', 'Alice', 0.9),
      createCharacter('bob', 'Bob', 0.6),
      createCharacter('carol', 'Carol', 0.2),
    ];

    const result = computePredictedTurnOrder({
      participants,
      turnState: createTurnState({ cycleOrder: ['carol', 'bob'], lastSpeakerId: 'alice' }),
      turnSelectionResult: null,
      isGenerating: true,
      respondingParticipantId: 'alice',
      userParticipantId: null,
    });

    expect(result.map((e) => e.participantId)).toEqual(['alice', 'carol', 'bob']);
    expect(result[0].status).toBe('generating');
    expect(result[1].status).toBe('eligible');
  });

  it('puts a seat the rotation never dealt in behind those it did', () => {
    const participants = [
      createCharacter('alice', 'Alice', 0.9), // loudest, but not in the rotation
      createCharacter('bob', 'Bob', 0.6),
      createCharacter('carol', 'Carol', 0.2),
    ];

    const result = computePredictedTurnOrder({
      participants,
      turnState: createTurnState({ cycleOrder: ['carol', 'bob'] }),
      turnSelectionResult: null,
      isGenerating: false,
      respondingParticipantId: null,
      userParticipantId: null,
    });

    expect(result.map((e) => e.participantId)).toEqual(['carol', 'bob', 'alice']);
  });

  it('falls back to the talkativeness sort with no rotation on file', () => {
    const participants = [
      createCharacter('carol', 'Carol', 0.2),
      createCharacter('alice', 'Alice', 0.9),
      createCharacter('bob', 'Bob', 0.6),
    ];

    const result = computePredictedTurnOrder({
      participants,
      turnState: createTurnState(),
      turnSelectionResult: null,
      isGenerating: false,
      respondingParticipantId: null,
      userParticipantId: null,
    });

    expect(result.map((e) => e.participantId)).toEqual(['alice', 'bob', 'carol']);
  });
});

describe('the queue helpers', () => {
  it('starts from an empty state (v4 createInitialTurnState)', () => {
    expect(createInitialTurnState()).toEqual({
      spokenSinceUserTurn: [],
      currentTurnParticipantId: null,
      queue: [],
      lastSpeakerId: null,
      cycleOrder: [],
    });
  });

  it('appends without duplicating (v4 addToQueue)', () => {
    const once = addToQueue(createInitialTurnState(), 'alice');
    expect(once.queue).toEqual(['alice']);
    // A duplicate returns the SAME object, not a copy.
    expect(addToQueue(once, 'alice')).toBe(once);
    expect(addToQueue(once, 'bob').queue).toEqual(['alice', 'bob']);
  });

  it('removes by id (v4 removeFromQueue)', () => {
    const state = createTurnState({ queue: ['alice', 'bob'] });
    expect(removeFromQueue(state, 'alice').queue).toEqual(['bob']);
    expect(removeFromQueue(state, 'nobody').queue).toEqual(['alice', 'bob']);
  });

  it('moves a nudged participant to the FRONT (v4 nudgeParticipant)', () => {
    const state = createTurnState({ queue: ['alice', 'bob'] });
    expect(nudgeParticipant(state, 'bob').queue).toEqual(['bob', 'alice']);
    expect(nudgeParticipant(state, 'carol').queue).toEqual(['carol', 'alice', 'bob']);
  });

  it('reports 1-indexed queue positions, 0 when absent (v4 getQueuePosition)', () => {
    const state = createTurnState({ queue: ['alice', 'bob'] });
    expect(getQueuePosition(state, 'alice')).toBe(1);
    expect(getQueuePosition(state, 'bob')).toBe(2);
    expect(getQueuePosition(state, 'carol')).toBe(0);
  });
});

/**
 * The impersonation-overlay participant filters — client mirror of v4
 * `lib/chat/turn-manager/utils.ts` and the differential-proven core
 * `participant_filters` (`is_user_driven_seat` / `find_user_participant` /
 * `find_active_user_participant`). Cases mirror v4's
 * `__tests__/unit/lib/chat/turn-manager.test.ts` `findActiveUserParticipant`
 * block (v4 Bug 44's overlay arm included) plus the reverted-then-restored
 * `isUserDrivenSeat` arms.
 */
describe('the impersonation-overlay participant filters (v4 turn-manager/utils.ts)', () => {
  const seat = (over: Partial<SeatView> & { id: string }): SeatView => ({
    controlledBy: 'llm',
    status: 'active',
    ...over,
  });

  describe('isUserDrivenSeat', () => {
    it('is true for a genuine user-controlled seat', () => {
      expect(isUserDrivenSeat({ id: 'p1', controlledBy: 'user' }, null)).toBe(true);
      expect(isUserDrivenSeat({ id: 'p1', controlledBy: 'user' }, ['p2'])).toBe(true);
    });

    it('is false for a plain LLM seat with no overlay', () => {
      expect(isUserDrivenSeat({ id: 'p1', controlledBy: 'llm' }, null)).toBe(false);
      expect(isUserDrivenSeat({ id: 'p1', controlledBy: 'llm' }, [])).toBe(false);
      expect(isUserDrivenSeat({ id: 'p1', controlledBy: 'llm' }, ['p2'])).toBe(false);
    });

    it('is true for an LLM seat the human is impersonating (the overlay arm)', () => {
      expect(isUserDrivenSeat({ id: 'p1', controlledBy: 'llm' }, ['p1'])).toBe(true);
      expect(isUserDrivenSeat({ id: 'p1', controlledBy: 'llm' }, ['p2', 'p1'])).toBe(true);
    });

    it('tolerates a nullish controlledBy and a nullish overlay list', () => {
      expect(isUserDrivenSeat({ id: 'p1', controlledBy: null }, null)).toBe(false);
      expect(isUserDrivenSeat({ id: 'p1', controlledBy: undefined }, undefined)).toBe(false);
      expect(isUserDrivenSeat({ id: 'p1', controlledBy: undefined }, ['p1'])).toBe(true);
    });
  });

  describe('findUserParticipant (ownership reader)', () => {
    it('finds the first present, user-controlled participant', () => {
      const char1 = seat({ id: 'p1' });
      const userChar = seat({ id: 'u1', controlledBy: 'user' });
      expect(findUserParticipant([char1, userChar])).toBe(userChar);
    });

    it('ignores absent user seats and returns null when none present', () => {
      const absentUser = seat({ id: 'u1', controlledBy: 'user', status: 'absent' });
      expect(findUserParticipant([seat({ id: 'p1' }), absentUser])).toBeNull();
    });
  });

  describe('findActiveUserParticipant (Speaking As)', () => {
    const jackie = seat({ id: 'u-jackie', controlledBy: 'user' });
    const abigail = seat({ id: 'p-abigail', controlledBy: 'llm' });
    const revenant = seat({ id: 'u-revenant', controlledBy: 'user' });
    const participants = [jackie, abigail, revenant];

    it('honors the active speaker when two characters are user-controlled', () => {
      expect(findActiveUserParticipant(participants, 'u-revenant')).toBe(revenant);
    });

    it('falls back to the first user-controlled participant when no selection', () => {
      expect(findActiveUserParticipant(participants, null)).toBe(jackie);
      expect(findActiveUserParticipant(participants)).toBe(jackie);
    });

    it('falls back when the selected id is not a user-controlled participant', () => {
      expect(findActiveUserParticipant(participants, 'p-abigail')).toBe(jackie);
      expect(findActiveUserParticipant(participants, 'does-not-exist')).toBe(jackie);
    });

    it('ignores a selected speaker who is no longer present', () => {
      const absentRevenant = seat({ id: 'u-revenant', controlledBy: 'user', status: 'absent' });
      expect(findActiveUserParticipant([jackie, abigail, absentRevenant], 'u-revenant')).toBe(
        jackie,
      );
    });

    it('returns null when there are no user-controlled participants', () => {
      expect(findActiveUserParticipant([abigail], 'anything')).toBeNull();
    });

    // v4 Bug 44: "Speak as an AI character" routes through impersonation, an
    // OVERLAY — the chosen seat's `controlledBy` stays 'llm'; only the chat's
    // `impersonatingParticipantIds` records it.
    it('honours an impersonated LLM seat via the overlay, without moving controlledBy', () => {
      const abigailSeat = seat({ id: 'p-abigail', controlledBy: 'llm' });
      // Without the overlay, selecting the LLM character is not honoured.
      expect(findActiveUserParticipant([jackie, abigailSeat], 'p-abigail')).toBe(jackie);
      // With the seat listed in impersonatingParticipantIds, the SAME still-LLM
      // seat IS honoured as the active speaker.
      expect(
        findActiveUserParticipant([jackie, abigailSeat], 'p-abigail', ['p-abigail']),
      ).toBe(abigailSeat);
      // The column was never touched.
      expect(abigailSeat.controlledBy).toBe('llm');
    });
  });
});

/**
 * `resolveFloorSeatId` — which seat the "your turn" banner speaks for, and
 * whose turn its Skip passes (v4 `2075242f9`, bug 146). Case-for-case from v4's
 * own `__tests__/unit/lib/chat/turn-manager/floor-seat.test.ts` (all seven),
 * plus the shapes that suite does not ask — the four the core's
 * `floor_seat_equivalence` corpus adds and the differential proves against v4's
 * real function.
 */
describe('resolveFloorSeatId (v4 floor-seat.test.ts)', () => {
  /** v4's factory: `controlledBy: 'llm'`, `status: 'active'`, `isActive: true`. */
  const seat = (id: string, over: Partial<SeatView> = {}): SeatView => ({
    id,
    controlledBy: 'llm',
    status: 'active',
    ...over,
  });

  /** The shape of the chat that produced bug 146: two seats the human drives. */
  const charlie = seat('charlie', { controlledBy: 'user' });
  const helene = seat('helene', { controlledBy: 'user' });
  const wahno = seat('wahno');
  const room = [wahno, charlie, helene];

  it('prefers the seat the rotation landed on over the composer’s seat', () => {
    // Helene has just posted; the floor is Charlie's. The composer is still
    // pointed at Helene, and before the fix that is what Skip passed.
    expect(resolveFloorSeatId(charlie.id, room, [], helene.id)).toBe(charlie.id);
  });

  it('keeps the composer’s seat when the floor belongs to an LLM', () => {
    // Bug 123's off-turn affordance: nothing of the human's is outstanding, so
    // the banner stays about the seat they are typing as.
    expect(resolveFloorSeatId(wahno.id, room, [], helene.id)).toBe(helene.id);
  });

  it('keeps the composer’s seat when there is no selection yet', () => {
    expect(resolveFloorSeatId(null, room, [], helene.id)).toBe(helene.id);
    expect(resolveFloorSeatId(undefined, room, [], charlie.id)).toBe(charlie.id);
  });

  it('agrees with the composer when both name the same seat', () => {
    expect(resolveFloorSeatId(helene.id, room, [], helene.id)).toBe(helene.id);
  });

  it('honours the impersonation overlay, not the bare controlledBy column', () => {
    // An impersonated seat's durable `controlledBy` stays 'llm' (v4 bug 44), so
    // a reader that consulted the column alone would hand the floor back to the
    // composer and pass the wrong turn.
    const lorian = seat('lorian');
    const withLorian = [...room, lorian];
    expect(resolveFloorSeatId(lorian.id, withLorian, ['lorian'], charlie.id)).toBe(lorian.id);
    expect(resolveFloorSeatId(lorian.id, withLorian, [], charlie.id)).toBe(charlie.id);
  });

  it('sends the floor to the owner seat when the composer holds an impersonated one', () => {
    // v4's eighth case (the second sighting, 2026-09-16 — uncommitted in v4 at
    // the `2075242f9` unification): Leilani is an LLM seat the operator had
    // taken up, so her `controlledBy` is still 'llm' and only the overlay makes
    // her theirs. She posts, the floor goes to the owner seat Charlie, and the
    // composer is still on Leilani — which is what recorded "Leilani declining
    // the floor" for a turn she had just held.
    const leilani = seat('leilani');
    const withLeilani = [...room, leilani];
    expect(resolveFloorSeatId(charlie.id, withLeilani, ['leilani'], leilani.id)).toBe(charlie.id);
  });

  it('falls back when the rotation names a seat that has left the room', () => {
    const departed = seat('departed', { controlledBy: 'user', status: 'removed' });
    expect(resolveFloorSeatId(departed.id, [...room, departed], [], helene.id)).toBe(helene.id);
  });

  it('returns null when neither the floor nor the composer names a seat', () => {
    expect(resolveFloorSeatId(wahno.id, room, [], null)).toBeNull();
    expect(resolveFloorSeatId(null, room, [], null)).toBeNull();
  });

  describe('resolveFloorSeatId — shapes v4’s suite does not ask', () => {
    it('treats an empty-string floor id as falsy, even when a seat carries it', () => {
      // JS truthiness: `if (nextSpeakerId)` is false for '', so the lookup is
      // never attempted — the seat with that id in the room does not win.
      const blank = seat('', { controlledBy: 'user' });
      expect(resolveFloorSeatId('', [...room, blank], [], helene.id)).toBe(helene.id);
      expect(resolveFloorSeatId('', room, [], null)).toBeNull();
    });

    it('returns the composer’s seat VERBATIM — un-validated, for the caller to gate', () => {
      // Not in the room at all…
      expect(resolveFloorSeatId(wahno.id, room, [], 'ghost-seat')).toBe('ghost-seat');
      // …an LLM seat nobody impersonates…
      expect(resolveFloorSeatId(wahno.id, room, [], wahno.id)).toBe(wahno.id);
      // …and a seat that has left the room.
      const departed = seat('departed', { controlledBy: 'user', status: 'removed' });
      expect(resolveFloorSeatId(wahno.id, [...room, departed], [], departed.id)).toBe(departed.id);
    });

    it('scans for the first WHOLE-predicate match, not the first id match', () => {
      // The value returned is the id, so a duplicate-id case cannot say WHICH
      // match won — what it says is that the scan does not stop at the first id
      // match and bail. A lookup-then-filter rewrite answers `helene` here.
      const llmFirst = [seat('dup'), seat('dup', { controlledBy: 'user' }), ...room];
      expect(resolveFloorSeatId('dup', llmFirst, [], helene.id)).toBe('dup');
      const absentFirst = [
        seat('dup', { controlledBy: 'user', status: 'absent' }),
        seat('dup', { controlledBy: 'user' }),
        ...room,
      ];
      expect(resolveFloorSeatId('dup', absentFirst, [], helene.id)).toBe('dup');
    });

    it('counts a SILENT seat as present and an ABSENT one as not', () => {
      const silent = seat('silent-user', { controlledBy: 'user', status: 'silent' });
      expect(resolveFloorSeatId(silent.id, [...room, silent], [], helene.id)).toBe(silent.id);
      const away = seat('absent-user', { controlledBy: 'user', status: 'absent' });
      expect(resolveFloorSeatId(away.id, [...room, away], [], helene.id)).toBe(helene.id);
      // Presence is checked BEFORE the overlay, so the overlay cannot resurrect
      // a seat that has left.
      const goneImp = seat('gone-imp', { status: 'removed' });
      expect(resolveFloorSeatId(goneImp.id, [...room, goneImp], [goneImp.id], helene.id)).toBe(
        helene.id,
      );
    });

    it('reads the three spellings of “no overlay” alike', () => {
      const lorian = seat('lorian');
      const withLorian = [...room, lorian];
      expect(resolveFloorSeatId(lorian.id, withLorian, [], helene.id)).toBe(helene.id);
      expect(resolveFloorSeatId(lorian.id, withLorian, null, helene.id)).toBe(helene.id);
      expect(resolveFloorSeatId(lorian.id, withLorian, undefined, helene.id)).toBe(helene.id);
      // …and an owner seat needs no overlay at all.
      expect(resolveFloorSeatId(charlie.id, room, undefined, helene.id)).toBe(charlie.id);
    });
  });
});
