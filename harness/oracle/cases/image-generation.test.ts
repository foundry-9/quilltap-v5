/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the W4.9a image-generation handler (v4
 * `lib/tools/handlers/image-generation-handler.ts` executeImageGenerationTool)
 * and the avatar trigger (`lib/wardrobe/avatar-generation.ts`
 * triggerAvatarGenerationIfEnabled).
 *
 * Drives v4's REAL handlers over the shared two-DB fixture, ONE fresh copy per
 * case (image generation WRITES `files` + the store tables; the avatar trigger
 * writes `background_jobs`). Model/infra seams pinned (real DB stack wired back in
 * past jest.setup — [[jest-real-db-oracle]]):
 *
 *   - `createImageProvider` (`@/lib/llm/plugin-factory`) → a fake provider whose
 *     `generateImage(params, key)` KEYS on `provider|model|<JSON of params>`,
 *     RECORDS the key (`kind:"cannedImage"` row) so the Rust CannedImageProvider
 *     replays the SAME key (proving mergeParameters + applyOrientation reach the
 *     wire), returning a canned image (revisedPrompt echoes the prompt).
 *   - `getImageProviderConstraints` + `getImageGenerationModels`
 *     (`@/lib/plugins/provider-registry`) → canned OrientationSupport (OPENAI a
 *     size-strategy so applyOrientation sets a concrete `size`); no styleInfo
 *     (styleOptions stays undefined).
 *   - `convertToWebP` (`@/lib/files/webp-conversion`) + `transcodeToWebP`
 *     (`@/lib/mount-index/blob-transcode`) → PASS-THROUGH (bytes/mime/filename
 *     unchanged, no width/height), so the store bytes match the Rust
 *     PassthroughTranscoder + link_blob_content byte-for-byte.
 *   - `createLLMProvider` (`@/lib/llm`) → the recording-completion pattern: keys
 *     the craft-prompt call on `provider|model|temperature|messages`, resolves it
 *     against the corpus rules (fail for the {{placeholder}} garden case → the
 *     tier-concatenation fallback; a success otherwise), RECORDS the exact key
 *     (`kind:"canned"` row) the Rust CannedCompletionProvider replays.
 *   - `getApiKeyForCheapLLMSelection` → constant.
 *   - `findApiKeyByIdAndUserId` (repos.connections) → the canned apiKeyId->key map.
 *   - moderation registry `getDefaultProvider` → null (unused: danger mode is OFF).
 *   - `postLanternImageNotification` → no-op (W4.6b seam).
 *   - Lantern store: the REAL getLanternBackgroundsStore + writeLanternBackgroundToMountStore
 *     drive the actual store write (the fixture provisions a `database` Lantern mount +
 *     the instance_settings pointer) — only `transcodeToWebP` is mocked pass-through.
 *   - Date.now() frozen (spec.frozenNowMs) so the provider filename
 *     `generated_<ts>.<ext>` is pinned.
 *
 * Emits one NDJSON line per RECORDED canned completion (kind:"canned"), one per
 * RECORDED canned image call (kind:"cannedImage"), and one per case
 * (kind:"result", { label, resultJson }); image-gen cases carry a `dumps`
 * (files + the 5 mount-index store tables); avatar cases carry a `dumps` with
 * `background_jobs`.
 *
 * Run (Node 24, from the v4 checkout; stage OUTSIDE any .claude path — v4's jest
 * ignores /\.claude/; multi-case → --testTimeout=120000):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; WT=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_IMGGEN_MAIN=/tmp/qt-imggen-main.db QT_FIXTURE_IMGGEN_MOUNT=/tmp/qt-imggen-mount.db \
 *     $N/node --import tsx $WT/harness/oracle/fixtures/build-image-generation-fixture.ts
 *   TMPO=/tmp/qt-imggen-oracle; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $WT/harness/oracle/cases/image-generation.test.ts "$TMPO/cases/"
 *   cp $WT/harness/oracle/fixtures/image-generation.json "$TMPO/fixtures/"
 *   QT_FIXTURE_IMGGEN_MAIN=/tmp/qt-imggen-main.db QT_FIXTURE_IMGGEN_MOUNT=/tmp/qt-imggen-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-image-generation.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "image-generation.test"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}
function canonicalizeRows(table: string, columns: string[], rawRows: Array<Record<string, unknown>>, orderBy: string) {
  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => {
      const av = String(a[orderBy] ?? '');
      const bv = String(b[orderBy] ?? '');
      return av < bv ? -1 : av > bv ? 1 : 0;
    });
  return { table, columns, rows };
}

interface ChatSpec {
  id: string;
  op: 'generate' | 'avatar';
  prompt?: string;
  orientation?: 'portrait' | 'landscape' | 'square';
  clearLantern?: boolean;
}
interface Spec {
  testPepperBase64: string;
  frozenNowMs: number;
  userId: string;
  charAId: string;
  profileId: string;
  callingParticipantId: string;
  apiKeys: Record<string, string>;
  chats: Record<string, ChatSpec>;
}

// ---- OrientationSupport (identical to the Rust orientation_data_for) ----
const OPENAI_SUPPORT = {
  strategy: 'size',
  portrait: { size: '1024x1792' },
  landscape: { size: '1792x1024' },
  square: { size: '1024x1024' },
};
function modelsFor(provider: string): Array<{ id: string; orientationSupport: unknown }> | null {
  if (provider === 'OPENAI') return [{ id: 'dall-e-3', orientationSupport: OPENAI_SUPPORT }];
  return null;
}
function constraintsFor(provider: string): { orientationSupport: unknown } | null {
  if (provider === 'OPENAI') return { orientationSupport: OPENAI_SUPPORT };
  return null;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'image-generation.json'), 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_IMGGEN_MAIN;
  const mountFixture = process.env.QT_FIXTURE_IMGGEN_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_IMGGEN_MAIN and QT_FIXTURE_IMGGEN_MOUNT must point at the seeded fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  const PNG_B64 =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+M8AAAMBAQAY3Y2wAAAAAElFTkSuQmCC';

  const lines: string[] = [];
  const RealDate = Date;
  const recordedCompletions = new Map<string, { provider: string; model: string; temperature: number | null; messages: Array<{ role: string; content: string }>; response: string; usage: unknown }>();
  const recordedImages = new Map<string, { provider: string; model: string; key: string; images: Array<{ data: string; mimeType?: string; revisedPrompt?: string }> }>();

  const craftUsage = { promptTokens: 120, completionTokens: 40, totalTokens: 160 };

  for (const [label, chat] of Object.entries(spec.chats)) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-imggen-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'imggen-main.db');
    const mountWork = join(scratch, 'imggen-mount.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    // W4.10b: a fresh per-case llm-logs DB for the un-mocked `logLLMCall`.
    process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'imggen-llm-logs.db');
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    // jest.setup mocks the Lantern store bridge to a FIXED { mountPointId:
    // 'mock-lantern-mount' } + a stub writeLanternBackgroundToMountStore — un-mock
    // it so the REAL getLanternBackgroundsStore resolves the provisioned mount (and
    // the DELETE for the unprovisioned case actually nulls it), and the REAL store
    // write lands byte-diffable rows in doc_mount_* ([[jest-real-db-oracle]]).
    jest.doMock('@/lib/file-storage/lantern-store-bridge', () =>
      jest.requireActual('@/lib/file-storage/lantern-store-bridge'),
    );
    // The real bridge write goes through storeMountFile, whose transcodeToWebP we
    // pass-through-mock below; un-mock the character-vault bridge too so the
    // overlaid characters read resolves the real minted vault.
    jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
      jest.requireActual('@/lib/file-storage/character-vault-bridge'),
    );

    // Image provider seam (record the exact key + return a canned image).
    jest.doMock('@/lib/llm/plugin-factory', () => {
      const actual = jest.requireActual('@/lib/llm/plugin-factory');
      return {
        __esModule: true,
        ...actual,
        createImageProvider: (provider: string) => ({
          generateImage: async (params: Record<string, unknown>, _key: string) => {
            const key = `${provider}|${params.model}|${JSON.stringify(params)}`;
            const prompt = String(params.prompt ?? '');
            const images = [{ data: PNG_B64, mimeType: 'image/png', revisedPrompt: `revised: ${prompt.slice(0, 48)}` }];
            if (!recordedImages.has(key)) {
              recordedImages.set(key, { provider, model: String(params.model), key, images });
            }
            return { images };
          },
        }),
      };
    });

    // Orientation registry seam.
    jest.doMock('@/lib/plugins/provider-registry', () => {
      const actual = jest.requireActual('@/lib/plugins/provider-registry');
      return {
        __esModule: true,
        ...actual,
        getImageGenerationModels: (name: string) => modelsFor(name),
        getImageProviderConstraints: (name: string) => constraintsFor(name),
      };
    });

    // WebP transcode seams (pass-through both).
    jest.doMock('@/lib/files/webp-conversion', () => ({
      __esModule: true,
      convertToWebP: async (buffer: Buffer, mimeType: string, filename: string) => ({
        buffer,
        mimeType,
        filename,
        width: null,
        height: null,
      }),
    }));
    jest.doMock('@/lib/mount-index/blob-transcode', () => {
      const actual = jest.requireActual('@/lib/mount-index/blob-transcode');
      const { sha256OfBuffer } = jest.requireActual('@/lib/utils/sha256');
      return {
        __esModule: true,
        ...actual,
        // storeMountFile consumes { data, storedMimeType, sizeBytes, sha256 }.
        transcodeToWebP: async (data: Buffer, originalMimeType: string) => ({
          data,
          storedMimeType: originalMimeType,
          sizeBytes: data.length,
          sha256: sha256OfBuffer(data),
        }),
      };
    });

    // Completion seam (recording pattern). Only the craft-prompt call fires (the
    // corpus keeps danger OFF + no messages, so no classify/resolve/sanitize).
    jest.doMock('@/lib/llm', () => {
      const actual = jest.requireActual('@/lib/llm');
      return {
        __esModule: true,
        ...actual,
        createLLMProvider: async (provider: string, _baseUrl?: string) => ({
          sendMessage: async (params: { messages: Array<{ role: string; content: string }>; model: string; temperature?: number }, _apiKey: string) => {
            const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
            const user = messages.find((m) => m.role === 'user')?.content ?? '';
            // Craft: fail (empty) for the rose-garden placeholder case → the
            // tier-concatenation fallback; a success otherwise.
            let response: string;
            if (user.startsWith('Original prompt: ')) {
              response = /rose garden/.test(user)
                ? ''
                : 'A cinematic rendering with rich detail and dramatic lighting.';
            } else {
              throw new Error(`unexpected non-craft completion call: ${user.slice(0, 60)}`);
            }
            const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
            if (!recordedCompletions.has(key)) {
              recordedCompletions.set(key, { provider, model: params.model, temperature: params.temperature ?? null, messages, response, usage: craftUsage });
            }
            return { content: response, finishReason: 'stop', usage: craftUsage };
          },
        }),
      };
    });

    jest.doMock('@/lib/services/api-key.service', () => {
      const actual = jest.requireActual('@/lib/services/api-key.service');
      return { __esModule: true, ...actual, getApiKeyForCheapLLMSelection: async () => 'test-cheap-key' };
    });
    // W4.10b: run the REAL `logLLMCall` so the image-gen (IMAGE_GENERATION) +
    // cheap-task (IMAGE_PROMPT_CRAFTING / APPEARANCE_RESOLUTION) rows land.
    jest.doMock('@/lib/services/llm-logging.service', () =>
      jest.requireActual('@/lib/services/llm-logging.service')
    );
    jest.doMock('@/lib/plugins/moderation-provider-registry', () => ({
      __esModule: true,
      moderationProviderRegistry: {
        isInitialized: () => true,
        getAllProviders: () => [],
        getDefaultProvider: () => null,
      },
    }));
    // Round-3 unification (Group 4): the Lantern character-image notification runs
    // LIVE (RealLanternNotification on the Rust side) so its byte-exact persisted
    // content is diffed via `lanternContent` below.
    // The avatar-trigger enqueue calls ensureProcessorRunning, which auto-starts
    // v4's in-process job runner and CLAIMS the PENDING job (→ PROCESSING, attempts++,
    // startedAt) before the dump. The Rust queue_service fires a no-op wake hook (no
    // runner registered), so the job stays PENDING. Pin v4's auto-start to a no-op to
    // match ([[tier3-completion-oracle]]).
    jest.doMock('@/lib/background-jobs/processor', () => {
      const actual = jest.requireActual('@/lib/background-jobs/processor');
      return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
    });

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { getRawLLMLogsDatabase } = await import(
      '@/lib/database/backends/sqlite/llm-logs-client'
    );
    const { getRepositories } = await import('@/lib/repositories/factory');

    await initializeDatabase();
    const repos = getRepositories();

    (repos.connections as any).findApiKeyByIdAndUserId = async (id: string, userId: string) => {
      const key = spec.apiKeys[id];
      if (!key) return null;
      return { id, userId, label: 'canned', provider: 'OPENAI', key_value: key, isActive: true, createdAt: '2020-01-01T00:00:00.000Z', updatedAt: '2020-01-01T00:00:00.000Z' };
    };

    if (chat.clearLantern) {
      await rawQuery('DELETE FROM "instance_settings" WHERE "key" = ?', ['lanternBackgroundsMountPointId']);
    }

    // Freeze Date.now() so the provider filename `generated_<ts>.<ext>` is pinned.
    const frozen = spec.frozenNowMs;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    global.Date = class extends RealDate {
      constructor(...a: unknown[]) {
        if (a.length === 0) super(frozen);
        // @ts-expect-error forward variadic args
        else super(...a);
      }
      static now(): number {
        return frozen;
      }
    } as unknown as DateConstructor;

    try {
      const record: Record<string, unknown> = { kind: 'result', label };

      if (chat.op === 'generate') {
        const { executeImageGenerationTool } = await import(
          '@/lib/tools/handlers/image-generation-handler'
        );
        const out = await executeImageGenerationTool(
          { prompt: chat.prompt, orientation: chat.orientation, count: 1 },
          {
            userId: spec.userId,
            profileId: spec.profileId,
            chatId: chat.id,
            callingParticipantId: spec.callingParticipantId,
          },
        );
        record.resultJson = JSON.stringify(out);

        const midb = getRawMountIndexDatabase();
        if (!midb) throw new Error('mount-index DB handle unavailable for dump');
        const dumpMount = (table: string, orderBy: string) => {
          const columns = (midb.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>).map((x) => x.name);
          const rawRows = midb.prepare(`SELECT * FROM ${table}`).all() as Array<Record<string, unknown>>;
          return canonicalizeRows(table, columns, rawRows, orderBy);
        };
        const dumpMain = async (table: string, orderBy: string) => {
          const columns = ((await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>).map((x) => x.name);
          const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
          return canonicalizeRows(table, columns, rawRows, orderBy);
        };
        record.dumps = {
          doc_mount_points: dumpMount('doc_mount_points', 'id'),
          doc_mount_files: dumpMount('doc_mount_files', 'sha256'),
          doc_mount_blobs: dumpMount('doc_mount_blobs', 'sha256'),
          doc_mount_file_links: dumpMount('doc_mount_file_links', 'relativePath'),
          doc_mount_folders: dumpMount('doc_mount_folders', 'path'),
          files: await dumpMain('files', 'sha256'),
        };
        // Round-3 Group 4: the persisted Lantern `character-image` notification body
        // (proves the full byte-exact content, incl. the "attached here" tail the
        // old placeholder truncated). The inline file uuid is uuid-normalized on both
        // sides in the harness.
        const lanternRows = (await rawQuery(
          `SELECT content FROM chat_messages WHERE chatId = ? AND systemSender = 'lantern' AND systemKind = 'character-image'`,
          [chat.id],
        )) as Array<{ content: string }>;
        record.lanternContent = lanternRows.length > 0 ? lanternRows[0].content : null;
      } else {
        const { triggerAvatarGenerationIfEnabled } = await import('@/lib/wardrobe/avatar-generation');
        await triggerAvatarGenerationIfEnabled(repos, {
          userId: spec.userId,
          chatId: chat.id,
          characterId: spec.charAId,
          callerContext: 'oracle-test',
        });
        record.resultJson = JSON.stringify({ triggered: true });
        const dumpMain = async (table: string, orderBy: string) => {
          const columns = ((await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>).map((x) => x.name);
          const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<Record<string, unknown>>;
          return canonicalizeRows(table, columns, rawRows, orderBy);
        };
        record.dumps = { background_jobs: await dumpMain('background_jobs', 'id') };
      }

      // W4.10b: the llm_logs rows this case wrote (IMAGE_GENERATION + the cheap
      // IMAGE_PROMPT_CRAFTING / APPEARANCE_RESOLUTION tasks). `logLLMCall` is
      // fire-and-forget, so settle first. Avatar cases make no model call, so
      // the table may be absent → empty rows.
      await new Promise((resolve) => setTimeout(resolve, 150));
      record.llmLogs = ((): { columns: string[]; rows: Array<Record<string, unknown>> } => {
        try {
          const lldb = getRawLLMLogsDatabase();
          if (!lldb) return { columns: [], rows: [] };
          const exists = lldb
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='llm_logs'")
            .get();
          if (!exists) return { columns: [], rows: [] };
          const columns = (
            lldb.pragma('table_info(llm_logs)') as Array<{ name: string }>
          ).map((c) => c.name);
          const rawRows = lldb.prepare('SELECT * FROM llm_logs').all() as Array<
            Record<string, unknown>
          >;
          const rows = rawRows
            .map((r) => {
              const out: Record<string, unknown> = {};
              for (const col of columns) out[col] = canonValue(r[col]);
              out.id = '<id>';
              out.createdAt = '<ts>';
              out.updatedAt = '<ts>';
              return out;
            })
            .sort((a, b) => {
              const sa = JSON.stringify(a);
              const sb = JSON.stringify(b);
              return sa < sb ? -1 : sa > sb ? 1 : 0;
            });
          return { columns, rows };
        } catch {
          return { columns: [], rows: [] };
        }
      })();

      lines.push(JSON.stringify(record));
    } finally {
      global.Date = RealDate;
      await new Promise((resolve) => setTimeout(resolve, 50));
      await closeDatabase();
      closeMountIndexSQLiteClient();
      rmSync(scratch, { recursive: true, force: true });
    }
  }

  for (const entry of recordedCompletions.values()) lines.push(JSON.stringify({ kind: 'canned', ...entry }));
  for (const entry of recordedImages.values()) lines.push(JSON.stringify({ kind: 'cannedImage', ...entry }));

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`image-generation oracle wrote ${outPath} (${lines.length} lines)\n`);
}

test('image-generation oracle', async () => {
  await main();
});
