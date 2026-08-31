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
interface AttachmentRef { file?: string; raw?: Record<string, unknown> }
interface AdaptCase {
  label: string;
  profile: string;
  messages: Array<{ role: string; content: string; attachments?: AttachmentRef[] }>;
}
interface MimeCase { label: string; attachments: AttachmentRef[] }
interface Spec {
  testPepperBase64: string;
  userId: string;
  chatId: string;
  respProfiles: Record<string, Record<string, unknown>>;
  files: FileSpec[];
  vision: VisionSpec[];
  adaptCases: AdaptCase[];
  mimeCases: MimeCase[];
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
          maxTokens?: number;
          topP?: number;
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
            // P4.D83 (v4 `d89babc4`): the knobs `describeImageWithProfile`
            // resolved off the describer profile's bag (`sampling.temperature ??
            // 0.7`, `sampling.maxTokens ?? 1000`, `sampling.topP`). `topP` never
            // reached this call in v5 — its completion params could not carry
            // one — and the corpus could not see it because both describer
            // profiles carried an EMPTY bag.
            sampling: {
              ...(params.temperature !== undefined ? { temperature: params.temperature } : {}),
              ...(params.maxTokens !== undefined ? { maxTokens: params.maxTokens } : {}),
              ...(params.topP !== undefined ? { topP: params.topP } : {}),
            },
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
    // Bug 91 (a14a1811): the chat profile's model reads images but its plugin
    // cannot send them — the describe-fallback fires and the raw bytes do NOT
    // ride in attachmentsToSend.
    { label: 'lap_vision_no_transport', profile: 'visionNoTransport', fileKeys: ['descImage'] },
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
    // Bug 91 (a14a1811): a ticked vision box on a non-transporting plugin
    // routes an image to the describe-fallback…
    { label: 'fb_vision_no_transport', profile: 'visionNoTransport', fileKey: 'descImage' },
    // …while a non-image type on the same profile takes the ordinary
    // unsupported-type text-inline path, untouched by the transport check.
    { label: 'fb_vision_no_transport_text', profile: 'visionNoTransport', fileKey: 'text' },
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

  // ---- (E) adaptMessagesForProfile / collectAttachmentMimeTypes -------------
  //
  // v4 `lib/chat/message-attachment-adapter.ts` (`a1d88aa3a`, bug 106). Placed
  // between (B) and (C) on BOTH sides so the describer cache each section leaves
  // behind is in the same state when the next one runs.
  {
    const adapter = await import('@/lib/chat/message-attachment-adapter');

    const buildAttachment = (entry: { file?: string; raw?: Record<string, unknown> }): unknown => {
      if (entry.raw) return entry.raw;
      const f = fileByKey[entry.file!];
      return {
        id: f.id,
        filepath: `/api/v1/files/${f.id}`,
        filename: f.originalFilename,
        mimeType: f.mimeType,
        size: dataByKey[entry.file!].length,
        data: dataByKey[entry.file!],
      };
    };
    const buildMessages = (
      ms: Array<{ role: string; content: string; attachments?: Array<{ file?: string; raw?: Record<string, unknown> }> }>,
    ): Array<Record<string, unknown>> =>
      ms.map((m) => {
        const out: Record<string, unknown> = { role: m.role, content: m.content };
        if (m.attachments !== undefined) out.attachments = m.attachments.map(buildAttachment);
        return out;
      });
    // Only the fields the port can carry: v4 spreads every extra key through
    // `{...message}` and v5's `StreamMessage` reconstructs its variant, so the
    // comparand is the three fields both sides actually decide about.
    //
    // An EMPTY list projects to `null` alongside an absent key. v4 distinguishes
    // the two (`delete next.attachments` vs a message that never had the key);
    // v5's `StreamMessage::User.attachments` is a `Vec` and structurally cannot,
    // which is a recorded narrowing rather than a divergence: every request
    // builder reads the list with JS truthiness, so `[]` and absent reach the
    // wire identically. Collapsing here keeps the rest of the projection
    // discriminating instead of failing on a difference nothing can observe.
    const project = (ms: Array<Record<string, unknown>>): unknown =>
      ms.map((m) => {
        const att = (m.attachments as unknown[] | undefined) ?? [];
        return { role: m.role, content: m.content, attachments: att.length > 0 ? att : null };
      });

    for (const c of spec.adaptCases) {
      const messages = buildMessages(c.messages);
      const profile = spec.respProfiles[c.profile];
      const result = await adapter.adaptMessagesForProfile(
        messages as never,
        profile as never,
        repos,
        spec.userId,
        { chatId: spec.chatId },
      );
      lines.push(
        JSON.stringify({
          kind: 'case',
          family: 'adapt',
          label: c.label,
          result: {
            // v4's same-array-reference contract, made a comparand. v5 answers
            // `None` for exactly this case.
            same: result === messages,
            messages: project(result as Array<Record<string, unknown>>),
          },
        }),
      );
    }

    for (const c of spec.mimeCases) {
      const messages = buildMessages([{ role: 'user', content: 'x', attachments: c.attachments }]);
      lines.push(
        JSON.stringify({
          kind: 'case',
          family: 'adapt',
          label: c.label,
          result: adapter.collectAttachmentMimeTypes(messages as never),
        }),
      );
    }
  }

  // ---- (C) loadChatFilesForLLM (mount-path branch) --------------------------
  {
    const attachments = await loadChatFilesForLLM([meta.mountLinkId], { provider: 'DEEPSEEK' as never });
    lines.push(JSON.stringify({ kind: 'case', family: 'lcffl', label: 'lcffl_mount', result: attachments }));
  }

  // ---- (D) the describer transport guard (bug 91, a14a1811) -----------------
  // Point the user's Image Description Profile at the OLLAMA describer — a
  // profile whose plugin cannot transport images. `describeImageWithProfile`
  // answers `unsupported` with the guard sentence and NO model call is made
  // (the canned mock throws on any unregistered vision send, so a send here
  // would fail the oracle run — the mock-level "sendMessage never called"
  // assert). Patched LAST so every earlier case sees the original settings;
  // deliberately not restored (nothing reads settings afterwards, and the
  // Rust side mirrors the same order).
  {
    // The uncensored id is cleared too: any primary failure cascades to the
    // uncensored describer, which would swallow the guard sentence with a
    // successful Z.AI description.
    await repos.chatSettings.updateForUser(spec.userId, {
      imageDescriptionProfileId: '30000000-0000-4000-8000-0000000000d3',
      uncensoredImageDescriptionProfileId: null,
    } as never);
    const profile = spec.respProfiles['noImg'];
    const f = fileByKey['descImage'];
    const fileMetadata = {
      id: f.id,
      filepath: `/api/v1/files/${f.id}`,
      filename: f.originalFilename,
      mimeType: f.mimeType,
      size: dataByKey['descImage'].length,
    };
    const fileAttachment = { ...fileMetadata, data: dataByKey['descImage'] };
    const result = await processFileAttachmentFallback(
      fileMetadata,
      fileAttachment as never,
      profile as never,
      repos,
      spec.userId,
    );
    lines.push(
      JSON.stringify({ kind: 'case', family: 'fb', label: 'fb_ollama_describer_guard', result }),
    );
  }

  // ---- (E) the auto-pick describer filter (bug 91, a14a1811) ----------------
  // Clear the configured describer and delete the two transporting profiles,
  // leaving only the OLLAMA vision profile. The auto-pick filter now requires
  // `providerCanTransportImages` alongside the mime support, so the OLLAMA
  // profile is excluded and the no-describer arm answers — where the old
  // filter would have picked it and described a picture it never received
  // (the canned mock has no OLLAMA entry, so that regression throws here).
  {
    await repos.chatSettings.updateForUser(spec.userId, {
      imageDescriptionProfileId: null,
    } as never);
    await repos.connections.delete(spec.descProfileId);
    await repos.connections.delete(spec.uncensoredProfileId);
    const profile = spec.respProfiles['noImg'];
    const f = fileByKey['descImage'];
    const fileMetadata = {
      id: f.id,
      filepath: `/api/v1/files/${f.id}`,
      filename: f.originalFilename,
      mimeType: f.mimeType,
      size: dataByKey['descImage'].length,
    };
    const fileAttachment = { ...fileMetadata, data: dataByKey['descImage'] };
    const result = await processFileAttachmentFallback(
      fileMetadata,
      fileAttachment as never,
      profile as never,
      repos,
      spec.userId,
    );
    lines.push(
      JSON.stringify({ kind: 'case', family: 'fb', label: 'fb_autopick_excludes_non_transporting', result }),
    );
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
