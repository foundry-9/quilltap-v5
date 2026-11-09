/**
 * Tier-1 oracle case — `lib/chat/established-fact.ts` (P4.D372, v4 `db6190b44`,
 * pin `624d0d25c`).
 *
 * Drives v4's REAL module (pure — no logger, no repositories) over a fixed
 * corpus and emits one NDJSON row per probe:
 *
 *   - `{op:'constants', …}` — every exported string, so the Rust constants are
 *     pinned against v4's values, not a transcription;
 *   - `{op:'predicates', input, announcement, seatLine, isFact, kind}` — the
 *     field grid (`isEstablishedFact` ∈ {absent, null, true, false, 1, "true"}
 *     × `systemSender` ∈ {absent, null, "", "host", "aurora"} × `systemKind` ∈
 *     {absent, "established-fact", "inform"});
 *   - `{op:'frame', body, kind, voice, out}`;
 *   - `{op:'strip', body, out}`;
 *   - `{op:'framedBody', input, voice, out}`;
 *   - `{op:'frameAll', label, voice, input, same, out}` — `same` is v4's
 *     "returns the SAME array" identity;
 *   - `{op:'count', label, input, seat, announcement}`.
 *
 * `input` objects are emitted as v4 saw them; an ABSENT key is absent.
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   $N/npx tsx $V5W/harness/oracle/cases/established-fact.ts \
 *     > /tmp/oracle-established-fact.ndjson
 */

import {
  ESTABLISHED_FACT_HOST_FRAME,
  ESTABLISHED_FACT_KIND,
  ESTABLISHED_FACT_PLAIN_FRAME,
  ESTABLISHED_FACT_REMINDER_HEADER,
  ESTABLISHED_FACT_SEAT_FRAME,
  ESTABLISHED_FACT_SPEAKER_LABEL,
  ESTABLISHED_FACT_TRANSCRIPT_RULE,
  countEstablishedFacts,
  establishedFactKindOf,
  frameEstablishedFact,
  frameEstablishedFacts,
  framedEstablishedFactBody,
  isEstablishedFact,
  isEstablishedFactAnnouncement,
  isEstablishedFactSeatLine,
  stripEstablishedFactFrame,
} from '@/lib/chat/established-fact'

const ABSENT = Symbol('absent')
type Cell = unknown

function emit(row: Record<string, unknown>): void {
  process.stdout.write(JSON.stringify(row) + '\n')
}

function build(entries: Array<[string, Cell]>): Record<string, unknown> {
  const o: Record<string, unknown> = {}
  for (const [k, v] of entries) if (v !== ABSENT) o[k] = v
  return o
}

emit({
  op: 'constants',
  kind: ESTABLISHED_FACT_KIND,
  speakerLabel: ESTABLISHED_FACT_SPEAKER_LABEL,
  seatFrame: ESTABLISHED_FACT_SEAT_FRAME,
  hostFrame: ESTABLISHED_FACT_HOST_FRAME,
  plainFrame: ESTABLISHED_FACT_PLAIN_FRAME,
  reminderHeader: ESTABLISHED_FACT_REMINDER_HEADER,
  transcriptRule: ESTABLISHED_FACT_TRANSCRIPT_RULE,
})

const FLAGS: Cell[] = [ABSENT, null, true, false, 1, 'true']
const SENDERS: Cell[] = [ABSENT, null, '', 'host', 'aurora']
const KINDS: Cell[] = [ABSENT, 'established-fact', 'inform']

for (const flag of FLAGS) {
  for (const sender of SENDERS) {
    for (const kind of KINDS) {
      const input = build([
        ['isEstablishedFact', flag],
        ['systemSender', sender],
        ['systemKind', kind],
      ])
      const m = input as never
      emit({
        op: 'predicates',
        input,
        announcement: isEstablishedFactAnnouncement(m),
        seatLine: isEstablishedFactSeatLine(m),
        isFact: isEstablishedFact(m),
        kind: establishedFactKindOf(m),
      })
    }
  }
}

const BODIES = [
  'The bridge to Ashford collapsed in the night.',
  '',
  'Line one\n\nline two — with ✨ and 漢字.',
  `${ESTABLISHED_FACT_SEAT_FRAME}\n\nalready framed (seat)`,
  `${ESTABLISHED_FACT_HOST_FRAME}\n\nalready framed (host)`,
  `${ESTABLISHED_FACT_PLAIN_FRAME}\n\nan operator body that starts with the plain opener`,
  `${ESTABLISHED_FACT_PLAIN_FRAME}\none newline only — not stripped`,
  `${ESTABLISHED_FACT_HOST_FRAME}\n\n${ESTABLISHED_FACT_SEAT_FRAME}\n\ndouble framed — one layer comes off`,
  `  ${ESTABLISHED_FACT_SEAT_FRAME}\n\nleading space — not stripped`,
  ESTABLISHED_FACT_PLAIN_FRAME,
]

for (const body of BODIES) {
  for (const kind of ['seat', 'announcement'] as const) {
    for (const voice of ['host', 'plain'] as const) {
      const out = frameEstablishedFact(body, { kind, voice })
      emit({ op: 'frame', body, kind, voice, out })
      // Re-frame idempotence, recorded per opener.
      emit({ op: 'frame', body: out, kind, voice, out: frameEstablishedFact(out, { kind, voice }) })
    }
  }
  emit({ op: 'strip', body, out: stripEstablishedFactFrame(body) })
}

const SEAT = { id: 's', role: 'USER', content: 'seat body', isEstablishedFact: true, participantId: 'p-user' }
const MACHINE = {
  id: 'f', role: 'ASSISTANT', content: 'machine body', opaqueContent: 'machine body',
  participantId: null, systemSender: 'host', systemKind: 'established-fact',
}
const MACHINE_NO_OPAQUE = {
  id: 'f2', role: 'ASSISTANT', content: 'machine body two', participantId: null,
  systemSender: 'host', systemKind: 'established-fact',
}
const MACHINE_STALE_OPAQUE = {
  id: 'f3', role: 'ASSISTANT', content: 'the edited body', opaqueContent: 'a stale opaque body',
  systemSender: 'host', systemKind: 'established-fact',
}
const BOTH = { id: 'b', content: 'both flags', isEstablishedFact: true, systemSender: 'host', systemKind: 'established-fact' }
const PLAIN = { id: 'p', role: 'ASSISTANT', content: 'a reply', participantId: 'p-alice' }
const STAFF = { id: 'st', role: 'ASSISTANT', content: 'an inform', systemSender: 'host', systemKind: 'inform' }
const NOT_BOOL = { id: 'nb', role: 'USER', content: 'numeric flag', isEstablishedFact: 1 }
const FLAGGED_STAFF = { id: 'fs', role: 'USER', content: 'flag + aurora', isEstablishedFact: true, systemSender: 'aurora' }
const NO_CONTENT_SEAT = { id: 'nc', role: 'USER', isEstablishedFact: true }
const NULL_CONTENT_MACHINE = { id: 'ncm', content: null, systemSender: 'host', systemKind: 'established-fact' }
const EMPTY_SENDER_SEAT = { id: 'es', content: 'empty sender', isEstablishedFact: true, systemSender: '' }

const ROWS = [SEAT, MACHINE, MACHINE_NO_OPAQUE, MACHINE_STALE_OPAQUE, BOTH, PLAIN, STAFF, NOT_BOOL,
  FLAGGED_STAFF, NO_CONTENT_SEAT, NULL_CONTENT_MACHINE, EMPTY_SENDER_SEAT]

for (const input of ROWS) {
  for (const voice of ['host', 'plain'] as const) {
    emit({ op: 'framedBody', input, voice, out: framedEstablishedFactBody(input as never, voice) })
  }
}

const HISTORIES: Array<[string, Array<Record<string, unknown>>]> = [
  ['empty', []],
  ['no facts', [PLAIN, STAFF, NOT_BOOL, FLAGGED_STAFF]],
  ['seat + machine + reply', [SEAT, MACHINE, PLAIN]],
  ['machine without opaqueContent', [MACHINE_NO_OPAQUE]],
  ['machine with a stale opaqueContent', [MACHINE_STALE_OPAQUE]],
  ['both flags', [BOTH]],
  ['no / null content', [NO_CONTENT_SEAT, NULL_CONTENT_MACHINE]],
  ['empty sender seat', [EMPTY_SENDER_SEAT, PLAIN]],
  ['every row', ROWS],
]

for (const [label, input] of HISTORIES) {
  for (const voice of ['host', 'plain'] as const) {
    const out = frameEstablishedFacts(input as never, { voice })
    emit({ op: 'frameAll', label, voice, input, same: out === input, out })
    // Idempotence: framing the framed history again.
    const again = frameEstablishedFacts(out as never, { voice })
    emit({ op: 'frameAll', label: `${label} (again)`, voice, input: out, same: again === out, out: again })
  }
  const c = countEstablishedFacts(input as never)
  emit({ op: 'count', label, input, seat: c.seat, announcement: c.announcement })
}
