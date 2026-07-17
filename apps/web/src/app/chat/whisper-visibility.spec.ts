import { describe, expect, it } from 'vitest';

import type { MessageDto, SystemSender } from '../core/core-contract';
import { isMessageVisibleToOperator } from './whisper-visibility';

/**
 * Case-for-case from v4 `__tests__/unit/app/salon/whisper-visibility.test.ts`
 * (the `8e4b00d4` re-port).
 */

const USER_PARTICIPANT = 'user-participant-id';
const CHARACTER_A = 'character-a-id';
const CHARACTER_B = 'character-b-id';

const audience = (showAllWhispers: boolean) => ({
  showAllWhispers,
  userParticipantIds: new Set([USER_PARTICIPANT]),
});

type FilterInput = Pick<MessageDto, 'systemSender' | 'participantId' | 'targetParticipantIds'>;

const message = (overrides: Partial<FilterInput> = {}): FilterInput => ({
  systemSender: null,
  participantId: CHARACTER_A,
  targetParticipantIds: null,
  ...overrides,
});

describe('isMessageVisibleToOperator', () => {
  it('shows public messages whatever the toggle says', () => {
    expect(isMessageVisibleToOperator(message(), audience(false))).toBe(true);
    expect(
      isMessageVisibleToOperator(message({ targetParticipantIds: [] }), audience(false)),
    ).toBe(true);
  });

  it('shows every whisper when "All Whispers" is on', () => {
    const whisper = message({
      systemSender: 'commonplaceBook',
      targetParticipantIds: [CHARACTER_A],
    });
    expect(isMessageVisibleToOperator(whisper, audience(true))).toBe(true);
  });

  it('hides Commonplace Book recall whispers to a character when the toggle is off', () => {
    const recall = message({
      systemSender: 'commonplaceBook',
      participantId: null,
      targetParticipantIds: [CHARACTER_A],
    });
    expect(isMessageVisibleToOperator(recall, audience(false))).toBe(false);
  });

  it.each(['carina', 'librarian', 'host'] as const satisfies readonly SystemSender[])(
    'hides %s whispers to a character when the toggle is off',
    (sender) => {
      const whisper = message({
        systemSender: sender,
        participantId: null,
        targetParticipantIds: [CHARACTER_A],
      });
      expect(isMessageVisibleToOperator(whisper, audience(false))).toBe(false);
    },
  );

  it.each(['pascal', 'prospero'] as const satisfies readonly SystemSender[])(
    'shows %s private runs even when the toggle is off — they are operator machinery',
    (sender) => {
      const run = message({
        systemSender: sender,
        participantId: null,
        targetParticipantIds: [CHARACTER_A],
      });
      expect(isMessageVisibleToOperator(run, audience(false))).toBe(true);
    },
  );

  it('shows Staff whispers addressed to the human regardless of sender', () => {
    const toUser = message({
      systemSender: 'commonplaceBook',
      participantId: null,
      targetParticipantIds: [USER_PARTICIPANT],
    });
    expect(isMessageVisibleToOperator(toUser, audience(false))).toBe(true);
  });

  it('shows whispers the human authored, and hides character-to-character ones', () => {
    const fromUser = message({
      participantId: USER_PARTICIPANT,
      targetParticipantIds: [CHARACTER_A],
    });
    const betweenCharacters = message({
      participantId: CHARACTER_A,
      targetParticipantIds: [CHARACTER_B],
    });
    expect(isMessageVisibleToOperator(fromUser, audience(false))).toBe(true);
    expect(isMessageVisibleToOperator(betweenCharacters, audience(false))).toBe(false);
  });
});
