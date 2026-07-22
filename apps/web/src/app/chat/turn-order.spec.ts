import { describe, expect, it } from 'vitest';

import {
  addToQueue,
  computePredictedTurnOrder,
  createInitialTurnState,
  getQueuePosition,
  nudgeParticipant,
  removeFromQueue,
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

describe('the queue helpers', () => {
  it('starts from an empty state (v4 createInitialTurnState)', () => {
    expect(createInitialTurnState()).toEqual({
      spokenSinceUserTurn: [],
      currentTurnParticipantId: null,
      queue: [],
      lastSpeakerId: null,
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
