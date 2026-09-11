/**
 * The committed IN-SCENE voice-rehearsal fixture builder (P4.D180 deliverable) —
 * the test-pepper substrate for `in_scene_voiced_tier3_equivalence` (tier 3
 * service rows + tier-2-shaped action rows).
 *
 * Baked entirely through v4's REAL repositories and store-write helpers (never
 * hand-rolled SQL except the two deliberate breakages at the end), with every id
 * and timestamp pinned so both differential sides read byte-identical rows.
 *
 * ## What is in it, and which arm each row exists for
 *
 *   1. TWO users: the operator, and NO_DEFAULT_USER — who owns no connection
 *      profile at all, which is the ONLY way to reach v4's
 *      `No connection profile to rewrite with` 400 while the operator still has
 *      an instance default for the `instance-default` arm to resolve.
 *      `chat_settings` for the operator carries `timezone: 'UTC'` and the
 *      `dangerousContentSettings` bag the reroute arm reads.
 *   2. SIX connection profiles, one per arm of v4's chain:
 *        - CONN_SEAT      the seat's own (`participant` source). NOT
 *                         `isDangerousCompatible` → the reroute arm's subject.
 *        - CONN_OVERRIDE  the dialog picker's override (`override` source).
 *        - CONN_CHAR      VESPER's `defaultConnectionProfileId`
 *                         (`character-default` source).
 *        - CONN_INSTANCE  `isDefault: true` (`instance-default` source).
 *        - CONN_UNCENSORED the configured `uncensoredTextProfileId`,
 *                         `isDangerousCompatible` → what the reroute picks.
 *        - CONN_SAFE_DC   `isDangerousCompatible` → the seat profile of the
 *                         NO-reroute arm (the gate's other direction).
 *   3. FIVE characters, each vault provisioned by v4's own `characters.create`:
 *        - VESPER (llm)  — the rehearsing seat's character. TWO `systemPrompts`
 *                          (SP_FIRST first, SP_DEFAULT flagged `isDefault`) so
 *                          the prompt chain's last two rungs are separable; a
 *                          `defaultConnectionProfileId`; three memories for the
 *                          Commonplace recall; and a `Subprompts/measure.md` in
 *                          its vault for the `precompiled ? null : subprompts`
 *                          arm.
 *        - BRAM   (llm)  — a bystander with an INTACT vault: the `[Name]`
 *                          attribution arm.
 *        - CORMAC (llm)  — a bystander whose vault pointer is repointed at a
 *                          mount point with no row, so `characters.findById`
 *                          THROWS: the warned-and-skipped arm.
 *        - DELL   (user) — a `controlledBy: 'user'` seat.
 *        - ESME   (llm)  — seated `absent`: the "not present" 400.
 *      GHOST_CHAR is an id with NO character row — the `notFound('Character')`
 *      arm, reached through a seat that IS impersonated.
 *   4. A project (standing instructions, carrying `{{char}}` so the section's
 *      template pass is measurable) and a user roleplay template.
 *   5. `instance_settings['taboo']` — two phrases, so the Taboo section lands.
 *   6. FIVE chats:
 *        - CHAT_STACK      the main one. Project, roleplay template, scenario
 *                          text, and a `compiledIdentityStacks` at the CURRENT
 *                          builder version for the rehearsing seat. Seven
 *                          seats, so every ladder arm has a subject:
 *                          P_VESPER (impersonated, llm, own profile, own
 *                          `selectedSystemPromptId`, `selectedSubpromptIds`),
 *                          P_BRAM (NOT impersonated → the "not being
 *                          impersonated" 400), P_CORMAC (the broken vault),
 *                          P_DELL (user-controlled), P_ESME (absent),
 *                          P_GHOST (impersonated, character row missing).
 *                          ⚠ There is NO `type !== 'CHARACTER'` seat, and there
 *                          cannot be: v4's own `ChatParticipantSchema` types
 *                          `type` as a one-member enum and `characterId` as a
 *                          required `UUIDSchema`, so BOTH halves of v4's
 *                          `Only a character seat can be spoken for.` guard are
 *                          unreachable through v4's write path. Measured, not
 *                          assumed — the builder was written with such a seat
 *                          and v4's `chats.create` refused it. Pinned by unit
 *                          test instead, the way P4.D112 pinned its unreachable
 *                          boundary escapes.
 *                          SIXTEEN played messages, so the `slice(-12)` window
 *                          drops four and is measurable; plus a whisper aimed
 *                          at BRAM alone, a `systemSender` bubble, and an
 *                          empty-content message — each of which the played
 *                          filter must drop.
 *        - CHAT_NOSTACK    the same cast with `compiledIdentityStacks: null`:
 *                          the OTHER side of `subprompts: precompiled ? null :
 *                          subprompts`. Without both chats that arm cannot be
 *                          measured at all.
 *        - CHAT_NOHIST     the rehearsing seat has `hasHistoryAccess: false`
 *                          and a Host status announcement sits mid-transcript,
 *                          so the presence-window leg does real work. The
 *                          announcement is itself a whisper, which is why v4
 *                          computes windows from the FULL event list.
 *        - CHAT_UNC        `conciergeOverride: 'UNCENSORED'` with the seat on
 *                          CONN_SEAT (not dangerous-compatible) → reroute.
 *        - CHAT_UNC_OK     `conciergeOverride: 'UNCENSORED'` with the seat on
 *                          CONN_SAFE_DC (dangerous-compatible) → NO reroute.
 *        - CHAT_NOPROF     neither seat carries a `connectionProfileId`, so
 *                          VESPER falls to her character default and BRAM — who
 *                          has none — to the instance default, or to
 *                          `No connection profile to rewrite with` when the
 *                          session user is NO_DEFAULT_USER. Three rungs, one
 *                          chat.
 *
 * No row lives in the llm-logs partition: the jest side no-ops `logLLMCall`
 * wholesale (`jest-setup-llm-logging-service-mocked`), so the log row is not a
 * tier-3 comparand — it is proven separately by the web-venue wire test.
 * `background_jobs` is materialized so both sides' fresh copies share a schema.
 *
 * Regenerate (Node 24, from the v4 checkout — the committed .db files land in
 * place):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   TZ=UTC \
 *   QT_FIXTURE_ISV_MAIN=$W/crates/quilltap-web/tests/fixtures/in-scene-voiced-main.db \
 *   QT_FIXTURE_ISV_MOUNT=$W/crates/quilltap-web/tests/fixtures/in-scene-voiced-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-in-scene-voiced-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  userIdNoDefault: string;
  seedTimestamp: string;
  frozenNowMs: number;
  scenarioText: string;
  projectInstructions: string;
  roleplayTemplatePrompt: string;
  tabooPhrases: string[];
  subpromptTitle: string;
  subpromptContent: string;
  compiledStack: string;
  memories: Array<{ id: string; content: string; summary: string; importance: number }>;
}

// ── Pinned entity ids (shared verbatim with the oracle case + the Rust test) ──
const VESPER = 'a1000000-0000-4000-8000-000000000001';
const BRAM = 'a1000000-0000-4000-8000-000000000002';
const CORMAC = 'a1000000-0000-4000-8000-000000000003';
const DELL = 'a1000000-0000-4000-8000-000000000004';
const ESME = 'a1000000-0000-4000-8000-000000000005';
/** No `characters` row — the `notFound('Character')` arm. */
const GHOST_CHAR = 'a1000000-0000-4000-8000-0000000000de';

const SP_FIRST = '52000000-0000-4000-8000-000000000001';
const SP_DEFAULT = '52000000-0000-4000-8000-000000000002';

const APIKEY = 'd0000000-0000-4000-8000-000000000001';
const CONN_SEAT = 'c0000000-0000-4000-8000-000000000001';
const CONN_OVERRIDE = 'c0000000-0000-4000-8000-000000000002';
const CONN_CHAR = 'c0000000-0000-4000-8000-000000000003';
const CONN_INSTANCE = 'c0000000-0000-4000-8000-000000000004';
const CONN_UNCENSORED = 'c0000000-0000-4000-8000-000000000005';
const CONN_SAFE_DC = 'c0000000-0000-4000-8000-000000000006';

const PROJ = '71000000-0000-4000-8000-000000000001';
const RT = '73000000-0000-4000-8000-000000000001';
/** No `doc_mount_points` row — CORMAC's vault pointer is repointed here. */
const DEAD_MP = '83000000-0000-4000-8000-0000000000de';

const CHAT_STACK = 'c1000000-0000-4000-8000-000000000001';
const CHAT_NOSTACK = 'c1000000-0000-4000-8000-000000000002';
const CHAT_NOHIST = 'c1000000-0000-4000-8000-000000000003';
const CHAT_UNC = 'c1000000-0000-4000-8000-000000000004';
const CHAT_UNC_OK = 'c1000000-0000-4000-8000-000000000005';
const CHAT_NOPROF = 'c1000000-0000-4000-8000-000000000006';

const P_VESPER = 'e1000000-0000-4000-8000-000000000001';
const P_BRAM = 'e1000000-0000-4000-8000-000000000002';
const P_CORMAC = 'e1000000-0000-4000-8000-000000000003';
const P_DELL = 'e1000000-0000-4000-8000-000000000004';
const P_ESME = 'e1000000-0000-4000-8000-000000000005';
const P_GHOST = 'e1000000-0000-4000-8000-000000000007';

const CHAT_SETTINGS = '91000000-0000-4000-8000-000000000001';

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(
      `in-scene-voiced fixture must be built under TZ=UTC (getTimezoneOffset=${offset})`,
    );
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'in-scene-voiced.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_ISV_MAIN;
  const mountOut = process.env.QT_FIXTURE_ISV_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_ISV_MAIN and QT_FIXTURE_ISV_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-isv-fixture-'));
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
  const {
    CharacterSchema,
    ChatMetadataSchema,
    BackgroundJobSchema,
    ConnectionProfileSchema,
    ApiKeySchema,
    MemorySchema,
    RoleplayTemplateSchema,
  } = await import('@/lib/schemas/types');
  const { ChatSettingsSchema } = await import('@/lib/schemas/settings.types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
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
  for (const [name, schema] of [
    ['users', UserSchema],
    ['characters', CharacterSchema],
    ['chats', ChatMetadataSchema],
    ['projects', ProjectSchema],
    ['background_jobs', BackgroundJobSchema],
    ['chat_settings', ChatSettingsSchema],
    ['connection_profiles', ConnectionProfileSchema],
    ['api_keys', ApiKeySchema],
    ['memories', MemorySchema],
    ['roleplay_templates', RoleplayTemplateSchema],
  ] as Array<[string, unknown]>) {
    await ensureCollection(name, schema as never);
  }

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  for (const [name, schema] of [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['group_character_members', GroupCharacterMemberSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ] as Array<[string, unknown]>) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  const repos = getRepositories();
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. Users. NO_DEFAULT_USER owns nothing — the only way to reach v4's
  //    `No connection profile to rewrite with` while the operator keeps an
  //    instance default for the `instance-default` arm.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'nobody', email: null, name: 'Nobody' } as never,
    { id: spec.userIdNoDefault, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. The dangerous-content bag lives on the operator's chat settings; the
  //    reroute arm reads `mode` + `uncensoredTextProfileId` from it.
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      timezone: 'UTC',
      dangerousContentSettings: {
        mode: 'AUTO_ROUTE',
        uncensoredTextProfileId: CONN_UNCENSORED,
      },
    } as never,
    { id: CHAT_SETTINGS, createdAt: TS, updatedAt: TS } as never,
  );

  // 3. Connection profiles — one per rung of v4's chain.
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
  const mkConn = async (
    id: string,
    name: string,
    provider: string,
    modelName: string,
    extra: object = {},
  ) =>
    repos.connections.create(
      { userId: spec.userId, name, provider, modelName, apiKeyId: APIKEY, ...extra } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  await mkConn(CONN_SEAT, 'The Seat Desk', 'OPENAI_COMPATIBLE', 'mock-model', {
    baseUrl: 'http://127.0.0.1:1/v1',
    isDangerousCompatible: false,
  });
  await mkConn(CONN_OVERRIDE, 'The Override Desk', 'OPENAI', 'gpt-4o-mini');
  await mkConn(CONN_CHAR, 'The Character Desk', 'OPENAI', 'char-default-model');
  await mkConn(CONN_INSTANCE, 'The House Desk', 'OPENAI', 'instance-default-model', {
    isDefault: true,
  });
  await mkConn(CONN_UNCENSORED, 'The Back Room', 'OPENAI', 'uncensored-model', {
    isDangerousCompatible: true,
  });
  await mkConn(CONN_SAFE_DC, 'The Open Desk', 'OPENAI', 'safe-dc-model', {
    isDangerousCompatible: true,
  });

  // 4. Characters (create owns vault provisioning).
  const mkChar = async (
    id: string,
    name: string,
    controlledBy: string,
    extra: object = {},
  ): Promise<string> => {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy, ...extra } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const raw = await repos.characters.findByIdRaw(id);
    const vault = raw?.characterDocumentMountPointId as string | null;
    if (!vault) throw new Error(`vault not minted for ${name}`);
    return vault;
  };
  const vesperVault = await mkChar(VESPER, 'Vesper', 'llm', {
    description: 'The signal-keeper on the eastern quay.',
    personality: 'Dry, exact, and slow to take offence.',
    defaultConnectionProfileId: CONN_CHAR,
    systemPrompts: [
      {
        id: SP_FIRST,
        name: 'Quay Watch',
        content: 'Speak as the keeper of the quay watch.',
        isDefault: false,
        createdAt: TS,
        updatedAt: TS,
      },
      {
        id: SP_DEFAULT,
        name: 'The Signal-Keeper',
        content: 'Speak as the signal-keeper, with the book open.',
        isDefault: true,
        createdAt: TS,
        updatedAt: TS,
      },
    ],
  });
  await mkChar(BRAM, 'Bram', 'llm', {
    description: 'A lamplighter who keeps to the far side of the river.',
  });
  await mkChar(CORMAC, 'Cormac', 'llm', { description: 'Whose vault will be broken below.' });
  await mkChar(DELL, 'Dell', 'user', { description: 'The operator’s voice at the table.' });
  await mkChar(ESME, 'Esme', 'llm', { description: 'Away on the night packet.' });

  // VESPER's vault gets a subprompt — the id is the file name without `.md`.
  {
    const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
    const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
    const { composeSubpromptContent, SUBPROMPTS_FOLDER } = await import(
      '@/lib/subprompts/subprompts'
    );
    await ensureFolderPath(vesperVault, SUBPROMPTS_FOLDER);
    await writeDatabaseDocument(
      vesperVault,
      `${SUBPROMPTS_FOLDER}/measure.md`,
      composeSubpromptContent(spec.subpromptTitle, spec.subpromptContent),
    );
  }

  // 5. The project's standing instructions, and a roleplay template.
  await repos.projects.create(
    {
      name: 'The Harbour Register',
      description: null,
      instructions: spec.projectInstructions,
      state: {},
    } as never,
    { id: PROJ, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.roleplayTemplates.create(
    {
      userId: spec.userId,
      name: 'Quay Style',
      description: 'The fixture template.',
      systemPrompt: spec.roleplayTemplatePrompt,
      isBuiltIn: false,
      tags: [],
      delimiters: [],
      renderingPatterns: [],
      dialogueDetection: null,
      narrationDelimiters: '*',
    } as never,
    { id: RT, createdAt: TS, updatedAt: TS } as never,
  );

  // 6. Taboo — instance-wide, so the section lands in every assembled prompt.
  const maindb = getRawDatabase();
  if (!maindb) throw new Error('main DB handle unavailable');
  maindb.exec(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  maindb
    .prepare('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)')
    .run('taboo', JSON.stringify({ phrases: spec.tabooPhrases }));

  // 7. The chats.
  const { IDENTITY_STACK_BUILDER_VERSION } = await import(
    '@/lib/chat/context/system-prompt-builder'
  );
  const seat = (
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
    hasHistoryAccess: true,
    createdAt: TS,
    updatedAt: TS,
    ...extra,
  });

  /** The seven seats of CHAT_STACK / CHAT_NOSTACK — one per ladder arm. */
  const fullCast = (vesperProfile: string) => [
    seat(P_VESPER, VESPER, 'llm', 0, {
      connectionProfileId: vesperProfile,
      selectedSystemPromptId: SP_FIRST,
      selectedSubpromptIds: ['measure'],
    }),
    seat(P_BRAM, BRAM, 'llm', 1),
    seat(P_CORMAC, CORMAC, 'llm', 2),
    seat(P_DELL, DELL, 'user', 3),
    seat(P_ESME, ESME, 'llm', 4, { status: 'absent' }),
    seat(P_GHOST, GHOST_CHAR, 'llm', 5),
  ];
  /** P_BRAM is deliberately ABSENT from this list — the "not impersonated" 400. */
  const impersonated = [P_VESPER, P_ESME, P_GHOST];

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Evening Bell',
      chatType: 'salon',
      projectId: PROJ,
      roleplayTemplateId: RT,
      scenarioText: spec.scenarioText,
      participants: fullCast(CONN_SEAT),
      impersonatingParticipantIds: impersonated,
      compiledIdentityStacks: {
        version: IDENTITY_STACK_BUILDER_VERSION,
        stacks: { [P_VESPER]: spec.compiledStack },
      },
      tags: [],
    } as never,
    { id: CHAT_STACK, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Evening Bell (no stack)',
      chatType: 'salon',
      projectId: PROJ,
      roleplayTemplateId: RT,
      scenarioText: spec.scenarioText,
      participants: fullCast(CONN_SEAT),
      impersonatingParticipantIds: impersonated,
      compiledIdentityStacks: null,
      tags: [],
    } as never,
    { id: CHAT_NOSTACK, createdAt: TS, updatedAt: TS } as never,
  );

  // The seat joined LATE and cannot see history; a Host status announcement
  // mid-transcript closes and reopens its presence window.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Late Watch',
      chatType: 'salon',
      projectId: null,
      scenarioText: null,
      participants: [
        seat(P_VESPER, VESPER, 'llm', 0, {
          connectionProfileId: CONN_SEAT,
          hasHistoryAccess: false,
          createdAt: '2026-05-01T10:00:00.000Z',
        }),
        seat(P_BRAM, BRAM, 'llm', 1),
      ],
      impersonatingParticipantIds: [P_VESPER],
      compiledIdentityStacks: null,
      tags: [],
    } as never,
    { id: CHAT_NOHIST, createdAt: TS, updatedAt: TS } as never,
  );

  for (const [id, title, profile] of [
    [CHAT_UNC, 'The Back Room Bell', CONN_SEAT],
    [CHAT_UNC_OK, 'The Open Desk Bell', CONN_SAFE_DC],
  ] as Array<[string, string, string]>) {
    await repos.chats.create(
      {
        userId: spec.userId,
        title,
        chatType: 'salon',
        projectId: null,
        scenarioText: null,
        conciergeOverride: 'UNCENSORED',
        participants: [seat(P_VESPER, VESPER, 'llm', 0, { connectionProfileId: profile })],
        impersonatingParticipantIds: [P_VESPER],
        compiledIdentityStacks: null,
        tags: [],
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // The profile-chain chat: NEITHER seat carries a `connectionProfileId`, so
  // VESPER falls to her character's `defaultConnectionProfileId` (CONN_CHAR) and
  // BRAM — who has none — falls to the instance default (CONN_INSTANCE), or to
  // v4's `No connection profile to rewrite with` when the session user is
  // NO_DEFAULT_USER. Three rungs of the chain, one chat.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Unassigned Desk',
      chatType: 'salon',
      projectId: null,
      scenarioText: null,
      participants: [
        seat(P_VESPER, VESPER, 'llm', 0),
        seat(P_BRAM, BRAM, 'llm', 1),
      ],
      impersonatingParticipantIds: [P_VESPER, P_BRAM],
      compiledIdentityStacks: null,
      tags: [],
    } as never,
    { id: CHAT_NOPROF, createdAt: TS, updatedAt: TS } as never,
  );

  // 8. The transcripts. SIXTEEN played messages on the two full chats, so the
  //    `slice(-12)` drops four and the window is measurable; plus the three
  //    shapes the played filter must drop.
  const add = async (chatId: string, event: object) =>
    repos.chats.addMessage(chatId, event as never);
  const minute = (n: number) =>
    new Date(Date.parse('2026-05-01T10:00:00.000Z') + n * 60_000).toISOString();

  const lines: Array<[string, string, string]> = [
    [P_DELL, 'USER', 'The lamps on the eastern quay are out again.'],
    [P_VESPER, 'ASSISTANT', 'They are. The oil came short in the autumn order.'],
    [P_BRAM, 'ASSISTANT', 'I walked the row at the second bell. Four dark, two guttering.'],
    [P_DELL, 'USER', 'And the ledger says what, exactly?'],
    [P_VESPER, 'ASSISTANT', 'That the order was filled. The ledger is wrong.'],
    [P_BRAM, 'ASSISTANT', 'Ledgers usually are, in my experience of them.'],
    [P_DELL, 'USER', 'Then who signed for it?'],
    [P_VESPER, 'ASSISTANT', 'A hand I do not know. I have the page.'],
    [P_BRAM, 'ASSISTANT', 'Bring it to the watch-house and we will read it together.'],
    [P_DELL, 'USER', 'Before or after the fog comes in?'],
    [P_VESPER, 'ASSISTANT', 'Before, if the packet is late. After, if it is not.'],
    [P_BRAM, 'ASSISTANT', 'The packet is always late.'],
    [P_DELL, 'USER', 'Then before.'],
    [P_VESPER, 'ASSISTANT', 'Before.'],
    [P_BRAM, 'ASSISTANT', 'I will set the spare lamps out on the way.'],
    [P_DELL, 'USER', 'Do. And say nothing to the harbourmaster yet.'],
  ];

  for (const chatId of [CHAT_STACK, CHAT_NOSTACK]) {
    let n = 0;
    for (const [participantId, role, content] of lines) {
      await add(chatId, {
        type: 'message',
        id: `f1000000-0000-4000-8000-${String(n).padStart(12, '0')}`.replace(
          /^f1/,
          chatId === CHAT_STACK ? 'f1' : 'f2',
        ),
        role,
        content,
        participantId,
        createdAt: minute(n),
        attachments: [],
      });
      n += 1;
    }
    // The three shapes the played filter must drop, interleaved at the tail so
    // a filter that let one through would change the window's contents.
    await add(chatId, {
      type: 'message',
      id: `${chatId === CHAT_STACK ? 'f1' : 'f2'}000000-0000-4000-8000-0000000000a1`,
      role: 'ASSISTANT',
      // Bram → Dell. NOT Vesper's own whisper and not aimed at her, so the
      // whisper filter must drop it. (A whisper she SENT would be visible to
      // her, and the arm would prove nothing.)
      content: 'A word for Dell alone: the harbourmaster already knows.',
      participantId: P_BRAM,
      targetParticipantIds: [P_DELL],
      createdAt: minute(n),
      attachments: [],
    });
    await add(chatId, {
      type: 'message',
      id: `${chatId === CHAT_STACK ? 'f1' : 'f2'}000000-0000-4000-8000-0000000000a2`,
      role: 'ASSISTANT',
      content: 'The Host notes the hour.',
      participantId: null,
      systemSender: 'host',
      createdAt: minute(n + 1),
      attachments: [],
    });
    await add(chatId, {
      type: 'message',
      id: `${chatId === CHAT_STACK ? 'f1' : 'f2'}000000-0000-4000-8000-0000000000a3`,
      role: 'USER',
      content: '   ',
      participantId: P_DELL,
      createdAt: minute(n + 2),
      attachments: [],
    });
  }

  // CHAT_NOHIST: six played messages either side of a Host status announcement
  // that marks VESPER absent, then present again. The announcement is itself a
  // whisper (no `targetParticipantIds` reaches the played subset anyway,
  // because it carries `systemSender`), which is exactly why v4 computes the
  // presence windows from the FULL event list.
  {
    // BEFORE the first window opens (dropped), inside it (kept), in the gap
    // (dropped), inside the second (kept) — four verdicts, so a dropped
    // presence leg cannot pass by accident.
    const nh: Array<[string, string, string, number]> = [
      [P_BRAM, 'ASSISTANT', 'The tide is out and the boards are showing.', 0],
      [P_BRAM, 'ASSISTANT', 'I counted eleven mooring rings, two of them loose.', 2],
      [P_VESPER, 'ASSISTANT', 'Then we will want the smith before the week is out.', 3],
      [P_BRAM, 'ASSISTANT', 'He is at the far yard until Thursday.', 6],
      [P_BRAM, 'ASSISTANT', 'Nobody has touched the rings since.', 10],
      [P_VESPER, 'ASSISTANT', 'I am back, and the smith is with me.', 11],
    ];
    for (const [participantId, role, content, n] of nh) {
      await add(CHAT_NOHIST, {
        type: 'message',
        id: `f3000000-0000-4000-8000-${String(n).padStart(12, '0')}`,
        role,
        content,
        participantId,
        createdAt: minute(n),
        attachments: [],
      });
    }
    // The FIRST event OPENS a window — without it v4's walk opens none before
    // the first announcement and everything earlier is dropped, which makes the
    // leg look right for the wrong reason (measured on the first regen).
    await add(CHAT_NOHIST, {
      type: 'message',
      id: 'f3000000-0000-4000-8000-0000000000b0',
      role: 'ASSISTANT',
      content: 'Vesper takes the quay watch.',
      participantId: null,
      systemSender: 'host',
      hostEvent: { participantId: P_VESPER, toStatus: 'active' },
      createdAt: minute(1),
      attachments: [],
    });
    await add(CHAT_NOHIST, {
      type: 'message',
      id: 'f3000000-0000-4000-8000-0000000000b1',
      role: 'ASSISTANT',
      content: 'Vesper steps away from the quay.',
      participantId: null,
      systemSender: 'host',
      hostEvent: { participantId: P_VESPER, toStatus: 'absent' },
      createdAt: minute(5),
      attachments: [],
    });
    await add(CHAT_NOHIST, {
      type: 'message',
      id: 'f3000000-0000-4000-8000-0000000000b2',
      role: 'ASSISTANT',
      content: 'Vesper returns to the quay.',
      participantId: null,
      systemSender: 'host',
      hostEvent: { participantId: P_VESPER, toStatus: 'active' },
      createdAt: minute(9),
      attachments: [],
    });
    // A line from the stretch VESPER was away for — the presence filter must
    // drop it, and it is the ONLY thing that makes that leg measurable.
    await add(CHAT_NOHIST, {
      type: 'message',
      id: 'f3000000-0000-4000-8000-0000000000b3',
      role: 'ASSISTANT',
      content: 'While she was gone the packet came in early, for once.',
      participantId: P_BRAM,
      createdAt: minute(7),
      attachments: [],
    });
  }

  for (const [id, tag] of [
    [CHAT_UNC, 'f4'],
    [CHAT_UNC_OK, 'f5'],
    [CHAT_NOPROF, 'f6'],
  ] as Array<[string, string]>) {
    await add(id, {
      type: 'message',
      id: `${tag}000000-0000-4000-8000-000000000001`,
      role: 'USER',
      content: 'Say the part you were not going to say.',
      participantId: null,
      createdAt: minute(0),
      attachments: [],
    });
  }

  // 9. VESPER's memories (the Commonplace recall). No embeddings are seeded, so
  //    `searchMemoriesSemantic` falls through its empty vector pool to the
  //    deterministic text path on BOTH sides.
  for (const m of spec.memories) {
    await repos.memories.create(
      {
        characterId: VESPER,
        aboutCharacterId: null,
        chatId: null,
        projectId: null,
        content: m.content,
        summary: m.summary,
        keywords: [],
        tags: [],
        importance: m.importance,
        embedding: null,
        source: 'MANUAL',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: m.importance,
      } as never,
      { id: m.id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 10. Break CORMAC's vault LAST, so nothing above trips over it.
  maindb
    .prepare('UPDATE "characters" SET "characterDocumentMountPointId" = ? WHERE "id" = ?')
    .run(DEAD_MP, CORMAC);

  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');

  // 11. The one MINTED value both differential sides need: VESPER's vault id
  //     (the subprompt lives under it) — echoed to the sidecar, the
  //     `chat-admin-web` precedent.
  const meta = { vesperVault };

  closeMountIndexSQLiteClient();
  await closeDatabase();

  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta, null, 2) + '\n');
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }
  process.stderr.write(`built in-scene-voiced fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`in-scene-voiced fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
