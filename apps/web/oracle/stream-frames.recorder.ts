/**
 * The v4-side recorder behind `src/app/core/stream-frames.oracle.spec.ts`
 * (P4.D384 item 3).
 *
 * `parseFrame` (`lib/chat/stream-frames.ts`, v4 `604a082c4`) is the Salon
 * client's ONE parser for a stream's `data:` payload: a Zod
 * `discriminatedUnion('kind', …)` that STRIPS unknown keys, re-emits every
 * schema-typed level in SCHEMA order, passes `z.custom` members through
 * untouched, and drops a frame that fails the schema with a WARN logged ONCE per
 * `kind` string for the module's life. The SPA has no `zod`, so
 * `core/stream-frames.ts` is a hand-written guard and THIS corpus is what proves
 * it — byte-for-byte on `JSON.stringify(out)` (key order is the point) and on
 * the WARN calls each row caused.
 *
 * ⚠ ORDER IS PART OF THE CORPUS. `reportedKinds` is module-global, so a row's
 * `warns` depend on every row before it. The spec replays the rows in file
 * order from ONE fresh module import.
 *
 * Sections (the `section` key):
 *   (a) `null`    — the payloads `parseFrame` answers null before the schema;
 *   (b) `valid`   — one row per kind × optionals present / absent, the five
 *                   outcomes, the four `done` producer shapes (finalizer,
 *                   recovery, Courier, turn closes), a `seat` on five kinds, an
 *                   envelope `chatId` (stripped), a swipe status with a nested
 *                   `kind` (stripped), extra keys at every nested level;
 *   (c) `drop`    — the frames the schema refuses (the WARN path); every
 *                   refused row also carries `issues`, the first three issue
 *                   texts the schema raised, so the issue generator is pinned
 *                   on every class, not only on each kind's first WARN;
 *   (d) `golden`  — every frame of v4's six golden streams, through v4's own
 *                   `loadFrameStream` (`__tests__/helpers/salon-stream-golden.ts`),
 *                   re-serialised and parsed — the frames the reducer specs
 *                   replay;
 *   (e) `doneLeavesNoReply` — the five outcomes.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...`.
 *
 * Run it from a LANE-UNIQUE pinned v4 worktree with the pin's OWN
 * `node_modules` (never a symlink to the live checkout's — the 2026-11-02
 * rulings), Node 24 at `~/.nvm/versions/node/v24.13.1/bin`:
 *
 * ```bash
 * PIN='/tmp/qt-v4-pin-p4d384-624d0d25c'
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" 624d0d25c
 * (cd "$PIN" && npm ci --offline --ignore-scripts --no-audit --no-fund && npm rebuild better-sqlite3)
 * # node_modules/zod/package.json must read 4.6.7
 * F=<V5>/apps/web/src/testing/fixtures/stream-frames.oracle.ndjson
 * rm -f "$F"
 * cp <V5>/apps/web/oracle/stream-frames.recorder.ts "$PIN/"
 * cd "$PIN" && npx tsx stream-frames.recorder.ts > "$F"
 * ```
 *
 * Expect the row count `src/testing/fixtures/README.md` records; a shorter
 * file means the recorder errored and the redirect already truncated the old
 * one (the empty-file trap). The spec asserts the count.
 */

import { logger } from '@/lib/logger';
import { StreamFrameSchema, doneLeavesNoReply, parseFrame } from '@/lib/chat/stream-frames';
import { GOLDEN_STREAMS, loadFrameStream } from './__tests__/helpers/salon-stream-golden';

type Warn = { msg: string; meta: unknown };
let captured: Warn[] = [];
// Splice the logger (memory note `v4-oracle-job-host-and-log-capture`): the
// module calls `logger.warn` on this same instance.
(logger as unknown as { warn: (m: string, c?: unknown) => void }).warn = (msg, meta) => {
  captured.push({ msg, meta });
};

const out: string[] = [];
function row(section: string, id: string, raw: string): void {
  captured = [];
  const parsed = parseFrame(raw);
  // `issues`: the first three formatted Zod issues v4's REAL schema raises for
  // this payload, recorded on EVERY row the schema refuses — not only on the
  // first row of a kind, which is all the once-per-kind WARN shows. Absent when
  // the schema never ran (section a) or accepted the frame.
  let issues: string[] | undefined;
  const trimmed = raw.trim();
  if (parsed === null && trimmed && trimmed !== '[DONE]' && trimmed !== '{}') {
    let json: unknown;
    let ok = true;
    try {
      json = JSON.parse(trimmed);
    } catch {
      ok = false;
    }
    if (ok) {
      const res = StreamFrameSchema.safeParse(json);
      if (!res.success) {
        issues = res.error.issues.slice(0, 3).map((i) => `${i.path.join('.')}: ${i.message}`);
      }
    }
  }
  out.push(JSON.stringify({ section, id, raw, out: parsed, warns: captured, ...(issues ? { issues } : {}) }));
}
const j = (v: unknown): string => JSON.stringify(v);

// ---------------------------------------------------------------- (a) null
row('null', 'empty', '');
row('null', 'spaces', '  ');
row('null', 'done-sentinel', '[DONE]');
row('null', 'done-sentinel-padded', '  [DONE]\n');
row('null', 'empty-object', '{}');
row('null', 'empty-object-padded', ' {} ');
row('null', 'not-json', '{not json');
row('null', 'truncated', '{"kind":"content","text":"ab');
row('null', 'number', '5');
row('null', 'json-null', 'null');
row('null', 'array', '[]');

// ---------------------------------------------------------------- (b) valid
const seat = { participantId: 'p-bo', concurrent: true };
const msg = { type: 'message', id: 'm-1', role: 'ASSISTANT', content: 'x', extra: { deep: 1 } };
const usage = { promptTokens: 1, completionTokens: 2, totalTokens: 3 };
const cacheUsage = { cacheCreationInputTokens: 4, cacheReadInputTokens: 5 };
const turn = { nextSpeakerId: 'p-2', reason: 'rotation', cycleComplete: false, isUsersTurn: false };

row('valid', 'content', j({ kind: 'content', text: 'Hello' }));
row('valid', 'content-empty-text', j({ kind: 'content', text: '' }));
row('valid', 'content-envelope-chatId', j({ chatId: 'c-1', kind: 'content', text: 'Hi' }));
row('valid', 'content-extra-keys', j({ text: 'Hi', zzz: 1, kind: 'content', participantId: 'p' }));
row('valid', 'content-seat', j({ kind: 'content', text: 'Rather', seat }));
row('valid', 'content-seat-first', j({ seat, kind: 'content', text: 'Rather' }));
row('valid', 'content-seat-extra', j({ kind: 'content', text: 'R', seat: { concurrent: true, x: 1, participantId: 'p' } }));
row('valid', 'reasoning', j({ kind: 'reasoning', text: 'Hmm.' }));
row('valid', 'status-full', j({ kind: 'status', status: { stage: 's', message: 'm', toolName: 't', characterName: 'Ada', characterId: 'c' } }));
row('valid', 'status-min', j({ kind: 'status', status: { message: 'm', stage: 's' } }));
row('valid', 'status-swipe-nested-kind', j({ kind: 'status', status: { kind: 'regenerating', stage: 'regenerating', message: 'Regenerating...' } }));
row('valid', 'status-envelope-extra', j({ chatId: 'c', kind: 'status', status: { stage: 's', message: 'm', zz: true }, extra: 1 }));
row('valid', 'status-seat', j({ kind: 'status', status: { stage: 's', message: 'm' }, seat }));
row('valid', 'fileProcessing-full', j({ kind: 'fileProcessing', info: [{ type: 'text', filename: 'a.txt', usedImageDescriptionLLM: false, error: 'bad', x: 1 }, { filename: 'b.png', type: 'image_description', usedImageDescriptionLLM: true }] }));
row('valid', 'fileProcessing-empty', j({ kind: 'fileProcessing', info: [] }));
row('valid', 'toolsDetected', j({ kind: 'toolsDetected', names: ['a', 'b'], arguments: [{ path: 'a.md', nested: { k: [1, 2] } }, {}] }));
row('valid', 'toolsDetected-empty', j({ kind: 'toolsDetected', arguments: [], names: [] }));
row('valid', 'toolResult-success', j({ kind: 'toolResult', index: 0, name: 'doc_read_file', success: true, result: { text: 'hello', z: [1] } }));
row('valid', 'toolResult-failure', j({ kind: 'toolResult', index: 1, name: 'generate_image', success: false, result: null, error: 'Error: no image profile' }));
row('valid', 'toolResult-min', j({ success: true, name: 'x', kind: 'toolResult' }));
row('valid', 'toolResult-result-scalar', j({ kind: 'toolResult', name: 'x', success: true, result: 'text' }));
row('valid', 'toolResult-seat', j({ kind: 'toolResult', name: 'x', success: true, seat }));
row('valid', 'turnStart', j({ kind: 'turnStart', participantId: 'p-ada', characterName: 'Ada', chainDepth: 0 }));
row('valid', 'turnStart-depth1-extra', j({ chainDepth: 1, characterName: 'Ada', participantId: 'p', kind: 'turnStart', concurrent: true }));
row('valid', 'turnStart-seat', j({ kind: 'turnStart', participantId: 'own', characterName: 'Ada', chainDepth: 0, seat }));
// done — the four producer shapes of the order's §A
row('valid', 'done-finalizer', j({
  kind: 'done', outcome: 'finished', messageId: 'm-1', participantId: 'p-ada', usage, cacheUsage,
  attachmentResults: { sent: ['f-1'], failed: [{ id: 'f-2', error: 'too big', x: 1 }] }, toolsExecuted: false, turn,
  provider: 'ANTHROPIC', modelName: 'claude', routeTrail: null, isSilentMessage: false,
  reasoningContent: 'Hmm.', reasoningSegments: [{ seq: 0, content: 'Hmm.', anchorOffset: 3, z: 1 }],
}));
row('valid', 'done-finalizer-silent-trail', j({
  kind: 'done', outcome: 'finished', messageId: 'm-1', participantId: 'p-ada', usage: { totalTokens: 3 }, cacheUsage: {},
  attachmentResults: { sent: [], failed: [] }, toolsExecuted: true, turn: { ...turn, extra: 1 },
  provider: 'OPENAI', modelName: 'gpt', routeTrail: [{ profileId: 'a', ok: false, nested: { q: 1 } }, { profileId: 'b' }],
  isSilentMessage: true, reasoningContent: null, reasoningSegments: null,
}));
row('valid', 'done-recovery', j({ kind: 'done', outcome: 'finished', messageId: 'm-2', usage: null, cacheUsage: null, attachmentResults: null, toolsExecuted: false }));
row('valid', 'done-courier', j({
  kind: 'done', outcome: 'finished', messageId: 'm-5', participantId: 'p-ext', usage: null, cacheUsage: null,
  attachmentResults: null, toolsExecuted: false, provider: 'COURIER', modelName: 'manual', pendingExternalTurn: true,
}));
row('valid', 'done-turn-yielded', j({
  kind: 'done', outcome: 'yielded', usage: null, cacheUsage: null, attachmentResults: null, provider: 'X', modelName: 'Y',
  messageId: null, participantId: 'p-bo', toolsExecuted: false, skippedParticipantId: 'p-bo',
}));
row('valid', 'done-turn-skipped', j({
  kind: 'done', outcome: 'skipped', usage: null, cacheUsage: null, attachmentResults: null,
  messageId: null, participantId: null, toolsExecuted: false, skippedParticipantId: null,
}));
row('valid', 'done-turn-empty', j({
  kind: 'done', outcome: 'empty', usage: null, cacheUsage: null, attachmentResults: null, provider: 'X', modelName: 'Y',
  messageId: null, participantId: 'p', toolsExecuted: false, emptyResponseReason: 'Nothing came back.',
}));
row('valid', 'done-turn-stopped', j({
  kind: 'done', outcome: 'stopped', usage: null, cacheUsage: null, attachmentResults: null,
  messageId: 'm-9', participantId: 'p', toolsExecuted: false,
}));
row('valid', 'done-envelope-legacy-flags', j({
  chatId: 'c-1', kind: 'done', outcome: 'finished', done: true, skipped: false, messageId: 'm', usage: null,
  cacheUsage: null, attachmentResults: null, toolsExecuted: false,
}));
row('valid', 'done-seat', j({ kind: 'done', outcome: 'finished', messageId: 'm', participantId: 'p-bo', usage: null, cacheUsage: null, attachmentResults: null, toolsExecuted: false, seat }));
row('valid', 'turnComplete', j({ kind: 'turnComplete', participantId: 'p', messageId: 'm', chainDepth: 1 }));
row('valid', 'turnComplete-skipped', j({ kind: 'turnComplete', skipped: true, chainDepth: 2, messageId: 'm', participantId: 'p' }));
row('valid', 'chainComplete', j({ kind: 'chainComplete', reason: 'user_turn', nextSpeakerId: null, chainDepth: 0 }));
row('valid', 'chainComplete-paused-held', j({ kind: 'chainComplete', heldUserTurn: true, paused: true, reason: 'paused', nextSpeakerId: 'p', chainDepth: 3 }));
for (const reason of ['max_depth', 'max_time', 'error', 'no_next_speaker', 'cycle_complete', 'yielded', 'stopped', 'round_open']) {
  row('valid', `chainComplete-${reason}`, j({ kind: 'chainComplete', reason, nextSpeakerId: null, chainDepth: 1 }));
}
row('valid', 'error-full', j({ kind: 'error', message: 'Failed to send', errorType: 'provider_error', details: 'boom', userMessagePersisted: true, pendingToolResultsPersisted: false }));
row('valid', 'error-min', j({ details: '', errorType: 'x', message: 'm', kind: 'error', error: 'legacy' }));
row('valid', 'pendingExternalTurn', j({ kind: 'pendingExternalTurn', messageId: 'm-5', participantId: 'p-ext', characterName: 'Dee' }));
for (const what of ['carinaAnswer', 'hostAnnouncement', 'pascalResult']) {
  row('valid', `posted-${what}`, j({ kind: 'posted', what, message: msg }));
}
row('valid', 'posted-seat', j({ kind: 'posted', what: 'hostAnnouncement', message: msg, seat }));
row('valid', 'storytellerRound', j({ kind: 'storytellerRound', round: { id: 'r-1', bids: [], extra: { q: 1 } } }));
row('valid', 'storytellerRound-array-round', j({ kind: 'storytellerRound', round: { 0: 'x' } }));
row('valid', 'storytellerBid', j({ kind: 'storytellerBid', roundId: 'r-1', bid: { participantId: 'p', status: 'ready', z: 1 } }));
row('valid', 'roundOpen', j({ kind: 'roundOpen', roundId: 'r-1', roundOpen: true }));
row('valid', 'handAsked', j({ kind: 'handAsked', participantId: 'p-bo', characterName: 'Bo' }));
row('valid', 'handAsked-seat', j({ kind: 'handAsked', participantId: 'p-bo', characterName: 'Bo', seat }));
row('valid', 'handAnswered-yield', j({ kind: 'handAnswered', participantId: 'p-bo', characterName: 'Bo', yielded: true }));
row('valid', 'handAnswered-decline-seat', j({ kind: 'handAnswered', yielded: false, characterName: '', participantId: 'p', seat }));
row('valid', 'confirmationResult', j({ kind: 'confirmationResult', messageId: 'm-1', confirmed: true, revised: false, notes: null }));
row('valid', 'confirmationResult-revised', j({ kind: 'confirmationResult', content: 'Better.', notes: 'fixed', revised: true, confirmed: null, messageId: 'm-1' }));
row('valid', 'swipeDone', j({ kind: 'swipeDone', message: { id: 'm-swipe', content: 'x', attachments: [] } }));
row('valid', 'swipeDone-envelope', j({ chatId: 'c', kind: 'swipeDone', message: { id: 'm' }, done: true }));

// ---------------------------------------------------------------- (c) drop
const doneBase = { kind: 'done', outcome: 'finished', messageId: 'm', usage: null, cacheUsage: null, attachmentResults: null, toolsExecuted: false };
row('drop', 'done-provider-null', j({ ...doneBase, provider: null }));
row('drop', 'done-provider-null-again', j({ ...doneBase, modelName: null }));
row('drop', 'done-turn-null', j({ ...doneBase, turn: null }));
row('drop', 'done-no-messageId', j({ kind: 'done', outcome: 'finished', usage: null, cacheUsage: null, attachmentResults: null, toolsExecuted: false }));
row('drop', 'done-attachments-sent-only', j({ ...doneBase, attachmentResults: { sent: [] } }));
row('drop', 'done-bad-outcome', j({ ...doneBase, outcome: 'halted' }));
row('drop', 'done-many-issues', j({ kind: 'done', outcome: 7, messageId: 3, usage: 'u', cacheUsage: 1, attachmentResults: false }));
row('drop', 'content-seat-not-concurrent', j({ kind: 'content', text: 'x', seat: { participantId: 'p', concurrent: false } }));
row('drop', 'posted-message-no-id', j({ kind: 'posted', what: 'carinaAnswer', message: { content: 'x' } }));
row('drop', 'posted-bad-what', j({ kind: 'posted', what: 'other', message: msg }));
row('drop', 'swipeDone-message-array', j({ kind: 'swipeDone', message: [{ id: 'm' }] }));
row('drop', 'unknown-kind', j({ kind: 'fromTheFuture', x: 1 }));
row('drop', 'unknown-kind-again', j({ kind: 'fromTheFuture', x: 2 }));
row('drop', 'missing-kind', j({ text: 'orphan' }));
row('drop', 'missing-kind-again', j({ content: 'legacy bag' }));
row('drop', 'kind-number', j({ kind: 5 }));
row('drop', 'kind-null', j({ kind: null }));
row('drop', 'handAnswered-missing-yielded', '{"kind":"handAnswered","participantId":"p","characterName":"Bo"}');
row('drop', 'handAnswered-missing-again', j({ kind: 'handAnswered', participantId: 'p', yielded: 'yes' }));
row('drop', 'turnStart-missing-fields', j({ kind: 'turnStart', characterName: 'A' }));
row('drop', 'turnStart-infinite-depth', '{"kind":"turnStart","participantId":"p","characterName":"A","chainDepth":1e999}');
row('drop', 'status-not-object', j({ kind: 'status', status: 'thinking' }));
row('drop', 'status-null', j({ kind: 'status', status: null }));
row('drop', 'fileProcessing-bad-entry', j({ kind: 'fileProcessing', info: [{ filename: 'a', type: 'text', usedImageDescriptionLLM: 'no' }, 5] }));
row('drop', 'toolsDetected-args-not-records', j({ kind: 'toolsDetected', names: ['a', 1], arguments: [[1], null, 'x'] }));
row('drop', 'toolResult-no-name', j({ kind: 'toolResult', success: true }));
row('drop', 'chainComplete-bad-reason', j({ kind: 'chainComplete', reason: 'bored', nextSpeakerId: null, chainDepth: 0 }));
row('drop', 'error-legacy-shape', j({ kind: 'error', error: 'boom', errorType: 'x', details: 'y' }));
row('drop', 'confirmationResult-nested-legacy', j({ kind: 'confirmationResult', confirmationResult: { messageId: 'm' } }));
row('drop', 'storytellerRound-not-object', j({ kind: 'storytellerRound', round: 'r-1' }));
row('drop', 'storytellerBid-legacy-nested', j({ kind: 'storytellerBid', storytellerBid: { roundId: 'r', bid: {} } }));
row('drop', 'roundOpen-no-id', j({ kind: 'roundOpen' }));
row('drop', 'handAsked-no-name', j({ kind: 'handAsked', participantId: 'p' }));
row('drop', 'pendingExternalTurn-legacy', j({ kind: 'pendingExternalTurn', pendingExternalTurn: true }));
row('drop', 'reasoning-number', j({ kind: 'reasoning', text: 5 }));
row('drop', 'done-routeTrail-scalar-entry', j({ ...doneBase, routeTrail: ['x'] }));
row('drop', 'done-segments-bad', j({ ...doneBase, reasoningSegments: [{ anchorOffset: '1', content: 2 }] }));
row('drop', 'done-usage-string-number', j({ ...doneBase, usage: { promptTokens: '1' } }));
row('drop', 'done-attachments-failed-bad', j({ ...doneBase, attachmentResults: { sent: [1], failed: [{ id: 'x' }] } }));
row('drop', 'seat-missing-participant', j({ kind: 'reasoning', text: 'x', seat: { concurrent: true } }));
row('drop', 'seat-null', j({ kind: 'reasoning', text: 'x', seat: null }));
row('drop', 'swipeDone-message-null', j({ kind: 'swipeDone', message: null }));
row('drop', 'swipeDone-id-number', j({ kind: 'swipeDone', message: { id: 5 } }));
row('drop', 'array-of-frames', j([{ kind: 'content', text: 'x' }]));
row('drop', 'string-json', j('content'));
row('drop', 'boolean-json', 'true');

// ---------------------------------------------------------------- (d) golden
for (const name of GOLDEN_STREAMS) {
  loadFrameStream(name).forEach((frame, i) => {
    row('golden', `${name}#${i}`, j(frame));
  });
}

// ---------------------------------------------------------------- (e) doneLeavesNoReply
for (const outcome of ['finished', 'skipped', 'yielded', 'stopped', 'empty'] as const) {
  out.push(JSON.stringify({ section: 'doneLeavesNoReply', id: outcome, outcome, out: doneLeavesNoReply(outcome) }));
}

process.stdout.write(out.join('\n') + '\n');
