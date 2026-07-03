/**
 * Oracle case: the `buildMessageContext` pure leaves (chat-orchestration Piece 2).
 *
 * Drives the REAL exported pure functions from v4's
 * `lib/services/chat-message/context-builder.service.ts`:
 *   - buildConversationMessages
 *   - normalizeWhisperRoles
 *   - collectLanternImageFileIdsForCharacter
 *
 * The corpus stresses the edge cases the orchestrator-tier3 composition corpus
 * cannot easily reach: TOOL-result rendering (verbatim vs the >3-turn elision, the
 * compact-args slice, an unparseable payload dropped), the `assistantAfter` reverse
 * pass, the whisper re-role / opaque-body-swap / attachment exemption, and the
 * Lantern walk's own-turn stop / history cutoff / dedup / lookback cap / reversal.
 *
 * Each output message is projected to the WhisperMessage field set with null/
 * undefined keys dropped, so `JSON.stringify`'s `undefined`-dropping and v4's
 * explicit `systemSender: null` both collapse to "absent" (matching the Rust
 * `Option::None` serialization). The Rust differential re-runs the ported leaves on
 * the same inputs and compares.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/message-context-leaves.ts \
 *     > /tmp/oracle-message-context-leaves.ndjson
 */

import {
  buildConversationMessages,
  normalizeWhisperRoles,
  collectLanternImageFileIdsForCharacter,
} from '@/lib/services/chat-message/context-builder.service';

type Msg = Record<string, unknown>;

// The WhisperMessage field set — the Rust `WhisperMessage` parses exactly these.
const WHISPER_FIELDS = [
  'type',
  'role',
  'content',
  'id',
  'thoughtSignature',
  'participantId',
  'targetParticipantIds',
  'createdAt',
  'attachments',
  'systemSender',
  'systemKind',
  'opaqueContent',
];
const CONV_FIELDS = ['role', 'content', 'id', 'thoughtSignature'];
const MWP_FIELDS = [
  'id',
  'role',
  'content',
  'participantId',
  'thoughtSignature',
  'createdAt',
  'targetParticipantIds',
];

function project(m: Msg, fields: string[]): Msg {
  const out: Msg = {};
  for (const f of fields) {
    const v = m[f];
    if (v !== null && v !== undefined) out[f] = v;
  }
  return out;
}

// ---------------------------------------------------------------------------
// buildConversationMessages corpus.
// ---------------------------------------------------------------------------

interface BcmCase {
  name: string;
  isMultiCharacter: boolean;
  messages: Msg[];
}

const bcmCases: BcmCase[] = [
  {
    name: 'filter-roles-and-types',
    isMultiCharacter: false,
    messages: [
      { type: 'message', role: 'USER', content: 'hi', id: 'u1' },
      { type: 'context-summary', role: 'SYSTEM', content: 'summary', id: 's1' },
      { type: 'message', role: 'SYSTEM', content: 'sys note', id: 's2' },
      { type: 'message', role: 'ASSISTANT', content: 'hello', id: 'a1', thoughtSignature: 'sig' },
    ],
  },
  {
    name: 'assistant-thoughtsig-only-on-assistant',
    isMultiCharacter: false,
    messages: [
      { type: 'message', role: 'USER', content: 'q', id: 'u1', thoughtSignature: 'ignored' },
      { type: 'message', role: 'ASSISTANT', content: 'a', id: 'a1' },
    ],
  },
  {
    name: 'tool-verbatim-and-elided',
    isMultiCharacter: false,
    messages: [
      // This TOOL has 3 ASSISTANTs after it → elided (with compact args).
      {
        type: 'message',
        role: 'TOOL',
        content: JSON.stringify({ toolName: 'roll', result: '42', arguments: { sides: 6 } }),
        id: 't1',
      },
      { type: 'message', role: 'ASSISTANT', content: 'a1', id: 'a1' },
      { type: 'message', role: 'ASSISTANT', content: 'a2', id: 'a2' },
      // This TOOL has 1 ASSISTANT after it → verbatim.
      { type: 'message', role: 'TOOL', content: JSON.stringify({ tool: 'lookup', result: 'ok' }), id: 't2' },
      { type: 'message', role: 'ASSISTANT', content: 'a3', id: 'a3' },
    ],
  },
  {
    name: 'tool-unparseable-dropped',
    isMultiCharacter: false,
    messages: [
      { type: 'message', role: 'USER', content: 'q', id: 'u1' },
      { type: 'message', role: 'TOOL', content: 'not json {{', id: 't1' },
      { type: 'message', role: 'ASSISTANT', content: 'a1', id: 'a1' },
    ],
  },
  {
    name: 'tool-no-result-no-name',
    isMultiCharacter: false,
    messages: [
      { type: 'message', role: 'TOOL', content: JSON.stringify({ arguments: { a: 1 } }), id: 't1' },
      { type: 'message', role: 'ASSISTANT', content: 'a1', id: 'a1' },
    ],
  },
  {
    name: 'multi-with-participants-and-targets',
    isMultiCharacter: true,
    messages: [
      { type: 'message', role: 'USER', content: 'hi', id: 'u1', participantId: 'p-user', createdAt: '2026-01-01T00:00:00.000Z' },
      {
        type: 'message',
        role: 'ASSISTANT',
        content: 'reply',
        id: 'a1',
        participantId: 'p-friday',
        thoughtSignature: 'sig',
        createdAt: '2026-01-01T00:01:00.000Z',
        targetParticipantIds: ['p-user'],
      },
      {
        type: 'message',
        role: 'TOOL',
        content: JSON.stringify({ toolName: 'dice', result: '6' }),
        id: 't1',
        participantId: 'p-friday',
        createdAt: '2026-01-01T00:02:00.000Z',
        targetParticipantIds: ['p-user'],
      },
    ],
  },
];

// ---------------------------------------------------------------------------
// normalizeWhisperRoles corpus.
// ---------------------------------------------------------------------------

interface NwrCase {
  name: string;
  isOpaqueAnywhere: boolean;
  messages: Msg[];
}

const nwrCases: NwrCase[] = [
  {
    name: 'opaque-swap-and-reroles',
    isOpaqueAnywhere: true,
    messages: [
      {
        type: 'message',
        role: 'ASSISTANT',
        content: 'The Librarian files a note.',
        systemSender: 'librarian',
        opaqueContent: 'A note was filed.',
      },
      // No opaqueContent → opaque body falls back to content.
      { type: 'message', role: 'ASSISTANT', content: 'Host says hi.', systemSender: 'host' },
      // Attachment-bearing whisper stays ASSISTANT.
      {
        type: 'message',
        role: 'ASSISTANT',
        content: 'An image.',
        systemSender: 'lantern',
        attachments: ['file-1'],
      },
      // Non-whisper passthrough.
      { type: 'message', role: 'USER', content: 'plain', id: 'u1' },
      // Empty systemSender is falsy → passthrough.
      { type: 'message', role: 'ASSISTANT', content: 'still assistant', systemSender: '' },
    ],
  },
  {
    name: 'transparent-keeps-persona',
    isOpaqueAnywhere: false,
    messages: [
      {
        type: 'message',
        role: 'ASSISTANT',
        content: 'The Librarian files a note.',
        systemSender: 'librarian',
        opaqueContent: 'A note was filed.',
      },
      {
        type: 'message',
        role: 'ASSISTANT',
        content: 'Lantern image.',
        systemSender: 'lantern',
        attachments: ['file-2'],
        opaqueContent: 'opaque image note',
      },
    ],
  },
];

// ---------------------------------------------------------------------------
// collectLanternImageFileIdsForCharacter corpus.
// ---------------------------------------------------------------------------

interface LanternCase {
  name: string;
  characterParticipantId: string;
  isMultiCharacter: boolean;
  historyCutoff: string | null;
  lookback: number;
  messages: Msg[];
}

const lanternCases: LanternCase[] = [
  {
    name: 'multi-stop-at-own-turn-dedup-reverse',
    characterParticipantId: 'cp',
    isMultiCharacter: true,
    historyCutoff: null,
    lookback: 6,
    messages: [
      { type: 'message', role: 'ASSISTANT', participantId: 'cp', content: 'own older' },
      { type: 'message', role: 'ASSISTANT', attachments: ['img-1', 'img-2'], content: 'lantern a', createdAt: '2026-01-02T00:00:00.000Z' },
      { type: 'message', role: 'ASSISTANT', attachments: ['img-2', 'img-3'], content: 'lantern b', createdAt: '2026-01-02T00:01:00.000Z' },
    ],
  },
  {
    name: 'single-stop-at-no-attachment-assistant',
    characterParticipantId: 'cp',
    isMultiCharacter: false,
    historyCutoff: null,
    lookback: 6,
    messages: [
      { type: 'message', role: 'ASSISTANT', content: 'own reply (no attachments)' },
      { type: 'message', role: 'ASSISTANT', attachments: ['img-9'], content: 'lantern' },
    ],
  },
  {
    name: 'history-cutoff-skips-older',
    characterParticipantId: 'cp',
    isMultiCharacter: true,
    historyCutoff: '2026-01-02T00:00:30.000Z',
    lookback: 6,
    messages: [
      { type: 'message', role: 'ASSISTANT', attachments: ['old'], content: 'before cutoff', createdAt: '2026-01-02T00:00:00.000Z' },
      { type: 'message', role: 'ASSISTANT', attachments: ['new'], content: 'after cutoff', createdAt: '2026-01-02T00:01:00.000Z' },
    ],
  },
  {
    name: 'lookback-cap',
    characterParticipantId: 'cp',
    isMultiCharacter: true,
    historyCutoff: null,
    lookback: 2,
    messages: [
      { type: 'message', role: 'ASSISTANT', attachments: ['deep'], content: 'oldest lantern' },
      { type: 'message', role: 'ASSISTANT', content: 'no-att-1' },
      { type: 'message', role: 'ASSISTANT', content: 'no-att-2' },
    ],
  },
];

// ---------------------------------------------------------------------------
// Emit.
// ---------------------------------------------------------------------------

const lines: string[] = [];

for (const c of bcmCases) {
  const { conversationMessages, messagesWithParticipants } = buildConversationMessages(
    c.messages as never,
    c.isMultiCharacter
  );
  lines.push(
    JSON.stringify({
      kind: 'bcm',
      name: c.name,
      isMultiCharacter: c.isMultiCharacter,
      messages: c.messages,
      conversationMessages: (conversationMessages as Msg[]).map((m) => project(m, CONV_FIELDS)),
      messagesWithParticipants: messagesWithParticipants
        ? (messagesWithParticipants as Msg[]).map((m) => project(m, MWP_FIELDS))
        : null,
    })
  );
}

for (const c of nwrCases) {
  const out = normalizeWhisperRoles(c.messages as never, c.isOpaqueAnywhere);
  lines.push(
    JSON.stringify({
      kind: 'nwr',
      name: c.name,
      isOpaqueAnywhere: c.isOpaqueAnywhere,
      messages: c.messages,
      output: (out as Msg[]).map((m) => project(m, WHISPER_FIELDS)),
    })
  );
}

for (const c of lanternCases) {
  const out = collectLanternImageFileIdsForCharacter(
    c.messages as never,
    c.characterParticipantId,
    c.isMultiCharacter,
    c.historyCutoff,
    c.lookback
  );
  lines.push(
    JSON.stringify({
      kind: 'lantern',
      name: c.name,
      characterParticipantId: c.characterParticipantId,
      isMultiCharacter: c.isMultiCharacter,
      historyCutoff: c.historyCutoff,
      lookback: c.lookback,
      messages: c.messages,
      output: out,
    })
  );
}

for (const l of lines) process.stdout.write(l + '\n');
