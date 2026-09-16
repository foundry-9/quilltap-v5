/**
 * Oracle case #16 (Wave 2 / B7): weighted next-speaker selection.
 *
 * Drives the REAL selectNextSpeaker from v4's lib/chat/turn-manager/selection.ts.
 * Its only impurity is Math.random() inside pickWeighted; we pin Math.random to
 * a fixed value per case (controlling the input, NOT reimplementing the
 * algorithm) and emit that value so the Rust port — which takes random01 as a
 * parameter — uses the identical draw.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/select-speaker.ts \
 *     > /tmp/oracle-select-speaker.ndjson
 */

import { selectNextSpeaker, selectNextSpeakerAfterUserMessage } from '@/lib/chat/turn-manager/selection';
import type { TurnState, TurnSelectionResult } from '@/lib/chat/turn-manager/types';
import type { ChatParticipantBase, Character } from '@/lib/schemas/types';

type WirePart = {
  id: string;
  type: string;
  status: string;
  characterId: string | null;
  controlledBy: string;
  talkativeness: number | null;
};
// characters: map characterId -> the two fields the selection reads. A bare
// number (or null) is the pre-P4.D63 talkativeness-only shorthand; the object
// form adds `archivedAt`, which v4 `d553f72a` made load-bearing.
type WireChar = number | null | { talkativeness?: number | null; archivedAt?: string | null };
type WireChars = Record<string, WireChar>;

const asParts = (ps: WirePart[]) => ps as unknown as ChatParticipantBase[];
const asChars = (c: WireChars): Map<string, Character> => {
  const m = new Map<string, Character>();
  for (const [cid, spec] of Object.entries(c)) {
    if (spec !== null && typeof spec === 'object') {
      const built: Record<string, unknown> = {};
      if (spec.talkativeness !== undefined && spec.talkativeness !== null) built.talkativeness = spec.talkativeness;
      if (spec.archivedAt !== undefined) built.archivedAt = spec.archivedAt;
      m.set(cid, built as unknown as Character);
      continue;
    }
    m.set(cid, (spec === null ? {} : { talkativeness: spec }) as unknown as Character);
  }
  return m;
};
const mkState = (queue: string[], spoken: string[], last: string | null, cycleOrder: string[] = []): TurnState =>
  ({ spokenSinceUserTurn: spoken, currentTurnParticipantId: null, queue, lastSpeakerId: last, cycleOrder } as TurnState);

/**
 * Pins `Math.random` to an ORDERED SEQUENCE and reports the draws actually
 * consumed (P4.D172).
 *
 * v4 `2aca73ad6`'s `drawCycleOrder` calls `Math.random()` once per remaining
 * candidate, so a single pinned scalar can no longer describe a turn. The
 * sequence REPEATS ITS LAST VALUE once exhausted, which makes a one-element
 * array `[r]` behave exactly like the old `Math.random = () => r` pin — that is
 * what keeps every pre-rotation row in this corpus byte-identical.
 *
 * `consumed` is emitted on the row so the Rust port's replay can be compared on
 * the COUNT as well as the values: a port that draws where v4 does not (or skips
 * a draw) diverges here even when the pick happens to agree.
 */
function withRandom<T>(draws: number[], fn: () => T): { out: T; consumed: number[] } {
  const orig = Math.random;
  const consumed: number[] = [];
  let i = 0;
  Math.random = () => {
    const v = draws.length === 0 ? 0 : draws[Math.min(i, draws.length - 1)];
    i += 1;
    consumed.push(v);
    return v;
  };
  try {
    return { out: fn(), consumed };
  } finally {
    Math.random = orig;
  }
}

type Scenario = {
  id: string;
  participants: WirePart[];
  characters: WireChars;
  queue: string[];
  spoken: string[];
  lastSpeakerId: string | null;
  /** The old single pin. Read as the one-element sequence `[random01]`. */
  random01: number;
  /** An explicit draw SEQUENCE, when one value cannot describe the case. */
  draws?: number[];
  impersonating?: string[];
  /**
   * P4.D195: the cycle's drawn rotation on the turn state. Defaults to `[]`,
   * which is what every pre-existing row carried implicitly (`mkState` never
   * set the field at all) — so those rows stay byte-identical. Until this
   * field existed NOTHING in this corpus drove `selectNextSpeaker`'s
   * cycle-order branch, and the Rust side hard-coded `&[]` at the call site
   * with a comment saying so: a measured blind spot, opened by the bug-147
   * agreement room below.
   */
  cycleOrder?: string[];
};

const p = (id: string, type: string, status: string, characterId: string | null, controlledBy: string, talkativeness: number | null): WirePart =>
  ({ id, type, status, characterId, controlledBy, talkativeness });

// P4.D195 / v4 bug 147 — the reported room, seat for seat from v4's own
// `client-server-agreement.test.ts`. Abigail carries all the talkativeness so
// a weighted re-roll is deterministic; Leilani's column stays 'llm' and she is
// user-driven only through the impersonation overlay.
const ABIGAIL = '1cbe72f5';
const CHARLIE = 'f08a46e0';
const BARAKA = '788b657a';
const LEILANI = '2c91e03b';
const GEN314 = '45be895a';
const agreementRoom: WirePart[] = [
  p(ABIGAIL, 'CHARACTER', 'active', `char-${ABIGAIL}`, 'llm', 1),
  p(CHARLIE, 'CHARACTER', 'active', `char-${CHARLIE}`, 'user', 0),
  p(BARAKA, 'CHARACTER', 'active', `char-${BARAKA}`, 'llm', 0),
  p(LEILANI, 'CHARACTER', 'active', `char-${LEILANI}`, 'llm', 0),
  p(GEN314, 'CHARACTER', 'active', `char-${GEN314}`, 'llm', 0),
];
const agreementChars: WireChars = Object.fromEntries(
  [ABIGAIL, CHARLIE, BARAKA, LEILANI, GEN314].map((id) => [`char-${id}`, 0.5]),
);

// Weighted trio: A=0.9, B=0.3, C=0.8 (total 2.0).
const trio: WirePart[] = [
  p('A', 'CHARACTER', 'active', 'ca', 'llm', 0.9),
  p('B', 'CHARACTER', 'active', 'cb', 'llm', 0.3),
  p('C', 'CHARACTER', 'active', 'cc', 'llm', 0.8),
];

const scenarios: Scenario[] = [
  { id: 'queue-wins', participants: trio, characters: {}, queue: ['Z'], spoken: [], lastSpeakerId: null, random01: 0.5 },
  { id: 'no-active', participants: [p('U', 'CHARACTER', 'absent', 'cu', 'user', 0.5)], characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.5 },
  { id: 'only-character', participants: [p('A', 'CHARACTER', 'active', 'ca', 'llm', 0.9)], characters: {}, queue: [], spoken: [], lastSpeakerId: 'A', random01: 0.5 },
  { id: 'only-character-user', participants: [p('U', 'CHARACTER', 'active', 'cu', 'user', 0.5)], characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.5 },
  // weighted picks: r chosen to land on A / B / C respectively (rv = r*2.0).
  { id: 'weighted-A', participants: trio, characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.1 }, // rv 0.2 < 0.9 → A
  { id: 'weighted-B', participants: trio, characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.5 }, // rv 1.0 → B
  { id: 'weighted-C', participants: trio, characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.95 }, // rv 1.9 → C
  // last speaker + spoken excluded.
  { id: 'eligible-excludes', participants: trio, characters: {}, queue: [], spoken: ['B'], lastSpeakerId: 'A', random01: 0.1 }, // only C eligible → C
  // P4.D63 (v4 `d553f72a`) — the archived filter. Mirrors v4's own new jest
  // case: an archived character whose seat somehow stayed `active` still
  // yields the user's turn.
  { id: 'archived-only-character', participants: [p('A', 'CHARACTER', 'active', 'ca', 'llm', 0.9)],
    characters: { ca: { archivedAt: '2026-08-10T00:00:00.000Z' } }, queue: [], spoken: [], lastSpeakerId: null, random01: 0.5 },
  // One of three archived: the rotation continues over the survivors, and the
  // weights are drawn from the SMALLER pool (so a wrong filter shifts the pick,
  // not just the count).
  { id: 'archived-one-of-three', participants: trio,
    characters: { cb: { talkativeness: 0.3, archivedAt: '2026-08-10T00:00:00.000Z' } },
    queue: [], spoken: [], lastSpeakerId: null, random01: 0.5 },
  // Two of three archived → the survivor takes the `only_character` branch.
  { id: 'archived-two-of-three', participants: trio,
    characters: { ca: { archivedAt: '2026-08-10T00:00:00.000Z' }, cb: { archivedAt: '2026-08-10T00:00:00.000Z' } },
    queue: [], spoken: [], lastSpeakerId: null, random01: 0.5 },
  // An EMPTY-STRING archivedAt is JS-falsy — no tombstone, so A still speaks.
  { id: 'archived-empty-string-is-not-archived', participants: [p('A', 'CHARACTER', 'active', 'ca', 'llm', 0.9)],
    characters: { ca: { archivedAt: '' } }, queue: [], spoken: [], lastSpeakerId: null, random01: 0.5 },
  // The queue wins BEFORE the archived filter (v4's step 1 is unconditional).
  { id: 'archived-but-queued', participants: trio,
    characters: { ca: { archivedAt: '2026-08-10T00:00:00.000Z' } },
    queue: ['A'], spoken: [], lastSpeakerId: null, random01: 0.5 },
  // user-controlled pick → reason user_turn, id kept.
  {
    id: 'user-pick',
    participants: [p('A', 'CHARACTER', 'active', 'ca', 'llm', 0.5), p('U', 'CHARACTER', 'active', 'cu', 'user', 0.5)],
    characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.7, // rv 0.7; A0.5,then U → U
  },
  // cycle wrap: both spoken, last = A → eligible empty, newCycle = [B] → B, wrap.
  { id: 'wrap', participants: [p('A', 'CHARACTER', 'active', 'ca', 'llm', 0.9), p('B', 'CHARACTER', 'active', 'cb', 'llm', 0.3)], characters: {}, queue: [], spoken: ['A', 'B'], lastSpeakerId: 'A', random01: 0.5 },
  // total weight zero → equal weights reset to 1.
  { id: 'zero-weights', participants: [p('A', 'CHARACTER', 'active', 'ca', 'llm', 0), p('B', 'CHARACTER', 'active', 'cb', 'llm', 0)], characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.6 }, // rv 1.2 → B
  // talkativeness fallback: participant null → character value → 0.5 default.
  { id: 'char-fallback', participants: [p('A', 'CHARACTER', 'active', 'ca', 'llm', null), p('B', 'CHARACTER', 'active', 'cb', 'llm', null)], characters: { ca: 0.9 }, queue: [], spoken: [], lastSpeakerId: null, random01: 0.5 }, // A0.9(char), B0.5(default) total1.4; rv0.7<0.9 → A
  // Bug 44 overlay: a single LLM seat is only_character without the overlay, and
  // a *user_turn* (pause for the human) once impersonated — the column stays 'llm'.
  { id: 'impersonated-off', participants: [p('p1', 'CHARACTER', 'active', 'char-1', 'llm', null)], characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.5 },
  { id: 'impersonated-on', participants: [p('p1', 'CHARACTER', 'active', 'char-1', 'llm', null)], characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.5, impersonating: ['p1'] },
  // Bug 44 overlay in a weighted pick: the pick lands on an impersonated LLM seat
  // (rv 0.2 < 0.9 → A), whose reason becomes user_turn via the overlay.
  { id: 'impersonated-weighted', participants: trio, characters: {}, queue: [], spoken: [], lastSpeakerId: null, random01: 0.1, impersonating: ['A'] },
  // --- P4.D195 / v4 bug 147: the client/server agreement room ------------
  //
  // v4 `1fefadb9a` ships `__tests__/unit/lib/chat/turn-manager/
  // client-server-agreement.test.ts` — its own guard that the client's local
  // recompute answers what the server already drew. v5's SPA has no such
  // recompute (it asks the server — the P4.D177 mechanism divergence), so the
  // CLIENT leg is a NO-COUNTERPART; what IS portable is the pair of pure
  // computations underneath, which v5 has as `select_next_speaker` and
  // `select_next_speaker_after_user_message`. Transcribed here as corpus rows
  // over v4's REAL functions rather than as Rust unit tests, because that is
  // the stronger proof — and because it is what finally drives the rotation
  // argument these two families had never touched.
  //
  // The room is v4's (Friday, chat `e59f8969`): the operator's Charlie plus
  // four LLM seats, one of them (Leilani) taken up by impersonation.
  // Talkativeness is lopsided on purpose — Abigail carries all the weight —
  // so the BLIND re-roll is deterministic and the divergence is exact rather
  // than statistical.
  //
  // (a) SIGHTED: the client's own state, given both columns, reaches the seat
  //     the rotation reserved — Charlie, the operator's seat → `user_turn`.
  {
    id: 'bug147-agreement-client-sighted',
    participants: agreementRoom,
    characters: agreementChars,
    queue: [],
    spoken: [ABIGAIL, BARAKA, GEN314],
    lastSpeakerId: LEILANI,
    cycleOrder: [LEILANI, CHARLIE],
    random01: 0.5,
    impersonating: [LEILANI],
  },
  // (a') SIGHTED, POST-POST: v4's "the client also agrees while its copy of
  //      the row is still pre-post" pair has a second half — the columns as
  //      `addMessage` rewrites them once Leilani's post lands
  //      (`computeSpokenThisCycleAfterMessage` / `computeCycleOrderAfterMessage`:
  //      Leilani joins the spoken set, the rotation shrinks to Charlie). The
  //      answer must not move. NOTE the shared `mkState` shape passes
  //      `userParticipantId: null` + `impersonating: [LEILANI]` where v4's
  //      test passes `CHARLIE` and no list — the pure function reads the seat
  //      through the overlay either way, and the rows still drive v4's REAL
  //      `selectNextSpeaker`; recorded rather than hidden.
  {
    id: 'bug147-agreement-client-sighted-post',
    participants: agreementRoom,
    characters: agreementChars,
    queue: [],
    spoken: [ABIGAIL, BARAKA, GEN314, LEILANI],
    lastSpeakerId: LEILANI,
    cycleOrder: [CHARLIE],
    random01: 0.5,
    impersonating: [LEILANI],
  },
  // (b) BLIND: the same room with NO rotation and NO spoken set — what the
  //     client saw before `1fefadb9a` put the two columns on the wire. The
  //     cycle-order branch is unreachable, so it falls through to a weighted
  //     roll and lands on Abigail: an LLM seat taking a floor the rotation had
  //     reserved for the operator, AND a seat that had already spoken.
  {
    id: 'bug147-agreement-client-blind',
    participants: agreementRoom,
    characters: agreementChars,
    queue: [],
    spoken: [],
    lastSpeakerId: LEILANI,
    cycleOrder: [],
    random01: 0.5,
    impersonating: [LEILANI],
  },
];

// selectNextSpeakerAfterUserMessage (Bug 50 fair rotation): projects the
// rotation one step past a user's just-typed (unpersisted) message. The persisted
// spokenThisCycle / turnQueue arrive as JSON strings (fail-soft parsed); the
// poster is set as lastSpeakerId and the full-rotation selectNextSpeaker runs.
// Math.random is pinned per case exactly as above so the Rust port (which takes
// random01) draws identically.
type AfterScenario = {
  id: string;
  participants: WirePart[];
  characters: WireChars;
  poster: string;
  persistedSpokenJson: string | null;
  turnQueueJson: string | null;
  userParticipantId: string | null;
  random01: number;
  draws?: number[];
  impersonating?: string[];
  /**
   * P4.D195: the persisted rotation, as the JSON string the 8th argument
   * takes. `undefined` leaves it unpassed — what every pre-existing row did,
   * so they stay byte-identical (the Rust side passed `None` to match).
   */
  cycleOrderJson?: string;
};

// The reported room: charlie (user) + lorian (LLM, impersonated) + kumar (sole
// real LLM). `impersonating: ['lorian']` puts Lorian under the user-driven
// overlay (controlledBy stays 'llm').
const fairRoom: WirePart[] = [
  p('charlie', 'CHARACTER', 'active', 'char-charlie', 'user', null),
  p('lorian', 'CHARACTER', 'active', 'char-lorian', 'llm', null),
  p('kumar', 'CHARACTER', 'active', 'char-kumar', 'llm', null),
];

const afterScenarios: AfterScenario[] = [
  // Kumar already spoke; Charlie posts → only Lorian eligible → user_turn (pause).
  { id: 'after-pause-to-impersonated', participants: fairRoom, characters: {}, poster: 'charlie', persistedSpokenJson: JSON.stringify(['kumar']), turnQueueJson: '[]', userParticipantId: 'charlie', random01: 0.5, impersonating: ['lorian'] },
  // Charlie already spoke; the human posts as Lorian → only Kumar eligible → no pause.
  { id: 'after-no-pause-to-llm', participants: fairRoom, characters: {}, poster: 'lorian', persistedSpokenJson: JSON.stringify(['charlie']), turnQueueJson: '[]', userParticipantId: 'charlie', random01: 0.5, impersonating: ['lorian'] },
  // Kumar+Lorian spoke; Charlie completes the 3-seat cycle → wrap, pick {lorian,kumar}.
  { id: 'after-cycle-wrap', participants: fairRoom, characters: {}, poster: 'charlie', persistedSpokenJson: JSON.stringify(['kumar', 'lorian']), turnQueueJson: '[]', userParticipantId: 'charlie', random01: 0.1, impersonating: ['lorian'] },
  // Queue wins ahead of the rotation.
  { id: 'after-queue-honored', participants: fairRoom, characters: {}, poster: 'charlie', persistedSpokenJson: JSON.stringify(['kumar']), turnQueueJson: JSON.stringify(['kumar']), userParticipantId: 'charlie', random01: 0.5, impersonating: ['lorian'] },
  // advancedJson === null no-op: poster already recorded AND no wrap → the
  // persisted set is kept. Extra seat 'dave' means the poster reappearing does
  // not complete the cycle (so computeSpokenThisCycleAfterMessage returns null).
  { id: 'after-noop-keeps-persisted', participants: [p('charlie', 'CHARACTER', 'active', 'char-charlie', 'user', null), p('kumar', 'CHARACTER', 'active', 'char-kumar', 'llm', null), p('dave', 'CHARACTER', 'active', 'char-dave', 'llm', null)], characters: {}, poster: 'charlie', persistedSpokenJson: JSON.stringify(['charlie']), turnQueueJson: '[]', userParticipantId: 'charlie', random01: 0.1, impersonating: [] },
  // Absent JSON (null) parses to []; a fresh cycle after the poster.
  { id: 'after-absent-json', participants: fairRoom, characters: {}, poster: 'charlie', persistedSpokenJson: null, turnQueueJson: null, userParticipantId: 'charlie', random01: 0.1, impersonating: ['lorian'] },
  // Bad JSON is fail-soft → []; behaves like absent.
  { id: 'after-bad-json', participants: fairRoom, characters: {}, poster: 'charlie', persistedSpokenJson: '{not json', turnQueueJson: '{bad', userParticipantId: 'charlie', random01: 0.1, impersonating: ['lorian'] },
  // No overlay: the same room but nobody impersonated → after Kumar spoke and
  // Charlie posts, Lorian (a plain LLM) is eligible and answers (no pause).
  { id: 'after-no-overlay-llm-answers', participants: fairRoom, characters: {}, poster: 'charlie', persistedSpokenJson: JSON.stringify(['kumar']), turnQueueJson: '[]', userParticipantId: 'charlie', random01: 0.5 },
  // P4.D195 / v4 bug 147: the SERVER leg of the agreement room — the exact
  // projection the orchestrator runs when the operator's post (typed as the
  // impersonated Leilani) lands. It must hand the floor to Charlie with
  // `user_turn`, which is the answer row (a) above reaches independently.
  // This is also the first row in this corpus to pass the 8th argument at all.
  {
    id: 'bug147-agreement-server',
    participants: agreementRoom,
    characters: agreementChars,
    poster: LEILANI,
    persistedSpokenJson: JSON.stringify([ABIGAIL, BARAKA, GEN314]),
    turnQueueJson: '[]',
    userParticipantId: CHARLIE,
    random01: 0.5,
    impersonating: [LEILANI],
    cycleOrderJson: JSON.stringify([LEILANI, CHARLIE]),
  },
];

type SelectRow = { kind: 'select'; id: string; scenario: Scenario; out: TurnSelectionResult; consumedDraws: number[] };
type AfterRow = { kind: 'select-after'; id: string; scenario: AfterScenario; out: TurnSelectionResult; consumedDraws: number[] };
const rows: Array<SelectRow | AfterRow> = [];

for (const s of scenarios) {
  const { out, consumed } = withRandom(s.draws ?? [s.random01], () =>
    selectNextSpeaker(asParts(s.participants), asChars(s.characters), mkState(s.queue, s.spoken, s.lastSpeakerId, s.cycleOrder ?? []), null, s.impersonating),
  );
  rows.push({ kind: 'select', id: s.id, scenario: s, out, consumedDraws: consumed });
}

for (const s of afterScenarios) {
  const { out, consumed } = withRandom(s.draws ?? [s.random01], () =>
    selectNextSpeakerAfterUserMessage(
      asParts(s.participants),
      asChars(s.characters),
      s.poster,
      s.persistedSpokenJson,
      s.turnQueueJson,
      s.userParticipantId,
      s.impersonating,
      s.cycleOrderJson,
    ),
  );
  rows.push({ kind: 'select-after', id: s.id, scenario: s, out, consumedDraws: consumed });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
