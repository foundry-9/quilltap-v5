/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the proactive pre-compute distill (P4.19): drives v4's REAL
 * `runPreContextPreCompute` (`lib/services/chat-message/pre-compute.service.ts`,
 * which runs the REAL `proactiveRecallTask` → the REAL
 * `extractMemorySearchKeywords` parse → the REAL `searchMemoriesSemantic`) over a
 * fresh copy of the committed episodic-recall fixture per case.
 *
 * ONLY the cheap-LLM executor is mocked (`executeCheapLLMTask` — the corpus row's
 * canned distill text feeds v4's REAL parse callback; a `fail` row returns
 * `{ success: false }`). Compression is disabled (`compressionEnabled: false`) so
 * the sibling compressionTask no-ops. Embeddings run REAL through the fixture's
 * fitted BUILTIN TF-IDF profile — deterministic on both sides. `Date.now()` +
 * `new Date()` are frozen to the corpus `$nowMs` (the search decay seam + the
 * distill TODAY line; the Rust side passes the same value as `now_ms`).
 *
 * Emits one NDJSON line per case:
 * `{ name, distillPrompt, preSearchedMemories, recallSignals }`.
 * `preSearchedMemories` is `null` (v4 undefined) or the ordered list of
 * `{ id, score, usedEmbedding, effectiveWeight, rawWeight, recallAdjustment }`;
 * `recallSignals` is `null` or v4's `MemorySearchExtraction` JSON. The search
 * path's `lastAccessedAt` bump never reaches the emitted shape.
 *
 * `distillPrompt` (P4.20) is the verbatim `LLMMessage[]` v4 handed the cheap-LLM
 * executor — `null` when the task bailed before distilling. Without it this
 * family cannot see the WINDOW at all: the canned distill answers the same text
 * whatever it is given, so `messagesSinceLastSpoke` could window anything and
 * every case would still pass. That blind spot is what let the P4.19 pre-compute
 * ship green; recording the prompt closes it, and pins the 20-message cap and
 * the 500-unit per-message truncation on the way past.
 *
 * Real-DB-under-jest (memory-gate-tier3 recipe): resetModules + doMock past
 * jest.setup's global DB mocks + better-sqlite3 → better-sqlite3-multiple-
 * ciphers. Stage the case OUTSIDE any `.claude/` path (v4's jest ignores it).
 *
 * Run (Node 24, from the v4 checkout):
 *   see precompute_equivalence.rs header for the full STAGE recipe.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface DistillSpec {
  kind: 'json' | 'fail';
  text?: string;
}
interface CaseSpec {
  name: string;
  characterId: string;
  characterName: string;
  characterParticipantId: string;
  presentAboutCharacterIds: string[];
  isContinueMode: boolean;
  content: string;
  chat: Record<string, unknown>;
  dangerSettings?: Record<string, unknown>;
  existingMessages: Array<Record<string, unknown>>;
  distill: DistillSpec;
}
interface Spec {
  $nowMs: number;
  userId: string;
  cheapSelection: Record<string, unknown>;
  allProfiles?: Record<string, unknown>[];
  cases: CaseSpec[];
}

function normalizeMemories(pre: unknown): unknown {
  if (!pre || !Array.isArray(pre)) return null;
  return pre.map((r: Record<string, any>) => ({
    id: r.memory?.id ?? null,
    score: r.score,
    usedEmbedding: r.usedEmbedding,
    effectiveWeight: r.effectiveWeight,
    rawWeight: r.rawWeight,
    recallAdjustment: r.recallAdjustment
      ? {
          multiplier: r.recallAdjustment.multiplier,
          fired: r.recallAdjustment.fired,
          blendedBefore: r.recallAdjustment.blendedBefore,
          blendedAfter: r.recallAdjustment.blendedAfter,
        }
      : null,
  }));
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'precompute-cases.json'), 'utf8'),
  ) as Spec;
  const fixtureSpec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'episodic-recall.json'), 'utf8'),
  ) as { testPepperBase64: string };

  const mainFixture = process.env.QT_FIXTURE_ER_MAIN;
  const mountFixture = process.env.QT_FIXTURE_ER_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_ER_MAIN and QT_FIXTURE_ER_MOUNT must point at the fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  // Freeze the WHOLE wall clock (constructor + now): the search decay reads
  // `new Date()` (memory-weighting) and the distill TODAY line reads
  // `new Date().toISOString()`. The Rust side receives the same value as `now_ms`.
  const RealDate = Date;
  const FakeDate = class extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) {
        super(spec.$nowMs);
      } else {
        // @ts-expect-error variadic forwarding
        super(...args);
      }
    }
    static now(): number {
      return spec.$nowMs;
    }
  } as DateConstructor;
  (global as { Date: DateConstructor }).Date = FakeDate;

  const lines: string[] = [];

  for (const c of spec.cases) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-precompute-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main-work.db');
    const mountWork = join(scratch, 'mount-work.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);

    process.env.ENCRYPTION_MASTER_PEPPER = fixtureSpec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () =>
      jest.requireActual('@/lib/database/repositories'),
    );
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/embedding/vector-store', () =>
      jest.requireActual('@/lib/embedding/vector-store'),
    );
    jest.doMock('@/lib/embedding/embedding-service', () =>
      jest.requireActual('@/lib/embedding/embedding-service'),
    );
    // The ONE model mock: the cheap-LLM executor. The canned text runs through
    // v4's REAL parse callback (the same seam recall-replay / memory-tasks use).
    // The `messages` argument is CAPTURED, not ignored (P4.20): it is the only
    // place the windowed conversation becomes observable, since the canned answer
    // is the same whatever the prompt says.
    let distillPrompt: Array<{ role: string; content: string }> | null = null;
    // P4.68: the selection the executor actually ran with. This is the ONLY
    // place the uncensored reroute becomes observable — the swap changes WHICH
    // profile does the distill, not the prompt or the canned answer, so with
    // `_sel` ignored the family could not see `resolveUncensoredCheapLLMSelection`
    // at all (P4.D141 measured that: forcing `shouldUseUncensoredRoute` false
    // left it GREEN). Only the fields the Rust side can also see are recorded.
    let cheapSelectionUsed: { provider: unknown; modelName: unknown; baseUrl: unknown } | null =
      null;
    jest.doMock('@/lib/memory/cheap-llm-tasks/core-execution', () => ({
      __esModule: true,
      executeCheapLLMTask: jest.fn(
        async (_sel: unknown, messages: unknown, _uid: unknown, parse: unknown) => {
          const sel = _sel as { provider?: unknown; modelName?: unknown; baseUrl?: unknown };
          cheapSelectionUsed = {
            provider: sel?.provider ?? null,
            modelName: sel?.modelName ?? null,
            baseUrl: sel?.baseUrl ?? null,
          };
          distillPrompt = (messages as Array<{ role: string; content: string }>).map((m) => ({
            role: m.role,
            content: m.content,
          }));
          if (c.distill.kind === 'fail') {
            return { success: false, error: 'canned failure' } as never;
          }
          return {
            success: true,
            result: (parse as (s: string) => unknown)(c.distill.text ?? ''),
            usage: { promptTokens: 100, completionTokens: 20, totalTokens: 120 },
          } as never;
        },
      ),
    }));

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    await initializeDatabase();
    const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
    await initializePlugins();

    const { runPreContextPreCompute } = await import(
      '@/lib/services/chat-message/pre-compute.service'
    );

    const controller = { enqueue: () => undefined } as never;
    const encoder = new TextEncoder();

    const result = await runPreContextPreCompute({
      chatId: (c.chat.id as string) ?? 'chat',
      userId: spec.userId,
      chat: c.chat as never,
      character: { id: c.characterId, name: c.characterName } as never,
      characterParticipant: { id: c.characterParticipantId } as never,
      isMultiCharacter: c.presentAboutCharacterIds.length > 1,
      presentAboutCharacterIds: c.presentAboutCharacterIds,
      isContinueMode: c.isContinueMode,
      content: c.content,
      existingMessages: c.existingMessages as never,
      compressionEnabled: false,
      bypassCompression: false,
      cheapLLMSelection: spec.cheapSelection as never,
      dangerSettings: (c.dangerSettings ?? { mode: 'OFF' }) as never,
      allProfiles: (c.allProfiles ?? []) as never,
      controller,
      encoder,
    });
    result.stopKeepAlive();

    // Let fire-and-forget promises settle before closing.
    await new Promise((resolve) => setTimeout(resolve, 50));
    await closeDatabase();

    lines.push(
      JSON.stringify({
        name: c.name,
        distillPrompt,
        cheapSelectionUsed,
        preSearchedMemories: normalizeMemories(result.preSearchedMemories),
        recallSignals: result.recallSignals ?? null,
        // P4.D95 (v4 `870a57fa`): the vector the proactive search embedded,
        // handed out through `captureQueryEmbedding`. It must survive BOTH
        // return paths — the success slice AND the memories-`undefined`
        // fall-through — and be absent when the task never embedded at all.
        queryEmbedding: result.preSearchedQueryEmbedding
          ? {
              query: result.preSearchedQueryEmbedding.query,
              embedding: Array.from(result.preSearchedQueryEmbedding.embedding),
            }
          : null,
      }),
    );
  }

  (global as { Date: DateConstructor }).Date = RealDate;
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`precompute oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('precompute oracle', async () => {
  await main();
}, 240000);
