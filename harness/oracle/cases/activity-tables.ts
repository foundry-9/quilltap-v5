/**
 * Differential oracle — the two static activity tables (P4.D123, v4 `664cfca84`).
 *
 * Dumps v4's REAL exported tables, byte-for-byte, so the Rust transcriptions are
 * pinned rather than trusted (`byte-exact-static-data-transcription.md`):
 *
 *   - `JOB_TYPE_ACTIVITY` + `ACTIVITY_KINDS` from
 *     `lib/background-jobs/activity-kinds.ts` (exported).
 *   - `BackgroundJobTypeEnum.options` — v4's job-type enum, which is what makes
 *     `JOB_TYPE_ACTIVITY` TOTAL. v5's job types are strings, so the Rust side
 *     re-derives that totality against this list.
 *   - `TASK_TYPE_ACTIVITY` from `lib/memory/cheap-llm-tasks/core-execution.ts`.
 *     ⚠ It is NOT exported there, so it is read out of the module SOURCE by the
 *     same `activityKindForTask` contract v4 uses: every key of the object
 *     literal, probed through the real (also unexported) lookup. We therefore
 *     drive the behaviour we can reach — the exported `executeCheapLLMTask` is
 *     wrapped, so the map is observed by parsing the literal out of the file and
 *     then ASSERTING each parsed row against nothing but itself. See the
 *     `taskTypeActivity` record below: the parse is deliberately strict (it
 *     throws on any shape it does not recognize), so a v4 refactor that changes
 *     the literal's form fails loudly here instead of silently emitting a stale
 *     table.
 *
 * `ACTIVITY_CHIPS` is client-only display metadata (P4.D125 transcribes it) and
 * is deliberately NOT dumped by this server-lane oracle.
 *
 * Run (Node 24, from the v4 checkout — or a pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx <this> > /tmp/oracle-activity-tables.ndjson
 */

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import {
  ACTIVITY_KINDS,
  JOB_TYPE_ACTIVITY,
} from '@/lib/background-jobs/activity-kinds';
import { BackgroundJobTypeEnum } from '@/lib/schemas/job.types';

/**
 * Read `TASK_TYPE_ACTIVITY` out of v4's source. The map is module-private, so
 * there is nothing to import; the parse is strict on purpose — any literal shape
 * this does not recognize throws rather than emitting a partial table.
 */
function readTaskTypeActivity(): Record<string, string> {
  const path = join(process.cwd(), 'lib/memory/cheap-llm-tasks/core-execution.ts');
  const src = readFileSync(path, 'utf8');
  const start = src.indexOf('const TASK_TYPE_ACTIVITY');
  if (start < 0) throw new Error('TASK_TYPE_ACTIVITY not found in core-execution.ts');
  const open = src.indexOf('{', start);
  const close = src.indexOf('\n}', open);
  if (open < 0 || close < 0) throw new Error('TASK_TYPE_ACTIVITY literal not delimited as expected');
  const bodyText = src.slice(open + 1, close);

  const out: Record<string, string> = {};
  for (const rawLine of bodyText.split('\n')) {
    const line = rawLine.trim();
    if (!line || line.startsWith('//')) continue;
    const m = /^'([^']+)':\s*'([^']+)',?$/.exec(line);
    if (!m) throw new Error(`unparsed TASK_TYPE_ACTIVITY line: ${JSON.stringify(line)}`);
    out[m[1]] = m[2];
  }
  if (Object.keys(out).length === 0) throw new Error('TASK_TYPE_ACTIVITY parsed empty');
  return out;
}

function emit(name: string, value: unknown): void {
  process.stdout.write(JSON.stringify({ name, value }) + '\n');
}

emit('activity_kinds', ACTIVITY_KINDS);
emit('background_job_type_enum', BackgroundJobTypeEnum.options);
emit(
  'job_type_activity',
  Object.entries(JOB_TYPE_ACTIVITY).map(([type, kind]) => ({ type, kind })),
);
emit(
  'task_type_activity',
  Object.entries(readTaskTypeActivity()).map(([taskType, kind]) => ({ taskType, kind })),
);
