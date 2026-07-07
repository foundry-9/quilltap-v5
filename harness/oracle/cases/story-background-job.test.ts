/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the W4.9c STORY_BACKGROUND_GENERATION job handler (v4
 * `lib/background-jobs/handlers/story-background.ts` handleStoryBackgroundGeneration;
 * Rust `crates/quilltap-core/src/services/story_background_job.rs`).
 *
 * Drives v4's REAL handler over the shared two-DB fixture, ONE fresh copy per case.
 * Model/infra seams pinned (real DB stack wired back in past jest.setup):
 *   - `createLLMProvider` (`@/lib/llm`) → the recording-completion pattern; branches
 *     on a STORYCASE:<label> marker (carried by the chat title, echoed by the derive
 *     result / scene-state location) + the provider (cheap OPENAI vs uncensored
 *     OLLAMA) to drive derive / resolve / craft and the retry cases; RECORDS the
 *     exact `provider|model|temperature|messages` key the Rust CannedCompletionProvider
 *     replays.
 *   - `createImageProvider` (`@/lib/llm/plugin-factory`) → keyed by the recorded key
 *     in Rust to_key_value order; returns [] images for the no_images case.
 *   - `getImageProviderConstraints` / `getImageGenerationModels` → OPENAI + GROK
 *     size-strategy OrientationSupport.
 *   - `convertToWebP` / `transcodeToWebP` → PASS-THROUGH.
 *   - `findApiKeyByIdAndUserId` → canned map; `logLLMCall` / `ensureProcessorRunning`
 *     → no-op.
 *   - `fileStorageManager.uploadFile` → a DETERMINISTIC recorded FsSeam (project case).
 *   - Un-mock the character-vault + lantern-store bridges so the REAL vault + Lantern
 *     store writes land byte-diffable rows.
 *   - Date.now() frozen (spec.frozenNowMs).
 *
 * Emits one NDJSON line per RECORDED canned completion / canned image, and one per
 * case (kind:"result", { label, threw, dumps, storyImageId, lastBackgroundAt,
 * projectStoryImageId, lanternContent }).
 *
 * Run (Node 24, from the v4 checkout; stage OUTSIDE any .claude path):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; WT=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_STORY_MAIN=/tmp/qt-story-main.db QT_FIXTURE_STORY_MOUNT=/tmp/qt-story-mount.db \
 *     $N/node --import tsx $WT/harness/oracle/fixtures/build-story-background-job-fixture.ts
 *   TMPO=/tmp/qt-story-oracle; rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $WT/harness/oracle/cases/story-background-job.test.ts "$TMPO/cases/"
 *   cp $WT/harness/oracle/fixtures/story-background-job.json "$TMPO/fixtures/"
 *   QT_FIXTURE_STORY_MAIN=/tmp/qt-story-main.db QT_FIXTURE_STORY_MOUNT=/tmp/qt-story-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-story-background-job.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "story-background-job.test"
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
  characterIds: string[];
  imageProfileId: string;
  projectId?: string;
}
interface Spec {
  testPepperBase64: string;
  frozenNowMs: number;
  userId: string;
  apiKeys: Record<string, string>;
  chats: Record<string, ChatSpec>;
}

const OPENAI_SUPPORT = { strategy: 'size', portrait: { size: '1024x1792' }, landscape: { size: '1792x1024' }, square: { size: '1024x1024' } };
const GROK_SUPPORT = { strategy: 'size', portrait: { size: '768x1344' }, landscape: { size: '1344x768' }, square: { size: '1024x1024' } };
function supportFor(provider: string): unknown | null {
  if (provider === 'OPENAI') return OPENAI_SUPPORT;
  if (provider === 'GROK') return GROK_SUPPORT;
  return null;
}
function modelsFor(provider: string): Array<{ id: string; orientationSupport: unknown }> | null {
  const s = supportFor(provider);
  if (!s) return null;
  const id = provider === 'GROK' ? 'grok-2-image' : 'dall-e-3';
  return [{ id, orientationSupport: s }];
}
function constraintsFor(provider: string): { orientationSupport: unknown } | null {
  const s = supportFor(provider);
  return s ? { orientationSupport: s } : null;
}

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
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'story-background-job.json'), 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_STORY_MAIN;
  const mountFixture = process.env.QT_FIXTURE_STORY_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_STORY_MAIN and QT_FIXTURE_STORY_MOUNT must point at the seeded fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const cipherDriverPath = require('node:path').join(process.cwd(), 'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers');

  const PNG_B64 = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+M8AAAMBAQAY3Y2wAAAAAElFTkSuQmCC';
  const usage = { promptTokens: 100, completionTokens: 30, totalTokens: 130 };

  const lines: string[] = [];
  const RealDate = Date;
  const recordedCompletions = new Map<string, unknown>();
  const recordedImages = new Map<string, unknown>();

  for (const [label, chat] of Object.entries(spec.chats)) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-story-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'story-main.db');
    const mountWork = join(scratch, 'story-mount.db');
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
    jest.doMock('@/lib/file-storage/lantern-store-bridge', () => jest.requireActual('@/lib/file-storage/lantern-store-bridge'));
    jest.doMock('@/lib/file-storage/character-vault-bridge', () => jest.requireActual('@/lib/file-storage/character-vault-bridge'));

    // Completion seam (recording).
    jest.doMock('@/lib/llm', () => {
      const actual = jest.requireActual('@/lib/llm');
      return {
        __esModule: true,
        ...actual,
        createLLMProvider: async (provider: string) => ({
          sendMessage: async (params: { messages: Array<{ role: string; content: string }>; model: string; temperature?: number }, _apiKey: string) => {
            const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
            const user = messages.find((m) => m.role === 'user')?.content ?? '';
            const model = params.model;
            const temp = params.temperature ?? null;
            const lm = user.match(/STORYCASE:(\w+)/);
            const caseLabel = lm ? lm[1] : '';
            let response: string;
            if (user.endsWith('describe the scene these characters might be in:')) {
              response = `A quiet candlelit study at dusk (STORYCASE:${caseLabel})`;
            } else if (user.includes('Determine what each character currently looks like and is wearing:')) {
              if (caseLabel === 'appearance_retry' && provider !== 'OLLAMA') {
                response = '[]';
              } else {
                const ids = [...user.matchAll(/\(ID: ([0-9a-f-]{36})\)/g)].map((m) => m[1]);
                response = JSON.stringify(
                  ids.map((id) => ({ characterId: id, selectedDescriptionId: null, clothingDescription: 'a moss-green cloak', clothingSource: 'stored' })),
                );
              }
            } else if (user.endsWith('Create the atmospheric background prompt:')) {
              if (caseLabel === 'empty_craft_retry' && provider !== 'OLLAMA') {
                response = '';
              } else if (caseLabel === 'missing_char_enum') {
                response = 'A grand hall at midnight where Zelda waits by the great doors, lanterns glowing warmly.';
              } else {
                response = `An atmospheric ${caseLabel} landscape at dusk with soft, painterly light.`;
              }
            } else {
              throw new Error(`unexpected completion task: ${user.slice(0, 80)}`);
            }
            const key = `${provider}|${model}|${temp ?? '-'}|${JSON.stringify(messages)}`;
            if (!recordedCompletions.has(key)) {
              recordedCompletions.set(key, { provider, model, temperature: temp, messages, response, usage });
            }
            return { content: response, finishReason: 'stop', usage };
          },
        }),
      };
    });

    // Image provider seam.
    jest.doMock('@/lib/llm/plugin-factory', () => {
      const actual = jest.requireActual('@/lib/llm/plugin-factory');
      return {
        __esModule: true,
        ...actual,
        createImageProvider: (provider: string) => ({
          generateImage: async (params: Record<string, unknown>, _key: string) => {
            const key = canonicalImageKey(provider, params);
            const prompt = String(params.prompt ?? '');
            if (prompt.includes('no_images')) {
              if (!recordedImages.has(key)) recordedImages.set(key, { provider, model: String(params.model), key, images: [] });
              return { images: [] };
            }
            const images = [{ data: PNG_B64, mimeType: 'image/png', revisedPrompt: `revised: ${prompt.slice(0, 48)}` }];
            if (!recordedImages.has(key)) recordedImages.set(key, { provider, model: String(params.model), key, images });
            return { images };
          },
        }),
      };
    });

    jest.doMock('@/lib/plugins/provider-registry', () => {
      const actual = jest.requireActual('@/lib/plugins/provider-registry');
      return { __esModule: true, ...actual, getImageGenerationModels: (n: string) => modelsFor(n), getImageProviderConstraints: (n: string) => constraintsFor(n) };
    });

    jest.doMock('@/lib/files/webp-conversion', () => ({
      __esModule: true,
      convertToWebP: async (buffer: Buffer, mimeType: string, filename: string) => ({ buffer, mimeType, filename, width: null, height: null }),
    }));
    jest.doMock('@/lib/mount-index/blob-transcode', () => {
      const actual = jest.requireActual('@/lib/mount-index/blob-transcode');
      const { sha256OfBuffer } = jest.requireActual('@/lib/utils/sha256');
      return { __esModule: true, ...actual, transcodeToWebP: async (data: Buffer, originalMimeType: string) => ({ data, storedMimeType: originalMimeType, sizeBytes: data.length, sha256: sha256OfBuffer(data) }) };
    });

    // Deterministic project FsSeam (project_latest case).
    jest.doMock('@/lib/file-storage/manager', () => {
      const actual = jest.requireActual('@/lib/file-storage/manager');
      return {
        __esModule: true,
        ...actual,
        fileStorageManager: {
          ...actual.fileStorageManager,
          uploadFile: async (opts: { filename: string; content: Buffer; contentType: string; projectId: string; folderPath: string }) => ({
            storageKey: `project-upload:${opts.projectId}:${opts.filename}`,
            storedMimeType: opts.contentType,
            sizeBytes: opts.content.length,
          }),
        },
      };
    });

    jest.doMock('@/lib/services/llm-logging.service', () => {
      const actual = jest.requireActual('@/lib/services/llm-logging.service');
      return { __esModule: true, ...actual, logLLMCall: async () => undefined };
    });
    jest.doMock('@/lib/plugins/moderation-provider-registry', () => ({ __esModule: true, moderationProviderRegistry: { isInitialized: () => true, getAllProviders: () => [], getDefaultProvider: () => null } }));
    jest.doMock('@/lib/background-jobs/processor', () => {
      const actual = jest.requireActual('@/lib/background-jobs/processor');
      return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
    });

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
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
        // @ts-expect-error forward variadic
        else super(...a);
      }
      static now(): number {
        return frozen;
      }
    } as unknown as DateConstructor;

    try {
      const record: Record<string, unknown> = { kind: 'result', label };
      const { handleStoryBackgroundGeneration } = await import('@/lib/background-jobs/handlers/story-background');
      const job = {
        id: `oracle-story-${label}`,
        userId: spec.userId,
        type: 'STORY_BACKGROUND_GENERATION',
        status: 'PROCESSING',
        payload: {
          chatId: chat.id,
          imageProfileId: chat.imageProfileId,
          characterIds: chat.characterIds,
          ...(chat.projectId ? { projectId: chat.projectId } : {}),
        },
      };
      try {
        await handleStoryBackgroundGeneration(job as never);
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
        folders: await dumpMain('folders', 'path'),
      };

      const chatRows = (await rawQuery(`SELECT storyBackgroundImageId, lastBackgroundGeneratedAt FROM chats WHERE id = ?`, [chat.id])) as Array<{ storyBackgroundImageId: string | null; lastBackgroundGeneratedAt: string | null }>;
      record.storyImageId = chatRows.length > 0 ? chatRows[0].storyBackgroundImageId : null;
      record.lastBackgroundAt = chatRows.length > 0 ? chatRows[0].lastBackgroundGeneratedAt : null;

      // The project storyBackgroundImageId (overlay read) — proves the properties RMW.
      if (chat.projectId) {
        const project = await repos.projects.findById(chat.projectId);
        record.projectStoryImageId = (project as { storyBackgroundImageId?: string | null })?.storyBackgroundImageId ?? null;
      } else {
        record.projectStoryImageId = null;
      }

      const lanternRows = (await rawQuery(
        `SELECT content, opaqueContent FROM chat_messages WHERE chatId = ? AND systemSender = 'lantern' AND systemKind = 'background'`,
        [chat.id],
      )) as Array<{ content: string; opaqueContent: string }>;
      record.lanternContent = lanternRows.length > 0 ? lanternRows[0].content : null;
      record.lanternOpaque = lanternRows.length > 0 ? lanternRows[0].opaqueContent : null;

      lines.push(JSON.stringify(record));
    } finally {
      global.Date = RealDate;
      await new Promise((resolve) => setTimeout(resolve, 50));
      await closeDatabase();
      closeMountIndexSQLiteClient();
      rmSync(scratch, { recursive: true, force: true });
    }
  }

  for (const entry of recordedCompletions.values()) lines.push(JSON.stringify({ kind: 'canned', ...(entry as object) }));
  for (const entry of recordedImages.values()) lines.push(JSON.stringify({ kind: 'cannedImage', ...(entry as object) }));

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`story-background-job oracle wrote ${outPath} (${lines.length} lines)\n`);
}

test('story-background-job oracle', async () => {
  await main();
});
