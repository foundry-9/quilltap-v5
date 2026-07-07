/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the W4.9c CHARACTER_AVATAR_GENERATION job handler (v4
 * `lib/background-jobs/handlers/character-avatar.ts` handleCharacterAvatarGeneration;
 * Rust `crates/quilltap-core/src/services/character_avatar_job.rs`).
 *
 * Drives v4's REAL handler over the shared two-DB fixture, ONE fresh copy per case
 * (the handler WRITES the character vault store tables + `files` + `chats` +
 * `characters` + a `chat_messages` Lantern notification row). Model/infra seams
 * pinned (real DB stack wired back in past jest.setup — [[jest-real-db-oracle]]):
 *
 *   - `createImageProvider` (`@/lib/llm/plugin-factory`) → a fake provider whose
 *     `generateImage(params, key)` KEYS on `provider|model|<JSON of params in Rust
 *     to_key_value field order>`, RECORDS the key (kind:"cannedImage") so the Rust
 *     CannedImageProvider replays the SAME key (proving the built portrait prompt +
 *     applyOrientation reach the wire). The `blocked-model` model THROWS a
 *     content-moderation error (recorded kind:"cannedImageFailure") to drive the
 *     post-hoc reroute; the uncensored `dall-e-3` reroute succeeds.
 *   - `getImageProviderConstraints` + `getImageGenerationModels` → canned OPENAI
 *     size-strategy OrientationSupport.
 *   - `convertToWebP` (`@/lib/files/webp-conversion`) → PASS-THROUGH.
 *   - `findApiKeyByIdAndUserId` (repos.connections) → the canned apiKeyId->key map.
 *   - moderation registry → null (danger mode is OFF for userA; userB skips the
 *     pre-scan via scanImagePrompts:false).
 *   - `createLLMProvider` → throws if called (avatars fire NO completion calls).
 *   - `logLLMCall` / `ensureProcessorRunning` → no-op.
 *   - Un-mock the character-vault bridge + mount-index modules so the REAL vault
 *     write lands byte-diffable rows; `transcodeToWebP` pass-through.
 *   - Date.now() frozen (spec.frozenNowMs) so the provider filename + the frozen
 *     characterAvatars.generatedAt are pinned.
 *
 * Emits one NDJSON line per RECORDED canned image call / failure, and one per case
 * (kind:"result", { label, threw, dumps, lanternContent, characterAvatars,
 * avatarOverrides }).
 *
 * Run (Node 24, from the v4 checkout; stage OUTSIDE any .claude path):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; WT=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_AVATAR_MAIN=/tmp/qt-avatar-main.db QT_FIXTURE_AVATAR_MOUNT=/tmp/qt-avatar-mount.db \
 *     $N/node --import tsx $WT/harness/oracle/fixtures/build-avatar-job-fixture.ts
 *   TMPO=/tmp/qt-avatar-oracle; rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $WT/harness/oracle/cases/avatar-job.test.ts "$TMPO/cases/"
 *   cp $WT/harness/oracle/fixtures/avatar-job.json "$TMPO/fixtures/"
 *   QT_FIXTURE_AVATAR_MAIN=/tmp/qt-avatar-main.db QT_FIXTURE_AVATAR_MOUNT=/tmp/qt-avatar-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-avatar-job.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "avatar-job.test"
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
  userId: string;
  characterId: string;
  imageProfileId: string;
  equipped: Record<string, string[]>;
  equippedSlotsOverride?: Record<string, string[]>;
  expectWrite: boolean;
}
interface Spec {
  testPepperBase64: string;
  frozenNowMs: number;
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

// The canonical image-gen key in Rust `ImageGenParams::to_key_value` field order,
// so the recorded key equals the Rust `image_gen_key` (with_raw_key looks up by the
// Rust key). Optional fields are omitted when undefined (JSON.stringify drop).
function canonicalImageKey(provider: string, params: Record<string, unknown>): string {
  const c: Record<string, unknown> = {};
  const put = (k: string, v: unknown) => {
    if (v !== undefined && v !== null) c[k] = v;
  };
  put('prompt', params.prompt);
  put('negativePrompt', params.negativePrompt);
  put('model', params.model);
  put('n', params.n);
  put('size', params.size);
  put('aspectRatio', params.aspectRatio);
  put('quality', params.quality);
  put('style', params.style);
  put('seed', params.seed);
  put('guidanceScale', params.guidanceScale);
  put('steps', params.steps);
  return `${provider}|${params.model}|${JSON.stringify(c)}`;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'avatar-job.json'), 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_AVATAR_MAIN;
  const mountFixture = process.env.QT_FIXTURE_AVATAR_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_AVATAR_MAIN and QT_FIXTURE_AVATAR_MOUNT must point at the seeded fixtures');
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
  const recordedImages = new Map<string, { provider: string; model: string; key: string; images: Array<{ data: string; mimeType?: string; revisedPrompt?: string }> }>();
  const recordedImageFailures = new Map<string, { key: string; message: string }>();

  for (const [label, chat] of Object.entries(spec.chats)) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-avatar-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'avatar-main.db');
    const mountWork = join(scratch, 'avatar-mount.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
      jest.requireActual('@/lib/file-storage/character-vault-bridge'),
    );

    // Image provider seam (record the exact key; blocked-model throws a moderation error).
    jest.doMock('@/lib/llm/plugin-factory', () => {
      const actual = jest.requireActual('@/lib/llm/plugin-factory');
      return {
        __esModule: true,
        ...actual,
        createImageProvider: (provider: string) => ({
          generateImage: async (params: Record<string, unknown>, _key: string) => {
            const key = canonicalImageKey(provider, params);
            if (params.model === 'blocked-model') {
              if (!recordedImageFailures.has(key)) {
                recordedImageFailures.set(key, { key, message: 'content policy violation on this prompt' });
              }
              throw new Error('content policy violation on this prompt');
            }
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

    jest.doMock('@/lib/plugins/provider-registry', () => {
      const actual = jest.requireActual('@/lib/plugins/provider-registry');
      return {
        __esModule: true,
        ...actual,
        getImageGenerationModels: (name: string) => modelsFor(name),
        getImageProviderConstraints: (name: string) => constraintsFor(name),
      };
    });

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
        transcodeToWebP: async (data: Buffer, originalMimeType: string) => ({
          data,
          storedMimeType: originalMimeType,
          sizeBytes: data.length,
          sha256: sha256OfBuffer(data),
        }),
      };
    });

    jest.doMock('@/lib/llm', () => {
      const actual = jest.requireActual('@/lib/llm');
      return {
        __esModule: true,
        ...actual,
        createLLMProvider: async () => ({
          sendMessage: async () => {
            throw new Error('unexpected completion call in avatar path');
          },
        }),
      };
    });

    jest.doMock('@/lib/services/llm-logging.service', () => {
      const actual = jest.requireActual('@/lib/services/llm-logging.service');
      return { __esModule: true, ...actual, logLLMCall: async () => undefined };
    });
    jest.doMock('@/lib/plugins/moderation-provider-registry', () => ({
      __esModule: true,
      moderationProviderRegistry: {
        isInitialized: () => true,
        getAllProviders: () => [],
        getDefaultProvider: () => null,
      },
    }));
    jest.doMock('@/lib/background-jobs/processor', () => {
      const actual = jest.requireActual('@/lib/background-jobs/processor');
      return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
    });

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
      '@/lib/database/backends/sqlite/mount-index-client'
    );
    const { getRepositories } = await import('@/lib/repositories/factory');

    await initializeDatabase();
    const repos = getRepositories();

    (repos.connections as any).findApiKeyByIdAndUserId = async (id: string, userId: string) => {
      const key = spec.apiKeys[id];
      if (!key) return null;
      return { id, userId, label: 'canned', provider: 'OPENAI', key_value: key, isActive: true, createdAt: '2020-01-01T00:00:00.000Z', updatedAt: '2020-01-01T00:00:00.000Z' };
    };

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

      const { handleCharacterAvatarGeneration } = await import(
        '@/lib/background-jobs/handlers/character-avatar'
      );
      const job = {
        id: `oracle-avatar-${label}`,
        userId: chat.userId,
        type: 'CHARACTER_AVATAR_GENERATION',
        status: 'PROCESSING',
        payload: {
          chatId: chat.id,
          characterId: chat.characterId,
          imageProfileId: chat.imageProfileId,
          ...(chat.equippedSlotsOverride ? { equippedSlotsOverride: chat.equippedSlotsOverride } : {}),
        },
      };

      try {
        await handleCharacterAvatarGeneration(job as never);
        record.threw = null;
      } catch (e) {
        record.threw = e instanceof Error ? e.message : String(e);
      }

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

      // The Lantern avatar notification (sender aurora, systemKind avatar).
      const lanternRows = (await rawQuery(
        `SELECT content, opaqueContent FROM chat_messages WHERE chatId = ? AND systemSender = 'aurora' AND systemKind = 'avatar'`,
        [chat.id],
      )) as Array<{ content: string; opaqueContent: string }>;
      record.lanternContent = lanternRows.length > 0 ? lanternRows[0].content : null;
      record.lanternOpaque = lanternRows.length > 0 ? lanternRows[0].opaqueContent : null;

      // chat.characterAvatars + character.avatarOverrides (the two JSON updates).
      const chatRows = (await rawQuery(`SELECT characterAvatars FROM chats WHERE id = ?`, [chat.id])) as Array<{ characterAvatars: string | null }>;
      record.characterAvatars = chatRows.length > 0 ? chatRows[0].characterAvatars : null;
      const charRows = (await rawQuery(`SELECT avatarOverrides FROM characters WHERE id = ?`, [chat.characterId])) as Array<{ avatarOverrides: string | null }>;
      record.avatarOverrides = charRows.length > 0 ? charRows[0].avatarOverrides : null;

      lines.push(JSON.stringify(record));
    } finally {
      global.Date = RealDate;
      await new Promise((resolve) => setTimeout(resolve, 50));
      await closeDatabase();
      closeMountIndexSQLiteClient();
      rmSync(scratch, { recursive: true, force: true });
    }
  }

  for (const entry of recordedImages.values()) lines.push(JSON.stringify({ kind: 'cannedImage', ...entry }));
  for (const entry of recordedImageFailures.values()) lines.push(JSON.stringify({ kind: 'cannedImageFailure', ...entry }));

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`avatar-job oracle wrote ${outPath} (${lines.length} lines)\n`);
}

test('avatar-job oracle', async () => {
  await main();
});
