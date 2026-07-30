/**
 * Tier-1 ORACLE for the deterministic day-reference resolver (v4 `505dcb1f`,
 * `lib/memory/day-references.ts` — P4.d26).
 *
 * Drives v4's REAL `resolveDayReference` over the committed corpus
 * (harness/oracle/fixtures/day-references.json) and emits one NDJSON row per
 * case: `{ id, tz, resolution }`, where `resolution` is null or
 * `{ timeRange: { from, to }, pastPointing, matched }`.
 *
 * TIMEZONE IS THE POINT. The module resolves calendar boundaries in the
 * SERVER-LOCAL zone, and the bug it fixes is precisely a UTC-vs-local
 * difference — so a TZ=UTC-only oracle could not tell the fix from the bug.
 * v4's own jest suite cannot pin TZ (jest hands the file a copy of
 * `process.env`, so V8's zone cache never sees the assignment); a plain tsx
 * script CAN, because the zone is read at process start. Every case's `now` is
 * an absolute instant, so the SAME corpus run under two zones is the
 * differential's local-calendar proof. Each row records the zone it ran in
 * (`Intl.DateTimeFormat().resolvedOptions().timeZone`) and the Rust side passes
 * that same IANA name explicitly (core never reads the process zone).
 *
 * Run (both legs — the second APPENDS; regenerate in ONE go so the file can
 * never hold half a vintage):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/node --import tsx $V5/harness/oracle/cases/day-references.ts \
 *     > /tmp/oracle-day-references.ndjson
 *   TZ=America/Chicago $N/node --import tsx $V5/harness/oracle/cases/day-references.ts \
 *     >> /tmp/oracle-day-references.ndjson
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import { resolveDayReference } from '@/lib/memory/day-references';

interface CaseSpec {
  id: string;
  text: string;
  now: string | null;
}

const here = dirname(fileURLToPath(import.meta.url));
const spec = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'day-references.json'), 'utf8'),
) as { cases: CaseSpec[] };

const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;

for (const c of spec.cases) {
  // `now: null` is v4's Invalid Date arm (`!Number.isFinite(now.getTime())`).
  const now = c.now === null ? new Date(NaN) : new Date(Date.parse(c.now));
  const r = resolveDayReference(c.text, now);
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
