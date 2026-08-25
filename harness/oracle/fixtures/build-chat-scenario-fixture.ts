/**
 * The committed scenario-change server-surface fixture builder (P4.D115
 * deliverable) — the test-pepper substrate for
 * `chat_scenario_routes_equivalence` (tier 2).
 *
 * Baked entirely through v4's REAL repositories and store-write helpers (never
 * hand-rolled SQL — `project-store-fixture-needs-real-create`), with every id
 * and timestamp pinned so both differential sides read byte-identical rows.
 *
 * ## What is in it, and which arm each row exists for
 *
 *   1. ONE user, plus a `chat_settings` row whose `timezone` is UTC (so the
 *      transcript's headings render identically on both differential sides —
 *      v5's `system_tz()` would otherwise read the host zone).
 *   2. FIVE characters, each vault provisioned by v4's own `characters.create`:
 *        - ARIA (llm)  — first in the target cast, NO scenarios. Proves
 *                        `findCharacterOwningScenario` keeps looking.
 *        - BEA  (llm)  — SECOND in the cast, owns "The parlour" and "Watch-change", so
 *                        the cast walk must look past ARIA to find an owner.
 *                        (An EMPTY-`content` scenario — v4's falsy `presetBody`
 *                        fall-through — is NOT seedable: v4's own
 *                        `CharacterScenario` schema is `min(1)`, so `create`
 *                        refuses it. That arm is unreachable through v4's write
 *                        path and is pinned by unit test instead, the way
 *                        P4.D112 pinned its unreachable boundary escapes.)
 *        - CLIO (user) — a USER-controlled CHARACTER participant. v4's cast
 *                        walk filters on `type`/`characterId`/`status`, NOT on
 *                        `controlledBy`, so CLIO is walked like anyone else.
 *        - DREW (llm)  — participant `status: 'removed'`, owns "The vacated room" → the
 *                        `status === 'removed'` skip's counterexample.
 *        - MOTE (llm)  — the lone participant of BROKEN_CHAT; its
 *                        `characterDocumentMountPointId` is repointed at a
 *                        mount point that does not exist, so the hydrated read
 *                        FAILS. v4 catches and `logger.debug`s past it; v5's
 *                        handler swallows the same way.
 *   3. Document stores + their `Scenarios/` folders, all through v4's real
 *      `ensure*ScenariosFolder` + `writeDatabaseDocument`:
 *        - PROJ's official store   → `Scenarios/tavern.md`
 *        - GRP's official store    → `Scenarios/quay.md`
 *        - the "Quilltap General"  → `Scenarios/welcome.md` and
 *          singleton (+ its          `Scenarios/blank.md` (frontmatter only,
 *          `instance_settings`       whitespace body → resolves to null, the
 *          pointer)                  "did not resolve to a body" fall-through)
 *      PROJ_BARE is a project with NO official store — the
 *      "provided but project has no officialMountPointId" arm.
 *   4. CHAT — in PROJ, cast ARIA / BEA / CLIO / DREW(removed), and
 *      `scenarioText` SEEDED to the general body verbatim, so re-picking
 *      `Scenarios/welcome.md` (or typing that exact text) is the no-op arm.
 *      Two seeded messages give the Markdown transcript something to render
 *      around the `scenario-change` notice the verb posts.
 *   5. NULL_CHAT — `projectId: null`, `scenarioText: null`: the
 *      "projectScenarioPath provided without projectId" fall-through AND the
 *      already-null no-op.
 *   6. BARE_CHAT — in PROJ_BARE: the "project has no officialMountPointId"
 *      fall-through.
 *   7. BROKEN_CHAT — MOTE only: the unreadable-character skip.
 *
 * The character-scenario ids are MINTED by v4's vault projection (`create` stamps
 * its own id + timestamps onto each `scenarios` entry), so they cannot be pinned
 * here. They are echoed, keyed by title, to the `<main>.meta.json` sidecar that
 * the oracle case and the Rust harness both read — the `chat-admin-web`
 * precedent.
 *
 * No row lives in the llm-logs partition. `background_jobs` is materialized so
 * both sides' fresh copies share a schema (nothing in this family enqueues, but
 * `chats.update` paths elsewhere in the crate touch it).
 *
 * Regenerate (Node 24, from the v4 checkout — the committed .db files land in
 * place):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   TZ=UTC \
 *   QT_FIXTURE_CS_MAIN=$W/crates/quilltap-web/tests/fixtures/chat-scenario-main.db \
 *   QT_FIXTURE_CS_MOUNT=$W/crates/quilltap-web/tests/fixtures/chat-scenario-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-chat-scenario-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  generalBody: string;
  projectBody: string;
  groupBody: string;
  beaScenarioOneBody: string;
  beaScenarioTwoBody: string;
  drewScenarioBody: string;
  freeTextNotes: string;
}

// ── Pinned entity ids (shared verbatim with the oracle case + the Rust test) ──
const ARIA = 'a1000000-0000-4000-8000-000000000001';
const BEA = 'a1000000-0000-4000-8000-000000000002';
const CLIO = 'a1000000-0000-4000-8000-000000000003';
const DREW = 'a1000000-0000-4000-8000-000000000004';
const MOTE = 'a1000000-0000-4000-8000-000000000005';

// Character-scenario ids are MINTED by the vault projection, so the ids written
// at create time are placeholders (v4's schema requires the key) and the REAL
// ids are echoed to the `<main>.meta.json` sidecar — see the header's note.
const SCEN_PLACEHOLDER_1 = '51000000-0000-4000-8000-000000000001';
const SCEN_PLACEHOLDER_2 = '51000000-0000-4000-8000-000000000002';
const SCEN_PLACEHOLDER_3 = '51000000-0000-4000-8000-000000000003';
// (The falsy-empty-`content` fall-through is not seedable — see the header.)

const PROJ = '71000000-0000-4000-8000-000000000001';
const PROJ_BARE = '71000000-0000-4000-8000-000000000002';
const GRP = '72000000-0000-4000-8000-000000000001';
const GENERAL_MP = '83000000-0000-4000-8000-000000000003';
/** No `doc_mount_points` row — MOTE's vault pointer is repointed here. */
const DEAD_MP = '83000000-0000-4000-8000-0000000000de';

const CHAT = 'c1000000-0000-4000-8000-000000000001';
const NULL_CHAT = 'c1000000-0000-4000-8000-000000000002';
const BARE_CHAT = 'c1000000-0000-4000-8000-000000000003';
const BROKEN_CHAT = 'c1000000-0000-4000-8000-000000000004';

const P_ARIA = 'e1000000-0000-4000-8000-000000000001';
const P_BEA = 'e1000000-0000-4000-8000-000000000002';
const P_CLIO = 'e1000000-0000-4000-8000-000000000003';
const P_DREW = 'e1000000-0000-4000-8000-000000000004';
const P_NULL_ARIA = 'e1000000-0000-4000-8000-000000000011';
const P_BARE_ARIA = 'e1000000-0000-4000-8000-000000000021';
const P_MOTE = 'e1000000-0000-4000-8000-000000000031';

const CHAT_SETTINGS = '91000000-0000-4000-8000-000000000001';

const M1 = 'd1000000-0000-4000-8000-000000000001';
const M2 = 'd1000000-0000-4000-8000-000000000002';

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(
      `chat-scenario fixture must be built under TZ=UTC (getTimezoneOffset=${offset})`,
    );
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'chat-scenario-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_CS_MAIN;
  const mountOut = process.env.QT_FIXTURE_CS_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_CS_MAIN and QT_FIXTURE_CS_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cs-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { CharacterSchema, ChatMetadataSchema, BackgroundJobSchema } = await import(
    '@/lib/schemas/types'
  );
  const { ChatSettingsSchema } = await import('@/lib/schemas/settings.types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { GroupSchema } = await import('@/lib/schemas/group.types');
  const { ProjectSchema } = await import('@/lib/schemas/project.types');
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
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('groups', GroupSchema);
  await ensureCollection('projects', ProjectSchema);
  await ensureCollection('background_jobs', BackgroundJobSchema);
  // The Markdown-transcript export reads the operator's chat settings (default
  // timestamp config + timezone); a real instance always has the table, and
  // without it v5's read errors into the route's fixed 500.
  await ensureCollection('chat_settings', ChatSettingsSchema);

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
  const repos = getRepositories();
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger CREATE via a read.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. The user + their chat settings. The `timezone` is seeded EXPLICITLY so
  //    the Markdown transcript's headings render in UTC on both differential
  //    sides — v5's `system_tz()` would otherwise read the host zone, and the
  //    two sides would disagree by the operator's offset (the
  //    `build-chat-dialogs-fixture` precedent).
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chatSettings.create(
    { userId: spec.userId, timezone: 'UTC' } as never,
    { id: CHAT_SETTINGS, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Characters. `scenarios` is routed into the vault by `create`, so the ids
  //    below are pinned rather than minted by `addScenario`.
  const mkChar = async (
    id: string,
    name: string,
    controlledBy: string,
    extra: object = {},
  ): Promise<void> => {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy, ...extra } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  await mkChar(ARIA, 'Aria', 'llm', {
    description: 'The Salon’s resident correspondent.',
    scenarios: [],
  });
  await mkChar(BEA, 'Bea', 'llm', {
    description: 'A signal-keeper from the lower deck.',
    // v4's `CharacterScenario` schema demands id/createdAt/updatedAt at write
    // time, but the vault projection MINTS its own id — so these are placeholders
    // and the real ids come back through the sidecar.
    scenarios: [
      {
        id: SCEN_PLACEHOLDER_1,
        title: 'The parlour',
        content: spec.beaScenarioOneBody,
        createdAt: TS,
        updatedAt: TS,
      },
      {
        id: SCEN_PLACEHOLDER_2,
        title: 'Watch-change',
        content: spec.beaScenarioTwoBody,
        createdAt: TS,
        updatedAt: TS,
      },
    ],
  });
  await mkChar(CLIO, 'Clio', 'user', { description: 'The operator’s voice at the table.' });
  await mkChar(DREW, 'Drew', 'llm', {
    description: 'Dismissed from the lower deck.',
    scenarios: [
      {
        id: SCEN_PLACEHOLDER_3,
        title: 'The vacated room',
        content: spec.drewScenarioBody,
        createdAt: TS,
        updatedAt: TS,
      },
    ],
  });
  await mkChar(MOTE, 'Mote', 'llm', { description: 'Whose vault will be broken below.' });

  // 3. Stores. Project / group official stores are provisioned by `create`;
  //    the General singleton is a bare mount point plus its `instance_settings`
  //    pointer, exactly as `build-groups-projects-fixture` does it.
  const { ensureGroupScenariosFolder } = await import('@/lib/mount-index/group-scenarios');
  const { ensureProjectScenariosFolder } = await import('@/lib/mount-index/project-scenarios');
  const { ensureGeneralScenariosFolder } = await import('@/lib/mount-index/general-scenarios');
  const { buildScenarioFileContent } = await import('@/lib/mount-index/scenarios-common');
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');

  const proj = await repos.projects.create(
    { name: 'The Ford', description: 'The project with a scene on file.', state: {} } as never,
    { id: PROJ, createdAt: TS, updatedAt: TS } as never,
  );
  const projMp = proj.officialMountPointId as string;
  if (!projMp) throw new Error('project official store not provisioned');
  await ensureProjectScenariosFolder(projMp);
  await writeDatabaseDocument(
    projMp,
    'Scenarios/tavern.md',
    buildScenarioFileContent({
      name: 'The Tavern',
      description: 'Rain at the ford',
      body: spec.projectBody,
    }),
  );

  // A project with NO official store — v4 nulls the pointer for the
  // "provided but project has no officialMountPointId" arm.
  await repos.projects.create(
    { name: 'The Bare Bank', state: {} } as never,
    { id: PROJ_BARE, createdAt: TS, updatedAt: TS } as never,
  );
  const maindb = getRawDatabase();
  if (!maindb) throw new Error('main DB handle unavailable');
  maindb.prepare('UPDATE "projects" SET "officialMountPointId" = NULL WHERE "id" = ?').run(
    PROJ_BARE,
  );

  const grp = await repos.groups.create(
    { name: 'The Quay', description: 'The group with a scene on file.', state: {} } as never,
    { id: GRP, createdAt: TS, updatedAt: TS } as never,
  );
  const grpMp = grp.officialMountPointId as string;
  if (!grpMp) throw new Error('group official store not provisioned');
  await ensureGroupScenariosFolder(grpMp);
  await writeDatabaseDocument(
    grpMp,
    'Scenarios/quay.md',
    buildScenarioFileContent({ name: 'The Quay', body: spec.groupBody }),
  );

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
    { id: GENERAL_MP, createdAt: TS, updatedAt: TS },
  );
  maindb.exec(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  maindb
    .prepare('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)')
    .run('generalMountPointId', GENERAL_MP);
  const ensuredGeneral = await ensureGeneralScenariosFolder();
  if (!ensuredGeneral.mountPointId) {
    throw new Error('general mount not resolved after seeding the pointer');
  }
  await writeDatabaseDocument(
    GENERAL_MP,
    'Scenarios/welcome.md',
    buildScenarioFileContent({
      name: 'The Concourse',
      description: 'Gaslamps at dusk',
      body: spec.generalBody,
    }),
  );
  // Frontmatter only, whitespace body → `resolveScenarioBody` returns null, so
  // the tier warns and falls through.
  await writeDatabaseDocument(GENERAL_MP, 'Scenarios/blank.md', '---\nname: Blank\n---\n\n   \n');

  // 4. The chats.
  const mkParticipant = (
    id: string,
    characterId: string,
    controlledBy: string,
    displayOrder: number,
    extra: object = {},
  ) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy,
    status: 'active',
    isActive: true,
    displayOrder,
    connectionProfileId: null,
    hasHistoryAccess: false,
    createdAt: TS,
    updatedAt: TS,
    ...extra,
  });

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Evening Post',
      chatType: 'salon',
      projectId: PROJ,
      // Seeded to the GENERAL body verbatim: re-picking `Scenarios/welcome.md`
      // (or typing this text) is then the no-op arm.
      scenarioText: spec.generalBody,
      participants: [
        mkParticipant(P_ARIA, ARIA, 'llm', 0),
        mkParticipant(P_BEA, BEA, 'llm', 1),
        mkParticipant(P_CLIO, CLIO, 'user', 2),
        mkParticipant(P_DREW, DREW, 'llm', 3, { status: 'removed', removedAt: TS }),
      ],
      tags: [],
    } as never,
    { id: CHAT, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'A Room With No Project',
      chatType: 'salon',
      projectId: null,
      scenarioText: null,
      participants: [mkParticipant(P_NULL_ARIA, ARIA, 'llm', 0)],
      tags: [],
    } as never,
    { id: NULL_CHAT, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Bare Bank Room',
      chatType: 'salon',
      projectId: PROJ_BARE,
      scenarioText: null,
      participants: [mkParticipant(P_BARE_ARIA, ARIA, 'llm', 0)],
      tags: [],
    } as never,
    { id: BARE_CHAT, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Unreadable Guest',
      chatType: 'salon',
      projectId: null,
      scenarioText: null,
      participants: [mkParticipant(P_MOTE, MOTE, 'llm', 0)],
      tags: [],
    } as never,
    { id: BROKEN_CHAT, createdAt: TS, updatedAt: TS } as never,
  );

  // 5. Two messages so the Markdown transcript has a conversation to render the
  //    `scenario-change` notice into.
  const add = async (chatId: string, event: object) =>
    repos.chats.addMessage(chatId, event as never);
  await add(CHAT, {
    type: 'message',
    id: M1,
    role: 'USER',
    content: 'The evening post is late again.',
    participantId: P_CLIO,
    createdAt: '2026-05-01T10:00:00.000Z',
    attachments: [],
  });
  await add(CHAT, {
    type: 'message',
    id: M2,
    role: 'ASSISTANT',
    content: 'Aria sets down her pen. "Late, and the lamps unlit besides."',
    participantId: P_ARIA,
    createdAt: '2026-05-01T10:01:00.000Z',
    attachments: [],
  });

  // 6. Break MOTE's vault LAST, so nothing above trips over it: repoint the
  //    keystone at a mount point that has no `doc_mount_points` row, which is
  //    what makes the hydrated `characters.findById` fail on both sides.
  maindb
    .prepare('UPDATE "characters" SET "characterDocumentMountPointId" = ? WHERE "id" = ?')
    .run(DEAD_MP, MOTE);

  // 7. The minted character-scenario ids → the sidecar both differential sides
  //    read (the `chat-admin-web` precedent for minted vault values).
  const scenarioIdsFor = async (characterId: string): Promise<Record<string, string>> => {
    const c = (await repos.characters.findById(characterId)) as unknown as {
      scenarios?: Array<{ id: string; title: string }> | null;
    } | null;
    const out: Record<string, string> = {};
    for (const s of c?.scenarios ?? []) out[s.title] = s.id;
    return out;
  };
  const meta = {
    beaScenarioIds: await scenarioIdsFor(BEA),
    drewScenarioIds: await scenarioIdsFor(DREW),
  };
  if (Object.keys(meta.beaScenarioIds).length !== 2 || Object.keys(meta.drewScenarioIds).length !== 1) {
    throw new Error(`scenario ids not minted as expected: ${JSON.stringify(meta)}`);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta, null, 2) + '\n');

  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }
  process.stderr.write(`built chat-scenario fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-scenario fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
