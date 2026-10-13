/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the shared vision request (P4.D283; v4
 * `lib/llm/vision-request.ts` `resolveVisionProfile` / `sendVisionRequest`,
 * NEW at `9ecebf986` — bug 197's half where v5 has the surface), ported to
 * `quilltap-core::llm::vision_request`.
 *
 * Drives v4's REAL module over a FRESH copy of the file-attachment fixture pair
 * per case (its `chat_settings` names the OPENAI describer; the profiles are
 * read REAL through `repos.connections`), with:
 *   - `createLLMProvider` (`@/lib/llm`) → a SCRIPTED `sendMessage` that records
 *     the params it was handed and answers per the case's `provider.mode` (`ok`
 *     → the scripted response, `throw` → a plain failure, `hang` → never, so the
 *     request's own `withTimeout` fires);
 *   - `logLLMCall` REAL into a scratch llm-logs DB (one row per send, answered
 *     or not — the request shape with the attachment's filename + SENT mime,
 *     the sampling's temperature + maxTokens);
 *   - the REAL `shrinkImageForLlmTransport` — over real sharp for the 1x1 PNG
 *     (under the transport bounds, so `shrunk: false` on both sides), and over
 *     the bug-151 SCRIPTED encoder (`../lib/shrink-script.ts`) for the one case
 *     carrying `shrink`, so the shrink's decision and ladder are comparands
 *     without comparing encoders (D19);
 *   - v4's logger replaced by a recorder whose `child` is itself, because the
 *     module's child logger never reaches a spy on the root (memory note:
 *     child-logger bindings invisible to the capture).
 *
 * Emits per `resolve` case `{ name, profileId, logs }` and per `send` case
 * `{ name, result, providerCalls, logs, llmLogs }`.
 *
 * Run (Node 24, from the v4 checkout — a /tmp mirror, jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-vr-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
 *   cp "$V5W/harness/oracle/cases/vision-request.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/vision-request.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/lib/shrink-script.ts" "$TMPO/lib/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_FILE_ATTACH_MAIN=/tmp/qt-vr-main.db QT_FIXTURE_FILE_ATTACH_MOUNT=/tmp/qt-vr-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-file-attachment-fixture.ts
 *   QT_FIXTURE_VR_MAIN=/tmp/qt-vr-main.db QT_FIXTURE_VR_MOUNT=/tmp/qt-vr-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-vision-request.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "cases/vision-request\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { originalBuffer, scriptedSharp, type RecordedCall, type ShrinkScript } from '../lib/shrink-script';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  return v;
}

interface ResolveCase { name: string; prefer: 'cheap' | 'capable'; configured: 'honour' | 'skip-incapable'; sql?: string[] }
interface SendCase {
  name: string;
  profile: string;
  sql?: string[];
  attachment: { id: string; filename: string; mimeType: string; size: number; data: string | null };
  systemPrompt?: string;
  instruction: string;
  sampling: { temperature?: number; maxTokens?: number; topP?: number };
  timeoutMs: number;
  logType: string;
  requireApiKey: boolean;
  /** The bug-151 scripted encoder for THIS case's original (`originalBuffer(originalLen)` is the attachment's data). */
  shrink?: { originalLen: number; script: ShrinkScript };
  provider: { mode: 'ok' | 'throw' | 'hang'; content?: string; finishReason?: string | null; usage?: unknown; attachmentResults?: unknown; cacheUsage?: unknown; message?: string };
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  resolve: ResolveCase[];
  send: SendCase[];
}

const LOG_PREFIXES = ['[Vision]'];

function sanitize(meta: unknown): unknown {
  if (meta === undefined) return null;
  return JSON.parse(JSON.stringify(meta, (_k, v) => (v instanceof Error ? v.message : v)));
}

async function openCase(
  spec: Spec,
  scratch: string,
  mainFixture: string,
  mountFixture: string,
  sql: string[],
  providerScript: SendCase['provider'] | null,
  providerCalls: Array<Record<string, unknown>>,
  shrink: SendCase['shrink'] | undefined,
  shrinkCalls: RecordedCall[],
) {
  jest.resetModules();
  // The scripted `sharp` BELOW v4's real `shrinkImageForLlmTransport` (the
  // llm-image-budget recipe: mock the RESOLVED path, since the case is staged
  // outside the checkout). A `doMock` registration SURVIVES `resetModules`, so
  // every unscripted case explicitly un-mocks it — measured: without this the
  // case after the scripted one shrank its real 1x1 PNG through the script.
  const sharpPath = join(process.cwd(), 'node_modules', 'sharp');
  if (shrink) {
    const sharpFn = scriptedSharp(shrink.script, originalBuffer(shrink.originalLen), shrinkCalls);
    jest.doMock(sharpPath, () => ({ __esModule: true, default: sharpFn }));
  } else {
    jest.dontMock(sharpPath);
  }
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/services/llm-logging.service', () =>
    jest.requireActual('@/lib/services/llm-logging.service'),
  );
  // A recorder logger whose `child` is itself — v4's module-level
  // `logger.child({ module: 'vision-request' })` would otherwise hide every
  // `[Vision]` line from a spy on the root (the image-failover recipe).
  const logs: Array<{ level: string; message: string; meta: unknown }> = [];
  jest.doMock('@/lib/logger', () => {
    const record = (level: string) => (message: string, meta?: unknown) => {
      if (typeof message === 'string' && LOG_PREFIXES.some((p) => message.startsWith(p))) {
        logs.push({ level, message, meta: sanitize(meta) });
      }
    };
    const recorder = (): Record<string, unknown> => {
      const self: Record<string, unknown> = {
        debug: record('debug'),
        info: record('info'),
        warn: record('warn'),
        error: record('error'),
        trace: record('trace'),
      };
      self.child = () => self;
      return self;
    };
    return {
      __esModule: true,
      LogLevel: { ERROR: 'error', WARN: 'warn', INFO: 'info', DEBUG: 'debug', TRACE: 'trace' },
      logger: recorder(),
    };
  });
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string, baseUrl?: string) => ({
        sendMessage: async (params: Record<string, unknown>, apiKey: string) => {
          providerCalls.push({ provider, baseUrl: baseUrl ?? null, apiKey, params: sanitize(params) });
          if (!providerScript) throw new Error('no provider script for a resolve case');
          if (providerScript.mode === 'throw') throw new Error(providerScript.message ?? 'canned vision failure');
          if (providerScript.mode === 'hang') return new Promise(() => {});
          return {
            content: providerScript.content,
            finishReason: providerScript.finishReason ?? null,
            usage: providerScript.usage ?? undefined,
            ...(providerScript.attachmentResults !== undefined ? { attachmentResults: providerScript.attachmentResults } : {}),
            ...(providerScript.cacheUsage !== undefined ? { cacheUsage: providerScript.cacheUsage } : {}),
          };
        },
      }),
    };
  });

  const work = mkdtempSync(join(scratch, 'vr-'));
  copyFileSync(mainFixture, join(work, 'main.db'));
  copyFileSync(mountFixture, join(work, 'mount.db'));
  process.env.SQLITE_PATH = join(work, 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(work, 'mount.db');
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm-logs.db');

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  await initializeDatabase();
  const raw = getRawDatabase();
  if (!raw) throw new Error('DB handle unavailable');
  for (const s of sql) raw.exec(s);

  const close = async () => {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  };
  const dumpLlmLogs = () => {
    const lldb = getRawLLMLogsDatabase();
    if (!lldb) throw new Error('llm-logs DB handle unavailable');
    const columns = (lldb.pragma('table_info(llm_logs)') as Array<{ name: string }>).map((c) => c.name);
    const rows = (columns.length
      ? (lldb.prepare('SELECT * FROM llm_logs').all() as Array<Record<string, unknown>>)
      : []
    ).map((r) => {
      const o: Record<string, unknown> = {};
      for (const col of columns) o[col] = canonValue(r[col]);
      o.id = '<id>';
      o.createdAt = '<ts>';
      o.updatedAt = '<ts>';
      return o;
    });
    return { columns, rows };
  };
  return { repos: getRepositories(), logs, close, dumpLlmLogs };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'vision-request.json'), 'utf8')) as Spec;
  const mainFixture = process.env.QT_FIXTURE_VR_MAIN;
  const mountFixture = process.env.QT_FIXTURE_VR_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_VR_MAIN / QT_FIXTURE_VR_MOUNT must point at the file-attachment seeds');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vr-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const out: string[] = [];
  for (const c of spec.resolve) {
    const calls: Array<Record<string, unknown>> = [];
    const ctx = await openCase(spec, scratch, mainFixture, mountFixture, c.sql ?? [], null, calls, undefined, []);
    try {
      const { resolveVisionProfile } = await import('@/lib/llm/vision-request');
      const picked = await resolveVisionProfile(ctx.repos as never, spec.userId, {
        prefer: c.prefer,
        configured: c.configured,
      });
      out.push(JSON.stringify({ kind: 'resolve', name: c.name, profileId: picked?.id ?? null, logs: ctx.logs }));
    } finally {
      await ctx.close();
    }
  }
  for (const c of spec.send) {
    const calls: Array<Record<string, unknown>> = [];
    const shrinkCalls: RecordedCall[] = [];
    const ctx = await openCase(spec, scratch, mainFixture, mountFixture, c.sql ?? [], c.provider, calls, c.shrink, shrinkCalls);
    const data = c.shrink ? originalBuffer(c.shrink.originalLen).toString('base64') : c.attachment.data ?? undefined;
    try {
      const { sendVisionRequest } = await import('@/lib/llm/vision-request');
      const profile = await ctx.repos.connections.findById(c.profile);
      if (!profile) throw new Error(`profile ${c.profile} missing from the fixture`);
      let result: unknown;
      try {
        const sent = await sendVisionRequest({
          profile: profile as never,
          repos: ctx.repos as never,
          userId: spec.userId,
          attachment: { ...c.attachment, data } as never,
          ...(c.systemPrompt !== undefined ? { systemPrompt: c.systemPrompt } : {}),
          instruction: c.instruction,
          sampling: c.sampling,
          timeoutMs: c.timeoutMs,
          logType: c.logType as never,
          ...(c.requireApiKey ? { requireApiKey: true } : {}),
        });
        result = sent.ok
          ? {
              ok: true,
              content: sent.response.content,
              usage: sent.response.usage ?? null,
              sentAttachment: { filename: sent.sentAttachment.filename, mimeType: sent.sentAttachment.mimeType, hasData: !!sent.sentAttachment.data },
            }
          : { ok: false, refusal: sent.refusal, detail: sent.detail, responseContent: sent.response?.content ?? null };
      } catch (e) {
        result = { threw: (e as Error).message };
      }
      out.push(JSON.stringify({ kind: 'send', name: c.name, result, providerCalls: calls, shrinkCalls, logs: ctx.logs, llmLogs: ctx.dumpLlmLogs() }));
    } finally {
      await ctx.close();
    }
  }

  fs.writeFileSync(outPath, out.join('\n') + '\n');
  process.stderr.write(`vision-request oracle wrote ${outPath} (${spec.resolve.length} resolve, ${spec.send.length} send)\n`);
}

test('vision-request tier-3 oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
