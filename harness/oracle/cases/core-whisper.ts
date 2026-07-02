/**
 * Oracle case (chat-orchestration wave 1.4): the Aurora Core whisper cadence
 * trigger.
 *
 * Drives the REAL pure function from v4's
 * lib/chat/context/core-whisper-trigger.ts: shouldFireCoreWhisper (which
 * internally exercises the private isVisibleConversationalTurn and
 * findLatestContextTransitionIndex helpers — not separately exported, so
 * covered only through the public function).
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/core-whisper.ts \
 *     > /tmp/oracle-core-whisper.ndjson
 */

import {
  shouldFireCoreWhisper,
  type ShouldFireCoreWhisperOptions,
} from '@/lib/chat/context/core-whisper-trigger';
import type { ChatEvent, MessageEvent } from '@/lib/schemas/chat.types';

interface Row {
  id: string;
  options: {
    events: Array<Record<string, unknown>>;
    respondingParticipantId: string;
    isContinue: boolean;
    isNudge: boolean;
    interval: number;
    silenceThreshold: number;
    fireOnContextTransition?: boolean;
  };
  out: ReturnType<typeof shouldFireCoreWhisper>;
}

const rows: Row[] = [];

const P1 = 'p-1111-participant-one';
const P2 = 'p-2222-participant-two';
const P3 = 'p-3333-participant-three';

let seq = 0;
function msg(overrides: Partial<MessageEvent> = {}): MessageEvent {
  seq += 1;
  return {
    type: 'message',
    id: `m-${seq}`,
    role: 'ASSISTANT',
    content: `content-${seq}`,
    attachments: [],
    createdAt: new Date(0).toISOString(),
    ...overrides,
  } as MessageEvent;
}

function contextSummary(): ChatEvent {
  seq += 1;
  return {
    type: 'context-summary',
    id: `cs-${seq}`,
    content: 'summary text',
    createdAt: new Date(0).toISOString(),
  } as unknown as ChatEvent;
}

function systemEvent(): ChatEvent {
  seq += 1;
  return {
    type: 'system',
    id: `sys-${seq}`,
    kind: 'note',
    createdAt: new Date(0).toISOString(),
  } as unknown as ChatEvent;
}

// Aurora's own core-whisper message targeting a given participant.
function coreWhisperMsg(targetId: string): MessageEvent {
  return msg({
    role: 'ASSISTANT',
    systemSender: 'aurora',
    systemKind: 'core-whisper',
    targetParticipantIds: [targetId],
    content: 'A quiet packet arrives.',
  });
}

function myTurn(participantId: string, content = 'my turn content'): MessageEvent {
  return msg({ role: 'ASSISTANT', participantId, content });
}

function theirTurn(participantId: string, content = 'their turn content'): MessageEvent {
  return msg({ role: 'ASSISTANT', participantId, content });
}

function userTurn(content = 'user says hi'): MessageEvent {
  return msg({ role: 'USER', participantId: null, content });
}

function staffWhisper(systemSender: MessageEvent['systemSender'] = 'lantern'): MessageEvent {
  return msg({ role: 'ASSISTANT', systemSender, content: 'staff announcement' });
}

function librarianFold(
  systemKind: string,
  targetParticipantIds?: string[] | null,
): MessageEvent {
  return msg({
    role: 'ASSISTANT',
    systemSender: 'librarian',
    systemKind,
    content: 'summary fold whisper',
    targetParticipantIds: targetParticipantIds ?? null,
  });
}

function silentMsg(participantId: string): MessageEvent {
  return msg({ role: 'ASSISTANT', participantId, isSilentMessage: true, content: 'silent' });
}

function emptyContentMsg(participantId: string): MessageEvent {
  return msg({ role: 'ASSISTANT', participantId, content: '   ' });
}

function privateWhisper(targetIds: string[], content = 'private aside'): MessageEvent {
  return msg({ role: 'ASSISTANT', targetParticipantIds: targetIds, content });
}

type Case = [string, Omit<ShouldFireCoreWhisperOptions, 'events'> & { events: ChatEvent[] }];

const cases: Case[] = [
  // ---- isContinue / isNudge short-circuit ---------------------------------
  [
    'continue-skips-entirely',
    {
      events: [],
      respondingParticipantId: P1,
      isContinue: true,
      isNudge: false,
      interval: 5,
      silenceThreshold: 3,
    },
  ],
  [
    'nudge-skips-entirely',
    {
      events: [myTurn(P1), theirTurn(P2)],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: true,
      interval: 5,
      silenceThreshold: 3,
    },
  ],
  [
    'continue-and-nudge-both-true-skips',
    {
      events: [],
      respondingParticipantId: P1,
      isContinue: true,
      isNudge: true,
      interval: 5,
      silenceThreshold: 3,
    },
  ],

  // ---- "first" trigger -----------------------------------------------------
  [
    'first-empty-history',
    {
      events: [],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 5,
      silenceThreshold: 3,
    },
  ],
  [
    'first-character-has-spoken-but-never-whispered-bootstrap',
    {
      events: [myTurn(P1), theirTurn(P2), myTurn(P1), theirTurn(P2)],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 2,
      silenceThreshold: 10,
    },
  ],
  [
    'first-other-participant-whispered-not-mine',
    {
      events: [coreWhisperMsg(P2), myTurn(P1)],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 5,
      silenceThreshold: 5,
    },
  ],

  // ---- "periodic" trigger ---------------------------------------------------
  [
    'periodic-exact-interval-reached',
    {
      events: [
        coreWhisperMsg(P1),
        myTurn(P1),
        theirTurn(P2),
        myTurn(P1),
        theirTurn(P2),
        myTurn(P1),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 3,
      silenceThreshold: 100,
    },
  ],
  [
    'periodic-one-below-interval-no-fire',
    {
      events: [coreWhisperMsg(P1), myTurn(P1), theirTurn(P2), myTurn(P1)],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 3,
      silenceThreshold: 100,
    },
  ],
  [
    'periodic-interval-one-fires-on-next-turn',
    {
      events: [coreWhisperMsg(P1), myTurn(P1)],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 1,
      silenceThreshold: 100,
    },
  ],
  [
    'periodic-only-counts-turns-after-whisper-not-before',
    {
      events: [myTurn(P1), myTurn(P1), coreWhisperMsg(P1), myTurn(P1)],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 2,
      silenceThreshold: 100,
    },
  ],
  [
    'periodic-resets-after-second-whisper',
    {
      events: [
        coreWhisperMsg(P1),
        myTurn(P1),
        myTurn(P1),
        coreWhisperMsg(P1), // second whisper resets the counter
        myTurn(P1),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 2,
      silenceThreshold: 100,
    },
  ],

  // ---- "silence" trigger -----------------------------------------------------
  [
    'silence-exact-threshold-reached',
    {
      events: [
        coreWhisperMsg(P1),
        myTurn(P1),
        theirTurn(P2),
        theirTurn(P3),
        userTurn(),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 3,
    },
  ],
  [
    'silence-one-below-threshold-no-fire',
    {
      events: [coreWhisperMsg(P1), myTurn(P1), theirTurn(P2), theirTurn(P3)],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 3,
    },
  ],
  [
    'silence-run-resets-on-my-turn',
    {
      events: [
        coreWhisperMsg(P1),
        theirTurn(P2),
        theirTurn(P3),
        myTurn(P1), // resets silenceRun to 0
        theirTurn(P2),
        theirTurn(P3),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 3,
    },
  ],
  [
    'silence-nonmessage-events-do-not-break-or-extend-run',
    {
      events: [
        coreWhisperMsg(P1),
        theirTurn(P2),
        contextSummary(),
        systemEvent(),
        theirTurn(P3),
        theirTurn(P2),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 3,
    },
  ],

  // ---- visibility exclusions (isVisibleConversationalTurn) -------------------
  [
    'staff-whisper-excluded-from-silence-run',
    {
      events: [
        coreWhisperMsg(P1),
        staffWhisper('host'),
        staffWhisper('concierge'),
        staffWhisper('prospero'),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 3,
    },
  ],
  [
    'silent-message-excluded-from-silence-run',
    {
      events: [coreWhisperMsg(P1), silentMsg(P2), silentMsg(P3), silentMsg(P2)],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 3,
    },
  ],
  [
    'empty-trimmed-content-excluded',
    {
      events: [coreWhisperMsg(P1), emptyContentMsg(P2), emptyContentMsg(P3), emptyContentMsg(P2)],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 3,
    },
  ],
  [
    'private-whisper-not-targeting-me-excluded',
    {
      events: [
        coreWhisperMsg(P1),
        privateWhisper([P2, P3]),
        privateWhisper([P2, P3]),
        privateWhisper([P2, P3]),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 3,
    },
  ],
  [
    'private-whisper-targeting-me-counted-as-not-mine',
    {
      events: [
        coreWhisperMsg(P1),
        privateWhisper([P1, P3], 'aside to you'),
        privateWhisper([P1, P3], 'another aside'),
        privateWhisper([P1, P3], 'third aside'),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 3,
    },
  ],
  [
    'my-own-private-whisper-still-counts-as-mine',
    {
      // A message authored BY me (role ASSISTANT + participantId=me) with
      // targetParticipantIds including me is visible and "mine" -> resets
      // silenceRun and counts toward periodic.
      events: [
        coreWhisperMsg(P1),
        theirTurn(P2),
        theirTurn(P3),
        msg({ role: 'ASSISTANT', participantId: P1, targetParticipantIds: [P1, P2], content: 'whisper from me to us' }),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 2,
    },
  ],
  [
    'empty-target-array-treated-as-public',
    {
      events: [
        coreWhisperMsg(P1),
        privateWhisper([], 'targets empty array = public'),
        privateWhisper([], 'still public'),
        privateWhisper([], 'still public again'),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 3,
    },
  ],
  [
    'mixed-visible-and-excluded-events',
    {
      events: [
        coreWhisperMsg(P1),
        staffWhisper('lantern'),
        silentMsg(P2),
        theirTurn(P2),
        emptyContentMsg(P3),
        theirTurn(P3),
        privateWhisper([P2, P3]),
        theirTurn(P2),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 3,
    },
  ],

  // ---- context-transition trigger --------------------------------------------
  [
    'context-transition-fires-after-whisper',
    {
      events: [
        coreWhisperMsg(P1),
        myTurn(P1),
        librarianFold('rolling-summary'),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 100,
      fireOnContextTransition: true,
    },
  ],
  [
    'context-transition-per-character-summary-kind',
    {
      events: [
        coreWhisperMsg(P1),
        myTurn(P1),
        librarianFold('per-character-summary'),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 100,
      fireOnContextTransition: true,
    },
  ],
  [
    'context-transition-before-last-whisper-does-not-fire',
    {
      events: [
        librarianFold('rolling-summary'),
        coreWhisperMsg(P1), // whisper AFTER the fold -> transition is stale
        myTurn(P1),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 100,
      fireOnContextTransition: true,
    },
  ],
  [
    'context-transition-disabled-by-flag',
    {
      events: [
        coreWhisperMsg(P1),
        myTurn(P1),
        librarianFold('rolling-summary'),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 100,
      fireOnContextTransition: false,
    },
  ],
  [
    'context-transition-loose-fold-substring-no-longer-matches',
    {
      // "folder-created-by-character" contains "fold" as a substring but its
      // kebab segments are ['folder','created','by','character'] — none is
      // 'summary' or 'rolling', so the tightened check correctly skips it.
      events: [
        coreWhisperMsg(P1),
        myTurn(P1),
        librarianFold('folder-created-by-character'),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 100,
      fireOnContextTransition: true,
    },
  ],
  [
    'context-transition-underscore-segment-rolling',
    {
      events: [
        coreWhisperMsg(P1),
        myTurn(P1),
        librarianFold('rolling_fold_event'),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 100,
      fireOnContextTransition: true,
    },
  ],
  [
    'context-transition-not-librarian-sender-ignored',
    {
      events: [
        coreWhisperMsg(P1),
        myTurn(P1),
        msg({ role: 'ASSISTANT', systemSender: 'host', systemKind: 'rolling-summary', content: 'not actually librarian' }),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 100,
      fireOnContextTransition: true,
    },
  ],
  [
    'context-transition-per-character-summary-not-targeting-me-excluded',
    {
      events: [
        coreWhisperMsg(P1),
        myTurn(P1),
        librarianFold('per-character-summary', [P2]),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 100,
      fireOnContextTransition: true,
    },
  ],
  [
    'context-transition-per-character-summary-targeting-me-included',
    {
      events: [
        coreWhisperMsg(P1),
        myTurn(P1),
        librarianFold('per-character-summary', [P1, P2]),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 100,
      fireOnContextTransition: true,
    },
  ],
  [
    'context-transition-picks-latest-not-first',
    {
      events: [
        coreWhisperMsg(P1),
        librarianFold('rolling-summary'),
        myTurn(P1),
        librarianFold('rolling-summary'), // latest one, still after whisper
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 100,
      fireOnContextTransition: true,
    },
  ],
  [
    'context-transition-default-flag-true-when-omitted',
    {
      events: [
        coreWhisperMsg(P1),
        myTurn(P1),
        librarianFold('rolling-summary'),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 100,
      // fireOnContextTransition omitted -> defaults to true in v4.
    },
  ],

  // ---- trigger precedence (periodic/silence checked before context-transition) --
  [
    'periodic-takes-precedence-over-context-transition',
    {
      events: [
        coreWhisperMsg(P1),
        myTurn(P1),
        myTurn(P1),
        librarianFold('rolling-summary'),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 2,
      silenceThreshold: 100,
      fireOnContextTransition: true,
    },
  ],
  [
    'silence-takes-precedence-over-context-transition',
    {
      events: [
        coreWhisperMsg(P1),
        theirTurn(P2),
        theirTurn(P3),
        librarianFold('rolling-summary'),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 100,
      silenceThreshold: 2,
      fireOnContextTransition: true,
    },
  ],

  // ---- no trigger fires -------------------------------------------------------
  [
    'no-trigger-fires-quiet-period',
    {
      events: [coreWhisperMsg(P1), myTurn(P1), theirTurn(P2)],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 5,
      silenceThreshold: 5,
      fireOnContextTransition: true,
    },
  ],
  [
    'no-trigger-fires-with-nonmessage-events-only',
    {
      events: [coreWhisperMsg(P1), contextSummary(), systemEvent()],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 5,
      silenceThreshold: 5,
      fireOnContextTransition: true,
    },
  ],

  // ---- multi-participant realistic scene --------------------------------------
  [
    'realistic-three-participant-scene',
    {
      events: [
        userTurn('Let us begin.'),
        myTurn(P1, 'I greet everyone.'),
        theirTurn(P2, 'Nice to meet you.'),
        coreWhisperMsg(P1),
        myTurn(P1, 'A thoughtful reply.'),
        theirTurn(P3, 'An aside from the third.'),
        userTurn('Continuing on.'),
        theirTurn(P2, 'Another remark.'),
      ],
      respondingParticipantId: P1,
      isContinue: false,
      isNudge: false,
      interval: 3,
      silenceThreshold: 3,
      fireOnContextTransition: true,
    },
  ],
];

for (const [id, opts] of cases) {
  const out = shouldFireCoreWhisper(opts as ShouldFireCoreWhisperOptions);
  rows.push({
    id,
    options: {
      events: opts.events as unknown as Array<Record<string, unknown>>,
      respondingParticipantId: opts.respondingParticipantId,
      isContinue: opts.isContinue,
      isNudge: opts.isNudge,
      interval: opts.interval,
      silenceThreshold: opts.silenceThreshold,
      fireOnContextTransition: opts.fireOnContextTransition,
    },
    out,
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
