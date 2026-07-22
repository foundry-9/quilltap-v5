/**
 * @jest-environment node
 *
 * Tier-1 ORACLE for the memory-extraction pure leaves (v4
 * `lib/memory/cheap-llm-tasks/memory-tasks.ts`): the SELF/OTHER extraction
 * prompt builders (preambles, orienting-context footer, CONTEXT footer, the
 * shared turn-context renderer) and the candidate response parsers
 * (`parseMemoryCandidateArray` / `parseOtherCandidatesBySubject` /
 * `coerceMemoryCandidate` / `applyTargetingTags`, plus `stripCodeFences`).
 *
 * Drives v4's REAL `extractSelfMemoriesFromTurn` / `extractOtherMemoriesFromTurn`
 * with ONLY `executeCheapLLMTask` mocked — the same seam v4's own
 * `__tests__/memory-extraction-tags.test.ts` uses. The mock captures the exact
 * messages the extractor built (proving the prompt bytes) and feeds the corpus
 * row's canned `responseText` straight into the real parser the extractor
 * passes in (proving the parse). Everything exercised is v4's real code.
 *
 * Runs under v4's JEST (not tsx) because the seam is a module export only
 * `jest.mock` can replace. The file lives in the v5 harness tree; v4's jest
 * resolves it via an extra `--roots`, with `@/` mapped to v4.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-memory-tasks.ndjson \
 *     $N/npx jest --silent --roots "$PWD" --roots "$V5/harness/oracle/cases" -- memory-tasks-tier1
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

jest.mock('@/lib/memory/cheap-llm-tasks/core-execution', () => ({
  __esModule: true,
  executeCheapLLMTask: jest.fn(),
}));

import {
  extractSelfMemoriesFromTurn,
  extractOtherMemoriesFromTurn,
} from '@/lib/memory/cheap-llm-tasks/memory-tasks';
import { executeCheapLLMTask } from '@/lib/memory/cheap-llm-tasks/core-execution';

interface CaseSpec {
  kind: 'self' | 'other';
  name: string;
  transcript: unknown;
  targetCharacterId?: string;
  canonBlock?: string;
  observerCharacterId?: string;
  subjects?: unknown[];
  resolvedMaxTokens: number | null;
  inAutonomousRoom: boolean;
  orienting?: { projectDescription?: string | null; chatContextSummary?: string | null };
  clock?: { nowIso: string; timelineMode: string; narrativeNow?: string | null };
  responseText: string;
}

test('memory-tasks tier-1 oracle', async () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'memory-tasks-tier1.json'), 'utf8')
  ) as { cases: CaseSpec[] };
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // The selection is opaque to the extractor (it only forwards it to the
  // mocked executor), so a minimal stub suffices.
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

    let success: boolean;
    let hasUsage: boolean;
    let result: unknown;

    if (c.kind === 'self') {
      const r = await extractSelfMemoriesFromTurn(
        c.transcript as never,
        c.targetCharacterId!,
        c.canonBlock!,
        selection,
        'user-1',
        undefined,
        'chat-1',
        c.resolvedMaxTokens ?? undefined,
        c.inAutonomousRoom,
        c.orienting ?? undefined,
        (c.clock ?? undefined) as never
      );
      success = r.success;
      hasUsage = r.usage !== undefined;
      result = r.result ?? null;
    } else {
      const r = await extractOtherMemoriesFromTurn(
        c.transcript as never,
        c.observerCharacterId!,
        (c.subjects ?? []) as never,
        selection,
        'user-1',
        undefined,
        'chat-1',
        c.resolvedMaxTokens ?? undefined,
        c.inAutonomousRoom,
        c.orienting ?? undefined,
        (c.clock ?? undefined) as never
      );
      success = r.success;
      hasUsage = r.usage !== undefined;
      // Map → plain object in insertion order (the subjects' order).
      result = r.result ? Object.fromEntries(r.result) : null;
    }

    lines.push(JSON.stringify({ name: c.name, calls, success, hasUsage, result }));
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  expect(lines.length).toBe(spec.cases.length);
});
