/**
 * @jest-environment node
 *
 * P4.D217 route-family ORACLE for `/api/v1/scenario-builder` (v4 `d1c06cd9d`,
 * `app/api/v1/scenario-builder/route.ts`), ported to
 * `quilltap-web`'s `scenario_builder_routes` (+ the engine's refusals).
 *
 * Drives v4's REAL route — the real `createContextHandler` and
 * `withCollectionActionDispatch`, the real repositories, the real api-key
 * resolver, the real Zod schema — over a per-case COPY of the committed
 * chat-send fixture pair with `scenario-builder-routes.json`'s plants applied
 * through v4's own `rawQuery`. Mocked: the auth session and the startup gate
 * (the seams every route family neutralizes), and `runScenarioBuilder` —
 * canned to enqueue the spec's `frames`, recording the `input` it was handed
 * and whether its `signal` aborted. That is the ONE place a mock is right: the
 * route's contract is what it does AROUND the run. One case also swaps in a
 * throwing `resolveScenarioBuilderCapabilities` (the 500 arm); two more
 * (P4.115) make the canned run THROW, before and after its first frame — the
 * route's `Scenario Builder stream failed` catch, inside the committed stream.
 *
 * Emits per case: { name, status, contentType, body (JSON) | sse (text),
 * runs: [{ mode, characterIds, chat, priorDraft, revision, aborted }],
 * lines: [{ level, message, context: [[key, value], …] | null }] } — `lines`
 * being every call on a `ScenarioBuilder` logger (the route's
 * `logger.child({ context: 'ScenarioBuilder' })` and the service's
 * `createServiceLogger('ScenarioBuilder')`, i.e. `resolveScenarioBuilder
 * Capabilities`'s DEBUG), recorded by a spy on the case registry's
 * `Logger.prototype` BEFORE the level gate (LOG_LEVEL=error would drop every
 * DEBUG). The context is emitted as ordered entries so key ORDER is compared.
 *
 * Run (Node 24, from the v4 checkout or a PINNED worktree; stage outside
 * `.claude/`, which v4's jest ignores):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
 *   STAGE=/tmp/qt-oracle-stage-sb-routes
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $W/harness/oracle/cases/scenario-builder-routes.test.ts $STAGE/harness/oracle/cases/
 *   cp $W/harness/oracle/fixtures/scenario-builder-routes.json $STAGE/harness/oracle/fixtures/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SBR_MAIN=$W/crates/quilltap-web/tests/fixtures/chat-send-main.db \
 *   QT_FIXTURE_SBR_MOUNT=$W/crates/quilltap-web/tests/fixtures/chat-send-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-scenario-builder-routes.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=240000 \
 *       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "scenario-builder-routes\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { createRequire } from 'node:module';

const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';

/** P4.115: the canned run's thrown message — v5's canned driver fails with
 *  the same text, so v4's ERROR `{ error }` context compares byte-for-byte. */
const RUN_THROWS_MESSAGE = 'the Host fell into the harbour';

interface CaseSpec {
  name: string;
  method: 'GET' | 'POST';
  query: string;
  body?: unknown;
  rawBody?: boolean;
  awaitAbort?: boolean;
  capabilitiesThrow?: boolean;
  /** P4.115: the canned run THROWS — before enqueueing anything, or after
   *  the first frame — driving the route's belt-and-braces catch. */
  runThrows?: 'before' | 'after';
}
interface Spec {
  plants: Array<{ sql: string; params: unknown[] }>;
  frames: unknown[];
  cases: CaseSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'scenario-builder-routes.json'), 'utf8'),
  ) as Spec;
  const fixtureMain = process.env.QT_FIXTURE_SBR_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_SBR_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_SBR_MAIN / QT_FIXTURE_SBR_MOUNT must point at the chat-send pair');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sbr-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  delete process.env.SERPER_API_KEY;
  process.env.LOG_LEVEL = 'error';

  const lines: string[] = [];
  for (const c of spec.cases) {
    jest.resetModules();
    let sessionUserId = '';
    const runs: Array<Record<string, unknown>> = [];

    const cipherDriverPath = require('node:path').join(
      process.cwd(),
      'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
    );
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/plugins/provider-validation', () =>
      jest.requireActual('@/lib/plugins/provider-validation'),
    );
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
    jest.doMock('@/lib/auth/session', () => ({
      __esModule: true,
      ...jest.requireActual('@/lib/auth/session'),
      getServerSession: async () => ({ user: { id: sessionUserId } }),
    }));
    jest.doMock('@/lib/startup/startup-state', () => {
      const actual = jest.requireActual('@/lib/startup/startup-state');
      return {
        __esModule: true,
        ...actual,
        startupState: {
          ...actual.startupState,
          isReady: () => true,
          waitForReady: async () => true,
          isPepperResolved: () => true,
          getPepperState: () => 'resolved',
          getPhase: () => 'ready',
          isLockedMode: () => false,
        },
      };
    });
    jest.doMock('@/lib/services/scenario-builder/scenario-builder.service', () => {
      const actual = jest.requireActual('@/lib/services/scenario-builder/scenario-builder.service');
      return {
        __esModule: true,
        ...actual,
        runScenarioBuilder: async (
          opts: { input: Record<string, unknown> },
          controller: { enqueue: (d: Uint8Array) => void },
          signal?: AbortSignal,
        ) => {
          const enc = new TextEncoder();
          const run: Record<string, unknown> = {
            mode: opts.input.mode,
            characterIds: opts.input.characterIds,
            chat: opts.input.chat ?? null,
            priorDraft: opts.input.priorDraft ?? null,
            revision: opts.input.revision ?? null,
            aborted: false,
          };
          runs.push(run);
          if (c.runThrows === 'before') throw new Error(RUN_THROWS_MESSAGE);
          if (c.runThrows === 'after') {
            controller.enqueue(enc.encode(`data: ${JSON.stringify(spec.frames[0])}\n\n`));
            throw new Error(RUN_THROWS_MESSAGE);
          }
          if (c.awaitAbort) {
            controller.enqueue(enc.encode(`data: ${JSON.stringify(spec.frames[0])}\n\n`));
            await new Promise<void>((resolve) => {
              if (signal?.aborted) return resolve();
              signal?.addEventListener('abort', () => resolve());
            });
            run.aborted = !!signal?.aborted;
            return;
          }
          for (const f of spec.frames) controller.enqueue(enc.encode(`data: ${JSON.stringify(f)}\n\n`));
        },
        ...(c.capabilitiesThrow
          ? {
              resolveScenarioBuilderCapabilities: async () => {
                throw new Error('the capability ledger is locked in the safe');
              },
            }
          : {}),
      };
    });

    const work = mkdtempSync(join(scratch, 'case-'));
    const mainWork = join(work, 'main.db');
    const mountWork = join(work, 'mount.db');
    copyFileSync(fixtureMain, mainWork);
    copyFileSync(fixtureMount, mountWork);
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

    // The REAL provider registry, so the REAL api-key resolver reads each
    // provider's key rules (an empty registry treats every provider as
    // key-required — OLLAMA included).
    {
      const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));
      const PLUGIN_DIRS = ['anthropic', 'openai', 'google', 'grok', 'deepseek', 'z-ai', 'openrouter', 'ollama', 'openai-compatible'];
      const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
      await initializeProviderRegistry(
        PLUGIN_DIRS.map((d) => {
          const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
          return m.plugin || m.default?.plugin || m.default;
        }),
      );
      await import('@/lib/plugins/provider-validation');
    }
    // The ScenarioBuilder log lines, recorded on THIS registry's Logger class
    // (the route's and the service's module-level loggers are built from it).
    const logLines: Array<{ level: string; message: string; context: unknown }> = [];
    const { Logger } = (await import('@/lib/logger')) as unknown as {
      Logger: { prototype: Record<string, (...a: unknown[]) => void> };
    };
    const logSpies = (['error', 'warn', 'info', 'debug'] as const).map((level) => {
      const original = Logger.prototype[level];
      return jest
        .spyOn(Logger.prototype, level)
        .mockImplementation(function (this: { context?: Record<string, unknown> }, ...args: unknown[]) {
          const ctx = this.context ?? {};
          if (ctx.context === 'ScenarioBuilder' || ctx.service === 'ScenarioBuilder') {
            const [message, context] = args as [string, Record<string, unknown> | undefined];
            logLines.push({
              level,
              message,
              context: context === undefined ? null : Object.entries(context),
            });
            return;
          }
          return original.apply(this, args);
        });
    });
    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    await initializeDatabase();
    try {
      // The instance's one owner (the fixture's own id — v5's venue rewrites
      // it to SINGLE_USER_ID; every planted row is cloned from that owner's
      // rows, so ownership agrees on both sides without naming either id).
      // The chat-send pair carries NO `users` table (v5 never reads one), but
      // v4's context handler resolves the session user through it, so the
      // owner's row is created through v4's own repository on this copy.
      const owner = (await rawQuery<Array<{ userId: string }>>(
        'SELECT "userId" FROM "connection_profiles" ORDER BY rowid LIMIT 1',
      )) ?? [];
      sessionUserId = owner[0]?.userId ?? '';
      const { getRepositories } = await import('@/lib/repositories/factory');
      const TS = '2026-09-23T00:00:00.000Z';
      await getRepositories().users.create(
        { username: 'scenario-builder-routes', name: 'Route Family' } as never,
        { id: sessionUserId, createdAt: TS, updatedAt: TS } as never,
      );
      for (const p of spec.plants) await rawQuery(p.sql, p.params);

      const route = (await import('@/app/api/v1/scenario-builder/route')) as unknown as Record<
        string,
        (req: unknown, ctx: unknown) => Promise<Response>
      >;
      const url = `http://localhost/api/v1/scenario-builder${c.query ? `?${c.query}` : ''}`;
      const abort = new AbortController();
      const req = {
        method: c.method,
        url,
        nextUrl: new URL(url),
        headers: new Headers({ 'Content-Type': 'application/json' }),
        signal: abort.signal,
        json: async () => {
          if (c.rawBody) return JSON.parse(c.body as string);
          return c.body;
        },
      };
      const resp = await route[c.method](req, { params: Promise.resolve({}) });
      const contentType = resp.headers.get('content-type');
      const out: Record<string, unknown> = { name: c.name, status: resp.status, contentType };
      if (contentType === 'text/event-stream') {
        out.headers = {
          'cache-control': resp.headers.get('cache-control'),
          connection: resp.headers.get('connection'),
        };
        if (c.awaitAbort) {
          const reader = resp.body!.getReader();
          const first = await reader.read();
          out.sse = new TextDecoder().decode(first.value);
          abort.abort();
          // Let the canned run observe the abort before the case closes.
          for (let i = 0; i < 20 && runs[0] && runs[0].aborted === false; i++) {
            await new Promise((r) => setTimeout(r, 10));
          }
        } else {
          // Drain the stream to its close (jest's NextResponse has no .text()).
          const reader = resp.body!.getReader();
          const dec = new TextDecoder();
          let text = '';
          for (;;) {
            const { value, done } = await reader.read();
            if (done) break;
            text += dec.decode(value, { stream: true });
          }
          out.sse = text;
        }
      } else {
        out.body = await resp.json();
      }
      out.runs = runs;
      out.lines = logLines;
      lines.push(JSON.stringify(out));
    } finally {
      for (const spy of logSpies) spy.mockRestore();
      await closeDatabase();
      closeMountIndexSQLiteClient();
      rmSync(work, { recursive: true, force: true });
    }
  }
  rmSync(scratch, { recursive: true, force: true });
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`scenario-builder routes oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('scenario-builder routes oracle', async () => {
  await main();
});
