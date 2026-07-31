/**
 * Oracle case: the Markdown transcript renderer (P4.d28, v4 `b3ee00f1`).
 *
 * Drives v4's REAL exported functions:
 *   lib/export/markdown-transcript.ts → buildMarkdownTranscript, transcriptFilename
 *   lib/api/content-disposition.ts    → buildContentDisposition
 *
 * The renderer is pure, so this is a tier-1 corpus rather than a fixture tier:
 * every row carries the exact JSON v4's renderer consumed (chat metadata, the
 * event list, the name map) plus its byte-for-byte output. That is what lets the
 * matrix reach arms the committed `chat-dialogs` fixture has none of — whispers,
 * Pascal/Carina/Brahma, all three customAnnouncer shapes, Host link notices vs
 * Staff housekeeping, swipe ties, fictional clocks, a non-ASCII title — without
 * churning a fixture five other oracle families read.
 *
 * The route tier (headers, the 404/500 arms, the character-id collection) lives
 * in `chat-dialogs-export.test.ts`, which drives the real Next route.
 *
 * `buildMarkdownTranscript` reads no wall clock, so nothing here is pinned; the
 * zone-less formatting path DOES read the host TZ, so this case MUST run under
 * `TZ=UTC` and records the resulting `localOffsetMin` (0) for the Rust side to
 * inject. `QUILLTAP_TIMEZONE` is cleared so `resolveTimezone`'s third link never
 * fires from the ambient environment.
 *
 * Run from inside the server checkout (TZ=UTC is required):
 *   cd ~/source/quilltap-server
 *   TZ=UTC ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
 *     ~/source/quilltap-v5/harness/oracle/cases/markdown-transcript.ts \
 *     > /tmp/oracle-markdown-transcript.ndjson
 */

import {
  buildMarkdownTranscript,
  transcriptFilename,
  type MarkdownTranscriptInput,
} from '@/lib/export/markdown-transcript'
import { buildContentDisposition } from '@/lib/api/content-disposition'
import { BRAHMA_CARINA_ANSWERER_ID } from '@/lib/services/carina/brahma-answerer'
import type { ChatMetadata, ChatEvent, MessageEvent, ChatParticipant } from '@/lib/schemas/chat.types'
import type { TimestampConfig } from '@/lib/schemas/types'

const LOCAL_OFFSET_MIN = new Date(0).getTimezoneOffset()
if (LOCAL_OFFSET_MIN !== 0) {
  throw new Error(
    `markdown-transcript oracle must run under TZ=UTC (getTimezoneOffset=${LOCAL_OFFSET_MIN})`,
  )
}
delete process.env.QUILLTAP_TIMEZONE

const CHAR_ID = 'c1000000-0000-4000-8000-000000000001'
const USER_CHAR_ID = 'c2000000-0000-4000-8000-000000000002'
const OFFSCENE_ID = 'c3000000-0000-4000-8000-000000000003'
const BROKEN_ID = 'c4000000-0000-4000-8000-000000000004'
const PARTICIPANT_ID = 'a1000000-0000-4000-8000-000000000001'
const USER_PARTICIPANT_ID = 'a2000000-0000-4000-8000-000000000002'
const BROKEN_PARTICIPANT_ID = 'a3000000-0000-4000-8000-000000000003'

type Row =
  | {
      kind: 'transcript'
      id: string
      chat: unknown
      events: unknown[]
      characterNames: Record<string, string>
      userName: string
      defaultTimestampConfig: unknown
      chatSettingsTimezone: string | null
      localOffsetMin: number
      out?: string
      threw?: boolean
    }
  | { kind: 'filename'; id: string; chat: unknown; out: string }
  | {
      kind: 'disposition'
      id: string
      filename: string
      disposition: 'inline' | 'attachment' | null
      out: string
    }

const rows: Row[] = []

function makeConfig(overrides: Partial<TimestampConfig> = {}): TimestampConfig {
  return {
    mode: 'NONE',
    format: 'FRIENDLY',
    customFormat: null,
    useFictionalTime: false,
    fictionalBaseTimestamp: null,
    fictionalBaseRealTime: null,
    autoPrepend: true,
    intervalMinutes: 15,
    timezone: 'UTC',
    ...overrides,
  } as TimestampConfig
}

const PARTICIPANTS: ChatParticipant[] = [
  { id: PARTICIPANT_ID, type: 'CHARACTER', characterId: CHAR_ID, controlledBy: 'llm' },
  { id: USER_PARTICIPANT_ID, type: 'CHARACTER', characterId: USER_CHAR_ID, controlledBy: 'user' },
  // A participant whose vault no longer resolves: present in the chat, absent
  // from the name map, so its messages fall through to the primary fallback.
  { id: BROKEN_PARTICIPANT_ID, type: 'CHARACTER', characterId: BROKEN_ID, controlledBy: 'llm' },
] as unknown as ChatParticipant[]

function makeChat(overrides: Record<string, unknown> = {}): ChatMetadata {
  return {
    id: 'b1000000-0000-4000-8000-000000000001',
    title: 'The Test Salon',
    createdAt: '2026-01-01T00:00:00.000Z',
    scenarioText: null,
    timestampConfig: makeConfig(),
    participants: PARTICIPANTS,
    ...overrides,
  } as unknown as ChatMetadata
}

let messageCounter = 0
function msg(overrides: Record<string, unknown> = {}): MessageEvent {
  messageCounter += 1
  return {
    type: 'message',
    id: `d${String(messageCounter).padStart(7, '0')}-0000-4000-8000-000000000001`,
    role: 'ASSISTANT',
    content: 'Hello there.',
    createdAt: '2026-01-01T12:00:00.000Z',
    participantId: PARTICIPANT_ID,
    ...overrides,
  } as unknown as MessageEvent
}

const NAMES: Record<string, string> = {
  [CHAR_ID]: 'Aria',
  [USER_CHAR_ID]: 'Charlie',
  [OFFSCENE_ID]: 'The Stranger',
}

function pushTranscript(
  id: string,
  events: ChatEvent[],
  overrides: {
    chat?: ChatMetadata
    characterNames?: Record<string, string>
    userName?: string
    defaultTimestampConfig?: TimestampConfig | null
    chatSettingsTimezone?: string | null
  } = {},
) {
  const chat = overrides.chat ?? makeChat()
  const characterNames = overrides.characterNames ?? NAMES
  const userName = overrides.userName ?? 'Charlie'
  const defaultTimestampConfig = overrides.defaultTimestampConfig ?? null
  const chatSettingsTimezone = overrides.chatSettingsTimezone ?? null
  const input: MarkdownTranscriptInput = {
    chat,
    events,
    characterNamesById: new Map(Object.entries(characterNames)),
    userName,
    defaultTimestampConfig,
    chatSettingsTimezone,
  }
  const common = {
    kind: 'transcript' as const,
    id,
    chat,
    events,
    characterNames,
    userName,
    defaultTimestampConfig,
    chatSettingsTimezone,
    localOffsetMin: LOCAL_OFFSET_MIN,
  }
  try {
    rows.push({ ...common, out: buildMarkdownTranscript(input) })
  } catch {
    rows.push({ ...common, threw: true })
  }
}

// ---- the shape v4's own test suite pins --------------------------------------
pushTranscript(
  'title-scenario-and-two-speakers',
  [
    msg({ role: 'USER', participantId: USER_PARTICIPANT_ID, content: 'Good evening.' }),
    msg({ content: 'And to you.', createdAt: '2026-01-01T12:05:00.000Z' }),
  ],
  {
    chat: makeChat({
      scenarioText: '{{char}} waits for {{user}} on a rainy night at the observatory.',
    }),
  },
)

// Empty title / no scenario / no messages at all: the heading-only document.
pushTranscript('empty-chat-untitled', [], { chat: makeChat({ title: '' }) })
pushTranscript('empty-chat-null-title', [], { chat: makeChat({ title: null }) })

// A title carrying stray whitespace and newlines — headingSafe's whole job.
pushTranscript('title-whitespace-collapsed', [msg()], {
  chat: makeChat({ title: '  The\t Test \n\n Salon  ' }),
})

// ---- the inclusion filter ----------------------------------------------------
pushTranscript('inclusion-filter', [
  msg({ role: 'SYSTEM', content: 'raw system prompt text' }),
  msg({ role: 'TOOL', content: 'tool result payload' }),
  msg({ systemSender: 'lantern', participantId: null, content: 'The Lantern flickers.' }),
  msg({ systemSender: 'commonplaceBook', participantId: null, content: 'A memory resurfaces.' }),
  msg({
    systemSender: 'host',
    systemKind: 'timestamp',
    participantId: null,
    content: 'The Host marks the time as noon.',
  }),
  msg({
    systemSender: 'pascal',
    systemKind: 'custom-tool-result',
    participantId: null,
    content: 'Pascal rolls the dice: 17.',
  }),
  msg({
    systemSender: 'host',
    systemKind: 'continuation-from',
    participantId: null,
    content: 'This conversation continues from [Older Chat](/salon/x).',
  }),
  msg({
    systemSender: 'aurora',
    systemKind: 'announcement',
    participantId: null,
    content: 'Dinner is served.',
  }),
])

// Every Host link kind is IN; every other host kind is OUT. And a Staff sender
// with no systemKind at all is out (the `announcement` test is on the kind).
pushTranscript('host-link-kinds', [
  msg({ systemSender: 'host', systemKind: 'continuation-from', participantId: null, content: 'from' }),
  msg({ systemSender: 'host', systemKind: 'continuation-to', participantId: null, content: 'to' }),
  msg({ systemSender: 'host', systemKind: 'merge-from', participantId: null, content: 'merged from' }),
  msg({ systemSender: 'host', systemKind: 'merge-to', participantId: null, content: 'merged to' }),
  msg({ systemSender: 'host', systemKind: 'nudge', participantId: null, content: 'invited to speak' }),
  msg({ systemSender: 'host', participantId: null, content: 'a Host line with no kind' }),
  msg({ systemSender: 'librarian', participantId: null, content: 'a Librarian line with no kind' }),
])

// Every Staff display name, as an announcement — the whole STAFF_DISPLAY_NAMES
// table plus the raw-key fallback for a sender the table has never heard of.
pushTranscript(
  'staff-display-names',
  [
    'lantern',
    'aurora',
    'librarian',
    'concierge',
    'prospero',
    'host',
    'commonplaceBook',
    'ariel',
    'suparna',
    'pascal',
    'carina',
    'someUnknownStaff',
  ].map((sender) =>
    msg({
      systemSender: sender,
      systemKind: 'announcement',
      participantId: null,
      content: `${sender} speaks.`,
    }),
  ),
)

// ---- speaker resolution ------------------------------------------------------
pushTranscript('carina-answerers', [
  msg({
    systemSender: 'carina',
    systemKind: 'carina-response',
    participantId: null,
    carinaMeta: { answererId: CHAR_ID, question: 'What year is it?' },
    content: 'It is 1926.',
  }),
  msg({
    systemSender: 'carina',
    systemKind: 'carina-response',
    participantId: null,
    carinaMeta: { answererId: BRAHMA_CARINA_ANSWERER_ID, question: 'How many chats?' },
    content: 'There are 42 chats.',
  }),
  msg({
    systemSender: 'carina',
    systemKind: 'carina-response',
    participantId: null,
    carinaMeta: { answererId: BROKEN_ID, question: 'Who are you?' },
    content: 'An answerer whose vault is gone.',
  }),
  msg({
    systemSender: 'carina',
    systemKind: 'carina-response',
    participantId: null,
    content: 'No carinaMeta at all.',
  }),
])

pushTranscript('custom-announcers', [
  msg({
    participantId: null,
    systemKind: 'announcement',
    customAnnouncer: { kind: 'custom', displayName: 'The Narrator' },
    content: 'Night falls.',
  }),
  msg({
    participantId: null,
    systemKind: 'announcement',
    customAnnouncer: { kind: 'character', characterId: OFFSCENE_ID },
    content: 'A knock at the door.',
  }),
  msg({
    participantId: null,
    systemKind: 'announcement',
    customAnnouncer: { kind: 'character', characterId: BROKEN_ID },
    content: 'An off-scene character nobody can name.',
  }),
  msg({
    participantId: null,
    systemKind: 'announcement',
    customAnnouncer: { kind: 'custom', displayName: '' },
    content: 'An announcer with no name.',
  }),
  // A customAnnouncer voiced by a Staff member — the announcer still wins.
  msg({
    systemSender: 'aurora',
    participantId: null,
    systemKind: 'announcement',
    customAnnouncer: { kind: 'staff', displayName: 'Aurora' },
    content: 'Voiced by the Staff, named by the announcer.',
  }),
])

pushTranscript('participant-and-role-fallbacks', [
  msg({ participantId: PARTICIPANT_ID, content: 'The primary speaks.' }),
  msg({ participantId: BROKEN_PARTICIPANT_ID, content: 'A broken vault speaks.' }),
  msg({ participantId: 'a9999999-0000-4000-8000-000000000009', content: 'An unknown participant.' }),
  msg({ role: 'USER', participantId: null, content: 'An unattributed user line.' }),
  msg({ role: 'ASSISTANT', participantId: null, content: 'An unattributed assistant line.' }),
])

// v4's `participants.find(p => p.type === 'CHARACTER')` is vestigial —
// `ParticipantTypeEnum` has exactly one member, so a validated chat cannot hold
// anything else — but both sides carry the check, and only a participant list
// that starts with a non-CHARACTER entry makes it observable. The renderer does
// no runtime validation, so the corpus can hand it one.
pushTranscript(
  'primary-skips-non-character-participant',
  [msg({ participantId: null, content: 'Unattributed, so who am I?' })],
  {
    chat: makeChat({
      participants: [
        {
          id: 'a4000000-0000-4000-8000-000000000004',
          type: 'USER',
          characterId: OFFSCENE_ID,
          controlledBy: 'user',
        },
        { id: PARTICIPANT_ID, type: 'CHARACTER', characterId: CHAR_ID, controlledBy: 'llm' },
      ],
      scenarioText: '{{char}} waits for {{user}}.',
    }),
  },
)

// No CHARACTER participant at all → the 'Assistant' floor, and {{char}} renders
// empty in the scenario.
pushTranscript(
  'no-character-participant',
  [msg({ participantId: null, content: 'Who is speaking?' })],
  {
    chat: makeChat({
      participants: [],
      scenarioText: 'A scene with {{char}} and {{user}}.',
    }),
  },
)

// ---- whispers ----------------------------------------------------------------
pushTranscript('whispers', [
  msg({ targetParticipantIds: [USER_PARTICIPANT_ID], content: 'Between us only.' }),
  msg({ targetParticipantIds: [], content: 'An empty target list is not a whisper.' }),
  msg({ targetParticipantIds: null, content: 'A null target list is not a whisper.' }),
  msg({
    role: 'USER',
    participantId: USER_PARTICIPANT_ID,
    targetParticipantIds: [PARTICIPANT_ID, BROKEN_PARTICIPANT_ID],
    content: 'A whisper to two.',
  }),
])

// ---- swipe collapse ----------------------------------------------------------
pushTranscript('swipes-highest-index-at-group-position', [
  msg({ role: 'USER', participantId: USER_PARTICIPANT_ID, content: 'Tell me a story.', createdAt: '2026-01-01T12:00:00.000Z' }),
  msg({ swipeGroupId: 'g1', swipeIndex: 0, content: 'First draft.', createdAt: '2026-01-01T12:01:00.000Z' }),
  msg({ swipeGroupId: 'g1', swipeIndex: 1, content: 'Second draft.', createdAt: '2026-01-01T12:02:00.000Z' }),
  msg({ role: 'USER', participantId: USER_PARTICIPANT_ID, content: 'Go on.', createdAt: '2026-01-01T12:03:00.000Z' }),
])

// A tie on swipeIndex: strict `>` means the FIRST-seen wins. And a group whose
// highest index arrives before its lowest still emits at the group's first
// chronological position.
pushTranscript('swipes-ties-and-out-of-order', [
  msg({ swipeGroupId: 'g1', swipeIndex: 2, content: 'tie-first', createdAt: '2026-01-01T12:00:00.000Z' }),
  msg({ swipeGroupId: 'g1', swipeIndex: 2, content: 'tie-second', createdAt: '2026-01-01T12:01:00.000Z' }),
  msg({ swipeGroupId: 'g2', swipeIndex: 3, content: 'g2-high-first', createdAt: '2026-01-01T12:02:00.000Z' }),
  msg({ swipeGroupId: 'g2', swipeIndex: 0, content: 'g2-low-later', createdAt: '2026-01-01T12:03:00.000Z' }),
  // A missing swipeIndex reads as 0.
  msg({ swipeGroupId: 'g3', content: 'g3-no-index', createdAt: '2026-01-01T12:04:00.000Z' }),
  msg({ swipeGroupId: 'g3', swipeIndex: 1, content: 'g3-index-one', createdAt: '2026-01-01T12:05:00.000Z' }),
])

// A swipe group whose winning variant is EXCLUDED by the filter is still picked
// (collapse runs after the filter, so the excluded row never enters the group).
pushTranscript('swipes-with-a-filtered-variant', [
  msg({ swipeGroupId: 'g1', swipeIndex: 0, content: 'kept draft', createdAt: '2026-01-01T12:00:00.000Z' }),
  msg({ swipeGroupId: 'g1', swipeIndex: 1, role: 'SYSTEM', content: 'system draft', createdAt: '2026-01-01T12:01:00.000Z' }),
])

// ---- bodies ------------------------------------------------------------------
pushTranscript('bodies-and-trimming', [
  msg({ content: '   \n\n  Padded on both sides. \n \n' }),
  msg({ content: '   ' }),
  msg({ content: '' }),
  msg({ content: 'Line one\nLine two\n\nLine four' }),
])

// ---- clocks ------------------------------------------------------------------
pushTranscript(
  'fictional-anchored',
  [msg({ createdAt: '2026-01-01T00:45:00.000Z', content: 'The clock strikes.' })],
  {
    chat: makeChat({
      timestampConfig: makeConfig({
        useFictionalTime: true,
        fictionalBaseTimestamp: '1920-05-01T20:00',
        fictionalBaseRealTime: '2026-01-01T00:00:00.000Z',
      }),
    }),
  },
)
pushTranscript(
  'fictional-unanchored-uses-chat-creation',
  [msg({ createdAt: '2026-01-01T01:30:00.000Z', content: 'Later that night.' })],
  {
    chat: makeChat({
      createdAt: '2026-01-01T00:00:00.000Z',
      timestampConfig: makeConfig({
        useFictionalTime: true,
        fictionalBaseTimestamp: '1920-05-01T20:00',
        fictionalBaseRealTime: null,
      }),
    }),
  },
)
// A zone-less fictional base read in a named story zone, across several messages
// — the shape a real fictional chat has.
pushTranscript(
  'fictional-story-zone-multi-message',
  [
    msg({ createdAt: '2026-01-01T00:00:00.000Z', content: 'Curtain up.' }),
    msg({ createdAt: '2026-01-01T02:30:00.000Z', content: 'Two and a half hours on.' }),
    msg({ createdAt: '2026-01-02T09:15:00.000Z', content: 'The next morning.' }),
  ],
  {
    chat: makeChat({
      createdAt: '2026-01-01T00:00:00.000Z',
      timestampConfig: makeConfig({
        format: 'ISO8601',
        timezone: 'Europe/Istanbul',
        useFictionalTime: true,
        fictionalBaseTimestamp: '1550-07-25T10:15',
        fictionalBaseRealTime: null,
      }),
    }),
  },
)
for (const fmt of ['DATE_ONLY', 'TIME_ONLY'] as const) {
  pushTranscript(
    `promotes-${fmt}`,
    [msg({ createdAt: '2026-01-01T12:00:00.000Z' })],
    { chat: makeChat({ timestampConfig: makeConfig({ format: fmt }) }) },
  )
}
for (const fmt of ['ISO8601', 'FRIENDLY', 'CUSTOM'] as const) {
  pushTranscript(
    `format-${fmt}`,
    [msg({ createdAt: '2026-01-01T12:00:00.000Z' })],
    {
      chat: makeChat({
        timestampConfig: makeConfig({
          format: fmt,
          customFormat: fmt === 'CUSTOM' ? 'YYYY/MM/DD h:mm A' : null,
        }),
      }),
    },
  )
}
// The config fallback chain: chat → Salon default → FALLBACK_CONFIG. The
// FALLBACK arm has no timezone, so it also exercises the zone-less host path.
pushTranscript(
  'config-falls-to-salon-default',
  [msg({ createdAt: '2026-01-01T12:00:00.000Z' })],
  {
    chat: makeChat({ timestampConfig: null }),
    defaultTimestampConfig: makeConfig({ format: 'ISO8601' }),
  },
)
pushTranscript('config-falls-to-fallback', [msg({ createdAt: '2026-01-01T12:00:00.000Z' })], {
  chat: makeChat({ timestampConfig: null }),
})
// resolveTimezone's second link: the chat config carries no zone, the Salon does.
pushTranscript(
  'timezone-from-chat-settings',
  [msg({ createdAt: '2026-01-01T12:00:00.000Z' })],
  {
    chat: makeChat({ timestampConfig: makeConfig({ timezone: null }) }),
    chatSettingsTimezone: 'America/New_York',
  },
)
// A chat-level zone wins over the Salon's.
pushTranscript(
  'timezone-chat-wins',
  [msg({ createdAt: '2026-01-01T12:00:00.000Z' })],
  {
    chat: makeChat({ timestampConfig: makeConfig({ timezone: 'Asia/Kolkata' }) }),
    chatSettingsTimezone: 'America/New_York',
  },
)
// Messages either side of a DST boundary in the resolved zone.
pushTranscript(
  'dst-boundary-messages',
  [
    msg({ createdAt: '2026-03-08T06:59:00.000Z', content: 'Before the spring forward.' }),
    msg({ createdAt: '2026-03-08T07:00:00.000Z', content: 'After the spring forward.' }),
    msg({ createdAt: '2026-11-01T05:59:00.000Z', content: 'Before the fall back.' }),
    msg({ createdAt: '2026-11-01T06:00:00.000Z', content: 'After the fall back.' }),
  ],
  { chat: makeChat({ timestampConfig: makeConfig({ timezone: 'America/New_York' }) }) },
)
// An unresolvable zone: Intl throws, which the route answers as a 500.
pushTranscript('invalid-timezone-throws', [msg()], {
  chat: makeChat({ timestampConfig: makeConfig({ timezone: 'Not/AZone' }) }),
})

// ---- non-ASCII + template rendering -----------------------------------------
pushTranscript(
  'non-ascii-title-and-names',
  [
    msg({ content: 'Καλή σας μέρα.' }),
    msg({ role: 'USER', participantId: USER_PARTICIPANT_ID, content: 'Et à vous. 🎩' }),
  ],
  {
    chat: makeChat({
      title: 'Suparṇā’s Salon 🎩',
      scenarioText: '{{char}} greets {{user}} — {{unknownVariable}} — beneath the awning.',
    }),
    characterNames: { ...NAMES, [CHAR_ID]: 'Ariá' },
  },
)
// A scenario that trims to nothing is omitted entirely (no `## Scenario`).
pushTranscript('scenario-trims-to-empty', [msg()], {
  chat: makeChat({ scenarioText: '  \n {{unknownVariable}} \n ' }),
})
// The un-ported-in-v5 `{{trim}}` quirk still rides the same processor.
pushTranscript('scenario-trim-macro', [msg()], {
  chat: makeChat({ scenarioText: '{{trim}}\n{{char}} and {{user}}\n{{/trim}}' }),
})

// ---- events that are not messages -------------------------------------------
pushTranscript('non-message-events-ignored', [
  { type: 'context-summary', id: 'e1', content: 'a summary', createdAt: '2026-01-01T11:00:00.000Z' },
  { type: 'system', id: 'e2', content: 'a system event', createdAt: '2026-01-01T11:30:00.000Z' },
  msg({ content: 'the only message' }),
] as unknown as ChatEvent[])

// ---- transcriptFilename ------------------------------------------------------
const filenameCases: Array<[string, Record<string, unknown>]> = [
  ['plain', {}],
  ['hostile-characters', { title: 'What: a "chat"?/Really' }],
  ['backslash-and-pipe', { title: 'a\\b|c<d>e*f' }],
  ['control-characters', { title: 'tab\tnewline\nbell' }],
  ['empty-title', { title: '' }],
  ['null-title', { title: null }],
  ['whitespace-only-title', { title: '   ' }],
  ['sanitizes-to-whitespace', { title: ' / ' }],
  ['non-ascii', { title: 'Suparṇā’s Salon 🎩' }],
  ['leading-trailing-space', { title: '  padded  ' }],
]
for (const [id, overrides] of filenameCases) {
  const chat = makeChat(overrides)
  rows.push({ kind: 'filename', id, chat, out: transcriptFilename(chat) })
}

// ---- buildContentDisposition -------------------------------------------------
const dispositionCases: Array<[string, string, 'inline' | 'attachment' | null]> = [
  ['ascii-default-inline', 'The Test Salon_transcript.md', null],
  ['ascii-attachment', 'The Test Salon_transcript.md', 'attachment'],
  ['ascii-inline', 'photo.webp', 'inline'],
  ['latin1-attachment', 'Café_transcript.md', 'attachment'],
  // Astral characters are TWO UTF-16 code units, so the ASCII fallback gets two
  // underscores — the case a `chars()`-based port gets wrong.
  ['astral-attachment', 'Suparṇā’s Salon 🎩_transcript.md', 'attachment'],
  ['astral-inline', '🎩🎩.webp', 'inline'],
  // A STRAIGHT ASCII apostrophe beside a non-ASCII character. The astral case
  // above uses a CURLY apostrophe (U+2019), which is itself non-ASCII and so is
  // percent-encoded — which is exactly why this family never saw the bug that
  // dogfood #46 found on a real chat title. `encodeURIComponent` keeps a
  // straight `'`, and RFC 8187 uses it as the charset'lang'value delimiter, so
  // v4 emits an ungrammatical parameter that browsers discard. v5 DIVERGES here
  // and percent-encodes it; the Rust side asserts the difference explicitly.
  ['ascii-apostrophe-with-non-ascii', "Wings Over Suparṇā's Quiet Governance.md", 'attachment'],
  ['quote-in-ascii-name', 'a"b.md', 'attachment'],
  ['empty-name', '', 'attachment'],
]
for (const [id, filename, disposition] of dispositionCases) {
  const out =
    disposition === null
      ? buildContentDisposition(filename)
      : buildContentDisposition(filename, disposition)
  rows.push({ kind: 'disposition', id, filename, disposition, out })
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n')
