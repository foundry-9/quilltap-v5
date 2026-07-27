/**
 * P4.9E4A fixture builder — the committed test-pepper substrate for
 * `attach_mount_file_equivalence` (the attach-mount-file route family, tier 2 +
 * tier 3). Everything is baked through v4's REAL repositories and the REAL
 * `linkBlobContent` store write, so the DDL, the content-addressed dedup, and
 * the blob/link rows are production-identical on both differential sides.
 *
 * ── THREE FILES, ONE FAMILY ──────────────────────────────────────────────────
 *   - `attach-file-main.db`     — users, connection profiles, chat_settings, the
 *                                 characters (whose creates mint vaults), and
 *                                 the chat the Librarian announcements land in.
 *   - `attach-file-mount.db`    — the mount-index sibling: the Reading Room
 *                                 store, its links + blobs, and the blob-less
 *                                 ghost link.
 *   - `attach-file-llmlogs.db`  — an EMPTY llm-logs partition (schema
 *                                 materialized, zero rows) so the vision arms'
 *                                 `IMAGE_DESCRIPTION` rows diff against a
 *                                 zero-row baseline.
 *
 * ── WHAT THE CORPUS STAGES (one file per rung of the description ladder) ─────
 *   library/described.png    — image WITH a cached blob description → the ladder
 *                              short-circuits at the cache; NO vision call.
 *   photos/kept-lantern.webp — under `photos/`, so `extractedText` carries the
 *                              kept-image markdown → the preferred rung; NO
 *                              vision call, even though the blob has no
 *                              description.
 *   library/undescribed.png  — image, no description, not under photos/ → the
 *                              vision rung: one mocked call, a cache write, an
 *                              IMAGE_DESCRIPTION llm_logs row.
 *   library/refuses.png      — same, but the PRIMARY profile answers with a
 *                              refusal ("I cannot…" trips v4's suspicious-content
 *                              heuristic) → the uncensored retry fires and
 *                              succeeds. TWO llm_logs rows.
 *   library/reasoning.png    — attached as the REASONING user, whose configured
 *                              profile's model matches v4's `/o1|o3|gpt-5|
 *                              reasoning/` sniff → the max_tokens bump to 4000,
 *                              visible in the logged request.
 *   library/ledger.txt       — text/plain → `ensureImageDescription` returns ''
 *                              before touching a profile.
 *   library/ghost.md         — a link + `doc_mount_files` row with NO blob row
 *                              (raw INSERTs; `linkBlobContent` always writes
 *                              one) → the `notFound('Mount-point file blob')`
 *                              arm, unreachable otherwise.
 *
 * ── THE PROFILE CORPUS ───────────────────────────────────────────────────────
 * The main user carries THREE vision-capable profiles: the CONFIGURED one
 * (`mock-vision`, NOT cheap), a CHEAP one (`mock-cheap-vision`), and the
 * uncensored fallback (`mock-uncensored`). The cheap profile exists so the
 * differential proves the CONFIGURED profile wins over v4's cheap-preferring
 * auto-pick — a fixture with only one vision profile could not tell the
 * difference. The reasoning user carries exactly one.
 *
 * Regenerate (Node 24, from the v4 checkout) — writes the committed .db files
 * in place:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   TZ=UTC \
 *   QT_FIXTURE_ATTACH_MAIN=$W/crates/quilltap-web/tests/fixtures/attach-file-main.db \
 *   QT_FIXTURE_ATTACH_MOUNT=$W/crates/quilltap-web/tests/fixtures/attach-file-mount.db \
 *   QT_FIXTURE_ATTACH_LLMLOGS=$W/crates/quilltap-web/tests/fixtures/attach-file-llmlogs.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-attach-file-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  reasoningUserId: string;
  seedTimestamp: string;
}

// ── Pinned entity ids (shared verbatim with the oracle case + the Rust test) ──
const APIKEY = 'a0000001-0000-4000-8000-000000000001';
const CONN_VISION = 'c0000001-0000-4000-8000-000000000001'; // the configured describer
const CONN_UNCENSORED = 'c0000001-0000-4000-8000-000000000002'; // the retry target
const CONN_CHEAP = 'c0000001-0000-4000-8000-000000000003'; // cheap + vision, never picked
const CONN_REASONING = 'c0000001-0000-4000-8000-000000000004'; // the reasoning user's

const CS_MAIN = 'c8000000-0000-4000-8000-000000000001';
const CS_REASONING = 'c8000000-0000-4000-8000-000000000002';

const ARIA = 'a1000000-0000-4000-8000-000000000001'; // user-controlled participant
const BRAM = 'a1000000-0000-4000-8000-000000000002'; // llm participant

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const P_ARIA = 'e1000000-0000-4000-8000-000000000001';
const P_BRAM = 'e1000000-0000-4000-8000-000000000002';
const MSG_SEED = 'd1000000-0000-4000-8000-000000000001';

const MP_ROOM = 'b0000000-0000-4000-8000-000000000001';

/** The blob-less link (raw INSERTs — `linkBlobContent` always writes a blob). */
const GHOST_FILE = 'b9000000-0000-4000-8000-000000000001';
const GHOST_LINK = 'b9000000-0000-4000-8000-000000000002';

const PNG_HEAD = [
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
  0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
  0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x62, 0x00, 0x01, 0x00, 0x00,
  0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
  0x42, 0x60, 0x82,
];
/** Distinct bytes per file → distinct sha256, so nothing dedups by accident. */
function imageBytes(tag: string): Buffer {
  return Buffer.concat([Buffer.from(PNG_HEAD), Buffer.from(`\n${tag}\n`, 'utf-8')]);
}

interface FileSpec {
  path: string;
  folder: string;
  originalFileName: string;
  mimeType: string;
  /** The blob's cached description (`''` = undescribed). */
  description: string;
  /** `true` → `extractedText` is kept-image markdown (the photos rung). */
  keptImage?: boolean;
  /** Plain-text payload instead of PNG bytes. */
  textBody?: string;
}

const FILES: FileSpec[] = [
  {
    path: 'library/described.png',
    folder: 'library',
    // Deliberately UNLIKE the basename: the display title is
    // `originalFileName || fileName`, and a fixture where the two agree could
    // not tell which side of that `||` the port reads.
    originalFileName: 'orrery-plate.png',
    mimeType: 'image/png',
    description: 'A brass orrery, already catalogued by the Librarian on a previous evening.',
  },
  {
    path: 'library/undescribed.png',
    folder: 'library',
    originalFileName: 'undescribed.png',
    mimeType: 'image/png',
    description: '',
  },
  {
    path: 'library/refuses.png',
    folder: 'library',
    originalFileName: 'refuses.png',
    mimeType: 'image/png',
    description: '',
  },
  {
    path: 'library/reasoning.png',
    folder: 'library',
    originalFileName: 'reasoning.png',
    mimeType: 'image/png',
    description: '',
  },
  {
    path: 'library/ledger.txt',
    folder: 'library',
    // EMPTY on purpose — the other half of the `||`: the display title must
    // fall back to the link's fileName.
    originalFileName: '',
    mimeType: 'text/plain',
    description: '',
    textBody: 'Quay ledger, week 14: three crossings, one barrel short.\n',
  },
  {
    path: 'photos/kept-lantern.webp',
    folder: 'photos',
    originalFileName: 'kept-lantern.webp',
    mimeType: 'image/webp',
    description: '',
    keptImage: true,
  },
];

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`attach-file fixture must be built under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'attach-file.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_ATTACH_MAIN;
  const mountOut = process.env.QT_FIXTURE_ATTACH_MOUNT;
  const llmLogsOut = process.env.QT_FIXTURE_ATTACH_LLMLOGS;
  if (!mainOut || !mountOut || !llmLogsOut) {
    throw new Error(
      'QT_FIXTURE_ATTACH_{MAIN,MOUNT,LLMLOGS} must point at the .db files to write',
    );
  }
  for (const out of [mainOut, mountOut, llmLogsOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-attach-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.SQLITE_LLM_LOGS_PATH = llmLogsOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, getCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema, ChatMetadataSchema, FileEntrySchema } = await import(
    '@/lib/schemas/types'
  );
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const { buildKeptImageMarkdown, sha256OfString, basenameOfRelativePath } = await import(
    '@/lib/photos/keep-image-markdown'
  );
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
  const { sha256OfBuffer } = await import('@/lib/utils/sha256');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('files', FileEntrySchema);
  await ensureCollection('api_keys', ApiKeySchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  const repos = getRepositories();
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger CREATE via a read.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. The two users.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'reasoner', email: null, name: 'The Reasoner' } as never,
    { id: spec.reasoningUserId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. One api key + four connection profiles. Every describer is
  //    `supportsImageUpload: true` (v4's whole vision test); the CHEAP one exists
  //    so the configured-beats-auto-pick claim is measurable.
  const apiKeyCol = await getCollection('api_keys');
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: APIKEY,
      userId: spec.userId,
      label: 'mock-key',
      provider: 'OPENAI_COMPATIBLE',
      key_value: 'sk-synthetic-mock-key',
      isActive: true,
      lastUsed: null,
      createdAt: TS,
      updatedAt: TS,
    }) as never,
  );
  const mkConn = async (
    id: string,
    userId: string,
    name: string,
    modelName: string,
    extra: object = {},
  ) =>
    repos.connections.create(
      {
        userId,
        name,
        provider: 'OPENAI_COMPATIBLE',
        modelName,
        baseUrl: 'http://127.0.0.1:1/v1',
        supportsImageUpload: true,
        isCheap: false,
        ...extra,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  await mkConn(CONN_VISION, spec.userId, 'The Describer', 'mock-vision', { apiKeyId: APIKEY });
  await mkConn(CONN_UNCENSORED, spec.userId, 'The Frank Describer', 'mock-uncensored');
  await mkConn(CONN_CHEAP, spec.userId, 'The Thrifty Eye', 'mock-cheap-vision', { isCheap: true });
  await mkConn(CONN_REASONING, spec.reasoningUserId, 'The Deliberator', 'gpt-5-vision-mock');

  // 3. chat_settings — the describer wiring per user.
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      imageDescriptionProfileId: CONN_VISION,
      uncensoredImageDescriptionProfileId: CONN_UNCENSORED,
    } as never,
    { id: CS_MAIN, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chatSettings.create(
    { userId: spec.reasoningUserId, imageDescriptionProfileId: CONN_REASONING } as never,
    { id: CS_REASONING, createdAt: TS, updatedAt: TS } as never,
  );

  // 4. Two characters (create owns vault provisioning) + the chat.
  const mkChar = async (id: string, name: string, controlledBy: string) => {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const raw = await repos.characters.findByIdRaw(id);
    const vault = raw?.characterDocumentMountPointId as string | null;
    if (!vault) throw new Error(`vault not minted for ${name}`);
    return vault;
  };
  const ariaVault = await mkChar(ARIA, 'Aria', 'user');
  const bramVault = await mkChar(BRAM, 'Bramwell', 'llm');

  const mkParticipant = (id: string, characterId: string, controlledBy: string) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy,
    status: 'active',
    connectionProfileId: null,
    createdAt: TS,
    updatedAt: TS,
  });
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Reading Room',
      chatType: 'salon',
      contextSummary: null,
      participants: [
        mkParticipant(P_ARIA, ARIA, 'user'),
        mkParticipant(P_BRAM, BRAM, 'llm'),
      ],
      tags: [],
    } as never,
    { id: CHAT, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chats.addMessages(CHAT, [
    {
      id: MSG_SEED,
      type: 'message',
      role: 'USER',
      content: 'Fetch me something from the shelves, would you?',
      participantId: P_ARIA,
      createdAt: '2026-06-02T00:00:00.000Z',
      attachments: [],
    },
  ] as never);
  await repos.chats.update(CHAT, { updatedAt: TS } as never);

  // 5. The Reading Room store + its files.
  await repos.docMountPoints.create(
    {
      name: 'The Reading Room',
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
      lastScannedAt: null,
      scanStatus: 'idle',
      lastScanError: null,
      conversionStatus: 'idle',
      conversionError: null,
      fileCount: 0,
      chunkCount: 0,
      totalSizeBytes: 0,
    } as never,
    { id: MP_ROOM, createdAt: TS, updatedAt: TS } as never,
  );

  const linkIds: Record<string, string> = {};
  const blobIds: Record<string, string> = {};
  for (const f of FILES) {
    const bytes = f.textBody
      ? Buffer.from(f.textBody, 'utf-8')
      : imageBytes(basenameOfRelativePath(f.path));
    const extractedText = f.keptImage
      ? buildKeptImageMarkdown({
          generationPrompt:
            'A storm lantern hung from a quay bollard, its glass fogged by sea air.',
          generationRevisedPrompt: null,
          generationModel: 'fixture-model',
          sceneState: null,
          linkedByName: 'Aria',
          linkedById: ARIA,
          linkedByRole: 'character',
          tags: ['lantern', 'quay'],
          caption: 'The lantern Aria keeps by the door',
          keptAt: '2026-06-01T12:00:00.000Z',
        })
      : (f.textBody ?? '');
    const folderId = await ensureFolderPath(MP_ROOM, f.folder);
    const { link } = await repos.docMountFileLinks.linkBlobContent({
      mountPointId: MP_ROOM,
      relativePath: f.path,
      fileName: basenameOfRelativePath(f.path),
      folderId,
      originalFileName: f.originalFileName,
      originalMimeType: f.mimeType,
      storedMimeType: f.mimeType,
      sha256: sha256OfBuffer(bytes),
      data: bytes,
      description: f.description,
      extractedText,
      extractedTextSha256: extractedText ? sha256OfString(extractedText) : null,
      extractionStatus: extractedText ? 'converted' : 'none',
    } as never);
    const linkId = (link as { id: string }).id;
    linkIds[f.path] = linkId;
    const blob = await repos.docMountBlobs.findByMountPointAndPath(MP_ROOM, f.path);
    if (!blob) throw new Error(`blob not visible for ${f.path}`);
    blobIds[f.path] = (blob as { id: string }).id;
  }

  // 5b. The blob-less ghost: a real `doc_mount_files` row + link with NO
  //     `doc_mount_blobs` row, so the file resolves and the blob lookup 404s.
  //     `linkBlobContent` always writes a blob, so this pair is raw.
  midb
    .prepare(
      'INSERT INTO "doc_mount_files" ("id","sha256","fileSizeBytes","fileType","source",' +
        '"createdAt","updatedAt") VALUES (?,?,?,?,?,?,?)',
    )
    .run(
      GHOST_FILE,
      '0'.repeat(64),
      12,
      'md',
      'database',
      TS,
      TS,
    );
  const ghostFolder = await ensureFolderPath(MP_ROOM, 'library');
  midb
    .prepare(
      'INSERT INTO "doc_mount_file_links" ("id","fileId","mountPointId","relativePath","fileName",' +
        '"folderId","originalFileName","originalMimeType","description","extractedText",' +
        '"extractionStatus","conversionStatus","chunkCount","lastModified","createdAt","updatedAt") ' +
        'VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)',
    )
    .run(
      GHOST_LINK,
      GHOST_FILE,
      MP_ROOM,
      'library/ghost.md',
      'ghost.md',
      ghostFolder,
      'ghost.md',
      'text/markdown',
      '',
      null,
      'none',
      'none',
      0,
      TS,
      TS,
      TS,
    );

  // 6. Pin the live-clock stamps `linkBlobContent` writes, so a rebuild is
  //    byte-reproducible everywhere except the minted ids (echoed to the sidecar).
  midb.prepare('UPDATE "doc_mount_file_links" SET "createdAt" = ?, "updatedAt" = ?').run(TS, TS);
  midb.prepare('UPDATE "doc_mount_files" SET "createdAt" = ?, "updatedAt" = ?').run(TS, TS);
  midb.prepare('UPDATE "doc_mount_blobs" SET "createdAt" = ?, "updatedAt" = ?').run(TS, TS);

  // 7. Materialize the EMPTY llm_logs partition (v4's repo runs the DDL lazily:
  //    write a sentinel through it, then delete the row).
  const SENTINEL_LL = 'ff000000-0000-4000-8000-0000000000ff';
  const sentinelCreated = await repos.llmLogs.create(
    {
      userId: spec.userId,
      type: 'CHAT_MESSAGE',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
      request: { messages: [], messageCount: 0, temperature: null, maxTokens: null },
      response: { content: '', contentLength: 0, error: null, finishReason: 'stop' },
    } as never,
    { id: SENTINEL_LL, createdAt: TS } as never,
  );
  if (!sentinelCreated) throw new Error('llm_logs sentinel row failed');
  const { getRawLLMLogsDatabase, closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const llmLogsDb = getRawLLMLogsDatabase() as unknown as {
    prepare: (s: string) => { run: (...a: unknown[]) => unknown };
  } | null;
  if (!llmLogsDb) throw new Error('llm-logs raw handle unavailable');
  llmLogsDb.prepare('DELETE FROM llm_logs WHERE id = ?').run(SENTINEL_LL);
  closeLLMLogsSQLiteClient();

  // Materialize the empty background-jobs table (the salon fixture's lesson —
  // the Rust side has no lazy CREATE TABLE).
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');

  closeMountIndexSQLiteClient();
  await closeDatabase();

  const meta = { ariaVault, bramVault, mountPointId: MP_ROOM, linkIds, blobIds, ghostLink: GHOST_LINK };
  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta));

  for (const out of [mainOut, mountOut, llmLogsOut]) {
    const j = out + '-journal';
    if (existsSync(j)) rmSync(j);
  }

  process.stderr.write(
    `built attach-file fixture: ${mainOut} (+${mountOut}, +${llmLogsOut}) — ` +
      `${FILES.length} staged files + 1 blob-less ghost; vaults ${ariaVault} / ${bramVault}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`attach-file fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
