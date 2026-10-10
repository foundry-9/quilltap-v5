/**
 * The canned consolidation answers (P4.D271) shared by the service and
 * trigger oracles: per answer set, per cluster topic letter (the first
 * `[<topic><n>]` marker in the NOTES), the model's answer — a JSON object, a
 * raw string, or `{ fail }` (the provider throws that message).
 */

export type Answer = Record<string, unknown> | { fail: string } | string;

export const VALID: Record<string, Answer> = {
  A: {
    digests: [
      {
        content: '(topic A) Laura keeps charts of the northern coast and drew the fjords; she prefers tea now, coffee before.',
        summary: 'laura keeps charts prefers tea',
        keywords: ['Charts', 'maps', 'present', 'scope: wide', 'information'],
        importance: 0.7,
        memberIds: ['m1', 'm2', 'm4'],
      },
    ],
    keepStandalone: ['m3'],
    contradictions: [{ olderId: 'm3', newerId: 'm2', note: 'tea now' }],
  },
  B: '```json\n' +
    JSON.stringify({
      digests: [
        {
          content: '(topic B) Laura sails the Kittiwake, now painted blue, moored at the east quay.',
          summary: 'laura sails the kittiwake',
          keywords: ['boat'],
          importance: 0.65,
          memberIds: ['m1', 'm2'],
        },
      ],
    }) +
    '\n```',
  C: {
    digests: [
      {
        content: "(topic C) Laura and the holder walked the sea wall, were caught by a storm at the lighthouse, and sheltered in the keeper's cottage.",
        summary: 'sea wall walk and storm',
        keywords: ['walk', 'storm'],
        importance: 0.8,
        kind: 'episodic',
        occurredAt: '2026-09-01',
        memberIds: ['m1', 'm2', 'm3'],
      },
    ],
  },
  F: {
    digests: [
      {
        content: '(topic F) I keep the Estate ledgers in green ink and balance them every Sunday.',
        summary: 'i keep the ledgers',
        keywords: ['ledger', 'present', 'scope: wide', 'mannerisms'],
        importance: 0.6,
        memberIds: ['m1', 'm2', 'm3'],
      },
    ],
  },
  G: {
    digests: [
      {
        content: '(topic G) The harbour bell, cast in 1802 and rung by Amos, sounds at noon.',
        summary: 'the harbour bell',
        keywords: [],
        importance: 0.5,
        memberIds: ['m1', 'm2', 'm3'],
      },
    ],
  },
  H: {
    digests: [
      {
        content: '(topic H) The vanished one grew and sold figs.',
        summary: 'figs',
        keywords: ['figs'],
        importance: 0.3,
        memberIds: ['m2', 'm3'],
      },
    ],
  },
  J: {
    digests: [
      {
        content: '(topic J) Laura distrusts barometers and says they lie.',
        summary: 'laura distrusts barometers',
        keywords: [],
        importance: 0.4,
        memberIds: ['m1', 'm2'],
      },
    ],
  },
};

export const ANSWER_SETS: Record<string, Record<string, Answer>> = {
  valid: VALID,
  failures: {
    ...VALID,
    A: { digests: [{ ...(VALID.A as any).digests[0], memberIds: ['m9'] }] },
    B: { fail: 'provider exploded' },
    C: { fail: 'connect ETIMEDOUT 10.0.0.1:443' },
    F: 'I could not decide which notes belong together.',
  },
  embed_fail: {
    ...VALID,
    G: { digests: [{ ...(VALID.G as any).digests[0], content: '(topic G) The harbour bell FAILEMBED sounds at noon.' }] },
  },
  handler_dry: { ...VALID, B: { fail: 'provider exploded' } },
  rerun: {
    A: {
      digests: [
        {
          content: '(topic A) Laura keeps charts of the northern coast, drew the fjords and framed her best chart.',
          summary: 'laura keeps and frames charts',
          keywords: ['charts', 'present', 'scope: wide', 'information'],
          importance: 0.75,
          memberIds: ['m1', 'm2'],
        },
      ],
    },
    F: { digests: [{ ...(VALID.F as any).digests[0], memberIds: ['m1'] }] },
  },
};


/** The provider's answer for a topic: `{ response }` or `{ fail }`. */
export function answerFor(
  set: Record<string, Answer>,
  topic: string,
): { response: string; fail?: undefined } | { response: ''; fail: string } {
  const answer = set[topic];
  if (answer === undefined) throw new Error(`no canned answer for topic ${topic}`);
  if (typeof answer === 'object' && 'fail' in answer) return { response: '', fail: answer.fail as string };
  return { response: typeof answer === 'string' ? answer : JSON.stringify(answer) };
}
