/**
 * Tier-1 differential oracle — every v4 display-date helper that renders in the
 * HOST's zone (P4.119, dogfood #121, ruled (a) 2026-09-29), run under TWO zones.
 *
 * v4 formats these with `toLocaleString` / `toLocaleDateString` /
 * `Intl.DateTimeFormat` and NO `timeZone`, so they follow the Node process's
 * zone. The harness long ran v4 under `TZ=UTC` only, so a port that always
 * rendered UTC could never be told apart from one that renders the host zone —
 * which is how #121 stayed invisible. This case drives v4's REAL exported
 * helpers over a committed corpus of DST-straddling and midnight-straddling
 * instants; run it under `TZ=UTC` and under `TZ=America/Chicago`. The first
 * row records the zone (`hostZone`), and the Rust side is fed that zone BY
 * ARGUMENT — it never reads the machine's.
 *
 * Why tsx, not jest: v4's `jest.config.ts` forces `process.env.TZ = 'UTC'`
 * before any worker starts, so a jest oracle cannot run in a second zone (the
 * mail-tools second-zone attempt recorded `tz: "UTC"` on every row under
 * `TZ=America/Chicago` — the per-row zone record caught it).
 *
 * Driven (all v4-real):
 *   - `formatDateTime(s, { monthStyle: 'long' })`       lib/format-time.ts      (H1)
 *   - `formatLetterDate` / `formatLetterHeading`        lib/post-office/instructions.ts
 *   - `buildReplyPreface`                               lib/post-office/mailbox.ts
 *   - `buildSuparnaMailWhisper` / `buildSuparnaMailLLMContext`
 *                                                       lib/services/suparna-notifications/writer.ts
 *   - `formatWebSearchResults` (built-in formatter)     lib/tools/handlers/web-search-handler.ts (H2)
 *   - `renderAlmanackMarkdown` over v4's own fixture with the corpus's dates
 *     planted — a space-form (`datetime('now')` vintage) stamp and a zone-less
 *     `T` stamp among them (H3/H4 + the zone-less LOCAL parse)
 *
 * Run (Node 24, from the v4 checkout or a pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/node --import tsx $V5W/harness/oracle/cases/host-zone-dates.ts \
 *     > /tmp/oracle-host-zone-dates.ndjson
 *   TZ=America/Chicago $N/node --import tsx $V5W/harness/oracle/cases/host-zone-dates.ts \
 *     > /tmp/oracle-host-zone-dates-chicago.ndjson
 *   TZ=XST-9 $N/node --import tsx $V5W/harness/oracle/cases/host-zone-dates.ts \
 *     > /tmp/oracle-host-zone-dates-posix.ndjson
 *
 * The third arm (P4.140) is a POSIX `TZ` rule with NO daylight saving — the
 * one rule shape ICU honours (it ignores a rule WITH DST and renders the
 * host's `/etc/localtime`, so a DST-rule arm would not be machine-independent
 * and is deliberately absent). v5 honours every rule; this arm proves the
 * renders agree where v4 honours one too.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import { formatDateTime } from '@/lib/format-time';
import { formatLetterDate, formatLetterHeading } from '@/lib/post-office/instructions';
import { buildReplyPreface, type DeliveredLetterSummary } from '@/lib/post-office/mailbox';
import {
  buildSuparnaMailLLMContext,
  buildSuparnaMailWhisper,
} from '@/lib/services/suparna-notifications/writer';
import { formatWebSearchResults } from '@/lib/tools/handlers/web-search-handler';
import { renderAlmanackMarkdown } from '@/lib/tools/almanack/render';
import { makeAlmanackFixture } from '@/__tests__/helpers/almanack/fixture';

interface Corpus {
  instants: string[];
  letters: DeliveredLetterSummary[];
  searchResults: Array<{ title: string; url: string; snippet: string; publishedDate?: string }>;
  almanackDates: {
    generatedAt: string;
    lastMigrationAt: string;
    backupNewestDate: string;
    backupOldestDate: string;
    lastMaintenanceSweepAt: string;
    oldestPendingScheduledAt: string;
  };
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'host-zone-dates.json'), 'utf8'),
) as Corpus;

const out = (row: Record<string, unknown>) => process.stdout.write(JSON.stringify(row) + '\n');

// `envTz` (P4.140): under a POSIX `TZ` rule ICU resolves no zone name
// (`resolvedOptions().timeZone` is `undefined` and drops out of the row), so
// the third arm identifies its run by the environment value instead.
out({
  op: 'hostZone',
  label: 'tz',
  tz: new Intl.DateTimeFormat().resolvedOptions().timeZone,
  envTz: process.env.TZ ?? null,
});

for (const s of corpus.instants) {
  out({ op: 'formatDateTime', label: s, result: formatDateTime(s, { monthStyle: 'long' }) });
  out({ op: 'formatLetterDate', label: s, result: formatLetterDate(s) });
  out({ op: 'buildReplyPreface', label: s, result: buildReplyPreface('Line one\n\nLine two', s) });
}
// An unrecorded `sentAt` takes the fallback phrase on both helpers.
out({ op: 'formatLetterDate', label: '', result: formatLetterDate('') });
out({ op: 'buildReplyPreface', label: '', result: buildReplyPreface('Hi.', '') });

corpus.letters.forEach((letter, i) => {
  out({ op: 'formatLetterHeading', label: letter.path, result: formatLetterHeading(letter, i + 1) });
});
out({ op: 'suparnaWhisper', label: 'all', result: buildSuparnaMailWhisper(corpus.letters) });
out({ op: 'suparnaLlmContext', label: 'all', result: buildSuparnaMailLLMContext(corpus.letters) });

out({ op: 'formatWebSearchResults', label: 'all', result: formatWebSearchResults(corpus.searchResults as never) });

// The Almanack: v4's own fixture (with the field its helper omits — see
// `almanack-render.test.ts`) and the corpus's dates planted in every
// date-rendering slot the fixture populates.
const data = makeAlmanackFixture();
data.featureConfig.impersonationVoiceRewrite = false;
const a = corpus.almanackDates;
data.generatedAt = a.generatedAt;
data.migrationState.lastMigrationAt = a.lastMigrationAt;
data.backupStatus[0].newestDate = a.backupNewestDate;
data.backupStatus[0].oldestDate = a.backupOldestDate;
data.instanceSettings.lastMaintenanceSweepAt = a.lastMaintenanceSweepAt;
data.backgroundJobs.oldestPendingScheduledAt = a.oldestPendingScheduledAt;
out({ op: 'almanackRender', label: 'planted', data, result: renderAlmanackMarkdown(data) });
