/**
 * Oracle case #18 (Wave 3 / B9): message attribution & presence.
 *
 * Drives the REAL pure helpers from v4's lib/chat/context/message-attribution.ts:
 *   filterMessagesByHistoryAccess, computePresenceWindowsForParticipant,
 *   filterMessagesByPresenceWindows, filterWhisperMessages, getParticipantName,
 *   attributeMessagesForCharacter, findUserParticipantName.
 *
 * The only impurity is `new Date(createdAt).getTime()` in the history-access
 * filter; we emit the parsed epoch-millis alongside each case so the Rust port
 * (which takes ms) uses the identical instants. The presence filters compare
 * ISO strings lexically, no parse involved.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/message-attribution.ts \
 *     > /tmp/oracle-message-attribution.ndjson
 */

import {
  filterMessagesByHistoryAccess,
  computePresenceWindowsForParticipant,
  filterMessagesByPresenceWindows,
  filterWhisperMessages,
  getParticipantName,
  attributeMessagesForCharacter,
  findUserParticipantName,
  type MessageWithParticipant,
  type PresenceWindow,
} from '@/lib/chat/context/message-attribution';
import type { ChatParticipantBase, Character } from '@/lib/schemas/types';

const asMsgs = (ms: unknown[]) => ms as unknown as MessageWithParticipant[];
const asPart = (p: unknown) => p as unknown as ChatParticipantBase;
const asParts = (ps: unknown[]) => ps as unknown as ChatParticipantBase[];
const asChars = (c: Record<string, string>): Map<string, Character> => {
  const m = new Map<string, Character>();
  for (const [cid, name] of Object.entries(c)) m.set(cid, { name } as unknown as Character);
  return m;
};
const idsOf = (ms: Array<{ id?: string }>) => ms.map(m => m.id as string);

const rows: unknown[] = [];

// ---- filterMessagesByHistoryAccess ---------------------------------------
{
  type HMsg = { id: string; createdAt?: string };
  const cases: Array<{ id: string; msgs: HMsg[]; hasHistoryAccess: boolean; joinCreatedAt: string }> = [
    {
      id: 'full-access-keeps-all',
      msgs: [{ id: 'm1', createdAt: '2026-01-01T00:00:00.000Z' }, { id: 'm2', createdAt: '2026-02-01T00:00:00.000Z' }],
      hasHistoryAccess: true,
      joinCreatedAt: '2026-03-01T00:00:00.000Z',
    },
    {
      id: 'after-join-only',
      msgs: [
        { id: 'before', createdAt: '2026-01-01T00:00:00.000Z' },
        { id: 'at-join', createdAt: '2026-02-01T00:00:00.000Z' },
        { id: 'after', createdAt: '2026-03-01T00:00:00.000Z' },
      ],
      hasHistoryAccess: false,
      joinCreatedAt: '2026-02-01T00:00:00.000Z',
    },
    {
      id: 'no-createdAt-kept',
      msgs: [{ id: 'nodate' }, { id: 'old', createdAt: '2025-01-01T00:00:00.000Z' }],
      hasHistoryAccess: false,
      joinCreatedAt: '2026-01-01T00:00:00.000Z',
    },
  ];
  for (const c of cases) {
    const participant = asPart({ hasHistoryAccess: c.hasHistoryAccess, createdAt: c.joinCreatedAt });
    const out = filterMessagesByHistoryAccess(asMsgs(c.msgs), participant);
    rows.push({
      kind: 'history',
      id: c.id,
      msgs: c.msgs.map(m => ({ id: m.id, createdAtMs: m.createdAt ? new Date(m.createdAt).getTime() : null })),
      hasHistoryAccess: c.hasHistoryAccess,
      joinMs: new Date(c.joinCreatedAt).getTime(),
      out: idsOf(out as Array<{ id?: string }>),
    });
  }
}

// ---- computePresenceWindowsForParticipant --------------------------------
{
  const hev = (id: string, at: string, participantId: string, toStatus?: string) =>
    ({ id, createdAt: at, hostEvent: toStatus === undefined ? { introducedCharacterIds: ['x'] } : { participantId, toStatus } });
  const cases: Array<{ id: string; msgs: unknown[]; participantId: string; participantCreatedAt: string }> = [
    { id: 'no-events-open-window', msgs: [], participantId: 'P', participantCreatedAt: '2026-01-01T00:00:00.000Z' },
    {
      id: 'active-then-absent',
      msgs: [hev('e1', '2026-01-02T00:00:00.000Z', 'P', 'active'), hev('e2', '2026-01-05T00:00:00.000Z', 'P', 'absent')],
      participantId: 'P', participantCreatedAt: '2026-01-01T00:00:00.000Z',
    },
    {
      id: 'reopen-second-open',
      msgs: [
        hev('e1', '2026-01-02T00:00:00.000Z', 'P', 'active'),
        hev('e2', '2026-01-05T00:00:00.000Z', 'P', 'absent'),
        hev('e3', '2026-01-08T00:00:00.000Z', 'P', 'silent'),
      ],
      participantId: 'P', participantCreatedAt: '2026-01-01T00:00:00.000Z',
    },
    {
      id: 'other-participant-ignored',
      msgs: [hev('e1', '2026-01-02T00:00:00.000Z', 'OTHER', 'absent')],
      participantId: 'P', participantCreatedAt: '2026-01-01T00:00:00.000Z',
    },
    {
      id: 'introduction-no-tostatus-ignored',
      msgs: [hev('e1', '2026-01-02T00:00:00.000Z', 'P', undefined)],
      participantId: 'P', participantCreatedAt: '2026-01-01T00:00:00.000Z',
    },
    {
      id: 'out-of-order-sorted',
      msgs: [hev('e2', '2026-01-05T00:00:00.000Z', 'P', 'absent'), hev('e1', '2026-01-02T00:00:00.000Z', 'P', 'active')],
      participantId: 'P', participantCreatedAt: '2026-01-01T00:00:00.000Z',
    },
  ];
  for (const c of cases) {
    const participant = asPart({ id: c.participantId, createdAt: c.participantCreatedAt });
    const out = computePresenceWindowsForParticipant(asMsgs(c.msgs), participant);
    rows.push({ kind: 'presence', id: c.id, msgs: c.msgs, participantId: c.participantId, participantCreatedAt: c.participantCreatedAt, out });
  }
}

// ---- filterMessagesByPresenceWindows -------------------------------------
{
  type PMsg = { id: string; createdAt?: string };
  const cases: Array<{ id: string; msgs: PMsg[]; windows: PresenceWindow[] }> = [
    {
      id: 'empty-windows-drop-all',
      msgs: [{ id: 'm1', createdAt: '2026-01-02T00:00:00.000Z' }],
      windows: [],
    },
    {
      id: 'closed-window-bounds',
      msgs: [
        { id: 'before', createdAt: '2026-01-01T00:00:00.000Z' },
        { id: 'at-from', createdAt: '2026-01-02T00:00:00.000Z' },
        { id: 'inside', createdAt: '2026-01-03T00:00:00.000Z' },
        { id: 'at-to', createdAt: '2026-01-05T00:00:00.000Z' },
        { id: 'nodate' },
      ],
      windows: [{ from: '2026-01-02T00:00:00.000Z', to: '2026-01-05T00:00:00.000Z' }],
    },
    {
      id: 'open-window-keeps-tail',
      msgs: [
        { id: 'before', createdAt: '2026-01-01T00:00:00.000Z' },
        { id: 'after', createdAt: '2026-09-01T00:00:00.000Z' },
      ],
      windows: [{ from: '2026-02-01T00:00:00.000Z', to: null }],
    },
    {
      id: 'two-windows',
      msgs: [
        { id: 'w1', createdAt: '2026-01-03T00:00:00.000Z' },
        { id: 'gap', createdAt: '2026-01-06T00:00:00.000Z' },
        { id: 'w2', createdAt: '2026-01-09T00:00:00.000Z' },
      ],
      windows: [
        { from: '2026-01-02T00:00:00.000Z', to: '2026-01-05T00:00:00.000Z' },
        { from: '2026-01-08T00:00:00.000Z', to: null },
      ],
    },
  ];
  for (const c of cases) {
    const out = filterMessagesByPresenceWindows(asMsgs(c.msgs), c.windows);
    rows.push({ kind: 'presenceFilter', id: c.id, msgs: c.msgs, windows: c.windows, out: idsOf(out as Array<{ id?: string }>) });
  }
}

// ---- filterWhisperMessages -----------------------------------------------
{
  type WMsg = { id: string; participantId?: string | null; targetParticipantIds?: string[] | null };
  const cases: Array<{ id: string; msgs: WMsg[]; respondingId: string }> = [
    {
      id: 'visibility',
      msgs: [
        { id: 'public', participantId: 'A' },
        { id: 'empty-targets', participantId: 'A', targetParticipantIds: [] },
        { id: 'to-others', participantId: 'A', targetParticipantIds: ['B'] },
        { id: 'sender-sees', participantId: 'R', targetParticipantIds: ['B'] },
        { id: 'target-sees', participantId: 'A', targetParticipantIds: ['R', 'B'] },
      ],
      respondingId: 'R',
    },
  ];
  for (const c of cases) {
    const out = filterWhisperMessages(asMsgs(c.msgs), c.respondingId);
    rows.push({ kind: 'whisper', id: c.id, msgs: c.msgs, respondingId: c.respondingId, out: idsOf(out as Array<{ id?: string }>) });
  }
}

// ---- getParticipantName --------------------------------------------------
{
  const parts = [
    { id: 'pc', type: 'CHARACTER', characterId: 'c-known', controlledBy: 'llm', status: 'active' },
    { id: 'pc-unknown', type: 'CHARACTER', characterId: 'c-missing', controlledBy: 'llm', status: 'active' },
    { id: 'pc-empty-cid', type: 'CHARACTER', characterId: '', controlledBy: 'llm', status: 'active' },
    { id: 'pc-noname', type: 'CHARACTER', characterId: 'c-empty', controlledBy: 'llm', status: 'active' },
    { id: 'p-other', type: 'OTHER', characterId: 'c-known', controlledBy: 'llm', status: 'active' },
  ];
  const chars = { 'c-known': 'Friday', 'c-empty': '' };
  const cases: Array<{ id: string; participantId: string | null }> = [
    { id: 'null-id', participantId: null },
    { id: 'missing-participant', participantId: 'nope' },
    { id: 'known-character', participantId: 'pc' },
    { id: 'character-not-in-map', participantId: 'pc-unknown' },
    { id: 'empty-characterId', participantId: 'pc-empty-cid' },
    { id: 'empty-name', participantId: 'pc-noname' },
    { id: 'non-character-type', participantId: 'p-other' },
  ];
  for (const c of cases) {
    const out = getParticipantName(c.participantId, asChars(chars), asParts(parts));
    rows.push({ kind: 'name', id: c.id, participantId: c.participantId, characters: chars, participants: parts, out: out ?? null });
  }
}

// ---- attributeMessagesForCharacter ---------------------------------------
{
  const parts = [
    { id: 'R', type: 'CHARACTER', characterId: 'c-r', controlledBy: 'llm', status: 'active' },
    { id: 'O', type: 'CHARACTER', characterId: 'c-o', controlledBy: 'llm', status: 'active' },
  ];
  const chars = { 'c-r': 'Responder', 'c-o': 'Other' };
  const msgs = [
    { id: 'self', role: 'ASSISTANT', content: 'mine', participantId: 'R', thoughtSignature: 'sig1' },
    { id: 'other', role: 'ASSISTANT', content: 'theirs', participantId: 'O', thoughtSignature: null },
    { id: 'usermsg', role: 'USER', content: 'hi', participantId: null },
    { id: 'empty-pid', role: 'USER', content: 'blank', participantId: '' },
  ];
  const out = attributeMessagesForCharacter(asMsgs(msgs), 'R', asChars(chars), asParts(parts));
  rows.push({ kind: 'attribute', id: 'responder-view', msgs, respondingId: 'R', characters: chars, participants: parts, out });
}

// ---- findUserParticipantName ---------------------------------------------
{
  const chars = { 'c-user': 'Bertie', 'c-user2': 'Jeeves', 'c-blank': '' };
  const cases: Array<{ id: string; participants: unknown[]; activeTyping: string | null }> = [
    {
      id: 'active-typing-selected',
      participants: [
        { id: 'u1', type: 'CHARACTER', characterId: 'c-user', controlledBy: 'user', status: 'active' },
        { id: 'u2', type: 'CHARACTER', characterId: 'c-user2', controlledBy: 'user', status: 'active' },
      ],
      activeTyping: 'u2',
    },
    {
      id: 'active-typing-invalid-falls-back',
      participants: [
        { id: 'u1', type: 'CHARACTER', characterId: 'c-user', controlledBy: 'user', status: 'active' },
        { id: 'u2', type: 'CHARACTER', characterId: 'c-user2', controlledBy: 'user', status: 'absent' },
      ],
      activeTyping: 'u2',
    },
    {
      id: 'no-active-typing-first-user',
      participants: [
        { id: 'llm', type: 'CHARACTER', characterId: 'c-user2', controlledBy: 'llm', status: 'active' },
        { id: 'u1', type: 'CHARACTER', characterId: 'c-user', controlledBy: 'user', status: 'active' },
      ],
      activeTyping: null,
    },
    {
      id: 'no-user-character',
      participants: [
        { id: 'llm', type: 'CHARACTER', characterId: 'c-user2', controlledBy: 'llm', status: 'active' },
      ],
      activeTyping: null,
    },
    {
      id: 'blank-name-falls-through',
      participants: [
        { id: 'u1', type: 'CHARACTER', characterId: 'c-blank', controlledBy: 'user', status: 'active' },
      ],
      activeTyping: null,
    },
  ];
  for (const c of cases) {
    const out = findUserParticipantName(asParts(c.participants), asChars(chars), c.activeTyping);
    rows.push({ kind: 'userName', id: c.id, participants: c.participants, characters: chars, activeTyping: c.activeTyping, out: out ?? null });
  }
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
