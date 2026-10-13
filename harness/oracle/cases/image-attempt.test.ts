/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the shared image attempt + provider-image decode (P4.D283;
 * v4 `lib/image-gen/image-attempt.ts` `makeLoggedImageAttempt` /
 * `decodeProviderImage`, NEW at `9ecebf986`), ported to
 * `quilltap-core::image_gen::image_attempt`.
 *
 * Drives v4's REAL `makeLoggedImageAttempt` over the wardrobe-item-image-
 * generation fixture pair (the owner, `chat_settings` with logging ON, the
 * image profiles read REAL through `repos.imageProfiles.findById`), with:
 *   - `createImageProvider` → a RECORDING provider scripted per step (`ok`
 *     answers the fixture WebP with a revised prompt, `okNoRevised` without,
 *     `throw` a plain failure);
 *   - the REAL provider registry (ten dist plugins) so `buildImageGenParams`
 *     resolves each profile's orientation + declarations;
 *   - `logLLMCall` REAL into a scratch llm-logs DB (one row per step);
 *   - v4's logger spied for `[ImageAttempt]`.
 * Then REAL `decodeProviderImage` (sharp's `convertToWebP`) over the corpus's
 * responses; the minted filename's `Date.now()` is masked `<ms>` on both sides.
 *
 * Run (Node 24, from the v4 checkout — a /tmp mirror, jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-ia-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/image-attempt.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/image-attempt.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WIIG_MAIN=/tmp/qt-ia-main.db QT_FIXTURE_WIIG_MOUNT=/tmp/qt-ia-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-item-image-generation-fixture.ts
 *   QT_FIXTURE_IA_MAIN=/tmp/qt-ia-main.db QT_FIXTURE_IA_MOUNT=/tmp/qt-ia-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-image-attempt.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "cases/image-attempt\.test\.ts$"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  return v;
}

interface Step { profile: string; apiKey: string; provider: 'ok' | 'okNoRevised' | 'throw' }
interface Attempt { name: string; options: Record<string, unknown> & { primaryProfile: string; prebuilt?: { profile: string; params: Record<string, unknown> } }; steps: Step[] }
interface Decode { name: string; response: { images: Array<Record<string, unknown>> } | null; stem: string; index: number }
interface Spec {
  testPepperBase64: string;
  userId: string;
  profiles: Record<string, string>;
  webp: string;
  png1x1: string;
  attempts: Attempt[];
  decodes: Decode[];
}

const LOG_PREFIXES = ['[ImageAttempt]'];
const PLUGIN_DIRS = [
  'anthropic', 'openai', 'google', 'grok', 'deepseek',
  'z-ai', 'openrouter', 'ollama', 'openai-compatible', 'nanogpt',
];

function sanitize(meta: unknown): unknown {
  if (meta === undefined) return null;
  return JSON.parse(JSON.stringify(meta, (_k, v) => (v instanceof Error ? v.message : v)));
}

function resolveData(spec: Spec, v: unknown): unknown {
  if (v === '$webp') return spec.webp;
  if (v === '$png1x1') return spec.png1x1;
  return v;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'image-attempt.json'), 'utf8')) as Spec;
  const mainFixture = process.env.QT_FIXTURE_IA_MAIN;
  const mountFixture = process.env.QT_FIXTURE_IA_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_IA_MAIN / QT_FIXTURE_IA_MOUNT must point at the wardrobe-item-image-generation seeds');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ia-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const out: string[] = [];

  // ---- the attempts: one fresh module graph + fixture copy per case ----
  for (const a of spec.attempts) {
    jest.resetModules();
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
    const providerCalls: Array<Record<string, unknown>> = [];
    let stepIndex = 0;
    jest.doMock('@/lib/llm/plugin-factory', () => {
      const actual = jest.requireActual('@/lib/llm/plugin-factory');
      return {
        __esModule: true,
        ...actual,
        createImageProvider: (provider: string) => ({
          generateImage: async (params: Record<string, unknown>, apiKey: string) => {
            const step = a.steps[stepIndex];
            providerCalls.push({ provider, apiKey, params });
            if (step.provider === 'throw') throw new Error('canned provider failure');
            return {
              images: [
                step.provider === 'ok'
                  ? { data: spec.webp, mimeType: 'image/webp', revisedPrompt: 'a revised coat' }
                  : { data: spec.webp, mimeType: 'image/webp' },
              ],
            };
          },
        }),
      };
    });

    const work = mkdtempSync(join(scratch, 'ia-'));
    copyFileSync(mainFixture, join(work, 'main.db'));
    copyFileSync(mountFixture, join(work, 'mount.db'));
    process.env.SQLITE_PATH = join(work, 'main.db');
    process.env.SQLITE_MOUNT_INDEX_PATH = join(work, 'mount.db');
    process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm-logs.db');

    const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
    const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
    const { getRepositories } = await import('@/lib/repositories/factory');
    const loggerModule = await import('@/lib/logger');
    const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
    await initializeProviderRegistry(
      PLUGIN_DIRS.map((d) => {
        const m = require(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
        return m.plugin || m.default?.plugin || m.default;
      }),
    );
    const { makeLoggedImageAttempt } = await import('@/lib/image-gen/image-attempt');
    await initializeDatabase();
    const repos = getRepositories();

    const logs: Array<{ level: string; message: string; meta: unknown }> = [];
    const spies: jest.SpyInstance[] = [];
    for (const level of ['debug', 'info', 'warn', 'error'] as const) {
      spies.push(
        jest.spyOn(loggerModule.logger as never, level as never).mockImplementation(((
          message: string,
          meta?: unknown,
        ) => {
          if (typeof message === 'string' && LOG_PREFIXES.some((p) => message.startsWith(p))) {
            logs.push({ level, message, meta: sanitize(meta) });
          }
        }) as never),
      );
    }

    const { primaryProfile, prebuilt, ...rest } = a.options;
    const attempt = makeLoggedImageAttempt({
      userId: spec.userId,
      primaryProfileId: spec.profiles[primaryProfile],
      ...(prebuilt ? { prebuilt: { profileId: spec.profiles[prebuilt.profile], params: prebuilt.params } } : {}),
      ...rest,
    } as never);
    const outcomes: unknown[] = [];
    for (const step of a.steps) {
      const profile = await repos.imageProfiles.findById(spec.profiles[step.profile]);
      if (!profile) throw new Error(`profile ${step.profile} missing from the fixture`);
      try {
        const r = await attempt(profile as never, step.apiKey);
        outcomes.push({ ok: true, imageCount: r.images?.length ?? 0, revisedPrompt: r.images?.[0]?.revisedPrompt ?? null });
      } catch (e) {
        outcomes.push({ ok: false, threw: (e as Error).message });
      }
      stepIndex += 1;
    }
    for (const s of spies) s.mockRestore();

    const lldb = getRawLLMLogsDatabase();
    if (!lldb) throw new Error('llm-logs DB handle unavailable');
    const llColumns = (lldb.pragma('table_info(llm_logs)') as Array<{ name: string }>).map((c) => c.name);
    const llRows = (llColumns.length
      ? (lldb.prepare('SELECT * FROM llm_logs').all() as Array<Record<string, unknown>>)
      : []
    ).map((r) => {
      const o: Record<string, unknown> = {};
      for (const col of llColumns) o[col] = canonValue(r[col]);
      o.id = '<id>';
      o.createdAt = '<ts>';
      o.updatedAt = '<ts>';
      return o;
    });
    out.push(JSON.stringify({ kind: 'attempt', name: a.name, providerCalls, outcomes, logs, llmLogs: { columns: llColumns, rows: llRows } }));
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }

  // ---- the decodes: real convertToWebP; no DB ----
  jest.resetModules();
  jest.doMock('@/lib/files/webp-conversion', () => jest.requireActual('@/lib/files/webp-conversion'));
  const loggerModule = await import('@/lib/logger');
  const { decodeProviderImage } = await import('@/lib/image-gen/image-attempt');
  for (const d of spec.decodes) {
    const logs: Array<{ level: string; message: string; meta: unknown }> = [];
    const spies: jest.SpyInstance[] = [];
    for (const level of ['debug', 'info', 'warn', 'error'] as const) {
      spies.push(
        jest.spyOn(loggerModule.logger as never, level as never).mockImplementation(((
          message: string,
          meta?: unknown,
        ) => {
          if (typeof message === 'string' && LOG_PREFIXES.some((p) => message.startsWith(p))) {
            logs.push({ level, message, meta: sanitize(meta) });
          }
        }) as never),
      );
    }
    const response = d.response
      ? {
          images: d.response.images.map((img) => {
            const o: Record<string, unknown> = {};
            for (const [k, v] of Object.entries(img)) o[k] = resolveData(spec, v);
            return o;
          }),
        }
      : null;
    const decoded = await decodeProviderImage(response as never, d.stem, d.index);
    for (const s of spies) s.mockRestore();
    out.push(
      JSON.stringify({
        kind: 'decode',
        name: d.name,
        decoded: decoded
          ? {
              mimeType: decoded.mimeType,
              filename: decoded.filename.replace(/_\d+\./, '_<ms>.'),
              width: decoded.width,
              height: decoded.height,
              sha256: decoded.sha256,
              bytesHex: decoded.buffer.toString('hex'),
              revisedPrompt: decoded.revisedPrompt,
            }
          : null,
        logs,
      }),
    );
  }

  fs.writeFileSync(outPath, out.join('\n') + '\n');
  process.stderr.write(`image-attempt oracle wrote ${outPath} (${spec.attempts.length} attempts, ${spec.decodes.length} decodes)\n`);
}

test('image-attempt tier-3 oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
