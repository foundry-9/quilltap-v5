/**
 * Oracle case (P4.D140, v4 `735d9408c` — bug 112): the chat-activity
 * chokepoint.
 *
 * Drives the REAL exports of v4's `lib/chat/chat-activity.ts`:
 * `isCharacterAuthoredMessage`, `CHARACTER_AUTHORED_MESSAGE_FILTER`,
 * `chatActivityAt`, `chatActivityTime` and `byChatActivityDesc`.
 *
 * The corpus is v4's own test table (`lib/chat/__tests__/chat-activity.test.ts`
 * — every Staff sender by name, the whisper/silent inclusions, the announcement
 * bubble, the TOOL/SYSTEM roles, the two non-message event types) PLUS the
 * edges v4's table does not state and this port must not guess:
 *   - `systemSender: ''` — the truthiness-vs-`IS NULL` seam between the
 *     in-memory predicate and the SQL mirror (v4 ships both spellings
 *     knowingly); measured here rather than assumed.
 *   - `customAnnouncer: {}` / `null` — `{}` is truthy in JS, so an announcement
 *     bubble with no fields is still an announcement.
 *   - a missing `role` / a lowercase `'user'`.
 *   - `lastMessageAt: ''` — `??` is nullish, so the empty string WINS over
 *     `createdAt` and then reads as time 0.
 *   - `byChatActivityDesc` over a list mixing stamped and never-stamped chats
 *     (the sort v5's Salon list, home, projects and Brahma console all share).
 *
 * The SQL mirror is emitted as v4's own filter object; the Rust side asserts
 * its WHERE-fragment spelling encodes exactly those four clauses (the two
 * spellings are prose in v4 and SQL in v5, so they cannot be byte-diffed).
 *
 * Run from inside the server checkout (pinned worktree — ledger §5.1):
 *   cd /tmp/qt-v4-pin-p4d140-735d9408c
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-activity.ts \
 *     > /tmp/oracle-chat-activity.ndjson
 */

import {
  isCharacterAuthoredMessage,
  CHARACTER_AUTHORED_MESSAGE_FILTER,
  chatActivityAt,
  chatActivityTime,
  byChatActivityDesc,
} from '@/lib/chat/chat-activity'
import type { ChatEvent } from '@/lib/schemas/types'

type Row =
  | { kind: 'filter'; id: string; filter: unknown }
  | { kind: 'predicate'; id: string; event: Record<string, unknown>; out: boolean }
  | {
      kind: 'activity'
      id: string
      chat: Record<string, unknown>
      at: string
      time: number
    }
  | { kind: 'sort'; id: string; chats: Record<string, unknown>[]; order: string[] }

const rows: Row[] = []

rows.push({
  kind: 'filter',
  id: 'sql-mirror',
  filter: CHARACTER_AUTHORED_MESSAGE_FILTER,
})

/** A minimal character-authored message; overrides carve out each edge case. */
function msg(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    type: 'message',
    id: '00000000-0000-4000-8000-000000000001',
    role: 'ASSISTANT',
    content: 'hello',
    createdAt: '2026-01-01T00:00:00.000Z',
    ...overrides,
  }
}

const predicate = (id: string, event: Record<string, unknown>) => {
  rows.push({
    kind: 'predicate',
    id,
    event,
    out: isCharacterAuthoredMessage(event as unknown as ChatEvent),
  })
}

// --- v4's table: counts as activity ----------------------------------------
predicate('assistant-speaking', msg({ role: 'ASSISTANT' }))
predicate('user-speaking', msg({ role: 'USER' }))
predicate('whisper', msg({ targetParticipantIds: ['participant-1'] }))
predicate('silent-message', msg({ isSilentMessage: true }))

// --- v4's table: does not count --------------------------------------------
for (const sender of [
  'lantern',
  'aurora',
  'librarian',
  'concierge',
  'prospero',
  'host',
  'commonplaceBook',
  'ariel',
  'carina',
  'suparna',
  'pascal',
]) {
  predicate(`staff-${sender}`, msg({ systemSender: sender }))
}
predicate(
  'announcement-bubble',
  msg({ role: 'USER', customAnnouncer: { kind: 'custom', displayName: 'The Narrator' } })
)
predicate('tool-role', msg({ role: 'TOOL' }))
predicate('system-role', msg({ role: 'SYSTEM' }))
predicate('context-summary-event', msg({ type: 'context-summary' }))
predicate('system-event', msg({ type: 'system' }))

// --- the edges v4's table does not state -----------------------------------
predicate('system-sender-empty-string', msg({ systemSender: '' }))
predicate('system-sender-null', msg({ systemSender: null }))
predicate('custom-announcer-empty-object', msg({ customAnnouncer: {} }))
predicate('custom-announcer-null', msg({ customAnnouncer: null }))
predicate('empty-whisper-array', msg({ targetParticipantIds: [] }))
predicate('role-lowercase-user', msg({ role: 'user' }))
predicate('role-missing', { type: 'message', id: 'x', content: 'hi', createdAt: '2026-01-01T00:00:00.000Z' })
predicate('type-missing', { id: 'x', role: 'USER', content: 'hi', createdAt: '2026-01-01T00:00:00.000Z' })
predicate('staff-whisper', msg({ systemSender: 'commonplaceBook', targetParticipantIds: ['p1'] }))

// --- chatActivityAt / chatActivityTime -------------------------------------
const activity = (id: string, chat: Record<string, unknown>) => {
  rows.push({
    kind: 'activity',
    id,
    chat,
    at: chatActivityAt(chat as never),
    time: chatActivityTime(chat as never),
  })
}

activity('stamped', {
  lastMessageAt: '2026-05-01T00:00:00.000Z',
  createdAt: '2026-01-01T00:00:00.000Z',
})
activity('null-last-message-at', {
  lastMessageAt: null,
  createdAt: '2026-01-01T00:00:00.000Z',
})
activity('absent-last-message-at', { createdAt: '2026-01-01T00:00:00.000Z' })
activity('unparseable-both', { lastMessageAt: 'not a date', createdAt: 'also not a date' })
// `??` is NULLISH: an empty string wins over createdAt, then reads as time 0.
activity('empty-string-last-message-at', {
  lastMessageAt: '',
  createdAt: '2026-01-01T00:00:00.000Z',
})
// `updatedAt` is never consulted — the whole point of the module.
activity('updated-at-is-ignored', {
  lastMessageAt: null,
  createdAt: '2024-01-01T00:00:00.000Z',
  updatedAt: '2026-06-01T00:00:00.000Z',
})
activity('millisecond-precision', {
  lastMessageAt: '2026-05-01T12:34:56.789Z',
  createdAt: '2026-01-01T00:00:00.000Z',
})

// --- byChatActivityDesc ----------------------------------------------------
const sort = (id: string, chats: Record<string, unknown>[]) => {
  rows.push({
    kind: 'sort',
    id,
    chats,
    order: [...chats].sort(byChatActivityDesc as never).map((c) => c.id as string),
  })
}

sort('loud-before-quiet', [
  { id: 'quiet', lastMessageAt: '2024-01-01T00:00:00.000Z', createdAt: '2023-01-01T00:00:00.000Z' },
  { id: 'loud', lastMessageAt: '2026-01-01T00:00:00.000Z', createdAt: '2023-01-01T00:00:00.000Z' },
])
sort('never-spoken-order-by-creation', [
  { id: 'older', lastMessageAt: null, createdAt: '2024-01-01T00:00:00.000Z' },
  { id: 'newer', lastMessageAt: null, createdAt: '2025-01-01T00:00:00.000Z' },
])
// The mix the Salon list actually sees: a Staff-only chat (now NULL) sinking
// below a stamped one, and `updatedAt` unable to rescue it.
sort('mixed-stamped-and-never', [
  { id: 'staff-only', lastMessageAt: null, createdAt: '2023-01-01T00:00:00.000Z', updatedAt: '2026-08-01T00:00:00.000Z' },
  { id: 'spoken-recently', lastMessageAt: '2026-07-01T00:00:00.000Z', createdAt: '2020-01-01T00:00:00.000Z' },
  { id: 'spoken-long-ago', lastMessageAt: '2024-06-01T00:00:00.000Z', createdAt: '2024-01-01T00:00:00.000Z' },
])
// Ties resolve by sort stability only.
sort('ties-keep-input-order', [
  { id: 'first', lastMessageAt: '2026-01-01T00:00:00.000Z', createdAt: '2020-01-01T00:00:00.000Z' },
  { id: 'second', lastMessageAt: '2026-01-01T00:00:00.000Z', createdAt: '2020-01-01T00:00:00.000Z' },
  { id: 'third', lastMessageAt: '2026-01-01T00:00:00.000Z', createdAt: '2020-01-01T00:00:00.000Z' },
])
// Unparseable timestamps sort as 0 — total order, no NaN.
sort('unparseable-sorts-last', [
  { id: 'broken', lastMessageAt: 'not a date', createdAt: 'nor this' },
  { id: 'fine', lastMessageAt: '2025-01-01T00:00:00.000Z', createdAt: '2020-01-01T00:00:00.000Z' },
])

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n')
