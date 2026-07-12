/**
 * The committed Groups + Projects server-surface fixture builder (P4.6k
 * deliverable #3, extended in P4.6n) — the shared test-pepper substrate for the
 * whole groups-projects-server differential set (`groups_routes_equivalence` /
 * `projects_routes_equivalence` / `scenarios_routes_equivalence`) AND the sibling
 * P4.6l/P4.6o Groups/Projects/Scenarios-SPA Playwright e2e.
 *
 * P4.6p additions (the listing-surfaces server round — all ADDITIVE and
 * invisible to the groups/projects/scenarios reads, which enumerate none of
 * these tables and reference neither DIANA nor MP_INDEXED):
 *   - the two built-in roleplay templates (via v4's REAL seedBuiltInTemplates)
 *     + two user templates (one carrying all three delimiter kinds + addOns;
 *     one with delimiters and EMPTY renderingPatterns for the read-time-regen),
 *   - four tags + a tagged character DIANA (the image sortByCharacter case),
 *   - three image profiles (one default + apiKey + tags; one tagged to match
 *     DIANA; one plain),
 *   - a dedicated "Indexed Store" mount carrying ONE embedded chunk (a non-null,
 *     non-empty Float32 embedding) for the mount-points embedded-count paths.
 *
 * P4.6n additions:
 *   - Groups Beacon (member Aria, one scenario; sorts before Gamma) + Zephyr
 *     (member Aria, ZERO scenarios) — the participant-union's sort + zero-scenario
 *     skip. Gamma/Delta are untouched (their existing route cases stay valid).
 *   - The singleton "Quilltap General" mount + `instance_settings.generalMountPointId`
 *     + two general scenarios that BOTH mark isDefault (the default-conflict
 *     warning: alphabetically-first 'aurora' wins, 'dusk' is demoted-in-response).
 *
 * Bakes, via v4's REAL repositories + store-write helpers (all ids/timestamps
 * pinned so both differential sides read identical rows — reads diff with no
 * remap; only the CREATE mutations mint fresh ids, remapped by the harness):
 *   - one user (the FIXTURE_USER; the web e2e rewrites it to SINGLE_USER_ID),
 *   - three characters (Aria / Bram / Cleo) for group members + project roster
 *     + chat participants,
 *   - one legacy IMAGE `files` row used as a project story-background,
 *   - TWO groups:
 *       Gamma — two members (Aria, Bram), one ADDITIONAL linked store (+ a
 *               DANGLING link whose mount point does not exist, to prove the
 *               mount-points read-time filter), two scenarios (one default),
 *       Delta — empty (no members, no extra stores, no scenarios),
 *   - THREE projects:
 *       Iota  — roster [Aria, Cleo], non-empty state, two chats (one with
 *               messages + a story background, one with NO messages so
 *               lastMessageAt is null → the sort fallback), lantern+aurora
 *               aesthetics, two scenarios (one default), two wardrobe items
 *               (one composite), and a DANGLING extra mount-point link,
 *       Lambda — its official-store link REMOVED (getProjectDocumentStore → null)
 *               + two legacy `files` rows scoped to it → the list-files LEGACY
 *               branch,
 *       Kappa  — empty/fresh.
 *
 * NB the list-files STORE-BACKED branch (Iota's official store's doc_mount_files)
 * is seeded here too (Iota carries one store-backed file).
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_GP_MAIN=$W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
 *   QT_FIXTURE_GP_MOUNT=$W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-groups-projects-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { randomUUID } from 'node:crypto';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

// ── Pinned entity ids (shared verbatim with the Rust differentials) ──────────
const ARIA = 'a1000000-0000-4000-8000-000000000001';
const BRAM = 'a1000000-0000-4000-8000-000000000002';
const CLEO = 'a1000000-0000-4000-8000-000000000003';
const CONN = 'c0000001-0000-4000-8000-000000000001';
const APIKEY = 'a0000001-0000-4000-8000-000000000001';

const GAMMA = 'a2000000-0000-4000-8000-000000000001';
const DELTA = 'a2000000-0000-4000-8000-000000000002';
// P4.6n union coverage: Beacon (member Aria, HAS scenarios; name sorts BEFORE
// Gamma) and Zephyr (member Aria, ZERO scenarios → the zero-scenario skip).
const BEACON = 'a2000000-0000-4000-8000-000000000003';
const ZEPHYR = 'a2000000-0000-4000-8000-000000000004';

// P4.6n general (instance-wide) family: the singleton "Quilltap General" mount +
// two scenarios that BOTH mark isDefault (the alphabetically-first wins, the other
// is demoted-in-response with a warning — the default-conflict case).
const GENERAL_MP = 'b0000000-0000-4000-8000-0000000000a0';

const IOTA = 'a3000000-0000-4000-8000-000000000001';
const LAMBDA = 'a3000000-0000-4000-8000-000000000002';
const KAPPA = 'a3000000-0000-4000-8000-000000000003';

// Additional linked stores (mount points) + the dangling link targets.
const GAMMA_EXTRA_MP = 'b0000000-0000-4000-8000-000000000001';
const GAMMA_DANGLING_MP = 'b0000000-0000-4000-8000-0000000000de'; // no doc_mount_points row
const IOTA_DANGLING_MP = 'b0000000-0000-4000-8000-0000000000df'; // no doc_mount_points row

const BG_FILE = 'f0000001-0000-4000-8000-000000000001'; // Iota chat story background
const LAMBDA_FILE_1 = 'f0000002-0000-4000-8000-000000000002';
const LAMBDA_FILE_2 = 'f0000003-0000-4000-8000-000000000003';

const CHAT_A = 'c1000000-0000-4000-8000-000000000001'; // Iota, has messages + bg
const CHAT_B = 'c1000000-0000-4000-8000-000000000002'; // Iota, NO messages
const MSG_A1 = 'd1000000-0000-4000-8000-000000000001';
const MSG_A2 = 'd1000000-0000-4000-8000-000000000002';

// P4.6p listing-surfaces coverage — additive rows INVISIBLE to the existing
// groups/projects/scenarios reads (they enumerate none of these tables, and
// DIANA/MP_INDEXED are referenced by no group/project). Two built-in roleplay
// templates (seeded via v4's REAL seedBuiltInTemplates) + two user templates,
// four tags, one tagged character (DIANA — sortByCharacter), three image
// profiles, and one dedicated indexed store carrying an embedded chunk.
const DIANA = 'a1000000-0000-4000-8000-000000000004';
const RT_USER_1 = 'a4000000-0000-4000-8000-000000000001'; // all 3 delimiter kinds + addOns, patterns present
const RT_USER_2 = 'a4000000-0000-4000-8000-000000000002'; // delimiters + EMPTY patterns → read-time regen
const TAG_A = 'a5000000-0000-4000-8000-000000000001';
const TAG_B = 'a5000000-0000-4000-8000-000000000002';
const TAG_C = 'a5000000-0000-4000-8000-000000000003';
const TAG_D = 'a5000000-0000-4000-8000-000000000004';
const IP_1 = 'a6000000-0000-4000-8000-000000000001'; // default, apiKey, tags [TAG_A]
const IP_2 = 'a6000000-0000-4000-8000-000000000002'; // non-default, tags [TAG_B, TAG_C] (matches DIANA)
const IP_3 = 'a6000000-0000-4000-8000-000000000003'; // non-default, no apiKey, no tags
const MP_INDEXED = 'b0000000-0000-4000-8000-0000000000c0'; // documents store w/ an embedded chunk
const IDX_CHUNK = 'a7000000-0000-4000-8000-000000000001';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'groups-projects.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_GP_MAIN;
  const mountOut = process.env.QT_FIXTURE_GP_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_GP_MAIN and QT_FIXTURE_GP_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-gp-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
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
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { GroupSchema } = await import('@/lib/schemas/group.types');
  const { ProjectSchema } = await import('@/lib/schemas/project.types');
  const { RoleplayTemplateSchema } = await import('@/lib/schemas/types');
  const { ImageProfileSchema } = await import('@/lib/schemas/profile.types');
  const { TagSchema } = await import('@/lib/schemas/tag.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    GroupCharacterMemberSchema,
    GroupDocMountLinkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('groups', GroupSchema);
  await ensureCollection('projects', ProjectSchema);
  await ensureCollection('roleplay_templates', RoleplayTemplateSchema);
  await ensureCollection('image_profiles', ImageProfileSchema);
  await ensureCollection('tags', TagSchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['group_character_members', GroupCharacterMemberSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger it via a read.
  const { DocMountBlobsRepository } = await import(
    '@/lib/database/repositories/doc-mount-blobs.repository'
  );
  await new DocMountBlobsRepository().findByFileId('00000000-0000-4000-8000-000000000000');

  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  // 1. The single user + a connection profile (chat participants reference it).
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
  await ensureCollection('api_keys', ApiKeySchema);
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

  // 2. Three characters.
  const chars: Array<[string, string, string]> = [
    [ARIA, 'Aria', 'llm'],
    [BRAM, 'Bram', 'llm'],
    [CLEO, 'Cleo', 'user'],
  ];
  for (const [id, name, controlledBy] of chars) {
    await repos.characters.create(
      {
        name,
        userId: spec.userId,
        title: null,
        description: `${name} the test character.`,
        personality: null,
        aliases: [],
        controlledBy,
        talkativeness: 0.5,
        defaultConnectionProfileId: CONN,
        isFavorite: false,
        npc: false,
        tags: [],
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 3. A legacy image file used as Iota chat A's story background.
  await repos.files.create(
    {
      userId: spec.userId,
      linkedTo: [],
      tags: [],
      sha256: '1111111111111111111111111111111111111111111111111111111111111111',
      originalFilename: 'iota-bg.png',
      mimeType: 'image/png',
      size: 128,
      source: 'UPLOADED',
      category: 'IMAGE',
      storageKey: `${spec.userId}/iota-bg.png`,
      width: 16,
      height: 16,
    } as never,
    { id: BG_FILE, createdAt: TS, updatedAt: TS } as never,
  );

  // 4. Groups.
  // Gamma — rich: create (provisions official store), two members, one extra
  // linked store + a dangling link, two scenarios (one default).
  const gamma = await repos.groups.create(
    {
      name: 'Gamma',
      description: 'The rich group.',
      color: '#112233',
      icon: 'star',
      state: {},
    } as never,
    { id: GAMMA, createdAt: TS, updatedAt: TS } as never,
  );
  if (gamma.id !== GAMMA) throw new Error(`Gamma id drift: got ${gamma.id}, expected ${GAMMA}`);
  await repos.groupCharacterMembers.addMember(GAMMA, ARIA);
  await repos.groupCharacterMembers.addMember(GAMMA, BRAM);
  // One additional linked (documents) store.
  await repos.docMountPoints.create(
    {
      name: 'Gamma Extra Store',
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
    { id: GAMMA_EXTRA_MP, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.groupDocMountLinks.link(GAMMA, GAMMA_EXTRA_MP);
  // A dangling link whose mount point does not exist (proves the read filter).
  midb
    .prepare(
      'INSERT INTO group_doc_mount_links (id, groupId, mountPointId, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?)',
    )
    .run(randomUUID(), GAMMA, GAMMA_DANGLING_MP, TS, TS);

  // Delta — empty.
  const delta = await repos.groups.create(
    { name: 'Delta', state: {} } as never,
    { id: DELTA, createdAt: TS, updatedAt: TS } as never,
  );
  if (delta.id !== DELTA) throw new Error(`Delta id drift: got ${delta.id}, expected ${DELTA}`);

  // Gamma scenarios (into its official store).
  const { ensureGroupScenariosFolder, setGroupScenarioDefault } = await import(
    '@/lib/mount-index/group-scenarios'
  );
  const { buildScenarioFileContent } = await import('@/lib/mount-index/scenarios-common');
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  const gammaMp = gamma.officialMountPointId as string;
  await ensureGroupScenariosFolder(gammaMp);
  await writeDatabaseDocument(
    gammaMp,
    'Scenarios/prologue.md',
    buildScenarioFileContent({ name: 'Prologue', description: 'Opening scene', isDefault: true, body: 'The tale begins.' }),
  );
  await writeDatabaseDocument(
    gammaMp,
    'Scenarios/interlude.md',
    buildScenarioFileContent({ name: 'Interlude', body: 'A quiet pause.' }),
  );
  await setGroupScenarioDefault(gammaMp, 'Scenarios/prologue.md');

  // Beacon — member Aria, one non-default scenario. Its name sorts BEFORE Gamma,
  // so the participant-union's groupName sort is observable.
  const beacon = await repos.groups.create(
    { name: 'Beacon', state: {} } as never,
    { id: BEACON, createdAt: TS, updatedAt: TS } as never,
  );
  if (beacon.id !== BEACON) throw new Error(`Beacon id drift: got ${beacon.id}, expected ${BEACON}`);
  await repos.groupCharacterMembers.addMember(BEACON, ARIA);
  const beaconMp = beacon.officialMountPointId as string;
  await ensureGroupScenariosFolder(beaconMp);
  await writeDatabaseDocument(
    beaconMp,
    'Scenarios/beacon-intro.md',
    buildScenarioFileContent({ name: 'Beacon Intro', body: 'A light in the fog.' }),
  );

  // Zephyr — member Aria, ZERO scenarios (its official store exists but carries no
  // Scenarios entries) → the union's zero-scenario skip.
  const zephyr = await repos.groups.create(
    { name: 'Zephyr', state: {} } as never,
    { id: ZEPHYR, createdAt: TS, updatedAt: TS } as never,
  );
  if (zephyr.id !== ZEPHYR) throw new Error(`Zephyr id drift: got ${zephyr.id}, expected ${ZEPHYR}`);
  await repos.groupCharacterMembers.addMember(ZEPHYR, ARIA);

  // General (instance-wide) "Quilltap General" mount + its Scenarios/. Persist the
  // pointer in the MAIN db's instance_settings (the general-scenarios helpers read
  // it via getGeneralMountPointId).
  await repos.docMountPoints.create(
    {
      name: 'Quilltap General',
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
    { id: GENERAL_MP, createdAt: TS, updatedAt: TS } as never,
  );
  const mainDb = getRawDatabase();
  if (!mainDb) throw new Error('main DB handle unavailable');
  mainDb.exec(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  mainDb
    .prepare('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)')
    .run('generalMountPointId', GENERAL_MP);
  // Two scenarios that BOTH set isDefault (no setGeneralScenarioDefault, so both
  // stay raw-default on disk) → the default-conflict warning: alphabetically-first
  // 'aurora' wins, 'dusk' is demoted-in-response.
  const { ensureGeneralScenariosFolder } = await import('@/lib/mount-index/general-scenarios');
  const ensuredGeneral = await ensureGeneralScenariosFolder();
  if (!ensuredGeneral.mountPointId) throw new Error('general mount not resolved after seeding pointer');
  await writeDatabaseDocument(
    GENERAL_MP,
    'Scenarios/aurora.md',
    buildScenarioFileContent({ name: 'Aurora', description: 'Dawn light', isDefault: true, body: 'The sky pales.' }),
  );
  await writeDatabaseDocument(
    GENERAL_MP,
    'Scenarios/dusk.md',
    buildScenarioFileContent({ name: 'Dusk', isDefault: true, body: 'The lamps are lit.' }),
  );

  // 5. Projects.
  // Iota — rich.
  const iota = await repos.projects.create(
    {
      name: 'Iota',
      description: 'The rich project.',
      instructions: 'Be thorough.',
      allowAnyCharacter: false,
      characterRoster: [ARIA, CLEO],
      color: '#445566',
      defaultDisabledTools: [],
      defaultDisabledToolGroups: [],
      state: { mood: 'calm', turns: 3 },
      backgroundDisplayMode: 'project',
      storyBackgroundImageId: BG_FILE,
    } as never,
    { id: IOTA, createdAt: TS, updatedAt: TS } as never,
  );
  if (iota.id !== IOTA) throw new Error(`Iota id drift: got ${iota.id}, expected ${IOTA}`);
  const iotaMp = iota.officialMountPointId as string;

  // Lambda — legacy-branch project (official-store link removed later).
  const lambda = await repos.projects.create(
    { name: 'Lambda', state: {} } as never,
    { id: LAMBDA, createdAt: TS, updatedAt: TS } as never,
  );
  if (lambda.id !== LAMBDA) throw new Error(`Lambda id drift: got ${lambda.id}, expected ${LAMBDA}`);

  // Kappa — empty.
  const kappa = await repos.projects.create(
    { name: 'Kappa', state: {} } as never,
    { id: KAPPA, createdAt: TS, updatedAt: TS } as never,
  );
  if (kappa.id !== KAPPA) throw new Error(`Kappa id drift: got ${kappa.id}, expected ${KAPPA}`);

  // Iota chats. Chat A has two messages + a story background; chat B has none
  // (so its lastMessageAt is null → the list-chats sort fallback to updatedAt).
  const mkParticipant = (id: string, characterId: string, controlledBy: string) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy,
    status: 'active',
    connectionProfileId: CONN,
    createdAt: TS,
    updatedAt: TS,
  });
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Chat A',
      participants: [
        mkParticipant('e1000000-0000-4000-8000-000000000001', ARIA, 'llm'),
        mkParticipant('e1000000-0000-4000-8000-000000000002', CLEO, 'user'),
      ],
      chatType: 'salon',
      contextSummary: null,
      projectId: IOTA,
      storyBackgroundImageId: BG_FILE,
      tags: [],
    } as never,
    { id: CHAT_A, createdAt: TS, updatedAt: '2026-03-02T00:00:00.000Z' } as never,
  );
  await repos.chats.addMessages(CHAT_A, [
    {
      id: MSG_A1,
      type: 'message',
      role: 'USER',
      content: 'Hello there.',
      participantId: 'e1000000-0000-4000-8000-000000000002',
      createdAt: '2026-03-03T00:00:00.000Z',
      attachments: [],
    },
    {
      id: MSG_A2,
      type: 'message',
      role: 'ASSISTANT',
      content: 'A fine day for a voyage.',
      participantId: 'e1000000-0000-4000-8000-000000000001',
      createdAt: '2026-03-04T00:00:00.000Z',
      attachments: [],
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
    },
  ] as never);
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Chat B',
      participants: [mkParticipant('e1000000-0000-4000-8000-000000000003', ARIA, 'llm')],
      chatType: 'salon',
      contextSummary: null,
      projectId: IOTA,
      tags: [],
    } as never,
    { id: CHAT_B, createdAt: TS, updatedAt: '2026-03-05T00:00:00.000Z' } as never,
  );

  // Iota aesthetics (lantern + aurora) into its official store.
  const { writeAesthetic } = await import('@/lib/image-gen/aesthetic');
  await writeAesthetic(iotaMp, 'lantern', 'Warm brass tones and soft gaslight.');
  await writeAesthetic(iotaMp, 'aurora', 'Cool starlit palette.');

  // Iota scenarios (two, one default).
  const { ensureProjectScenariosFolder, setProjectScenarioDefault } = await import(
    '@/lib/mount-index/project-scenarios'
  );
  await ensureProjectScenariosFolder(iotaMp);
  await writeDatabaseDocument(
    iotaMp,
    'Scenarios/opening.md',
    buildScenarioFileContent({ name: 'Opening', description: 'Start here', isDefault: true, body: 'The curtain rises.' }),
  );
  await writeDatabaseDocument(
    iotaMp,
    'Scenarios/climax.md',
    buildScenarioFileContent({ name: 'Climax', body: 'The turning point.' }),
  );
  await setProjectScenarioDefault(iotaMp, 'Scenarios/opening.md');

  // Iota wardrobe (two items, one composite referencing the other).
  const { ensureProjectWardrobeFolder } = await import('@/lib/mount-index/project-wardrobe');
  const { createProjectWardrobeItem } = await import(
    '@/lib/database/repositories/vault-overlay/wardrobe-writes'
  );
  await ensureProjectWardrobeFolder(iotaMp);
  const CLOAK = 'aa000000-0000-4000-8000-000000000001';
  const ENSEMBLE = 'aa000000-0000-4000-8000-000000000002';
  await createProjectWardrobeItem(iotaMp, {
    id: CLOAK,
    characterId: null,
    title: 'Traveller Cloak',
    description: 'A weathered cloak.',
    imagePrompt: 'grey wool cloak',
    types: ['top'],
    componentItemIds: [],
    appropriateness: null,
    isDefault: false,
    replace: false,
    migratedFromClothingRecordId: null,
    archivedAt: null,
    createdAt: TS,
    updatedAt: TS,
  } as never);
  await createProjectWardrobeItem(iotaMp, {
    id: ENSEMBLE,
    characterId: null,
    title: 'Full Ensemble',
    description: 'Cloak plus boots.',
    imagePrompt: null,
    types: ['top', 'footwear'],
    componentItemIds: [CLOAK],
    appropriateness: null,
    isDefault: true,
    replace: false,
    migratedFromClothingRecordId: null,
    archivedAt: null,
    createdAt: TS,
    updatedAt: TS,
  } as never);

  // Iota store-backed file (list-files branch A) — a doc_mount_file in the
  // official store. `storeMountFile` writes the blob + manifest + link exactly
  // as the request path would.
  const { storeMountFile } = await import('@/lib/mount-index/store-file');
  await storeMountFile({
    mountPointId: iotaMp,
    relativePath: 'assets/logo.png',
    data: Buffer.from([137, 80, 78, 71, 13, 10, 26, 10, 0, 1, 2, 3, 4, 5]),
    originalMimeType: 'image/png',
    transcodeImages: false,
    extractText: false,
    enqueueEmbedding: false,
  } as never);

  // Iota dangling extra mount-point link (proves the mount-points read filter).
  midb
    .prepare(
      'INSERT INTO project_doc_mount_links (id, projectId, mountPointId, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?)',
    )
    .run(randomUUID(), IOTA, IOTA_DANGLING_MP, TS, TS);

  // Lambda — force the list-files LEGACY branch: remove its official-store link
  // so getProjectDocumentStore returns null, then seed two legacy files.
  midb.prepare('DELETE FROM project_doc_mount_links WHERE projectId = ?').run(LAMBDA);
  for (const [fid, name] of [
    [LAMBDA_FILE_1, 'notes.txt'],
    [LAMBDA_FILE_2, 'diagram.png'],
  ] as Array<[string, string]>) {
    await repos.files.create(
      {
        userId: spec.userId,
        linkedTo: [],
        tags: [],
        sha256: fid.replace(/-/g, '').padEnd(64, '0'),
        originalFilename: name,
        mimeType: name.endsWith('.png') ? 'image/png' : 'text/plain',
        size: 42,
        source: 'UPLOADED',
        category: name.endsWith('.png') ? 'IMAGE' : 'DOCUMENT',
        storageKey: `${spec.userId}/sub/${name}`,
        projectId: LAMBDA,
        width: name.endsWith('.png') ? 8 : null,
        height: name.endsWith('.png') ? 8 : null,
      } as never,
      { id: fid, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // ── P4.6p listing-surfaces rows (additive; invisible to the reads above) ──

  // Roleplay templates: the two built-ins via v4's REAL seeder (ids minted here,
  // baked stable into the committed .db — the Rust test resolves them by name),
  // plus two user templates.
  await repos.roleplayTemplates.seedBuiltInTemplates();
  // RT_USER_1 — all three delimiter kinds, an addOns-bearing wrap, patterns
  // PRESENT (so GET returns them verbatim, no read-time regen).
  await repos.roleplayTemplates.create(
    {
      userId: spec.userId,
      name: 'Bracket Prose',
      description: 'User template exercising every delimiter kind.',
      systemPrompt: 'Write with brackets and tags.',
      isBuiltIn: false,
      tags: [],
      delimiters: [
        {
          kind: 'wrap',
          name: 'Emphasis',
          buttonName: 'Em',
          delimiters: '*',
          style: 'qt-rp-emph',
          addOns: { bold: true, underline: 'single' },
        },
        { kind: 'linePrefix', name: 'Aside', buttonName: 'Asd', marker: '// ', style: 'qt-rp-ooc' },
        {
          kind: 'tagPrefix',
          name: 'Speaker',
          buttonName: 'Spk',
          open: '[',
          close: ']',
          style: 'qt-rp-speaker',
        },
      ],
      renderingPatterns: [{ pattern: '\\*[^*]+\\*', className: 'qt-rp-emph' }],
      dialogueDetection: null,
      narrationDelimiters: '*',
    } as never,
    { id: RT_USER_1, createdAt: TS, updatedAt: TS } as never,
  );
  // RT_USER_2 — delimiters present but renderingPatterns EMPTY → the GET-[id]
  // read-time auto-regen (non-persisted).
  await repos.roleplayTemplates.create(
    {
      userId: spec.userId,
      name: 'Quiet Draft',
      description: null,
      systemPrompt: 'Minimal.',
      isBuiltIn: false,
      tags: [],
      delimiters: [
        { kind: 'wrap', name: 'Underline', buttonName: 'Ul', delimiters: '_', style: 'qt-rp-u' },
      ],
      renderingPatterns: [],
      dialogueDetection: null,
      narrationDelimiters: '_',
    } as never,
    { id: RT_USER_2, createdAt: TS, updatedAt: TS } as never,
  );

  // Four tags (for image-profile tag enrichment + the sortByCharacter match).
  for (const [tid, name] of [
    [TAG_A, 'Portrait'],
    [TAG_B, 'Landscape'],
    [TAG_C, 'Night'],
    [TAG_D, 'Sketch'],
  ] as Array<[string, string]>) {
    await repos.tags.create(
      { userId: spec.userId, name, nameLower: name.toLowerCase(), quickHide: false } as never,
      { id: tid, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // DIANA — a tagged character used ONLY by the image sortByCharacter case (no
  // group/project references it, so the existing reads are unperturbed).
  await repos.characters.create(
    {
      name: 'Diana',
      userId: spec.userId,
      title: null,
      description: 'The tagged test character.',
      personality: null,
      aliases: [],
      controlledBy: 'llm',
      talkativeness: 0.5,
      defaultConnectionProfileId: CONN,
      isFavorite: false,
      npc: false,
      tags: [TAG_B, TAG_C],
    } as never,
    { id: DIANA, createdAt: TS, updatedAt: TS } as never,
  );

  // Three image profiles: IP_1 default + apiKey + [TAG_A]; IP_2 non-default +
  // [TAG_B, TAG_C] (matches DIANA → 2 matching tags); IP_3 plain.
  await repos.imageProfiles.create(
    {
      userId: spec.userId,
      name: 'Primary Imagery',
      provider: 'OPENAI_COMPATIBLE',
      apiKeyId: APIKEY,
      baseUrl: 'http://127.0.0.1:1/v1',
      modelName: 'mock-image-model',
      parameters: { steps: 30 },
      isDefault: true,
      isDangerousCompatible: false,
      tags: [TAG_A],
    } as never,
    { id: IP_1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.imageProfiles.create(
    {
      userId: spec.userId,
      name: 'Scenic',
      provider: 'OPENAI_COMPATIBLE',
      apiKeyId: null,
      baseUrl: null,
      modelName: 'mock-image-model',
      parameters: {},
      isDefault: false,
      isDangerousCompatible: false,
      tags: [TAG_B, TAG_C],
    } as never,
    { id: IP_2, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.imageProfiles.create(
    {
      userId: spec.userId,
      name: 'Bare',
      provider: 'OPENAI_COMPATIBLE',
      apiKeyId: null,
      baseUrl: null,
      modelName: 'mock-image-model',
      parameters: {},
      isDefault: false,
      isDangerousCompatible: false,
      tags: [],
    } as never,
    { id: IP_3, createdAt: TS, updatedAt: TS } as never,
  );

  // MP_INDEXED — a dedicated documents store carrying one embedded chunk (a
  // non-null, non-empty Float32 embedding) so the mount-points embedded-count
  // paths (LIST GROUP BY + GET-[id] hydrate-and-filter) both see a non-zero.
  await repos.docMountPoints.create(
    {
      name: 'Indexed Store',
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
    { id: MP_INDEXED, createdAt: TS, updatedAt: TS } as never,
  );
  await storeMountFile({
    mountPointId: MP_INDEXED,
    relativePath: 'notes/intro.md',
    data: Buffer.from('# Intro\n\nIndexed content.', 'utf8'),
    originalMimeType: 'text/markdown',
    transcodeImages: false,
    extractText: false,
    enqueueEmbedding: false,
  } as never);
  const idxLinkId = (
    midb
      .prepare('SELECT id FROM doc_mount_file_links WHERE mountPointId = ? LIMIT 1')
      .get(MP_INDEXED) as { id: string } | undefined
  )?.id;
  if (!idxLinkId) throw new Error('indexed-store file link not found after storeMountFile');
  // One embedded chunk (4-byte Float32 BLOB → a single 1.0 component).
  const embedding = Buffer.from(new Uint8Array(new Float32Array([1]).buffer));
  midb
    .prepare(
      'INSERT INTO doc_mount_chunks (id, linkId, mountPointId, chunkIndex, content, tokenCount, headingContext, embedding, createdAt, updatedAt) ' +
        'VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)',
    )
    .run(IDX_CHUNK, idxLinkId, MP_INDEXED, 0, 'Indexed content.', 3, null, embedding, TS, TS);

  // Materialize empty tables the read paths sweep so both sides read identical
  // (empty) state rather than v4 auto-creating per-case while the Rust raw SQL
  // would 404.
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built groups-projects fixture: main=${mainOut} mount=${mountOut} (2 groups, 3 projects, 4 roleplay templates, 3 image profiles, indexed store)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`groups-projects fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
