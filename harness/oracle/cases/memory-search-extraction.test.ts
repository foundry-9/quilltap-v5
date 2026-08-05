/**
 * @jest-environment node
 *
 * Tier-1 ORACLE for the SEARCH-side memory extraction (v4
 * `extractMemorySearchKeywords`, lib/memory/cheap-llm-tasks/memory-tasks.ts):
 * the prompt build (system prompt bytes, the TODAY line rendered from the
 * ExtractionClock, the conversation window slice/truncation) and the response
 * parse (keywords / temporal / context / paraphrase plus the episodic signals:
 * strict-`=== true` retrospective, the timeRange regex + finite-Date.parse +
 * full-day normalization + from<=to validation, the entities trim/cap).
 *
 * SPLIT from the memory-tasks tier-1 family (P4.d13): the CREATION-side cases
 * (`memory-tasks-tier1`) stay at their `7e6d13e5` vintage until round 3 ports
 * the clocked creation prompts; THIS family regenerates at `8bf3cb5f`+. Same
 * mock seam as memory-tasks-tier1: only `executeCheapLLMTask` is mocked,
 * capturing the exact messages and feeding the corpus response into the REAL
 * parser the extractor passes in.
 *
 * Every case supplies an explicit clock (the `clock: undefined` arm would read
 * the wall clock — nondeterministic, and the Rust side pins the same
 * fallback separately).
 *
 * ## The zone, and why it does NOT come from `TZ`
 *
 * This family has two zone legs (the TODAY line and the day-reference scan
 * resolve their calendar in the server-local zone; under UTC the fix is
 * indistinguishable from the bug). Since v4 `f7f1a956`, `jest.config.ts`
 * assigns `process.env.TZ = 'UTC'` before Jest forks its workers, so an
 * env-passed `TZ=America/Chicago` on the command line is silently clobbered
 * and the Chicago leg would quietly re-record the UTC one. The zone therefore
 * arrives in **`QT_ORACLE_TZ`**, is re-applied here at module load — before
 * anything in this file touches a `Date`, which is when Node still honours the
 * assignment — and is then PROVEN to have taken, at a winter and a summer
 * instant, against an independently computed offset. The zone is also stamped
 * into the NDJSON as a `zone` marker line so the Rust side can refuse an
 * oracle recorded for the wrong leg (a silent clobber was the whole hazard).
 *
 * Run from the v4 server checkout under Node 24, once per leg:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   GS=$V5/harness/oracle/lib/jest-zone-globalsetup.cjs
 *   QT_ORACLE_TZ=UTC QT_ORACLE_OUT=/tmp/oracle-distill.ndjson \
 *     $N/npx jest --silent --globalSetup "$GS" --roots "$PWD" --roots "$V5/harness/oracle/cases" -- memory-search-extraction
 *   QT_ORACLE_TZ=America/Chicago QT_ORACLE_OUT=/tmp/oracle-distill-chicago.ndjson \
 *     $N/npx jest --silent --globalSetup "$GS" --roots "$PWD" --roots "$V5/harness/oracle/cases" -- memory-search-extraction
 */

// ── The zone GUARD (see the header) ──────────────────────────────────────────
// This file cannot SET the zone: `jest-environment-node` hands the test a deep
// copy of `process`, so an in-worker `process.env.TZ = …` writes to a sandbox
// object libuv never reads. The pin comes from
// `--globalSetup <v5>/harness/oracle/lib/jest-zone-globalsetup.cjs`; this
// proves it took.
const ORACLE_ZONE = process.env.QT_ORACLE_TZ ?? 'UTC';

/** The offset (minutes WEST of UTC, `getTimezoneOffset`'s sign) `zone` has at
 *  `at` — computed with an EXPLICIT `timeZone`, so it is independent of
 *  whatever the process default happens to be. */
function zoneOffsetMinutes(zone: string, at: Date): number {
  const parts = Object.fromEntries(
    new Intl.DateTimeFormat('en-US', {
      timeZone: zone,
      hour12: false,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    })
      .formatToParts(at)
      .map((p) => [p.type, p.value])
  ) as Record<string, string>;
  const asUtc = Date.UTC(
    Number(parts.year),
    Number(parts.month) - 1,
    Number(parts.day),
    Number(parts.hour) % 24,
    Number(parts.minute),
    Number(parts.second)
  );
  return (at.getTime() - asUtc) / 60000;
}

for (const instant of ['2026-01-15T12:00:00Z', '2026-07-15T12:00:00Z']) {
  const at = new Date(instant);
  const want = zoneOffsetMinutes(ORACLE_ZONE, at);
  if (at.getTimezoneOffset() !== want) {
    throw new Error(
      `zone pin did not take: process zone reports ${at.getTimezoneOffset()} at ` +
        `${instant}, ${ORACLE_ZONE} is ${want}. Pass ` +
        '--globalSetup <v5>/harness/oracle/lib/jest-zone-globalsetup.cjs; v4 ' +
        'jest.config.ts pins UTC and a bare TZ= on the command line is clobbered.'
    );
  }
}

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

jest.mock('@/lib/memory/cheap-llm-tasks/core-execution', () => ({
  __esModule: true,
  executeCheapLLMTask: jest.fn(),
}));

import { extractMemorySearchKeywords } from '@/lib/memory/cheap-llm-tasks/memory-tasks';
import { executeCheapLLMTask } from '@/lib/memory/cheap-llm-tasks/core-execution';

interface CaseSpec {
  name: string;
  messages: Array<{ role: string; content: string }>;
  characterName: string;
  clock: { nowIso: string; timelineMode: 'realtime' | 'narrative' };
  responseText: string;
}

test('memory search-extraction oracle', async () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'memory-search-extraction.json'), 'utf8')
  ) as { cases: CaseSpec[] };
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const selection = {
    provider: 'ANTHROPIC',
    modelName: 'canned-cheap',
    isLocal: false,
  } as never;

  const lines: string[] = [];

  for (const c of spec.cases) {
    const calls: Array<{ messages: Array<{ role: string; content: string }> }> = [];
    jest
      .mocked(executeCheapLLMTask)
      .mockImplementation(async (_sel: unknown, messages: unknown, _uid: unknown, parse: unknown) => {
        calls.push({
          messages: (messages as Array<{ role: string; content: string }>).map((m) => ({
            role: m.role,
            content: m.content,
          })),
        });
        return {
          success: true,
          result: (parse as (s: string) => unknown)(c.responseText),
          usage: { promptTokens: 100, completionTokens: 20, totalTokens: 120 },
        } as never;
      });

    const r = await extractMemorySearchKeywords(
      c.messages as never,
      c.characterName,
      selection,
      'user-1',
      'chat-1',
      'char-1',
      c.clock
    );

    lines.push(
      JSON.stringify({
        name: c.name,
        calls,
        success: r.success,
        hasUsage: r.usage !== undefined,
        result: r.result ?? null,
      })
    );
  }

  // The leg marker: the Rust side refuses an oracle recorded for another zone,
  // so a future TZ clobber is LOUD instead of a misleading corpus-sensitivity
  // failure. Emitted FIRST so a truncated file still carries it.
  fs.writeFileSync(
    outPath,
    [JSON.stringify({ kind: 'zone', zone: ORACLE_ZONE }), ...lines].join('\n') + '\n'
  );
  expect(lines.length).toBe(spec.cases.length);
});
