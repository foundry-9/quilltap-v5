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
 * fallback separately). Run with TZ=UTC (Date.parse on zone-less forms).
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-distill.ndjson \
 *     $N/npx jest --silent --roots "$PWD" --roots "$V5/harness/oracle/cases" -- memory-search-extraction
 */

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

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  expect(lines.length).toBe(spec.cases.length);
});
