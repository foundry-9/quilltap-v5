/**
 * @jest-environment node
 *
 * Differential ORACLE for the chat file/attachment subsystem (W4.4b). Drives v4's
 * REAL exported functions over the shared two-DB fixture:
 *   - `loadAndProcessFiles(repos, chatId, userId, connectionProfile, fileIds)`
 *     (`lib/services/chat-message/context-builder.service.ts`),
 *   - `processFileAttachmentFallback(file, fileAttachment, profile, repos, userId)`
 *     (`lib/chat/file-attachment-fallback.ts`),
 *   - `loadChatFilesForLLM(fileIds, { provider })` (`lib/chat-files-v2.ts`) — the
 *     Scriptorium mount-path branch (the fileId → doc_mount_blobs resolution the
 *     Lantern K-seam relies on).
 *
 * Pinned seams (mirrored on the Rust side):
 *   - `fileStorageManager.downloadFile(entry)` → the fixture's per-fileId bytes,
 *     or a throw for the missing-bytes file (mirrors the Rust `FileBytesStore`).
 *   - `createLLMProvider(...)` → a canned `sendMessage` resolving each vision call
 *     by (provider, model, attachment filename) from `spec.vision`, and RECORDING
 *     the exact call it answered as a `kind:"canned"` row so the Rust
 *     `CannedCompletionProvider` replays it (see [[tier3-completion-oracle]]).
 *   - `logLLMCall` → no-op. The compared results carry no llm_logs state, so the
 *     table dump is out of scope for this differential (the logging port is
 *     verified separately by W4.7e / the Phase-2 `llm_logs_tier2`); the Rust
 *     `log_llm_call` write is symmetrically inert (no llm-logs partition opened).
 *   - No `sharp` mock: every corpus image is small, so `resizeImageForProvider`
 *     early-returns before touching the codec (oversized-resize is skipped — the
 *     schedule is unit-tested in Rust).
 *
 * Real-DB-under-jest (memory-gate-tier3 recipe): resetModules + doMock past
 * jest.setup's global DB mocks + better-sqlite3 -> better-sqlite3-multiple-ciphers.
 *
 * Emits one NDJSON `{ kind:"case", family, label, result }` line per op plus one
 * `{ kind:"canned", provider, model, temperature, filename, mimeType, content,
 * finishReason, usage }` line per distinct vision call.
 *
 * Run (Node 24, from the v4 checkout; stage the case OUTSIDE any .claude path):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5 ; TMPO=/tmp/qt-oracle-run
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_FILE_ATTACH_MAIN=/tmp/qt-fa-main.db QT_FIXTURE_FILE_ATTACH_MOUNT=/tmp/qt-fa-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-file-attachment-fixture.ts
 *   mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $V5/harness/oracle/cases/file-attachment-tier3.test.ts "$TMPO/cases/"
 *   cp $V5/harness/oracle/fixtures/file-attachment.json       "$TMPO/fixtures/"
 *   QT_FIXTURE_FILE_ATTACH_MAIN=/tmp/qt-fa-main.db QT_FIXTURE_FILE_ATTACH_MOUNT=/tmp/qt-fa-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-file-attachment.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- file-attachment-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface FileSpec {
  key: string;
  id: string;
  originalFilename: string;
  mimeType: string;
  category: string;
  fsmBytes?: number[];
  fsmBytesUtf8?: string;
  fsmMissing?: boolean;
  generationPrompt?: string;
  generationRevisedPrompt?: string;
  description?: string;
}
interface VisionSpec {
  provider: string;
  modelName: string;
  filename: string;
  content: string;
  finishReason: string | null;
  usage: { promptTokens: number; completionTokens: number; totalTokens: number } | null;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  chatId: string;
  respProfiles: Record<string, Record<string, unknown>>;
  files: FileSpec[];
  vision: VisionSpec[];
}
interface Meta {
  mountLinkId: string;
  mountPointId: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'file-attachment.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_FILE_ATTACH_MAIN;
  const mountFixture = process.env.QT_FIXTURE_FILE_ATTACH_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_FILE_ATTACH_MAIN and QT_FIXTURE_FILE_ATTACH_MOUNT must point at the seeded fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  const meta = JSON.parse(fs.readFileSync(mainFixture + '.meta.json', 'utf8')) as Meta;

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );

  const fileByKey: Record<string, FileSpec> = {};
  for (const f of spec.files) fileByKey[f.key] = f;

  // fileId -> stored bytes for the FSM downloadFile mock (missing files absent).
  const fsmByFileId: Record<string, Buffer | null> = {};
  for (const f of spec.files) {
    if (f.fsmMissing) {
      fsmByFileId[f.id] = null;
      continue;
    }
    if (f.fsmBytesUtf8 !== undefined) fsmByFileId[f.id] = Buffer.from(f.fsmBytesUtf8, 'utf-8');
    else if (f.fsmBytes) fsmByFileId[f.id] = Buffer.from(f.fsmBytes);
    else fsmByFileId[f.id] = Buffer.alloc(0);
  }

  // The base64 `data` a (B) processFileAttachmentFallback case feeds as the
  // fileAttachment payload (mirrored byte-for-byte on the Rust side).
  const dataByKey: Record<string, string> = {};
  for (const f of spec.files) {
    if (f.fsmBytesUtf8 !== undefined) dataByKey[f.key] = Buffer.from(f.fsmBytesUtf8, 'utf-8').toString('base64');
    else if (f.fsmBytes) dataByKey[f.key] = Buffer.from(f.fsmBytes).toString('base64');
    else dataByKey[f.key] = '';
  }

  // The recorded canned vision calls (deduped by key).
  const cannedByKey: Record<string, Record<string, unknown>> = {};
  const cannedKey = (provider: string, model: string, temp: unknown, filename: string, mimeType: string): string =>
    `${provider}|${model}|${temp === undefined || temp === null ? '-' : String(temp)}|${filename}|${mimeType}`;

  // Copy the fixture to a scratch pair (loadAndProcessFiles is read-only after the
  // logLLMCall no-op; one copy serves the whole run).
  const scratch = mkdtempSync(join(tmpdir(), 'qt-fa-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'fa-main.db');
  const mountWork = join(scratch, 'fa-mount.db');
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

  // FSM downloadFile → the fixture's per-fileId bytes (throw for missing).
  jest.doMock('@/lib/file-storage/manager', () => ({
    __esModule: true,
    fileStorageManager: {
      downloadFile: async (entry: { id: string }) => {
        const buf = fsmByFileId[entry.id];
        if (buf === undefined) throw new Error(`no canned bytes for fileId ${entry.id}`);
        if (buf === null) throw new Error(`storage read failed for fileId ${entry.id}`);
        return buf;
      },
    },
  }));

  // Vision provider → canned by (provider, model, attachment filename); record.
  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string) => ({
        sendMessage: async (params: {
          model: string;
          temperature?: number;
          messages: Array<{ role: string; content: string; attachments?: Array<{ filename: string; mimeType: string }> }>;
        }) => {
          const att = params.messages?.[0]?.attachments?.[0];
          const filename = att?.filename ?? '';
          const mimeType = att?.mimeType ?? '';
          const entry = spec.vision.find(
            (v) => v.provider === provider && v.modelName === params.model && v.filename === filename,
          );
          if (!entry) {
            throw new Error(`no canned vision response for ${provider} ${params.model} ${filename}`);
          }
          const key = cannedKey(provider, params.model, params.temperature, filename, mimeType);
          cannedByKey[key] = {
            kind: 'canned',
            provider,
            model: params.model,
            temperature: params.temperature ?? null,
            filename,
            mimeType,
            content: entry.content,
            finishReason: entry.finishReason,
            usage: entry.usage,
          };
          return { content: entry.content, finishReason: entry.finishReason, usage: entry.usage };
        },
      }),
    };
  });

  // logLLMCall → no-op (the compared results carry no llm_logs state).
  jest.doMock('@/lib/services/llm-logging.service', () => {
    const actual = jest.requireActual('@/lib/services/llm-logging.service');
    return { __esModule: true, ...actual, logLLMCall: async () => {} };
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { loadAndProcessFiles } = await import('@/lib/services/chat-message/context-builder.service');
  const { processFileAttachmentFallback } = await import('@/lib/chat/file-attachment-fallback');
  const { loadChatFilesForLLM } = await import('@/lib/chat-files-v2');
  await initializeDatabase();
  const repos = getRepositories();

  const lines: string[] = [];

  // ---- (A) loadAndProcessFiles ----------------------------------------------
  interface LapCase {
    label: string;
    profile: string;
    fileKeys: string[];
  }
  const lapCases: LapCase[] = [
    { label: 'lap_text', profile: 'noImg', fileKeys: ['text'] },
    { label: 'lap_image_kept', profile: 'imgOK', fileKeys: ['keptImage'] },
    { label: 'lap_image_desc', profile: 'noImg', fileKeys: ['descImage'] },
    { label: 'lap_refusal_retry', profile: 'noImg', fileKeys: ['refusalImage'] },
    { label: 'lap_unsupported', profile: 'noImg', fileKeys: ['zip'] },
    { label: 'lap_load_skip', profile: 'noImg', fileKeys: ['missing'] },
    { label: 'lap_multi', profile: 'noImg', fileKeys: ['text', 'zip'] },
  ];
  for (const c of lapCases) {
    const profile = spec.respProfiles[c.profile];
    const fileIds = c.fileKeys.map((k) => fileByKey[k].id);
    const result = await loadAndProcessFiles(repos, spec.chatId, spec.userId, profile as never, fileIds);
    lines.push(
      JSON.stringify({
        kind: 'case',
        family: 'lap',
        label: c.label,
        result: {
          prefix: result.messageContentPrefix,
          attachedFileIds: result.attachedFiles.map((f: { id: string }) => f.id),
          attachmentsToSend: result.attachmentsToSend,
        },
      }),
    );
  }

  // ---- (B) processFileAttachmentFallback ------------------------------------
  interface FbCase {
    label: string;
    profile: string;
    fileKey: string;
  }
  const fbCases: FbCase[] = [
    { label: 'fb_text', profile: 'noImg', fileKey: 'text' },
    { label: 'fb_kept_raw', profile: 'imgOK', fileKey: 'keptImage' },
    { label: 'fb_desc', profile: 'noImg', fileKey: 'descImage' },
    { label: 'fb_refusal_retry', profile: 'noImg', fileKey: 'refusalImage' },
    { label: 'fb_unsupported', profile: 'noImg', fileKey: 'zip' },
    { label: 'fb_reuse_revised', profile: 'noImg', fileKey: 'reuseRevised' },
    { label: 'fb_reuse_prompt', profile: 'noImg', fileKey: 'reusePrompt' },
    { label: 'fb_reuse_desc', profile: 'noImg', fileKey: 'reuseDesc' },
    { label: 'fb_reuse_whitespace', profile: 'noImg', fileKey: 'reuseWhitespace' },
  ];
  for (const c of fbCases) {
    const profile = spec.respProfiles[c.profile];
    const f = fileByKey[c.fileKey];
    const fileMetadata = {
      id: f.id,
      filepath: `/api/v1/files/${f.id}`,
      filename: f.originalFilename,
      mimeType: f.mimeType,
      size: dataByKey[c.fileKey].length,
    };
    const fileAttachment = { ...fileMetadata, data: dataByKey[c.fileKey] };
    const result = await processFileAttachmentFallback(
      fileMetadata,
      fileAttachment as never,
      profile as never,
      repos,
      spec.userId,
    );
    lines.push(JSON.stringify({ kind: 'case', family: 'fb', label: c.label, result }));
  }

  // ---- (C) loadChatFilesForLLM (mount-path branch) --------------------------
  {
    const attachments = await loadChatFilesForLLM([meta.mountLinkId], { provider: 'DEEPSEEK' as never });
    lines.push(JSON.stringify({ kind: 'case', family: 'lcffl', label: 'lcffl_mount', result: attachments }));
  }

  // ---- canned rows ----------------------------------------------------------
  for (const row of Object.values(cannedByKey)) lines.push(JSON.stringify(row));

  await new Promise((resolve) => setTimeout(resolve, 50));
  await closeDatabase();
  rmSync(scratch, { recursive: true, force: true });

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(
    `file-attachment oracle wrote ${outPath} (${lapCases.length + fbCases.length + 1} cases, ${Object.keys(cannedByKey).length} canned)\n`,
  );
}

test('file-attachment oracle', async () => {
  await main();
});
