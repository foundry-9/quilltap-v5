/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the recall-replay verb (P4.d13 — episodic recall §3):
 * drives v4's REAL `handleRecallReplay` route handler (which runs the REAL
 * `runRecallReplay` → the REAL `searchMemoriesSemantic` twice, multiplier loop
 * and all) over a fresh copy of the committed episodic-recall fixture per case.
 *
 * ONLY the cheap-LLM executor is mocked (`executeCheapLLMTask` — the corpus
 * row's canned distill text feeds v4's REAL parse callback; a `fail` row
 * returns `{ success: false }`). Embeddings run REAL through the fixture's
 * fitted BUILTIN TF-IDF profile — deterministic on both sides. `Date.now()` is
 * frozen to the corpus `$nowMs` (the search decay seam; the Rust side passes
 * the same value as `now_ms`).
 *
 * Emits one NDJSON line per case: `{ name, status, body, distillCalls, lines }`
 * where `body` is the route's JSON (the full replay result on 200; `{ error }`
 * — plus `details` on a validation 400 — on 4xx), `distillCalls` counts the
 * cheap-LLM executor calls (a pinned-signals case must make none), and `lines`
 * are the route's two `[Chats v1]` lines, the RecallReplay service's three and
 * the search's `[Memory]` lines, captured by splicing a transport into the
 * singleton logger (`LOG_LEVEL=debug`).
 *
 * P4.D267 (v4 `f7d8064be` / `3f7320138` / `d58548051`, pin `01a83539d`): each
 * copy first gets v4's REAL `add-memory-tiers-v1` (`applyMemoryTiersMigration`,
 * idempotent on the already-widened pair — §R.3), and a case may carry a
 * `plantSql` run on the copy after open (identically on the Rust side) — the
 * `asOf` cutoff's newer memory. Since `3f7320138` the replay's searches stamp
 * nothing at all.
 *
 * Real-DB-under-jest (memory-gate-tier3 recipe): resetModules + doMock past
 * jest.setup's global DB mocks + better-sqlite3 → better-sqlite3-multiple-
 * ciphers. Stage the case OUTSIDE any `.claude/` path (v4's jest ignores it).
 *
 * Run (Node 24, from the v4 checkout):
 *   see recall_replay_equivalence.rs header for the full STAGE recipe.
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface DistillSpec {
  kind: 'json' | 'fail';
  text?: string;
}
interface CaseSpec {
  name: string;
  body: Record<string, unknown>;
  distill: DistillSpec;
  deleteChatSettings?: boolean;
  deleteConnections?: boolean;
  plantSql?: string;
}

/** The lines this family pins (v4's exact messages). */
const CAPTURED = new Set([
  '[Chats v1] Recall replay requested',
  '[Chats v1] Recall replay failed',
  'Recall replay using pinned signals',
  'Recall replay corpus cutoff',
  'Recall replay complete',
  // P4.D291 (v4 `584d28164`): the TF-IDF gate-off and R6 reservation lines
  // are retired; the anchors line is the search's one.
  '[Memory] Specific entity anchors chosen',
]);
interface Spec {
  $nowMs: number;
  chatId: string;
  userId: string;
  cases: CaseSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'recall-replay-cases.json'), 'utf8'),
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
  // `new Date()` (memory-weighting), not `Date.now()` — the build-context
  // FakeDate idiom. The Rust side receives the same value as `now_ms`.
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
  const { applyMemoryTiersMigration } = await import('../fixtures/apply-memory-tiers-migration');

  for (const c of spec.cases) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-replay-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main-work.db');
    const mountWork = join(scratch, 'mount-work.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);
    // §R.3: v4's REAL tier migration on the copy (a no-op on the widened pair).
    await applyMemoryTiersMigration(mainWork, {
      pepperBase64: fixtureSpec.testPepperBase64,
      ledger: false,
    });

    process.env.ENCRYPTION_MASTER_PEPPER = fixtureSpec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'debug';

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
    // v4's REAL parse callback (the same seam memory-tasks-tier1 uses).
    let distillCalls = 0;
    jest.doMock('@/lib/memory/cheap-llm-tasks/core-execution', () => ({
      __esModule: true,
      executeCheapLLMTask: jest.fn(
        async (_sel: unknown, _messages: unknown, _uid: unknown, parse: unknown) => {
          distillCalls++;
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

    const { logger } = await import('@/lib/logger');
    const captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
    const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
    internal.transports.splice(0, internal.transports.length, {
      write: (d: unknown) => {
        const data = d as {
          level: string;
          message: string;
          context?: Record<string, unknown>;
          error?: { message: string };
        };
        if (!CAPTURED.has(data.message)) return;
        const { service: _s, environment: _e, ...fields } = data.context ?? {};
        if (data.error) fields.error = data.error.message;
        captured.push({ level: data.level, message: data.message, fields });
      },
    });

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    await initializeDatabase();
    if (c.plantSql) {
      const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
      getRawDatabase()!.exec(c.plantSql);
    }
    const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
    await initializePlugins();
    const { getRepositories } = await import('@/lib/repositories/factory');
    const repos = getRepositories();

    if (c.deleteChatSettings) {
      const settings = await repos.chatSettings.findByUserId(spec.userId);
      if (settings) await repos.chatSettings.delete(settings.id);
    }
    if (c.deleteConnections) {
      const profiles = await repos.connections.findByUserId(spec.userId);
      for (const p of profiles) await repos.connections.delete(p.id);
    }

    const chat = await repos.chats.findById(spec.chatId);
    if (!chat) throw new Error('fixture chat missing');

    const { handleRecallReplay } = await import(
      '@/app/api/v1/chats/[id]/actions/recall-replay'
    );
    const req = { text: async () => JSON.stringify(c.body) } as never;
    const ctx = { user: { id: spec.userId }, repos } as never;
    const res = await handleRecallReplay(req, spec.chatId, chat as never, ctx);
    const status = (res as { status: number }).status;
    const body = await (res as { json: () => Promise<unknown> }).json();

    // Let fire-and-forget promises settle before closing.
    await new Promise((resolve) => setTimeout(resolve, 50));
    await closeDatabase();
    rmSync(scratch, { recursive: true, force: true });

    lines.push(JSON.stringify({ name: c.name, status, body, distillCalls, lines: captured }));
  }

  (global as { Date: DateConstructor }).Date = RealDate;
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`recall-replay oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('recall-replay oracle', async () => {
  await main();
}, 240000);
