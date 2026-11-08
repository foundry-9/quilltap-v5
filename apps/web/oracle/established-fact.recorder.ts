/**
 * The v4-side recorder behind `src/app/chat/established-fact.oracle.spec.ts`.
 *
 * v4 `db6190b44` added Deus Ex Machina: an operator line, or a Host row, that
 * the story treats as established fact. Its client-facing rules are four pure
 * predicates in `lib/chat/established-fact.ts`, the kind label / accent /
 * importance rows in `app/salon/[id]/components/system-message-labels.ts`, and
 * the never-collapsed arm in `app/salon/[id]/announcement-render-items.ts`
 * (`671c59a53`). THIS corpus, recorded from v4's REAL modules at the round pin
 * `624d0d25c`, is what proves v5's `chat/established-fact.ts`,
 * `chat/system-message-labels.ts` and `chat-view-model.ts`'s
 * `isAnnouncementChip` transcriptions.
 *
 * One row per message shape: every function's answer over the same input (the
 * `inventory-display.recorder.ts` idiom). The corpus crosses `isEstablishedFact`
 * (true / false / null / absent / 1 / 'true'), `systemSender` (absent / null /
 * '' / host / Host / carina / suparna / pascal / lantern) and `systemKind`
 * (absent / null / established-fact / Established-Fact / mail-delivery /
 * custom-tool-result / nudge / inform).
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Run it from a lane-unique pinned v4 worktree at `624d0d25c` (Node 24.13.1):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-<order>-624d0d25c
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" 624d0d25c
 * (cd "$PIN" && npm ci --offline --ignore-scripts --no-audit --no-fund && npm rebuild better-sqlite3)
 * export PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH
 * cp <V5>/apps/web/oracle/established-fact.recorder.ts "$PIN/"
 * rm -f <V5>/apps/web/src/testing/fixtures/established-fact.oracle.ndjson
 * cd "$PIN" && npx tsx established-fact.recorder.ts \
 *   > <V5>/apps/web/src/testing/fixtures/established-fact.oracle.ndjson
 * wc -l <V5>/apps/web/src/testing/fixtures/established-fact.oracle.ndjson
 * ```
 *
 * Expect the line count recorded in `src/testing/fixtures/README.md`'s
 * `established-fact.oracle.ndjson` section; a shorter file means the recorder
 * errored and the redirect already truncated the old one (the empty-file trap).
 * A second run must reproduce the file byte for byte (`cmp`). Pin marker: the
 * module does not exist before `db6190b44`, so an older tree cannot run this.
 */

import {
  establishedFactKindOf,
  isEstablishedFact,
  isEstablishedFactAnnouncement,
  isEstablishedFactSeatLine,
} from '@/lib/chat/established-fact';
import {
  isAlwaysExpandedAnnouncement,
  isCollapsedAnnouncement,
} from '@/app/salon/[id]/announcement-render-items';
import {
  getAnnouncementAccentClasses,
  getAnnouncementImportance,
  getSystemKindDisplayLabel,
} from '@/app/salon/[id]/components/system-message-labels';

const ABSENT = Symbol('absent');
type Cell<T> = T | typeof ABSENT;

const FACT_FLAGS: Array<[string, Cell<unknown>]> = [
  ['absent', ABSENT],
  ['true', true],
  ['false', false],
  ['null', null],
  ['1', 1],
  ["'true'", 'true'],
];
const SENDERS: Array<[string, Cell<string | null>]> = [
  ['absent', ABSENT],
  ['null', null],
  ["''", ''],
  ['host', 'host'],
  ['Host', 'Host'],
  ['carina', 'carina'],
  ['suparna', 'suparna'],
  ['pascal', 'pascal'],
  ['lantern', 'lantern'],
];
const KINDS: Array<[string, Cell<string | null>]> = [
  ['absent', ABSENT],
  ['null', null],
  ['established-fact', 'established-fact'],
  ['Established-Fact', 'Established-Fact'],
  ['mail-delivery', 'mail-delivery'],
  ['custom-tool-result', 'custom-tool-result'],
  ['nudge', 'nudge'],
  ['inform', 'inform'],
];

const rows: unknown[] = [];
const seen = new Set<string>();
let n = 0;

function record(id: string, cells: { isEstablishedFact: Cell<unknown>; systemSender: Cell<string | null>; systemKind: Cell<string | null> }): void {
  if (seen.has(id)) throw new Error(`duplicate vector label: ${id}`);
  seen.add(id);
  n += 1;
  const message: Record<string, unknown> = { id: `m-${n}`, role: 'ASSISTANT', content: 'The bridge is out.' };
  if (cells.isEstablishedFact !== ABSENT) message.isEstablishedFact = cells.isEstablishedFact;
  if (cells.systemSender !== ABSENT) message.systemSender = cells.systemSender;
  if (cells.systemKind !== ABSENT) message.systemKind = cells.systemKind;
  // v4's Message type is wider than these fields; the functions read only them.
  const m = message as never;
  rows.push({
    id,
    message,
    out: {
      isEstablishedFactAnnouncement: isEstablishedFactAnnouncement(m),
      isEstablishedFactSeatLine: isEstablishedFactSeatLine(m),
      isEstablishedFact: isEstablishedFact(m),
      establishedFactKindOf: establishedFactKindOf(m),
      isAlwaysExpandedAnnouncement: isAlwaysExpandedAnnouncement(m),
      isCollapsedAnnouncement: isCollapsedAnnouncement(m, new Set()),
      isCollapsedWhenExpanded: isCollapsedAnnouncement(m, new Set([`m-${n}`])),
      kindLabel: getSystemKindDisplayLabel(m),
      accent: getAnnouncementAccentClasses(m),
      importance: getAnnouncementImportance(m),
    },
  });
}

// The full cross of sender × kind, with the fact flag absent (the transcript's
// ordinary shape).
for (const [sName, sender] of SENDERS) {
  for (const [kName, kind] of KINDS) {
    record(`sender ${sName} × kind ${kName}`, { isEstablishedFact: ABSENT, systemSender: sender, systemKind: kind });
  }
}
// Every fact flag over the seat-line shapes (no sender) and the Host fact.
for (const [fName, flag] of FACT_FLAGS) {
  if (fName === 'absent') continue; // covered above
  for (const [sName, sender] of SENDERS) {
    for (const [kName, kind] of [KINDS[0], KINDS[1], KINDS[2]] as Array<[string, Cell<string | null>]>) {
      record(`flag ${fName} × sender ${sName} × kind ${kName}`, {
        isEstablishedFact: flag,
        systemSender: sender,
        systemKind: kind,
      });
    }
  }
}

for (const row of rows) process.stdout.write(`${JSON.stringify(row)}\n`);
