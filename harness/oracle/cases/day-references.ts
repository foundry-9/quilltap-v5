/**
 * Tier-1 ORACLE for the read-side day-reference resolver at the pin
 * (`83334b9a5`) — v4 `lib/memory/time/clock.ts`'s
 * `resolveDayReference(text, now, timeZone)` (P4.D289 re-aim; first ported as
 * P4.d26 against `505dcb1f`'s `lib/memory/day-references.ts`, which v4
 * deleted in `e0ac7493a` — bug 212 moved it onto the ONE memory calendar).
 *
 * Drives v4's REAL `resolveDayReference` (imported by FILE path, never the
 * `time/index.ts` barrel) over the committed corpus
 * (harness/oracle/fixtures/day-references.json) and emits one NDJSON row per
 * (zone, case): `{ id, tz, resolution }`, where `resolution` is null or
 * `{ timeRange: { from, to }, pastPointing, matched }`.
 *
 * ZONES ARE EXPLICIT. At the pin the resolver takes its calendar as an
 * argument (the memory calendar — never the process zone), so the case walks
 * the corpus once per zone in `ZONES`, in order, passing each EXPLICITLY; the
 * process zone is irrelevant (the recipe pins `TZ=UTC` by convention). Every
 * `now` is an absolute instant, so the same corpus under two zones is still
 * the local-calendar proof: the Rust side requires UTC and a DST-carrying zone
 * and requires the zone blocks to disagree somewhere.
 *
 * Run from inside a v4 checkout (or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/day-references.ts" > /tmp/oracle-day-references.ndjson
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import { resolveDayReference } from '@/lib/memory/time/clock';

interface CaseSpec {
  id: string;
  text: string;
  now: string | null;
}

const here = dirname(fileURLToPath(import.meta.url));
const spec = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'day-references.json'), 'utf8'),
) as { cases: CaseSpec[] };

/** The zones the corpus is walked in, in this order (one block each). */
const ZONES = ['UTC', 'America/Chicago', 'Pacific/Chatham'];

for (const tz of ZONES) {
  for (const c of spec.cases) {
    // `now: null` is v4's Invalid Date arm (`!Number.isFinite(clock.nowMs)`).
    const now = c.now === null ? new Date(NaN) : new Date(Date.parse(c.now));
    const r = resolveDayReference(c.text, now, tz);
    process.stdout.write(
      JSON.stringify({
        id: c.id,
        tz,
        resolution: r
          ? { timeRange: { from: r.timeRange.from, to: r.timeRange.to }, pastPointing: r.pastPointing, matched: r.matched }
          : null,
      }) + '\n',
    );
  }
}
