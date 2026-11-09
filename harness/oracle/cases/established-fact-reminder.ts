/**
 * Tier-1 oracle case — `buildEstablishedFactReminder` (P4.D372, v4 `db6190b44`,
 * pin `624d0d25c`, `lib/chat/context/established-fact-reminder.ts`).
 *
 * Drives v4's REAL function over a fixed corpus of event lists. Its only seams
 * are parameters: `loadSpeakerNames` answers a fixed map and COUNTS its calls
 * (v4 loads names only when a seat line is selected), and the root logger
 * singleton's `debug` is patched to record v4's
 * `[EstablishedFact] Built the since-you-last-spoke reminder` fields.
 *
 * Each row emits `{ label, input, text, loaderCalls, lines }` where `input` is
 * the `{events, respondingParticipantId, joinedAt, hasHistoryAccess,
 * regenerationOfMessageIds, chatId}` the function got (absent keys absent).
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   $N/npx tsx $V5W/harness/oracle/cases/established-fact-reminder.ts \
 *     > /tmp/oracle-ef-reminder.ndjson
 */

type Ev = Record<string, unknown>;

interface Input {
  events: Ev[];
  respondingParticipantId: string;
  joinedAt?: string | null;
  hasHistoryAccess?: boolean;
  regenerationOfMessageIds?: string[];
  chatId?: string;
}

const SEAT = 'p-a';
const NAMES: Array<[string, string]> = [['p-user', 'Captain'], ['p-a', 'Alice']];

function msg(id: string, role: string, content: string, createdAt: string | undefined, extra: Ev = {}): Ev {
  const e: Ev = { type: 'message', id, role, content, participantId: null };
  if (createdAt !== undefined) e.createdAt = createdAt;
  return { ...e, ...extra };
}
const seatLine = (id: string, content: string, at: string | undefined, extra: Ev = {}) =>
  msg(id, 'USER', content, at, { participantId: 'p-user', isEstablishedFact: true, ...extra });
const machine = (id: string, content: string, at: string | undefined, extra: Ev = {}) =>
  msg(id, 'ASSISTANT', content, at, { systemSender: 'host', systemKind: 'established-fact', ...extra });
const own = (id: string, at: string) => msg(id, 'ASSISTANT', 'I spoke.', at, { participantId: SEAT });
const t = (min: number) => new Date(Date.UTC(2026, 0, 1, 10, min)).toISOString();

async function main(): Promise<void> {
  const { buildEstablishedFactReminder } = await import('@/lib/chat/context/established-fact-reminder');
  const { ESTABLISHED_FACT_REMINDER_HEADER } = await import('@/lib/chat/established-fact');
  const { logger } = await import('@/lib/logger');

  const lines: Array<Record<string, unknown>> = [];
  (logger as unknown as { debug: unknown }).debug = (message: string, meta?: Record<string, unknown>) => {
    if (message === '[EstablishedFact] Built the since-you-last-spoke reminder') {
      // JSON round trip: an `undefined` field is ABSENT (v4's omitted key).
      lines.push(JSON.parse(JSON.stringify(meta ?? {})));
    }
  };

  const H = ESTABLISHED_FACT_REMINDER_HEADER.length;
  const base = (events: Ev[], extra: Partial<Input> = {}): Input => ({
    events, respondingParticipantId: SEAT, hasHistoryAccess: true, chatId: 'chat-1', ...extra,
  });

  // Exactly-at-the-cap: H + (a + 2) + (b + 2) === 8000.
  const capA = 'a'.repeat(4000);
  const capB = 'b'.repeat(8000 - H - 4 - 4000);
  const capB1 = capB + 'b'; // one unit over: the oldest drops.
  // Multi-byte: 3000 CJK units each (9000 UTF-8 bytes) — under the cap in
  // UTF-16, far over it in bytes.
  const cjk = '漢'.repeat(3000);
  // Astral: each emoji is 2 UTF-16 units (1 code point, 4 bytes).
  const emoji = '😀'.repeat(2500);

  const many: Ev[] = [];
  for (let i = 0; i < 200; i++) {
    many.push(machine(`m${i}`, `Fact number ${i} about the harbour and its many quays, recorded with care.`, t(i % 59)));
  }

  const cases: Array<[string, Input]> = [
    ['no facts → empty, loader not called', base([own('o1', t(1)), msg('u1', 'USER', 'hi', t(2), { participantId: 'p-user' })])],
    ['history access, no own turn → every fact', base([machine('f1', 'The bell is cracked.', t(1)), seatLine('s1', 'The bridge fell.', t(2))])],
    ['no history, joinedAt → facts after the join', base([machine('f1', 'before join', t(1)), machine('f2', 'after join', t(5))], { hasHistoryAccess: false, joinedAt: t(3) })],
    ['no history, no joinedAt → every fact', base([machine('f1', 'one', t(1)), machine('f2', 'two', t(5))], { hasHistoryAccess: false })],
    ['no history, empty joinedAt → every fact', base([machine('f1', 'one', t(1))], { hasHistoryAccess: false, joinedAt: '' })],
    ['no history, null joinedAt → every fact', base([machine('f1', 'one', t(1))], { hasHistoryAccess: false, joinedAt: null })],
    ['unparseable joinedAt → NaN floor', base([machine('f1', 'one', t(1)), machine('f2', 'no stamp', undefined)], { hasHistoryAccess: false, joinedAt: 'not a date' })],
    ['prior own turn → only facts after it', base([machine('f1', 'old fact', t(1)), own('o1', t(2)), machine('f2', 'new fact', t(3)), seatLine('s2', 'new seat fact', t(4))])],
    ['equal-ms fact is not newer', base([own('o1', t(2)), machine('f1', 'same instant', t(2)), machine('f2', 'after', t(3))])],
    ['unparseable fact createdAt with a floor → dropped', base([own('o1', t(2)), machine('f1', 'garbled', 'yesterday-ish'), machine('f2', 'absent', undefined), machine('f3', 'ok', t(3))])],
    ['unparseable fact createdAt without a floor → kept', base([machine('f1', 'garbled', 'yesterday-ish'), machine('f2', 'absent', undefined)])],
    ['targets: away, at this seat, empty', base([
      machine('f1', 'away', t(1), { targetParticipantIds: ['p-b'] }),
      machine('f2', 'at me', t(2), { targetParticipantIds: ['p-b', SEAT] }),
      machine('f3', 'empty targets', t(3), { targetParticipantIds: [] }),
      machine('f4', 'null targets', t(4), { targetParticipantIds: null }),
    ])],
    ['seat line + machine fact → names loaded once', base([seatLine('s1', 'The bridge fell.', t(1)), machine('f1', 'The river rose.', t(2)), seatLine('s2', 'Again.', t(3))])],
    ['seat line with an unresolved speaker', base([
      seatLine('s1', 'no seat name', t(1), { participantId: 'p-ghost' }),
      seatLine('s2', 'no participant', t(2), { participantId: null }),
      seatLine('s3', 'assistant-roled flag', t(3), { role: 'ASSISTANT', participantId: null }),
    ])],
    ['a seat line with a staff sender is no fact', base([seatLine('s1', 'aurora', t(1), { systemSender: 'aurora' }), seatLine('s2', 'empty sender is no sender', t(2), { systemSender: '' })])],
    ['a non-boolean flag is no fact', base([seatLine('s1', 'numeric', t(1), { isEstablishedFact: 1 })])],
    ['non-message events skipped', base([{ type: 'context-summary', id: 'cs', content: 'x', systemSender: 'host', systemKind: 'established-fact' }, machine('f1', 'real', t(1))])],
    ['swipe: cut at the first re-rolled row', base([machine('f1', 'before', t(1)), own('o1', t(2)), machine('f2', 'after own', t(3)), own('o2', t(4)), machine('f3', 'after the rerolled turn', t(5))], { regenerationOfMessageIds: ['o2', 'o2b'] })],
    ['swipe: the original turn being re-rolled', base([machine('f1', 'fact', t(1)), own('o1', t(2))], { regenerationOfMessageIds: ['o1'] })],
    ['swipe id not found → no cut', base([own('o1', t(2)), machine('f1', 'after', t(3))], { regenerationOfMessageIds: ['nope'] })],
    ['empty regeneration list is not a swipe', base([machine('f1', 'x', t(1))], { regenerationOfMessageIds: [] })],
    ['cap: exactly at the boundary → both kept', base([machine('f1', capA, t(1)), machine('f2', capB, t(2))])],
    ['cap: one unit over → the oldest drops', base([machine('f1', capA, t(1)), machine('f2', capB1, t(2))])],
    ['cap: a single over-cap fact still rides', base([machine('f1', 'x'.repeat(9000), t(1))])],
    ['cap: multi-byte CJK counts UTF-16 units', base([machine('f1', cjk, t(1)), machine('f2', cjk, t(2))])],
    ['cap: astral emoji count two units each', base([machine('f1', emoji, t(1)), machine('f2', emoji, t(2))])],
    ['no chatId → the key is absent', { events: [machine('f1', 'x', t(1))], respondingParticipantId: SEAT, hasHistoryAccess: true }],
    ['dogfood scale: 200 facts', base(many)],
  ];

  for (const [label, input] of cases) {
    lines.length = 0;
    let loaderCalls = 0;
    const text = await buildEstablishedFactReminder({
      ...(input as unknown as Parameters<typeof buildEstablishedFactReminder>[0]),
      loadSpeakerNames: async () => {
        loaderCalls++;
        return new Map(NAMES);
      },
    });
    process.stdout.write(JSON.stringify({ label, input, text, loaderCalls, lines: [...lines] }) + '\n');
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
