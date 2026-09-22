/**
 * @jest-environment node
 *
 * P4.D205 — the Inform ROUTE-SURFACE oracle (v4 `e7d77bb60`).
 *
 * Drives v4's REAL `handleInform` / `handleGetInforms` / `handleCancelInform`
 * from `app/api/v1/chats/[id]/actions/inform.ts`, mocking exactly what v4's own
 * test mocks (the repos, the announcer writer, the audience resolver, the
 * realtime bus) — so the handler logic under test is v4's, unmodified.
 *
 * Emits, per case: the HTTP status, the response body, and the CALLS the
 * handler made to the mocked collaborators (which target ids reached
 * `createBatch`, whether the record was written and with what audience, whether
 * the record message was deleted, whether the realtime hint was published).
 * The calls are the discriminator the body cannot be: the coverage rule decides
 * `recordTargets`, and only the record-write call shows it.
 *
 * Run (Node 24, from a TARGET-pinned v4 worktree — cp to a /tmp mirror, because
 * jest ignores `.claude/` paths):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-informs-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/chat-informs-routes.test.ts" "$TMPO/cases/"
 *   cd /tmp/qt-v4-pin-p4d205-f45a517a9
 *   QT_ORACLE_OUT=/tmp/oracle-chat-informs-routes.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "chat-informs-routes\.test\.ts$"
 */

import * as fs from 'fs'

jest.mock('@/lib/logger', () => ({
  logger: { error: jest.fn(), warn: jest.fn(), info: jest.fn(), debug: jest.fn() },
}))

jest.mock('@/lib/services/announcer/writer', () => ({
  postInformRecord: jest.fn(),
}))

jest.mock('@/lib/services/announcer/audience', () => ({
  resolveAnnouncementAudience: jest.fn(),
}))

jest.mock('@/lib/realtime/bus', () => ({
  publishRealtime: jest.fn(),
}))

const {
  handleInform,
  handleGetInforms,
  handleCancelInform,
} = require('@/app/api/v1/chats/[id]/actions/inform')
const { postInformRecord } = require('@/lib/services/announcer/writer')
const { resolveAnnouncementAudience } = require('@/lib/services/announcer/audience')
const { publishRealtime } = require('@/lib/realtime/bus')

const CHAT_ID = '3f1c9f4a-1111-4a2b-9c3d-000000000001'
const OTHER_CHAT = '3f1c9f4a-1111-4a2b-9c3d-000000000002'
const ALICE = '3f1c9f4a-1111-4a2b-9c3d-00000000000a'
const BOB = '3f1c9f4a-1111-4a2b-9c3d-00000000000b'
const OPERATOR = '3f1c9f4a-1111-4a2b-9c3d-00000000000c'
const DEPARTED = '3f1c9f4a-1111-4a2b-9c3d-00000000000d'
const BATCH_ID = '3f1c9f4a-1111-4a2b-9c3d-00000000000e'
const RECORD_ID = '3f1c9f4a-1111-4a2b-9c3d-00000000000f'
const STRANGER = '3f1c9f4a-1111-4a2b-9c3d-000000000099'

const BODY = 'You notice the clock has stopped.'

const outLines: string[] = []

function makeRequest(body: unknown): any {
  return { json: async () => body }
}

/** The room: two LLM seats (one silent), one user seat, one departed seat. */
function makeChat(overrides: Record<string, unknown> = {}) {
  return {
    id: CHAT_ID,
    participants: [
      { id: ALICE, type: 'CHARACTER', controlledBy: 'llm', status: 'active', removedAt: null },
      { id: BOB, type: 'CHARACTER', controlledBy: 'llm', status: 'silent', removedAt: null },
      { id: OPERATOR, type: 'CHARACTER', controlledBy: 'user', status: 'active', removedAt: null },
      {
        id: DEPARTED,
        type: 'CHARACTER',
        controlledBy: 'llm',
        status: 'removed',
        removedAt: '2026-01-01T00:00:00.000Z',
      },
    ],
    ...overrides,
  }
}

function makeRow(overrides: Record<string, unknown> = {}) {
  return {
    id: 'row-1',
    chatId: CHAT_ID,
    batchId: BATCH_ID,
    participantId: ALICE,
    contentMarkdown: BODY,
    recordMessageId: RECORD_ID,
    createdAt: '2026-01-01T21:14:00.000Z',
    updatedAt: '2026-01-01T21:14:00.000Z',
    consumedAt: null,
    consumedByMessageId: null,
    ...overrides,
  }
}

/** Read a `NextResponse` (or a plain `NextResponse.json`) into {status, body}. */
async function read(res: any): Promise<{ status: number; body: unknown }> {
  return { status: res.status, body: await res.json() }
}

function emit(label: string, payload: Record<string, unknown>): void {
  outLines.push(JSON.stringify({ case: 'chat-informs-routes', label, ...payload }))
}

describe('chats [id] inform actions — oracle', () => {
  let ctx: any

  beforeEach(() => {
    jest.clearAllMocks()
    postInformRecord.mockResolvedValue({ id: RECORD_ID, type: 'message' })
    resolveAnnouncementAudience.mockImplementation(
      async (_chatId: string, requested: string[] | null) => ({
        targetParticipantIds:
          requested && requested.length > 0 ? [...new Set(requested)] : null,
        targetNames: [],
        unknownIds: [],
      }),
    )
    ctx = {
      user: { id: 'user-1' },
      repos: {
        chats: {
          findById: jest.fn().mockResolvedValue(makeChat()),
          deleteMessagesByIds: jest.fn().mockResolvedValue(1),
        },
        chatInforms: {
          createBatch: jest
            .fn()
            .mockImplementation(async ({ participantIds }: any) =>
              participantIds.map((participantId: string, i: number) =>
                makeRow({ id: `row-${i}`, participantId }),
              ),
            ),
          findPendingBatches: jest.fn().mockResolvedValue([]),
          findByBatchId: jest.fn().mockResolvedValue([]),
          deletePendingByBatch: jest.fn().mockResolvedValue(0),
        },
      },
    }
  })

  /** The collaborator calls that the body cannot show. */
  function calls() {
    return {
      recordWritten: postInformRecord.mock.calls.length > 0,
      recordTargets:
        postInformRecord.mock.calls.length > 0
          ? (postInformRecord.mock.calls[0][0].targetParticipantIds ?? null)
          : null,
      recordBody:
        postInformRecord.mock.calls.length > 0
          ? postInformRecord.mock.calls[0][0].contentMarkdown
          : null,
      createBatchTargets:
        ctx.repos.chatInforms.createBatch.mock.calls.length > 0
          ? ctx.repos.chatInforms.createBatch.mock.calls[0][0].participantIds
          : null,
      createBatchBody:
        ctx.repos.chatInforms.createBatch.mock.calls.length > 0
          ? ctx.repos.chatInforms.createBatch.mock.calls[0][0].contentMarkdown
          : null,
      createBatchRecordId:
        ctx.repos.chatInforms.createBatch.mock.calls.length > 0
          ? (ctx.repos.chatInforms.createBatch.mock.calls[0][0].recordMessageId ?? null)
          : null,
      deleteMessagesCalled: ctx.repos.chats.deleteMessagesByIds.mock.calls.length > 0,
      deletedMessageIds:
        ctx.repos.chats.deleteMessagesByIds.mock.calls.length > 0
          ? ctx.repos.chats.deleteMessagesByIds.mock.calls[0][1]
          : null,
      realtimePublished: publishRealtime.mock.calls.length > 0,
    }
  }

  // ---- POST ?action=inform -------------------------------------------------

  it('404s when the chat is gone', async () => {
    ctx.repos.chats.findById.mockResolvedValue(null)
    const res = await handleInform(
      makeRequest({ contentMarkdown: BODY, targetParticipantIds: null }),
      CHAT_ID,
      ctx,
    )
    emit('inform: 404s when the chat is gone', { ...(await read(res)), calls: calls() })
  })

  it('null targets reach every eligible seat, silent ones included, and post a public record', async () => {
    const res = await handleInform(
      makeRequest({ contentMarkdown: BODY, targetParticipantIds: null }),
      CHAT_ID,
      ctx,
    )
    emit('inform: null targets reach every eligible seat, public record', {
      ...(await read(res)),
      calls: calls(),
    })
  })

  it('an explicit list covering every eligible seat still posts a PUBLIC record', async () => {
    const res = await handleInform(
      makeRequest({ contentMarkdown: BODY, targetParticipantIds: [ALICE, BOB] }),
      CHAT_ID,
      ctx,
    )
    emit('inform: a FULL explicit list is still PUBLIC', {
      ...(await read(res)),
      calls: calls(),
    })
  })

  it('a subset whispers the record to just those seats', async () => {
    const res = await handleInform(
      makeRequest({ contentMarkdown: BODY, targetParticipantIds: [ALICE] }),
      CHAT_ID,
      ctx,
    )
    emit('inform: a subset whispers', { ...(await read(res)), calls: calls() })
  })

  it('400s on an id that is not a participant of this chat', async () => {
    resolveAnnouncementAudience.mockResolvedValue({
      targetParticipantIds: null,
      targetNames: [],
      unknownIds: [STRANGER],
    })
    const res = await handleInform(
      makeRequest({ contentMarkdown: BODY, targetParticipantIds: [STRANGER] }),
      CHAT_ID,
      ctx,
    )
    emit('inform: 400 on an unknown target', { ...(await read(res)), calls: calls() })
  })

  it('400s on a user-controlled seat, naming it', async () => {
    const res = await handleInform(
      makeRequest({ contentMarkdown: BODY, targetParticipantIds: [OPERATOR] }),
      CHAT_ID,
      ctx,
    )
    emit('inform: 400 on a user-controlled seat', { ...(await read(res)), calls: calls() })
  })

  // NOT a case: a REMOVED seat. v4's own test mocks `resolveAnnouncementAudience`
  // to answer every requested id as known, so through this seam a departed seat
  // reaches the eligibility gate and is reported as "Not an LLM-controlled seat".
  // v4's REAL resolver excludes a removed participant as UNKNOWN (its header says
  // so in as many words), so the real answer is the unknown-target sentence — and
  // the mock cannot show which. Measuring it here would pin the MOCK. The
  // audience resolver has its own differential; the eligibility gate is pinned by
  // the user-controlled-seat case above, whose seat is live on both sides.

  it('400s when the room has nobody an LLM speaks for', async () => {
    ctx.repos.chats.findById.mockResolvedValue(
      makeChat({
        participants: [
          {
            id: OPERATOR,
            type: 'CHARACTER',
            controlledBy: 'user',
            status: 'active',
            removedAt: null,
          },
        ],
      }),
    )
    const res = await handleInform(
      makeRequest({ contentMarkdown: BODY, targetParticipantIds: null }),
      CHAT_ID,
      ctx,
    )
    emit('inform: 400 when no LLM seat exists', { ...(await read(res)), calls: calls() })
  })

  it('still creates the batch when the record cannot be written', async () => {
    postInformRecord.mockResolvedValue(null)
    const res = await handleInform(
      makeRequest({ contentMarkdown: BODY, targetParticipantIds: null }),
      CHAT_ID,
      ctx,
    )
    emit('inform: the batch survives a lost record', {
      ...(await read(res)),
      calls: calls(),
    })
  })

  it('trims the body for the rows', async () => {
    const res = await handleInform(
      makeRequest({ contentMarkdown: `  ${BODY}  \n`, targetParticipantIds: null }),
      CHAT_ID,
      ctx,
    )
    emit('inform: the body is trimmed for the rows', {
      ...(await read(res)),
      calls: calls(),
    })
  })

  // ---- GET ?action=informs -------------------------------------------------

  it('returns the pending batches', async () => {
    ctx.repos.chatInforms.findPendingBatches.mockResolvedValue([
      {
        batchId: BATCH_ID,
        contentMarkdown: BODY,
        createdAt: '2026-01-01T21:14:00.000Z',
        recordMessageId: RECORD_ID,
        pendingParticipantIds: [ALICE, BOB],
      },
    ])
    const res = await handleGetInforms(CHAT_ID, ctx)
    emit('informs: the pending batches', { ...(await read(res)), calls: calls() })
  })

  it('filters seats that have left, and drops a batch left with nobody', async () => {
    ctx.repos.chatInforms.findPendingBatches.mockResolvedValue([
      {
        batchId: BATCH_ID,
        contentMarkdown: BODY,
        createdAt: '2026-01-01T21:14:00.000Z',
        recordMessageId: RECORD_ID,
        pendingParticipantIds: [ALICE, DEPARTED],
      },
      {
        batchId: 'batch-gone',
        contentMarkdown: 'Only the departed were owed this.',
        createdAt: '2026-01-01T21:15:00.000Z',
        recordMessageId: null,
        pendingParticipantIds: [DEPARTED],
      },
    ])
    const res = await handleGetInforms(CHAT_ID, ctx)
    emit('informs: departed seats filtered, empty batch dropped', {
      ...(await read(res)),
      calls: calls(),
    })
  })

  it('404s when the chat is gone', async () => {
    ctx.repos.chats.findById.mockResolvedValue(null)
    const res = await handleGetInforms(CHAT_ID, ctx)
    emit('informs: 404s when the chat is gone', { ...(await read(res)), calls: calls() })
  })

  // ---- POST ?action=cancel-inform ------------------------------------------

  it('takes the record with it when nobody has read the passage', async () => {
    ctx.repos.chatInforms.findByBatchId.mockResolvedValue([makeRow(), makeRow({ id: 'row-2', participantId: BOB })])
    ctx.repos.chatInforms.deletePendingByBatch.mockResolvedValue(2)
    const res = await handleCancelInform(makeRequest({ batchId: BATCH_ID }), CHAT_ID, ctx)
    emit('cancel: the record goes when nothing was consumed', {
      ...(await read(res)),
      calls: calls(),
    })
  })

  it('keeps the record once any seat has consumed its row', async () => {
    ctx.repos.chatInforms.findByBatchId.mockResolvedValue([
      makeRow(),
      makeRow({
        id: 'row-2',
        participantId: BOB,
        consumedAt: '2026-01-01T21:20:00.000Z',
        consumedByMessageId: 'msg-1',
      }),
    ])
    ctx.repos.chatInforms.deletePendingByBatch.mockResolvedValue(1)
    const res = await handleCancelInform(makeRequest({ batchId: BATCH_ID }), CHAT_ID, ctx)
    emit('cancel: the record STAYS once anything was consumed', {
      ...(await read(res)),
      calls: calls(),
    })
  })

  it('still answers when the record message refuses to go', async () => {
    ctx.repos.chatInforms.findByBatchId.mockResolvedValue([makeRow()])
    ctx.repos.chatInforms.deletePendingByBatch.mockResolvedValue(1)
    ctx.repos.chats.deleteMessagesByIds.mockRejectedValue(new Error('nope'))
    const res = await handleCancelInform(makeRequest({ batchId: BATCH_ID }), CHAT_ID, ctx)
    emit('cancel: a refusing record message does not break the cancel', {
      ...(await read(res)),
      calls: calls(),
    })
  })

  it('404s on an unknown batch', async () => {
    ctx.repos.chatInforms.findByBatchId.mockResolvedValue([])
    const res = await handleCancelInform(makeRequest({ batchId: BATCH_ID }), CHAT_ID, ctx)
    emit('cancel: 404 on an unknown batch', { ...(await read(res)), calls: calls() })
  })

  it('400s on a batch belonging to another conversation', async () => {
    ctx.repos.chatInforms.findByBatchId.mockResolvedValue([makeRow({ chatId: OTHER_CHAT })])
    const res = await handleCancelInform(makeRequest({ batchId: BATCH_ID }), CHAT_ID, ctx)
    emit('cancel: 400 on another conversation’s batch', {
      ...(await read(res)),
      calls: calls(),
    })
  })

  afterAll(() => {
    const outPath = process.env.QT_ORACLE_OUT
    if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write')
    fs.writeFileSync(outPath, outLines.join('\n') + '\n')
  })
})
