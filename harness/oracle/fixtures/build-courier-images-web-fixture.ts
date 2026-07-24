/**
 * The committed courier + chat-images server-surface fixture builder (P4.6ab
 * lane A deliverable #2) — the shared test-pepper substrate for the whole
 * courier/images-server differential (`courier_images_routes_equivalence`) AND
 * the sibling P4.6ac courier+images-Salon-SPA Playwright e2e (fixture-guarded,
 * activated at unification; the web e2e rewrites the fixture userId to
 * SINGLE_USER_ID — see [[p4.6stu-round-plan]] for the per-spec rewrite recipe).
 *
 * Bakes, via v4's REAL repositories + store-write helpers (all ids/timestamps
 * pinned so both differential sides read identical rows — reads diff with no
 * remap; only the mutating cases mint fresh ids, remapped/blanked by the
 * harness). Modeled on `build-photo-tools-fixture.ts` (character vaults +
 * ingested images) and `build-groups-projects-fixture.ts` (the DDL + real-repo
 * mold):
 *
 *   1. The single user (FIXTURE_USER) + a SECOND user (OTHER_USER, the ownership
 *      arms) + a connection profile + api key.
 *   2. Two characters with provisioned vaults — Lora (llm) and Riya (user
 *      persona) — the photo-album targets + the save-image attribution branches.
 *   3. A "Quilltap Uploads" mount + `userUploadsMountPointId` (so ingest has a
 *      store) and a "Quilltap General" mount + `generalMountPointId` (the
 *      photo-albums General option).
 *   4. A project (Proj) with its official store + ONE additional linked document
 *      store (the photo-albums project + document-store options).
 *   5. CHAT_PENDING — a chat with a PENDING external-turn ASSISTANT placeholder
 *      (`pendingExternalPrompt` + `…Full` + one attachment) whose responder
 *      participant carries a characterId (→ the checkpoint write) but NO
 *      connection profile (participant.connectionProfileId absent AND the
 *      character has no defaultConnectionProfileId) → the resolve route's three
 *      post-turn triggers are skipped on BOTH sides (v4 `if (connectionProfile)`
 *      is false; the trigger machinery is independently tier-3 proven) — plus a
 *      settled USER sibling. `isPaused: true`.
 *   6. CHAT_IMG — a project chat (participants Lora + Riya[user], activeTyping =
 *      Riya) whose ASSISTANT message carries an ingested image attachment: the
 *      save-image / photo-albums / add-tool-result / chat-files subject. Plus
 *      two chat files (one same-name-as-an-upload for the conflict branch, one
 *      GENERATED) linked to it.
 *   7. CHAT_OTHER — owned by OTHER_USER (the ownership arms: a delete/list on a
 *      chat the acting user doesn't own).
 *   8. An image profile (the imageProfileGenerate arm's subject; provider
 *      OPENAI so v4's createImageProvider probe passes).
 *   9. CHAT_CADENCE — the AT-CADENCE courier chat (the fold-episode pin): 21
 *      settled alternating messages (11 USER + 10 ASSISTANT) + a pending
 *      ASSISTANT placeholder whose responder participant carries
 *      `connectionProfileId: CONN` — so the resolve settles interchange 11 and
 *      the three post-turn triggers FIRE (v4 `if (connectionProfile)` is true):
 *      memory extraction enqueues, danger bails (mode defaults OFF), and the
 *      summary check's gate folds turns 1–5 (11 − 0 > FOLD_TRIGGER_DELTA 10),
 *      running the summary fold AND `runFoldEpisodePass`. A `chat_settings`
 *      row supplies `cheapLLMSettings` (PROVIDER_CHEAPEST, no user-defined /
 *      default-cheap profile, fallbackToLocal false — branch-identical to the
 *      v5 route path's fixed selection config); `lastRenameCheckInterchange:
 *      10` keeps the TITLE_UPDATE checkpoint quiet at interchange 11. The
 *      empty `courier-images-llmlogs.db` partition (schema materialized, zero
 *      rows) is emitted alongside so the Rust side can open an llm-logs
 *      partition and diff the fold + episode SUMMARIZATION rows.
 *
 * The minted char-vault ids / project official-store id / the real ingested
 * image fileId + its STORED bytes are echoed to a JSON sidecar
 * (QT_FIXTURE_CI_MAIN + '.meta.json') the oracle case + the Rust harness read.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CI_MAIN=$W/crates/quilltap-web/tests/fixtures/courier-images-main.db \
 *   QT_FIXTURE_CI_MOUNT=$W/crates/quilltap-web/tests/fixtures/courier-images-mount.db \
 *   QT_FIXTURE_CI_LLMLOGS=$W/crates/quilltap-web/tests/fixtures/courier-images-llmlogs.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-courier-images-web-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  otherUserId: string;
  seedTimestamp: string;
}

// ── Pinned entity ids (shared verbatim with the Rust differential) ──────────
const CONN = 'c0000001-0000-4000-8000-000000000001';
const APIKEY = 'a0000001-0000-4000-8000-000000000001';

const LORA = 'a1000000-0000-4000-8000-000000000001'; // llm character (photo-album target)
const RIYA = 'a1000000-0000-4000-8000-000000000002'; // user persona (attribution + default album)

const PROJ = 'a3000000-0000-4000-8000-000000000001';
const PROJ_EXTRA_MP = 'b0000000-0000-4000-8000-000000000001'; // an additional linked documents store
const GENERAL_MP = 'b0000000-0000-4000-8000-0000000000a0';
const UPLOADS_MP = 'b0000000-0000-4000-8000-0000000000ff';

const CHAT_PENDING = 'c1000000-0000-4000-8000-000000000001';
const CHAT_IMG = 'c1000000-0000-4000-8000-000000000002';
const CHAT_OTHER = 'c1000000-0000-4000-8000-000000000003';

// CHAT_PENDING participants + messages.
const PP_LORA = 'e1000000-0000-4000-8000-000000000001';
const PP_USER = 'e1000000-0000-4000-8000-000000000002';
const MSG_PENDING = 'd1000000-0000-4000-8000-000000000001';
const MSG_SETTLED = 'd1000000-0000-4000-8000-000000000002';
const ATT_FILE = 'f2000000-0000-4000-8000-000000000001'; // the pending attachment file

// CHAT_IMG participants + messages.
const PI_LORA = 'e2000000-0000-4000-8000-000000000001';
const PI_RIYA = 'e2000000-0000-4000-8000-000000000002';
const MSG_IMG = 'd2000000-0000-4000-8000-000000000001';

// Chat files linked to CHAT_IMG.
const CF_NOTES = 'f1000000-0000-4000-8000-000000000001'; // 'notes.txt' (list)
const CF_DUP = 'f1000000-0000-4000-8000-000000000002'; // 'duplicate.png' (upload filename conflict)
const CF_GEN = 'f1000000-0000-4000-8000-000000000003'; // a GENERATED image (list → generatedImage)
const CF_OTHER = 'f1000000-0000-4000-8000-000000000004'; // linked to CHAT_OTHER (ownership arm)

const IP = 'a6000000-0000-4000-8000-000000000001'; // image profile (generate arm)

// CHAT_CADENCE — the at-cadence courier chat (the fold-episode pin).
const CHAT_CADENCE = 'c1000000-0000-4000-8000-000000000004';
const PC_LORA = 'e4000000-0000-4000-8000-000000000001';
const PC_USER = 'e4000000-0000-4000-8000-000000000002';
const MSG_CAD_PENDING = 'd4000000-0000-4000-8000-000000000022';
const CHAT_SETTINGS_ID = 'a9000000-0000-4000-8000-000000000001';

// A tiny valid PNG (the ingest input for the save-image subject image).
const PNG_BYTES = [
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
  0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
  0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x62, 0x00, 0x01, 0x00, 0x00,
  0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
  0x42, 0x60, 0x82,
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'courier-images-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_CI_MAIN;
  const mountOut = process.env.QT_FIXTURE_CI_MOUNT;
  const llmLogsOut = process.env.QT_FIXTURE_CI_LLMLOGS;
  if (!mainOut || !mountOut || !llmLogsOut) {
    throw new Error(
      'QT_FIXTURE_CI_MAIN, QT_FIXTURE_CI_MOUNT and QT_FIXTURE_CI_LLMLOGS must point at the .db files',
    );
  }
  for (const out of [mainOut, mountOut, llmLogsOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ci-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.SQLITE_LLM_LOGS_PATH = llmLogsOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, getCollection, closeDatabase, rawQuery } =
    await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema, ChatMetadataSchema, FileEntrySchema } = await import(
    '@/lib/schemas/types'
  );
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { ApiKeySchema, ImageProfileSchema } = await import('@/lib/schemas/profile.types');
  const { ProjectSchema } = await import('@/lib/schemas/project.types');
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
  const { ingestImageBuffer, readImageBuffer } = await import('@/lib/images-v2');
  const { sha256OfBuffer } = await import('@/lib/utils/sha256');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('files', FileEntrySchema);
  await ensureCollection('projects', ProjectSchema);
  await ensureCollection('api_keys', ApiKeySchema);
  await ensureCollection('image_profiles', ImageProfileSchema);

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
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger the CREATE via a read.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. Users + api key + connection profile.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'stranger', email: null, name: 'Stranger' } as never,
    { id: spec.otherUserId, createdAt: TS, updatedAt: TS } as never,
  );
  const apiKeyCol = await getCollection('api_keys');
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: APIKEY,
      userId: spec.userId,
      label: 'mock-key',
      provider: 'OPENAI',
      key_value: 'sk-synthetic-mock-key',
      isActive: true,
      lastUsed: null,
      createdAt: TS,
      updatedAt: TS,
    }) as never,
  );
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'Local Mock',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
      baseUrl: 'http://127.0.0.1:1/v1',
      apiKeyId: APIKEY,
    } as never,
    { id: CONN, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Two characters WITH vaults (no defaultConnectionProfileId → the courier
  //    resolve's trigger short-circuit). Lora is llm, Riya is the user persona.
  await repos.characters.create(
    { name: 'Lora', userId: spec.userId, controlledBy: 'llm' } as never,
    { id: LORA, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.create(
    { name: 'Riya', userId: spec.userId, controlledBy: 'user' } as never,
    { id: RIYA, createdAt: TS, updatedAt: TS } as never,
  );
  const loraRaw = await repos.characters.findByIdRaw(LORA);
  const riyaRaw = await repos.characters.findByIdRaw(RIYA);
  const loraVault = loraRaw?.characterDocumentMountPointId as string | null;
  const riyaVault = riyaRaw?.characterDocumentMountPointId as string | null;
  if (!loraVault || !riyaVault) throw new Error('character vault(s) not minted');

  // 3. Uploads + General mounts + the instance-settings pointers.
  for (const [id, name] of [
    [UPLOADS_MP, 'Quilltap Uploads'],
    [GENERAL_MP, 'Quilltap General'],
    [PROJ_EXTRA_MP, 'Proj Extra Store'],
  ] as Array<[string, string]>) {
    await repos.docMountPoints.create(
      {
        name,
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
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'userUploadsMountPointId',
    UPLOADS_MP,
  ]);
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    GENERAL_MP,
  ]);

  // 4. Project + its extra linked store.
  const proj = await repos.projects.create(
    {
      name: 'Proj',
      description: 'The chat-images project.',
      characterRoster: [LORA, RIYA],
      state: {},
    } as never,
    { id: PROJ, createdAt: TS, updatedAt: TS } as never,
  );
  if (proj.id !== PROJ) throw new Error(`Proj id drift: ${proj.id}`);
  const projMp = proj.officialMountPointId as string;
  await repos.projectDocMountLinks.link(PROJ, PROJ_EXTRA_MP);

  // 5. Ingest the save-image subject image (ingest mints its OWN fileId + may
  //    transcode to WebP; record the real id + STORED bytes for the canned store).
  const ingested = await ingestImageBuffer({
    buffer: Buffer.from(PNG_BYTES),
    originalFilename: 'scene.png',
    mimeType: 'image/png',
    userId: spec.userId,
  });
  const storedBytes = await readImageBuffer(ingested.id);
  const img1FileId = ingested.id;
  const img1Sha = sha256OfBuffer(storedBytes);

  // 6. CHAT_PENDING — the courier pending placeholder + a settled sibling.
  const attFile = await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [CHAT_PENDING],
      tags: [],
      sha256: 'aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111',
      originalFilename: 'reference.png',
      mimeType: 'image/png',
      size: 256,
      source: 'UPLOADED',
      category: 'IMAGE',
      storageKey: `${spec.userId}/reference.png`,
      width: 16,
      height: 16,
    } as never,
    { id: ATT_FILE, createdAt: TS, updatedAt: TS } as never,
  );
  const mkParticipant = (
    id: string,
    characterId: string,
    controlledBy: string,
    connectionProfileId: string | null,
  ) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy,
    status: 'active',
    connectionProfileId,
    createdAt: TS,
    updatedAt: TS,
  });
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Courier Pending',
      chatType: 'salon',
      contextSummary: null,
      isPaused: true,
      participants: [
        mkParticipant(PP_LORA, LORA, 'llm', null),
        mkParticipant(PP_USER, RIYA, 'user', null),
      ],
      tags: [],
    } as never,
    { id: CHAT_PENDING, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chats.addMessages(CHAT_PENDING, [
    {
      id: MSG_SETTLED,
      type: 'message',
      role: 'USER',
      content: 'Please continue.',
      participantId: PP_USER,
      createdAt: '2026-04-02T00:00:00.000Z',
      attachments: [],
    },
    {
      id: MSG_PENDING,
      type: 'message',
      role: 'ASSISTANT',
      content: '',
      participantId: PP_LORA,
      createdAt: '2026-04-03T00:00:00.000Z',
      attachments: [],
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
      pendingExternalPrompt: 'Copy this to your LLM, then paste Lora’s reply.',
      pendingExternalPromptFull: 'Full context bundle for Lora’s turn.',
      pendingExternalAttachments: [
        {
          fileId: ATT_FILE,
          filename: attFile.originalFilename,
          mimeType: 'image/png',
          sizeBytes: 256,
          downloadUrl: `/api/v1/files/${ATT_FILE}`,
        },
      ],
    },
  ] as never);
  // addMessages bumps chat metadata; restore the seed isPaused + updatedAt.
  await repos.chats.update(CHAT_PENDING, { isPaused: true, updatedAt: TS } as never);

  // 7. CHAT_IMG — a project chat carrying the ingested image attachment.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Chat Images',
      chatType: 'salon',
      contextSummary: null,
      projectId: PROJ,
      activeTypingParticipantId: PI_RIYA,
      participants: [
        mkParticipant(PI_LORA, LORA, 'llm', null),
        mkParticipant(PI_RIYA, RIYA, 'user', null),
      ],
      tags: [],
    } as never,
    { id: CHAT_IMG, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chats.addMessages(CHAT_IMG, [
    {
      id: MSG_IMG,
      type: 'message',
      role: 'ASSISTANT',
      content: 'Here is the scene.',
      participantId: PI_LORA,
      createdAt: '2026-04-04T00:00:00.000Z',
      attachments: [img1FileId],
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
    },
  ] as never);
  await repos.chats.update(CHAT_IMG, { updatedAt: TS } as never);
  // Link the ingested image to CHAT_IMG so it also appears in the chat-files list.
  await repos.files.addLink(img1FileId, CHAT_IMG);

  // Chat files: notes.txt (list), duplicate.png (upload conflict), a GENERATED image.
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [CHAT_IMG],
      tags: [],
      sha256: 'bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222',
      originalFilename: 'notes.txt',
      mimeType: 'text/plain',
      size: 42,
      source: 'UPLOADED',
      category: 'ATTACHMENT',
      storageKey: `${spec.userId}/notes.txt`,
      projectId: PROJ,
    } as never,
    { id: CF_NOTES, createdAt: '2026-04-05T00:00:00.000Z', updatedAt: TS } as never,
  );
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [CHAT_IMG],
      tags: [],
      sha256: 'cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333',
      originalFilename: 'duplicate.png',
      mimeType: 'image/png',
      size: 64,
      source: 'UPLOADED',
      category: 'IMAGE',
      storageKey: `${spec.userId}/duplicate.png`,
      projectId: PROJ,
      width: 8,
      height: 8,
    } as never,
    { id: CF_DUP, createdAt: '2026-04-06T00:00:00.000Z', updatedAt: TS } as never,
  );
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [CHAT_IMG],
      tags: [],
      sha256: 'dddd4444dddd4444dddd4444dddd4444dddd4444dddd4444dddd4444dddd4444',
      originalFilename: 'generated.webp',
      mimeType: 'image/webp',
      size: 128,
      source: 'GENERATED',
      category: 'IMAGE',
      storageKey: `${spec.userId}/generated.webp`,
      generationPrompt: 'a lantern-lit alley',
      generationModel: 'mock-image-model',
      width: 32,
      height: 32,
    } as never,
    { id: CF_GEN, createdAt: '2026-04-07T00:00:00.000Z', updatedAt: TS } as never,
  );

  // 8. CHAT_OTHER — owned by OTHER_USER (ownership arms).
  await repos.chats.create(
    {
      userId: spec.otherUserId,
      title: 'Someone Else',
      chatType: 'salon',
      contextSummary: null,
      participants: [mkParticipant('e3000000-0000-4000-8000-000000000001', LORA, 'llm', null)],
      tags: [],
    } as never,
    { id: CHAT_OTHER, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.files.create(
    {
      userId: spec.otherUserId,
      linkedTo: [CHAT_OTHER],
      tags: [],
      sha256: 'eeee5555eeee5555eeee5555eeee5555eeee5555eeee5555eeee5555eeee5555',
      originalFilename: 'secret.txt',
      mimeType: 'text/plain',
      size: 10,
      source: 'UPLOADED',
      category: 'ATTACHMENT',
      storageKey: `${spec.otherUserId}/secret.txt`,
    } as never,
    { id: CF_OTHER, createdAt: TS, updatedAt: TS } as never,
  );

  // 9. Image profile (the imageProfileGenerate subject).
  await repos.imageProfiles.create(
    {
      userId: spec.userId,
      name: 'Studio',
      provider: 'OPENAI',
      apiKeyId: APIKEY,
      baseUrl: null,
      modelName: 'gpt-image-1',
      parameters: {},
      isDefault: true,
      isDangerousCompatible: false,
      tags: [],
    } as never,
    { id: IP, createdAt: TS, updatedAt: TS } as never,
  );

  // 9. CHAT_CADENCE — the at-cadence courier chat (the fold-episode pin). The
  //    responder participant carries CONN, so the resolve fires the three
  //    post-turn triggers; 11 USER + 10 ASSISTANT settled messages put the
  //    post-resolve interchange at 11 (> FOLD_TRIGGER_DELTA 10 → 'fold').
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Courier Cadence',
      chatType: 'salon',
      contextSummary: null,
      isPaused: true,
      participants: [
        mkParticipant(PC_LORA, LORA, 'llm', CONN),
        mkParticipant(PC_USER, RIYA, 'user', null),
      ],
      tags: [],
    } as never,
    { id: CHAT_CADENCE, createdAt: TS, updatedAt: TS } as never,
  );
  const cadenceMessages: unknown[] = [];
  for (let k = 1; k <= 21; k++) {
    const isUser = k % 2 === 1;
    const turn = isUser ? (k + 1) / 2 : k / 2;
    cadenceMessages.push({
      id: `d4000000-0000-4000-8000-0000000000${String(k).padStart(2, '0')}`,
      type: 'message',
      role: isUser ? 'USER' : 'ASSISTANT',
      content: isUser
        ? `Turn ${turn}: Riya asks what the harbor lanterns are signalling tonight.`
        : `Turn ${turn}: Lora reads the lantern code aloud and notes it in her ledger.`,
      participantId: isUser ? PC_USER : PC_LORA,
      createdAt: new Date(Date.UTC(2026, 2, 20, 0, k * 10, 0)).toISOString(),
      attachments: [],
      ...(isUser ? {} : { provider: 'OPENAI_COMPATIBLE', modelName: 'mock-model' }),
    });
  }
  cadenceMessages.push({
    id: MSG_CAD_PENDING,
    type: 'message',
    role: 'ASSISTANT',
    content: '',
    participantId: PC_LORA,
    createdAt: new Date(Date.UTC(2026, 2, 20, 3, 40, 0)).toISOString(),
    attachments: [],
    provider: 'OPENAI_COMPATIBLE',
    modelName: 'mock-model',
    pendingExternalPrompt: 'Copy this to your LLM, then paste Lora’s reply.',
    pendingExternalPromptFull: 'Full context bundle for Lora’s eleventh turn.',
  });
  await repos.chats.addMessages(CHAT_CADENCE, cadenceMessages as never);
  // addMessages bumps chat metadata; restore the seed flags + park the title
  // checkpoint cursor at 10 so interchange 11 fires ONLY the summary gate.
  await repos.chats.update(CHAT_CADENCE, {
    isPaused: true,
    lastRenameCheckInterchange: 10,
    updatedAt: TS,
  } as never);

  // The user's chat_settings row: cheapLLMSettings gates the memory-extraction
  // + summary-check triggers ON. PROVIDER_CHEAPEST with no user-defined /
  // default-cheap profile and fallbackToLocal false is branch-identical to the
  // fixed selection config v5's route path passes ("AUTO" matches neither the
  // USER_DEFINED nor the LOCAL_FIRST branch). dangerousContentSettings is left
  // absent — both sides default mode OFF, so the danger trigger bails.
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      tagStyles: {},
      cheapLLMSettings: {
        strategy: 'PROVIDER_CHEAPEST',
        fallbackToLocal: false,
        embeddingProvider: 'OPENAI',
      },
    } as never,
    { id: CHAT_SETTINGS_ID, createdAt: TS, updatedAt: TS } as never,
  );

  // Materialize the EMPTY llm_logs partition schema (create a sentinel row via
  // v4's real repo — which lazily runs the DDL — then delete it), so the Rust
  // side can open the committed llm-logs partition and diff the at-cadence
  // resolve's fold + episode SUMMARIZATION rows against a zero-row baseline.
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

  // Materialize the empty background-jobs table the add-tool-result / chat-files
  // paths never sweep but the harness's fresh copies must read identically.
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');

  closeMountIndexSQLiteClient();
  await closeDatabase();

  const meta = {
    loraVault,
    riyaVault,
    projMp,
    img1FileId,
    img1Bytes: Array.from(storedBytes),
    img1Sha,
    img1MimeType: ingested.mimeType,
  };
  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta));
  process.stderr.write(
    `built courier-images fixture: main=${mainOut} mount=${mountOut} ` +
      `loraVault=${loraVault} riyaVault=${riyaVault} projMp=${projMp} img1=${img1FileId}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`courier-images fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
