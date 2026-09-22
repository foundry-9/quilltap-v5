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
 *   - `ensureProcessorRunning` → no-op; `logLLMCall` runs REAL (W4.10b) so the
 *     IMAGE_GENERATION rows land + are dumped/diffed.
 *   - Un-mock the character-vault bridge + mount-index modules so the REAL vault
 *     write lands byte-diffable rows; `transcodeToWebP` pass-through.
 *   - Date.now() frozen (spec.frozenNowMs) so the provider filename + the frozen
 *     characterAvatars.generatedAt are pinned.
 *
 * P4.104 — THE IMAGE ROW (bug 159's image half, v4 `186eb09cb`). A case with
 * `imageSeed` flips `__qtRealTranscode` for its run, so BOTH mocked transcode
 * modules delegate to the REAL ones (real sharp) — `convertToWebP` AND the
 * vault bridge's `transcodeToWebP`, which is also what `linkBlobContent`'s
 * normalization calls — and the provider answers that seed (`../fixtures/`)
 * as `image/webp`. The seed is the 748 KB LOSSLESS `photo-lossless.webp`:
 * `convertToWebP` skips an already-WebP input, so the only step that can move
 * it is the vault write's normalization — v4 re-encodes it lossy, same path,
 * smaller. The comparand is D19 — `imageFacts` over the written
 * `images/history/` row (`../lib/blob-image-facts`); the Rust family blanks
 * that case's encoder-owned values on both sides.
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
 *   mkdir -p "$TMPO/lib"
 *   cp $WT/harness/oracle/lib/blob-image-facts.ts "$TMPO/lib/"
 *   cp $WT/harness/oracle/fixtures/normalize-blob-image/photo-lossless.webp "$TMPO/fixtures/"
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
import { blobImageFacts, sharpMeasure, STORED_BLOB_SELECT } from '../lib/blob-image-facts';

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
  projectId?: string;
  equipped: Record<string, string[]>;
  equippedSlotsOverride?: Record<string, string[]>;
  expectWrite: boolean;
  /**
   * P4.D184: run the handler TWICE on one fixture copy. Run 1 writes the keyed
   * row; run 2 meets it. What run 2 does — bind and stop, or generate again —
   * is the configuration cache, and the `files` census after run 2 says which.
   */
  runTwice?: boolean;
  /** Run 2 carries `force: true` (the manual regenerate button's reroll). */
  forceOnSecondRun?: boolean;
  /**
   * A surgical change between the runs, so run 2 meets a cache the feature must
   * refuse or accept for a specific reason:
   *   `blob-gone`       the written row's mount blob is deleted (a MISS).
   *   `legacy-key`      the row is re-keyed under its v0 key, derived from its
   *                     OWN generationModel/generationPrompt exactly as the
   *                     collapse migration does — so run 2 exercises the v0
   *                     fallback and must NOT upgrade the row to a v1 key.
   *   `cross-character` the row is re-tagged to another character (a MISS: a
   *                     key must never hand one character another's face).
   */
  mutateBetweenRuns?: 'blob-gone' | 'legacy-key' | 'cross-character';
  /**
   * P4.104: a seed image under `fixtures/` the provider answers with, and the
   * switch that makes this case's transcode + blob normalization REAL.
   */
  imageSeed?: string;
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

/** P4.D138: a canned per-model `loraSupport`, so the shared params builder's
 *  cap + trigger-phrase append are MEASURABLE on this path. Two adapters, no
 *  scale block (the host's DEFAULT_LORA_SCALE applies). */
const CANNED_LORA_SUPPORT = { maxLoras: 2, sourceKinds: ['url', 'hf-repo'] };
function modelsFor(
  provider: string,
): Array<{ id: string; orientationSupport: unknown; loraSupport?: unknown }> | null {
  if (provider === 'OPENAI')
    return [{ id: 'dall-e-3', orientationSupport: OPENAI_SUPPORT, loraSupport: CANNED_LORA_SUPPORT }];
  return null;
}
function constraintsFor(provider: string): { orientationSupport: unknown } | null {
  if (provider === 'OPENAI') return { orientationSupport: OPENAI_SUPPORT };
  return null;
}

// The canonical image-gen key, equal to the Rust `image_gen_key` (with_raw_key looks
// up by the Rust key).
function canonicalImageKey(provider: string, params: Record<string, unknown>): string {
  // The key is `JSON.stringify` of the params in v4's OWN insertion order —
  // `params` here IS the object `buildImageGenParams` built, so walking its
  // keys reproduces the order v5's `ImageGenParams::to_key_value` reproduces
  // (`{prompt, model, n}` first, each conditional assignment in source order,
  // then `loras`, then `profileParameters` — and a `size`/`aspectRatio` the
  // ORIENTATION pass inserted lands after `steps`, which a fixed-order rebuild
  // put in the wrong slot; the round-2 unification's `to_key_value` fix
  // surfaced it). Only the known host keys are kept, in the object's order;
  // undefined/null drop as JSON.stringify drops undefined.
  const KNOWN = new Set(['prompt','model','n','negativePrompt','size','aspectRatio','quality','style','responseFormat','seed','guidanceScale','steps','loras','profileParameters']);
  const c: Record<string, unknown> = {};
  for (const k of Object.keys(params)) {
    const v = params[k];
    if (KNOWN.has(k) && v !== undefined && v !== null) c[k] = v;
  }
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
    // W4.10b: a fresh per-case llm-logs DB for the un-mocked `logLLMCall`.
    process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'avatar-llm-logs.db');
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
            // P4.104: an `imageSeed` case answers the seed's bytes as WebP.
            const seed = (globalThis as { __qtImageSeed?: Buffer }).__qtImageSeed;
            const images = seed
              ? [{ data: seed.toString('base64'), mimeType: 'image/webp', revisedPrompt: `revised: ${prompt.slice(0, 48)}` }]
              : [{ data: PNG_B64, mimeType: 'image/png', revisedPrompt: `revised: ${prompt.slice(0, 48)}` }];
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

    // P4.104: pass-through, or REAL under `__qtRealTranscode`.
    jest.doMock('@/lib/files/webp-conversion', () => {
      const actual = jest.requireActual('@/lib/files/webp-conversion');
      return {
        __esModule: true,
        convertToWebP: async (buffer: Buffer, mimeType: string, filename: string) =>
          (globalThis as { __qtRealTranscode?: boolean }).__qtRealTranscode
            ? actual.convertToWebP(buffer, mimeType, filename)
            : {
                buffer,
                mimeType,
                filename,
                width: null,
                height: null,
              },
      };
    });
    jest.doMock('@/lib/mount-index/blob-transcode', () => {
      const actual = jest.requireActual('@/lib/mount-index/blob-transcode');
      const { sha256OfBuffer } = jest.requireActual('@/lib/utils/sha256');
      return {
        __esModule: true,
        ...actual,
        transcodeToWebP: async (data: Buffer, originalMimeType: string) =>
          (globalThis as { __qtRealTranscode?: boolean }).__qtRealTranscode
            ? actual.transcodeToWebP(data, originalMimeType)
            : {
                data,
                storedMimeType: originalMimeType,
                sizeBytes: data.length,
                sha256: sha256OfBuffer(data),
              },
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

    // W4.10b: run the REAL `logLLMCall` so the IMAGE_GENERATION rows land.
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

    const seedBytes = chat.imageSeed
      ? fs.readFileSync(join(here, '..', 'fixtures', chat.imageSeed))
      : undefined;
    if (seedBytes) {
      (globalThis as { __qtImageSeed?: Buffer }).__qtImageSeed = seedBytes;
      (globalThis as { __qtRealTranscode?: boolean }).__qtRealTranscode = true;
    }

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

      const runOnce = async (force: boolean): Promise<string | null> => {
        try {
          await handleCharacterAvatarGeneration({
            ...job,
            payload: { ...job.payload, ...(force ? { force: true } : {}) },
          } as never);
          return null;
        } catch (e) {
          return e instanceof Error ? e.message : String(e);
        }
      };

      record.threw = await runOnce(false);

      if (chat.runTwice) {
        // P4.D184: the surgical change between the runs, when the case wants one.
        if (chat.mutateBetweenRuns === 'blob-gone') {
          const rows = (await rawQuery(
            `SELECT storageKey FROM files WHERE generationKey IS NOT NULL AND storageKey IS NOT NULL`,
          )) as Array<{ storageKey: string }>;
          const midb0 = getRawMountIndexDatabase();
          for (const r of rows) {
            const rest = r.storageKey.startsWith('mount-blob:')
              ? r.storageKey.slice('mount-blob:'.length)
              : '';
            const sep = rest.indexOf(':');
            if (sep < 1) continue;
            midb0?.prepare('DELETE FROM doc_mount_blobs WHERE id = ?').run(rest.slice(sep + 1));
          }
        } else if (chat.mutateBetweenRuns === 'legacy-key') {
          const { deriveLegacyAvatarCacheKey } = await import('@/lib/wardrobe/avatar-cache');
          const rows = (await rawQuery(
            `SELECT id, generationModel, generationPrompt FROM files WHERE generationKey IS NOT NULL`,
          )) as Array<{ id: string; generationModel: string | null; generationPrompt: string | null }>;
          for (const r of rows) {
            const legacy = deriveLegacyAvatarCacheKey({
              modelName: r.generationModel,
              prompt: r.generationPrompt ?? '',
            });
            await rawQuery(`UPDATE files SET generationKey = ? WHERE id = ?`, [legacy, r.id]);
          }
        } else if (chat.mutateBetweenRuns === 'cross-character') {
          await rawQuery(
            `UPDATE files SET tags = ? WHERE generationKey IS NOT NULL`,
            [JSON.stringify(['00000000-0000-4000-8000-0000000000ff'])],
          );
        }
        record.threwSecond = await runOnce(chat.forceOnSecondRun === true);
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
        // P4.D184: v4 creates a collection's table lazily, so a table nothing has
        // written is ABSENT rather than empty. `folders` is exactly that case —
        // and "the avatar path mints no folder row" is most honestly measured as
        // "the table was never brought into being". An absent table dumps as an
        // empty one on BOTH sides, so a side that DID mint a row diverges.
        if (columns.length === 0) return canonicalizeRows(table, [], [], orderBy);
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
        // P4.D184: v4 `7fbf8a55b` stopped minting a legacy `folders` row per
        // avatar. Nothing in the census could see that until the table was
        // dumped, so it is dumped — the project-chat case is the arm that
        // would have carried one before.
        folders: await dumpMain('folders', 'id'),
      };

      // The Lantern avatar notification (sender aurora, systemKind avatar).
      // `f45a517a9` compresses `chat_messages.content`/`opaqueContent` at rest
      // (a BLOB above the codec's threshold), so a raw SELECT answers a Buffer
      // the Rust side cannot parse as the string it compares. Read them through
      // `qt_text()`, which v4 registers on every connection and which is total
      // (a plaintext cell decodes to itself) — the comparand is the TEXT, as the
      // Rust side's `get_messages` decodes it.
      const lanternRows = (await rawQuery(
        `SELECT qt_text(content) AS content, qt_text(opaqueContent) AS opaqueContent FROM chat_messages WHERE chatId = ? AND systemSender = 'aurora' AND systemKind = 'avatar'`,
        [chat.id],
      )) as Array<{ content: string; opaqueContent: string }>;
      record.lanternContent = lanternRows.length > 0 ? lanternRows[0].content : null;
      record.lanternOpaque = lanternRows.length > 0 ? lanternRows[0].opaqueContent : null;
      // P4.D184: a cache HIT produces nothing, so it posts no notification —
      // and on a `runTwice` case run 1's own notification is still there. The
      // COUNT is what tells the two apart; the content of the first row cannot.
      record.lanternCount = lanternRows.length;

      if (seedBytes) {
        // P4.104 — D19: what the vault write did to the seed, never its bytes.
        const rows = midb
          .prepare(STORED_BLOB_SELECT + 'WHERE l.relativePath LIKE ? ORDER BY l.relativePath')
          .all(`images/history/avatar_%_${frozen}.%`) as Parameters<typeof blobImageFacts>[0];
        // eslint-disable-next-line @typescript-eslint/no-require-imports
        const sharp = require(require('node:path').join(process.cwd(), 'node_modules/sharp'));
        record.imageFacts = await blobImageFacts(rows, seedBytes, sharpMeasure(sharp));
      }

      // chat.characterAvatars + character.avatarOverrides (the two JSON updates).
      const chatRows = (await rawQuery(`SELECT characterAvatars FROM chats WHERE id = ?`, [chat.id])) as Array<{ characterAvatars: string | null }>;
      record.characterAvatars = chatRows.length > 0 ? chatRows[0].characterAvatars : null;
      const charRows = (await rawQuery(`SELECT avatarOverrides FROM characters WHERE id = ?`, [chat.characterId])) as Array<{ avatarOverrides: string | null }>;
      record.avatarOverrides = charRows.length > 0 ? charRows[0].avatarOverrides : null;

      // W4.10b: the IMAGE_GENERATION rows this case wrote (fire-and-forget →
      // settle first; a skipped case makes no model call → empty).
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
      (globalThis as { __qtImageSeed?: Buffer }).__qtImageSeed = undefined;
      (globalThis as { __qtRealTranscode?: boolean }).__qtRealTranscode = false;
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
