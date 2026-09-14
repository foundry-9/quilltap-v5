/**
 * The v4-side recorder behind `src/app/chat/transcript-reconcile.oracle.spec.ts`.
 *
 * `reconcileTranscript` (`app/salon/[id]/hooks/transcript-reconcile.ts`, added by
 * `5029075bb`) is pure and import-free of React, so it records straight from a
 * pinned worktree. v5's `chat/transcript-reconcile.ts` is a character-faithful
 * transcription and THIS corpus is what proves it.
 *
 * What each row records is the module's three stated properties, in the only
 * form a differential can compare:
 *
 *   - `ids` — what to render, in order (the read is the authority).
 *   - `reusedFrom` — for each output row, the index in `previous` whose OBJECT
 *     it is (`===`), else null. This is object identity made observable; a port
 *     that rebuilt every row would return the right ids and the wrong map.
 *   - `messagesIsPrevious` / `swipeStatesIsPrevious` — whether the very input
 *     object came back. The "nothing changed, so nothing re-renders" property.
 *   - `swipeStates` — group set, selection, totals, variant ids in order.
 *   - `provisionalFlags` — `isProvisionalMessage` over `previous`.
 *
 * The inputs are JSON round-tripped BEFORE v4 sees them, so the oracle and the
 * SPA spec operate on structurally identical, non-shared objects: no corpus row
 * can accidentally pass because two array slots happened to share a reference
 * in the recorder but not after `JSON.parse`.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/app/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Run it from a pinned v4 worktree (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-p4d187-31436bae4
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" 31436bae4
 * ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
 * cp <V5>/apps/web/oracle/transcript-reconcile.recorder.ts "$PIN/"
 * cd "$PIN" && npx tsx transcript-reconcile.recorder.ts \
 *   > <V5>/apps/web/src/testing/fixtures/transcript-reconcile.ndjson
 * ```
 *
 * Expect 38 lines; a shorter file means the recorder errored and the redirect
 * already truncated the old one (the empty-file trap).
 */

import {
  isProvisionalMessage,
  reconcileTranscript,
} from '@/app/salon/[id]/hooks/transcript-reconcile'
import type { SwipeState } from '@/app/salon/[id]/hooks/useChatData'
import type { Message } from '@/app/salon/[id]/types'

// The incident chat's own stamps: 41 ms apart, then 34 s later.
const T0 = '2026-09-11T12:49:25.818Z'
const T1 = '2026-09-11T12:49:25.859Z'
const T2 = '2026-09-11T12:49:59.812Z'

function row(overrides: Partial<Message> & { id: string }): Message {
  return {
    role: 'ASSISTANT',
    content: 'hello',
    createdAt: T0,
    ...overrides,
  } as Message
}

/** `createdAt` at a fixed offset from T0, in ms — for the clock-slack edges. */
function at(offsetMs: number): string {
  return new Date(new Date(T0).getTime() + offsetMs).toISOString()
}

interface Scenario {
  id: string
  rows: Message[]
  previous?: Message[]
  previousSwipeStates?: Record<string, SwipeState>
}

const variants = [
  row({ id: 'v0', swipeGroupId: 'g', swipeIndex: 0, content: 'first take' }),
  row({ id: 'v1', swipeGroupId: 'g', swipeIndex: 1, content: 'second take' }),
]
const variantsPlus = [
  ...variants,
  row({ id: 'v2', swipeGroupId: 'g', swipeIndex: 2, content: 'third take' }),
]
/** The swipe map a fresh reconcile of `variants` produces, with the operator swiped back to v0. */
const swipedToV0: Record<string, SwipeState> = {
  g: { current: 0, total: 2, messages: variants },
}

const provisional = row({
  id: 'temp-user-1757594965818',
  role: 'USER',
  content: 'Tell me about the orchard.',
  createdAt: T0,
})

const SCENARIOS: Scenario[] = [
  // --- v4's suite: the authoritative read (7) ------------------------------
  {
    id: 'drops SYSTEM rows, which are prompt plumbing and never bubbles',
    rows: [row({ id: 'sys', role: 'SYSTEM', content: 'you are…' }), row({ id: 'a', role: 'USER', content: 'hi' })],
  },
  {
    id: 'orders by createdAt, breaking ties on the server’s own order',
    rows: [row({ id: 'first', createdAt: T1 }), row({ id: 'second', createdAt: T1 }), row({ id: 'earlier', createdAt: T0 })],
  },
  {
    id: 'returns the very same array when nothing changed, so nothing re-renders',
    rows: [row({ id: 'a' }), row({ id: 'b', createdAt: T1 })],
    previous: [row({ id: 'a' }), row({ id: 'b', createdAt: T1 })],
  },
  {
    id: 'reuses the previous object for a row that did not change',
    rows: [row({ id: 'a' }), row({ id: 'b', createdAt: T1, content: 'edited' })],
    previous: [row({ id: 'a' }), row({ id: 'b', createdAt: T1 })],
  },
  {
    id: 'takes a row that changed from the read, not from the display',
    rows: [row({ id: 'a', content: 'fixed' })],
    previous: [row({ id: 'a', content: 'typo' })],
  },
  {
    id: 'surfaces a row the display has never seen — the incident in one line',
    rows: [
      row({ id: 'user', role: 'USER', content: 'go on', createdAt: T0 }),
      row({ id: 'abigail', content: 'As you like.', createdAt: T2 }),
    ],
    previous: [row({ id: 'user', role: 'USER', content: 'go on', createdAt: T0 })],
  },
  {
    id: 'removes a row the read no longer carries (a swept whisper)',
    rows: [row({ id: 'a' })],
    previous: [row({ id: 'a' }), row({ id: 'whisper', createdAt: T1 })],
  },

  // --- v4's suite: swipe selection (4) -------------------------------------
  { id: 'defaults a group to its newest variant', rows: variants },
  {
    id: 'carries the operator’s selection across a refetch',
    rows: variants,
    previous: [variants[0]],
    previousSwipeStates: swipedToV0,
  },
  {
    id: 'keeps the selection on the same variant when a regenerate appends another',
    rows: variantsPlus,
    previous: [variants[0]],
    previousSwipeStates: swipedToV0,
  },
  {
    id: 'falls back to the newest variant when the selected one is gone',
    rows: [variants[1]],
    previous: [variants[0]],
    previousSwipeStates: swipedToV0,
  },

  // --- v4's suite: provisional bubbles (7, incl. the id predicate) ---------
  {
    id: 'recognises a provisional id',
    rows: [],
    previous: [provisional, row({ id: 'abigail' })],
  },
  { id: 'keeps the bubble while the read has not caught up', rows: [], previous: [provisional] },
  {
    id: 'drops the bubble the moment the read carries the same line',
    rows: [row({ id: 'real-user', role: 'USER', content: 'Tell me about the orchard.', createdAt: T1 })],
    previous: [provisional],
  },
  {
    id: 'drops a bubble whose persisted row reads differently — an attachment send',
    rows: [row({ id: 'real-user', role: 'USER', content: 'Please look at the attached file(s).', createdAt: T1 })],
    previous: [row({ id: 'temp-user-2', role: 'USER', content: '[Attached: plan.png]', createdAt: T0 })],
  },
  {
    id: 'does not let one persisted row absorb two bubbles',
    rows: [row({ id: 'real-a', role: 'USER', content: 'again', createdAt: T1 })],
    previous: [
      row({ id: 'temp-user-a', role: 'USER', content: 'again', createdAt: T0 }),
      row({ id: 'temp-user-b', role: 'USER', content: 'again', createdAt: T1 }),
    ],
  },
  {
    id: 'does not match a bubble against a row the display already had',
    rows: [row({ id: 'old-user', role: 'USER', content: 'earlier line', createdAt: T0 })],
    previous: [
      row({ id: 'old-user', role: 'USER', content: 'earlier line', createdAt: T0 }),
      row({ id: 'temp-user-c', role: 'USER', content: 'new line', createdAt: T1 }),
    ],
  },
  {
    id: 'renders the bubble last — it is always the newest thing in the room',
    rows: [row({ id: 'abigail', createdAt: T2 })],
    previous: [row({ id: 'abigail', createdAt: T2 }), provisional],
  },

  // --- the order's named extras -------------------------------------------
  {
    // A batch written in one call shares a timestamp outright: three rows, one
    // millisecond, server order is the only thing that can order them.
    id: 'a tie at the millisecond with a batch of three',
    rows: [
      row({ id: 'batch-c', createdAt: T1, content: 'third written' }),
      row({ id: 'batch-a', createdAt: T1, content: 'first written' }),
      row({ id: 'batch-b', createdAt: T1, content: 'second written' }),
    ],
  },
  {
    id: 'pass 2 matches a bubble at exactly 60000 ms of drift',
    rows: [row({ id: 'real-slack', role: 'USER', content: 'stored differently', createdAt: at(60_000) })],
    previous: [row({ id: 'temp-slack', role: 'USER', content: 'typed', createdAt: T0 })],
  },
  {
    id: 'pass 2 refuses a bubble at 60001 ms of drift',
    rows: [row({ id: 'real-slack', role: 'USER', content: 'stored differently', createdAt: at(60_001) })],
    previous: [row({ id: 'temp-slack', role: 'USER', content: 'typed', createdAt: T0 })],
  },
  {
    // The lower bound: a row stamped BEFORE the bubble still answers it (a slow
    // POST), but only within the same slack — this one is inside.
    id: 'pass 2 matches a row OLDER than the bubble, inside the slack',
    rows: [row({ id: 'real-early', role: 'USER', content: 'stored differently', createdAt: at(-59_000) })],
    previous: [row({ id: 'temp-late', role: 'USER', content: 'typed', createdAt: T0 })],
  },
  {
    // ...and this one is outside it. An unbounded lower edge would match here.
    id: 'pass 2 refuses a row older than the bubble by more than the slack',
    rows: [row({ id: 'real-early', role: 'USER', content: 'stored differently', createdAt: at(-60_001) })],
    previous: [row({ id: 'temp-late', role: 'USER', content: 'typed', createdAt: T0 })],
  },
  {
    // Two bubbles, one row, DIFFERENT text: pass 2 claims for the first bubble
    // only, and the second survives.
    id: 'two bubbles and one row — pass 2 claims once and the second survives',
    rows: [row({ id: 'real-one', role: 'USER', content: 'stored', createdAt: T1 })],
    previous: [
      row({ id: 'temp-one', role: 'USER', content: 'typed one', createdAt: T0 }),
      row({ id: 'temp-two', role: 'USER', content: 'typed two', createdAt: T1 }),
    ],
  },
  {
    // The two-tabs row. Bubble A can ONLY be answered by the far row (content
    // match, which pass 1 does not time-bound); bubble B can only be answered
    // by the near one. Running pass 2 first lets A eat the near row and strands B.
    id: 'pass 1 completes over every bubble before pass 2 begins',
    rows: [
      row({ id: 'real-near', role: 'USER', content: 'quite other prose', createdAt: T0 }),
      row({ id: 'real-far', role: 'USER', content: 'exact text', createdAt: at(200_000) }),
    ],
    previous: [
      row({ id: 'temp-exact', role: 'USER', content: 'exact text', createdAt: T0 }),
      row({ id: 'temp-other', role: 'USER', content: 'shown differently', createdAt: T0 }),
    ],
  },
  {
    id: 'a swipe group whose selected variant vanished, with siblings left',
    rows: [variantsPlus[0], variantsPlus[2]],
    previous: [variantsPlus[1]],
    previousSwipeStates: { g: { current: 1, total: 3, messages: variantsPlus } },
  },
  {
    id: 'a SYSTEM row mixed in among a swipe group and plain rows',
    rows: [
      row({ id: 'plain', role: 'USER', content: 'hi', createdAt: T0 }),
      row({ id: 'sys-1', role: 'SYSTEM', content: 'plumbing', createdAt: T1 }),
      variants[0],
      variants[1],
      row({ id: 'sys-2', role: 'SYSTEM', content: 'more plumbing', createdAt: T2 }),
    ],
  },

  // --- further edges ------------------------------------------------------
  { id: 'the mount case — nothing read, nothing shown', rows: [] },
  {
    id: 'an empty read leaves a lone bubble standing',
    rows: [],
    previous: [provisional],
  },
  {
    id: 'the swipe map object survives a read that moved nothing',
    rows: variants,
    previous: [variants[1]],
    previousSwipeStates: { g: { current: 1, total: 2, messages: variants } },
  },
  {
    id: 'the swipe map is rebuilt when a variant is appended',
    rows: variantsPlus,
    previous: [variants[1]],
    previousSwipeStates: { g: { current: 1, total: 2, messages: variants } },
  },
  {
    id: 'a bubble is never answered by a row of another role',
    rows: [row({ id: 'assistant-reply', role: 'ASSISTANT', content: 'Tell me about the orchard.', createdAt: T1 })],
    previous: [provisional],
  },
  {
    id: 'a swipe group and a provisional bubble in one read',
    rows: [...variants, row({ id: 'real-user', role: 'USER', content: 'nothing alike', createdAt: T2 })],
    previous: [variants[1], row({ id: 'temp-user-d', role: 'USER', content: 'typed', createdAt: T2 })],
    previousSwipeStates: { g: { current: 1, total: 2, messages: variants } },
  },
  {
    // `current` past the end of the remembered variant list: the guard reads it
    // as "no selection" and the group falls back to newest.
    id: 'a remembered selection index out of range falls back to newest',
    rows: variantsPlus,
    previous: [variants[0]],
    previousSwipeStates: { g: { current: 7, total: 2, messages: variants } },
  },
  {
    id: 'server order breaks a tie the read did not present in order',
    rows: [
      row({ id: 'late-but-first', createdAt: T2 }),
      row({ id: 'tie-a', createdAt: T1 }),
      row({ id: 'tie-b', createdAt: T1 }),
    ],
  },
  {
    // The id-carry discriminator. The operator is on v1; the read has DELETED
    // v0, so v1's INDEX moves from 1 to 0. Carrying the selection by index
    // would yank the view onto v2 — the very "don't move me onto a different
    // reply" the module's doc comment promises.
    id: 'a delete above the selected variant does not move the selection',
    rows: [variantsPlus[1], variantsPlus[2]],
    previous: [variantsPlus[1]],
    previousSwipeStates: { g: { current: 1, total: 3, messages: variantsPlus } },
  },
  {
    // The server-order tiebreak discriminator. `collapseSwipeGroups` emits
    // plain rows first and grouped ones after, so display order and server
    // order DISAGREE; with both rows on the same millisecond only the tiebreak
    // can restore the read's own order. A stable sort with no tiebreak leaves
    // the plain row first.
    id: 'the server-order tiebreak outranks the collapse order on an exact tie',
    rows: [
      row({ id: 'grouped', swipeGroupId: 'h', swipeIndex: 0, content: 'grouped take', createdAt: T1 }),
      row({ id: 'plain', role: 'USER', content: 'plain line', createdAt: T1 }),
    ],
  },
  {
    id: 'pass 1 compares trimmed content',
    rows: [row({ id: 'real-trim', role: 'USER', content: 'a padded line', createdAt: at(200_000) })],
    previous: [row({ id: 'temp-trim', role: 'USER', content: '   a padded line   ', createdAt: T0 })],
  },
]

const seen = new Set<string>()
for (const scenario of SCENARIOS) {
  if (seen.has(scenario.id)) throw new Error(`duplicate scenario id: ${scenario.id}`)
  seen.add(scenario.id)

  // Round-trip the inputs so the oracle sees exactly what the SPA spec will.
  const input = JSON.parse(
    JSON.stringify({
      rows: scenario.rows,
      previous: scenario.previous ?? [],
      previousSwipeStates: scenario.previousSwipeStates ?? {},
    }),
  ) as { rows: Message[]; previous: Message[]; previousSwipeStates: Record<string, SwipeState> }

  const out = reconcileTranscript(input.rows, input.previous, input.previousSwipeStates)

  const swipeStates: Record<string, { current: number; total: number; messageIds: string[] }> = {}
  for (const [groupId, state] of Object.entries(out.swipeStates)) {
    swipeStates[groupId] = {
      current: state.current,
      total: state.total,
      messageIds: state.messages.map((m) => m.id),
    }
  }

  console.log(
    JSON.stringify({
      id: scenario.id,
      in: input,
      out: {
        ids: out.messages.map((m) => m.id),
        reusedFrom: out.messages.map((m) => {
          const index = input.previous.indexOf(m)
          return index >= 0 ? index : null
        }),
        messagesIsPrevious: out.messages === input.previous,
        swipeStatesIsPrevious: out.swipeStates === input.previousSwipeStates,
        swipeStates,
        provisionalFlags: input.previous.map((m) => isProvisionalMessage(m)),
      },
    }),
  )
}
