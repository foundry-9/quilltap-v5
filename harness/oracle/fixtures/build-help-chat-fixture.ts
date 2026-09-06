/**
 * The committed Help/HelpChat server-surface fixture builder (P4.9I2A
 * deliverable) — the test-pepper substrate for the help-server differentials
 * (`help_docs_routes_equivalence`, `help_chats_routes_equivalence`, and the
 * tier-3 `help_chat_orchestrator_tier3_equivalence`).
 *
 * Bakes, via v4's REAL repositories (all ids/timestamps pinned so both
 * differential sides read identical rows — reads diff with no remap; only the
 * create/rename mutations and sent messages mint fresh ids/stamps, blanked by
 * the harness):
 *   - THREE users: USER_A (the fixture owner), USER_B (the ownership/404 arm),
 *     USER_C (a user with NO tool-capable profile and no chat_settings row),
 *   - TWO api keys + SIX connection profiles, on a fictional model absent from
 *     FALLBACK_PRICING so v4's `checkModelSupportsTools` answers `true`
 *     deterministically (no pricing fetch). Five are ANTHROPIC — P1
 *     default/tool capable, P2 `allowToolUse: false`, P3 `pseudoToolMode:
 *     'text-block'`, P4 carrying a DANGLING `apiKeyId`, PB1 (USER_B) default
 *     but tool-less — and PG1 is **GOOGLE**, the one provider whose plugin
 *     KEEPS an id-less `tool` row instead of dropping it at format time
 *     (`qtap-plugin-google/provider.ts:376`). Before PG1 that branch — the
 *     orchestrator's `keeps_idless_tool_rows` rule and the oracle's matching
 *     filter — was pinned at unit tier only (the §3 review of the `p4.9i2`
 *     unification named the corpus arm as its follow-up),
 *   - SIX characters across the three users, each with its own vault (created
 *     through `repos.characters.create`, never raw SQL), covering every
 *     `handleEligibility` branch: help-enabled with a pinned profile, help
 *     enabled with NO default profile (the "any tool-capable profile" arm),
 *     help-DISABLED, and a user whose only profile refuses tools,
 *   - TWO legacy IMAGE `files` rows linked to C1/C2 (the avatar-resolution
 *     arm: `findByLinkedTo` → tag match → `images[0]`),
 *   - ONE `chat_settings` row (USER_A only) carrying `cheapLLMSettings`,
 *   - FIFTEEN chats: TWELVE `help` chats for USER_A with distinct
 *     `updatedAt`s (the list-ordering arm) and distinct `helpPageUrl`s (the
 *     page-context arms, incl. a NULL one), ONE `help` chat for USER_B (the
 *     404/ownership arm), plus a `salon` and a `brahma` chat (the list-filter
 *     arms). H1 carries a five-row transcript (SYSTEM → USER → empty tool-turn
 *     ASSISTANT → TOOL → ASSISTANT); H2 carries three; H12 is the GOOGLE seat
 *     and carries H1's five-row shape so its id-less TOOL history row reaches
 *     the provider seam,
 *   - SEVENTEEN `help_docs` rows + their section chunks, minted by v4's REAL
 *     `syncHelpDocs()` over a scratch `help/` holding exactly the spec's
 *     `helpDocFiles` (byte-copied from the v4 checkout's `help/`), so titles,
 *     urls, contentHash and chunk text are v4's own. The sync runs with NO
 *     embedding profile in the database, so `background_jobs` stays EMPTY.
 *
 * Every pinned id and the sync's minted doc ids are written alongside the
 * databases as `help-chat-main.db.meta.json`.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_HELP_CHAT_MAIN=$V5W/crates/quilltap-web/tests/fixtures/help-chat-main.db \
 *   QT_FIXTURE_HELP_CHAT_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/help-chat-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-help-chat-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  mkdtempSync,
  mkdirSync,
  copyFileSync,
  readFileSync,
  writeFileSync,
  rmSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';

interface SpecUser {
  id: string;
  username: string;
  name: string | null;
}

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  users: { A: SpecUser; B: SpecUser; C: SpecUser };
  provider: string;
  model: string;
  helpDocFiles: string[];
}

// ── Pinned entity ids (shared verbatim with the Rust differentials) ─────────

// Api keys.
const KA = 'a0000002-0000-4000-8000-000000000001'; // USER_A
const KB = 'a0000002-0000-4000-8000-000000000002'; // USER_B
/** Deliberately NOT inserted — P4's dangling reference. */
const K_DANGLING = 'a0000002-0000-4000-8000-0000000000ff';

// Connection profiles.
const P1 = 'c0000002-0000-4000-8000-000000000001'; // USER_A default, tool-capable
const P2 = 'c0000002-0000-4000-8000-000000000002'; // USER_A allowToolUse: false
const P3 = 'c0000002-0000-4000-8000-000000000003'; // USER_A pseudoToolMode text-block
const P4 = 'c0000002-0000-4000-8000-000000000004'; // USER_A dangling apiKeyId
const PB1 = 'c0000002-0000-4000-8000-000000000011'; // USER_B default, tool-less
const PG1 = 'c0000002-0000-4000-8000-000000000005'; // USER_A GOOGLE, tool-capable

// Characters.
const C1 = 'b0000002-0000-4000-8000-000000000001'; // USER_A help-on, pinned P1
const C2 = 'b0000002-0000-4000-8000-000000000002'; // USER_A help-on, NO default profile
const C3 = 'b0000002-0000-4000-8000-000000000003'; // USER_A help-OFF
const CB1 = 'b0000002-0000-4000-8000-000000000011'; // USER_B help-on, pinned PB1 (tool-less)
const CB2 = 'b0000002-0000-4000-8000-000000000012'; // USER_B help-on, no default profile
const CC1 = 'b0000002-0000-4000-8000-000000000021'; // USER_C help-OFF

// Legacy image files.
const F1 = 'f0000002-0000-4000-8000-000000000001'; // linked to C1, carries the avatar tag
const F2 = 'f0000002-0000-4000-8000-000000000002'; // linked to C2, untagged
/**
 * The tag id on F1. v4's `handleEligibility` looks for the LITERAL string
 * `'avatar'` in `file.tags`, but `FileEntrySchema.tags` is `z.array(z.uuid())`
 * and the base repository re-validates on READ (`findByFilter` →
 * `validateSafe`), so a row carrying a non-UUID tag is silently DROPPED from
 * every read. The literal-'avatar' arm is therefore unreachable with any valid
 * row; a pinned UUID tag keeps F1 visible and both sides resolve the avatar
 * through the `images[0]` fallback, which is what v4 does on real data.
 */
const TAG_AVATAR = 'a0000003-0000-4000-8000-00000000000a';

// Chat settings (USER_A only).
const CHAT_SETTINGS_A = 'e0000002-0000-4000-8000-000000000001';

// Chats.
const CHAT_PREFIX = 'c1000002-0000-4000-8000-0000000000';
const HB1 = `${CHAT_PREFIX}21`;
const S1 = `${CHAT_PREFIX}31`;
const B1 = `${CHAT_PREFIX}32`;

/** Chat id for help chat N (1-based). */
const helpChatId = (n: number): string => `${CHAT_PREFIX}${String(n).padStart(2, '0')}`;
/**
 * Participant ids: `d0000002-0000-4000-8000-0000000000NN`, NN written in
 * DECIMAL (every value used here is also a valid pair of hex digits), where
 * NN = chatSuffix*2 for the first participant and chatSuffix*2+1 for the
 * second. HB1 (suffix 21) therefore takes 42.
 */
const participantId = (chatSuffix: number, index: number): string =>
  `d0000002-0000-4000-8000-0000000000${String(chatSuffix * 2 + index).padStart(2, '0')}`;

const MSG_PREFIX = 'd1000002-0000-4000-8000-0000000000';

/** Help-chat page urls, by chat number. `null` is H2's deliberate NULL arm. */
const HELP_PAGE_URLS: Array<string | null> = [
  '/salon/abc-123', // H1
  null, // H2
  '/files', // H3
  '/aurora', // H4
  '/settings?tab=chat', // H5
  '/salon', // H6
  '/', // H7
  '/settings?tab=chat&section=taboo', // H8
  '/aurora/new', // H9
  '/prospero/p-1', // H10
  '/aurora/xyz/edit', // H11
  '/settings?tab=providers', // H12 — the GOOGLE seat
];

/** Distinct `updatedAt` per help chat (the list-ordering arm). */
const HELP_UPDATED_AT = [
  '2026-05-03T00:00:00.000Z', // H1
  '2026-05-02T00:00:00.000Z', // H2
  '2026-05-01T00:00:00.000Z', // H3
  '2026-05-04T00:00:00.000Z', // H4
  '2026-05-05T00:00:00.000Z', // H5
  '2026-05-06T00:00:00.000Z', // H6
  '2026-05-07T00:00:00.000Z', // H7
  '2026-05-08T00:00:00.000Z', // H8
  '2026-05-09T00:00:00.000Z', // H9
  '2026-05-10T00:00:00.000Z', // H10
  '2026-05-11T00:00:00.000Z', // H11
  '2026-05-12T00:00:00.000Z', // H12
];

/** Per help chat: the characters seated, and each seat's pinned profile. */
const HELP_SEATS: Array<Array<{ characterId: string; connectionProfileId: string | null }>> = [
  [
    { characterId: C1, connectionProfileId: P1 },
    { characterId: C2, connectionProfileId: P1 },
  ], // H1
  [{ characterId: C1, connectionProfileId: P1 }], // H2
  [{ characterId: C2, connectionProfileId: null }], // H3
  [{ characterId: C1, connectionProfileId: P1 }], // H4
  [{ characterId: C1, connectionProfileId: P1 }], // H5
  [{ characterId: C1, connectionProfileId: P3 }], // H6 — text-block tool mode
  [{ characterId: C1, connectionProfileId: P1 }], // H7
  [{ characterId: C1, connectionProfileId: P1 }], // H8
  [{ characterId: C1, connectionProfileId: P1 }], // H9
  [{ characterId: C1, connectionProfileId: P1 }], // H10
  [
    { characterId: C1, connectionProfileId: P4 }, // dangling api key
    { characterId: C2, connectionProfileId: P1 },
  ], // H11
  [{ characterId: C1, connectionProfileId: PG1 }], // H12 — the GOOGLE seat
];

const CHARACTER_NAME: Record<string, string> = {
  [C1]: 'Marigold',
  [C2]: 'Thistle',
  [C3]: 'Quiet Bob',
  [CB1]: 'Rook',
  [CB2]: 'Wren',
  [CC1]: 'Ash',
};

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'help-chat-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;
  const PROVIDER = spec.provider;
  const MODEL = spec.model;
  const USER_A = spec.users.A.id;
  const USER_B = spec.users.B.id;
  const USER_C = spec.users.C.id;

  const mainOut = process.env.QT_FIXTURE_HELP_CHAT_MAIN;
  const mountOut = process.env.QT_FIXTURE_HELP_CHAT_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_HELP_CHAT_MAIN and QT_FIXTURE_HELP_CHAT_MOUNT must point at the .db files',
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  // Captured BEFORE the help-sync chdir: the v4 checkout root, whose `help/`
  // supplies the curated subset, and the meta sidecar's destination.
  const V4_ROOT = process.cwd();
  const metaOut = `${mainOut}.meta.json`;
  if (existsSync(metaOut)) rmSync(metaOut);

  const scratch = mkdtempSync(join(tmpdir(), 'qt-help-chat-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, getCollection } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { HelpDocSchema } = await import('@/lib/schemas/help-doc.types');
  const { HelpDocChunkSchema } = await import('@/lib/schemas/help-doc-chunk.types');
  const { EmbeddingStatusSchema } = await import('@/lib/schemas/embedding-job.types');
  const { BackgroundJobSchema } = await import('@/lib/schemas/job.types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  const { initializePlugins } = await import('@/lib/startup/plugin-initialization');
  await initializePlugins();

  await ensureCollection('users', UserSchema);
  await ensureCollection('api_keys', ApiKeySchema);
  await ensureCollection('characters', CharacterSchema);
  // The help-sync + eligibility surfaces read these; a missing table would make
  // the repo call RETHROW and both sides would compare identical failures.
  await ensureCollection('help_docs', HelpDocSchema);
  await ensureCollection('help_doc_chunks', HelpDocChunkSchema);
  await ensureCollection('embedding_status', EmbeddingStatusSchema);
  await ensureCollection('background_jobs', BackgroundJobSchema);

  // Materialize the standard 3-partition mount-index shape — every character
  // create mints a vault store in here.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  // `doc_mount_blobs` carries a BLOB column the schema translator doesn't
  // model; its repository creates it lazily from hand-written DDL. Trigger that
  // with a read so any vault write has somewhere to land bytes.
  const { DocMountBlobsRepository } = await import(
    '@/lib/database/repositories/doc-mount-blobs.repository'
  );
  await new DocMountBlobsRepository().findByFileId('00000000-0000-4000-8000-000000000000');

  const mainDb = getRawDatabase();
  if (!mainDb) throw new Error('main DB handle unavailable');
  // `instance_settings` is a raw key/value table (no Zod collection); the
  // system-prompt builder and the maintenance sweeps read it.
  mainDb.exec(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );

  const repos = getRepositories();

  // ── 1. Users ──────────────────────────────────────────────────────────────
  for (const u of [spec.users.A, spec.users.B, spec.users.C]) {
    await repos.users.create(
      { username: u.username, email: null, name: u.name } as never,
      { id: u.id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // ── 2. Api keys (pinned ids via a direct collection insert) ───────────────
  const apiKeyCol = await getCollection('api_keys');
  for (const [id, userId, label] of [
    [KA, USER_A, 'Help test key (A)'],
    [KB, USER_B, 'Help test key (B)'],
  ] as Array<[string, string, string]>) {
    await apiKeyCol.insertOne(
      ApiKeySchema.parse({
        id,
        userId,
        label,
        provider: PROVIDER,
        key_value: 'sk-synthetic-help-key',
        isActive: true,
        lastUsed: null,
        createdAt: TS,
        updatedAt: TS,
      }) as never,
    );
  }

  // ── 3. Connection profiles ────────────────────────────────────────────────
  const mkProfile = async (
    id: string,
    userId: string,
    name: string,
    apiKeyId: string,
    isDefault: boolean,
    extra: Record<string, unknown> = {},
  ): Promise<void> => {
    await repos.connections.create(
      {
        userId,
        name,
        provider: PROVIDER,
        modelName: MODEL,
        apiKeyId,
        isDefault,
        ...extra,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  await mkProfile(P1, USER_A, 'Primary Help', KA, true);
  await mkProfile(P2, USER_A, 'No Tools', KA, false, { allowToolUse: false });
  await mkProfile(P3, USER_A, 'Text-Block Help', KA, false, { pseudoToolMode: 'text-block' });
  // No foreign key exists on `apiKeyId`, so the dangling reference goes in
  // through the ordinary repository create — no raw-SQL escape hatch needed.
  await mkProfile(P4, USER_A, 'Dangling Key', K_DANGLING, false);
  await mkProfile(PB1, USER_B, 'B No Tools', KB, true, { allowToolUse: false });
  // The GOOGLE seat. Its plugin is the ONE that keeps an id-less `tool` row
  // (`provider.ts:376` — `if (msg.role === 'tool' || msg.toolCallId)`), so H12
  // is what makes the orchestrator's `keeps_idless_tool_rows` branch and the
  // oracle's matching filter corpus-visible.
  await mkProfile(PG1, USER_A, 'Google Help', KA, false, { provider: 'GOOGLE' });

  // ── 4. Characters (each mints its own vault store) ────────────────────────
  const mkCharacter = async (
    id: string,
    userId: string,
    fields: Record<string, unknown>,
  ): Promise<void> => {
    await repos.characters.create(
      {
        userId,
        name: CHARACTER_NAME[id],
        controlledBy: 'llm',
        talkativeness: 0.5,
        ...fields,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  await mkCharacter(C1, USER_A, {
    description: 'A brisk librarian of the Guide.',
    personality: "Brisk, warm, fond of {{user}}'s questions; {{char}} keeps answers short.",
    pronouns: { subject: 'she', object: 'her', possessive: 'hers' },
    defaultHelpToolsEnabled: true,
    defaultConnectionProfileId: P1,
  });
  await mkCharacter(C2, USER_A, {
    description: 'A patient archivist.',
    personality: 'Patient and dry.',
    defaultHelpToolsEnabled: true,
    defaultConnectionProfileId: null,
  });
  // C3 carries an EXPLICIT `false` (not an absent key) so the eligibility
  // filter's `=== true` test is measured against a stored value.
  await mkCharacter(C3, USER_A, {
    description: 'Says little.',
    defaultHelpToolsEnabled: false,
  });
  await mkCharacter(CB1, USER_B, {
    description: 'A rook of the B household.',
    defaultHelpToolsEnabled: true,
    defaultConnectionProfileId: PB1,
  });
  await mkCharacter(CB2, USER_B, {
    description: 'A wren of the B household.',
    defaultHelpToolsEnabled: true,
    defaultConnectionProfileId: null,
  });
  await mkCharacter(CC1, USER_C, {
    description: 'An ash of the C household.',
    defaultHelpToolsEnabled: false,
  });

  // ── 5. Legacy image files (the avatar-resolution arm) ─────────────────────
  await repos.files.create(
    {
      userId: USER_A,
      linkedTo: [C1],
      tags: [TAG_AVATAR],
      sha256: '1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa',
      originalFilename: 'marigold.png',
      mimeType: 'image/png',
      size: 10,
      source: 'UPLOADED',
      category: 'IMAGE',
      storageKey: `${USER_A}/marigold.png`,
    } as never,
    { id: F1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.files.create(
    {
      userId: USER_A,
      linkedTo: [C2],
      tags: [],
      sha256: '2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb',
      originalFilename: 'thistle.png',
      mimeType: 'image/png',
      size: 10,
      source: 'UPLOADED',
      category: 'IMAGE',
      storageKey: `${USER_A}/thistle.png`,
    } as never,
    { id: F2, createdAt: TS, updatedAt: TS } as never,
  );

  // ── 6. Chat settings — USER_A only (B and C deliberately have none) ───────
  await repos.chatSettings.create(
    {
      userId: USER_A,
      cheapLLMSettings: { strategy: 'PROVIDER_CHEAPEST', fallbackToLocal: false },
    } as never,
    { id: CHAT_SETTINGS_A, createdAt: TS, updatedAt: TS } as never,
  );

  // ── 7. Chats ──────────────────────────────────────────────────────────────
  // A help chat mirrors v4's `handleCreate` (app/api/v1/help-chats/route.ts).
  const mkHelpChat = async (
    id: string,
    userId: string,
    chatSuffix: number,
    seats: Array<{ characterId: string; connectionProfileId: string | null }>,
    helpPageUrl: string | null,
    updatedAt: string,
  ): Promise<string[]> => {
    const participants = seats.map((seat, i) => ({
      id: participantId(chatSuffix, i),
      type: 'CHARACTER',
      characterId: seat.characterId,
      controlledBy: 'llm',
      connectionProfileId: seat.connectionProfileId,
      imageProfileId: null,
      displayOrder: i,
      isActive: true,
      createdAt: TS,
      updatedAt: TS,
    }));
    await repos.chats.create(
      {
        userId,
        participants,
        title: `Help: ${CHARACTER_NAME[seats[0].characterId]}`,
        contextSummary: null,
        tags: [],
        messageCount: 0,
        lastMessageAt: null,
        lastRenameCheckInterchange: 0,
        projectId: null,
        disabledTools: [],
        disabledToolGroups: [],
        imageProfileId: null,
        chatType: 'help',
        helpPageUrl,
      } as never,
      { id, createdAt: TS, updatedAt } as never,
    );
    return participants.map((p) => p.id);
  };

  const helpParticipants: Record<string, string[]> = {};
  for (let n = 1; n <= 12; n++) {
    const id = helpChatId(n);
    helpParticipants[id] = await mkHelpChat(
      id,
      USER_A,
      n,
      HELP_SEATS[n - 1],
      HELP_PAGE_URLS[n - 1],
      HELP_UPDATED_AT[n - 1],
    );
  }
  helpParticipants[HB1] = await mkHelpChat(
    HB1,
    USER_B,
    21,
    [{ characterId: CB1, connectionProfileId: PB1 }],
    '/salon',
    TS,
  );

  // The two non-help chats (the list-filter arms).
  await repos.chats.create(
    {
      userId: USER_A,
      participants: [],
      title: 'An Ordinary Salon',
      contextSummary: null,
      tags: [],
      chatType: 'salon',
    } as never,
    { id: S1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.chats.create(
    {
      userId: USER_A,
      participants: [],
      title: 'A Console',
      contextSummary: null,
      tags: [],
      chatType: 'brahma',
      consoleConnectionProfileId: P1,
    } as never,
    { id: B1, createdAt: TS, updatedAt: TS } as never,
  );

  // ── 8. Transcripts (H1 five rows, H2 three) ───────────────────────────────
  const H1 = helpChatId(1);
  const H2 = helpChatId(2);
  const addMsg = async (
    chatId: string,
    id: string,
    role: string,
    content: string,
    createdAt: string,
    extras: Record<string, unknown> = {},
  ): Promise<void> => {
    await repos.chats.addMessage(chatId, {
      type: 'message',
      id,
      role,
      content,
      attachments: [],
      createdAt,
      ...extras,
    } as never);
  };
  const llmExtras = (chatId: string) => ({
    participantId: helpParticipants[chatId][0],
    provider: PROVIDER,
    modelName: MODEL,
  });

  await addMsg(
    H1,
    `${MSG_PREFIX}01`,
    'SYSTEM',
    'Help chat initiated for page: /salon/abc-123',
    '2026-05-01T00:00:01.000Z',
  );
  await addMsg(
    H1,
    `${MSG_PREFIX}02`,
    'USER',
    'How do I add a character?',
    '2026-05-01T00:00:02.000Z',
  );
  await addMsg(H1, `${MSG_PREFIX}03`, 'ASSISTANT', '', '2026-05-01T00:00:03.000Z', llmExtras(H1));
  await addMsg(
    H1,
    `${MSG_PREFIX}04`,
    'TOOL',
    JSON.stringify({
      toolName: 'help_search',
      success: true,
      result: 'Found: Characters (/aurora)',
      arguments: { query: 'add a character' },
      callId: 'call_prior',
    }),
    '2026-05-01T00:00:04.000Z',
  );
  await addMsg(
    H1,
    `${MSG_PREFIX}05`,
    'ASSISTANT',
    'Use Aurora to add a character.',
    '2026-05-01T00:00:05.000Z',
    llmExtras(H1),
  );

  await addMsg(
    H2,
    `${MSG_PREFIX}11`,
    'SYSTEM',
    'Help chat initiated for page: /',
    '2026-05-01T00:00:01.000Z',
  );
  await addMsg(H2, `${MSG_PREFIX}12`, 'USER', 'hi', '2026-05-01T00:00:02.000Z');
  await addMsg(
    H2,
    `${MSG_PREFIX}13`,
    'ASSISTANT',
    'hello',
    '2026-05-01T00:00:03.000Z',
    llmExtras(H2),
  );

  // H12 — the GOOGLE seat. It carries H1's five-row shape (SYSTEM → USER →
  // empty tool-turn ASSISTANT → TOOL → ASSISTANT) so its persisted TOOL row
  // arrives as an ID-LESS `tool` message: the ONE shape whose treatment differs
  // by provider (nine plugins drop it, GOOGLE keeps it and emits an
  // `unknown_function` `functionResponse`).
  const H12 = helpChatId(12);
  await addMsg(
    H12,
    `${MSG_PREFIX}21`,
    'SYSTEM',
    'Help chat initiated for page: /settings?tab=providers',
    '2026-05-01T00:00:01.000Z',
  );
  await addMsg(
    H12,
    `${MSG_PREFIX}22`,
    'USER',
    'Which providers can I use?',
    '2026-05-01T00:00:02.000Z',
  );
  await addMsg(H12, `${MSG_PREFIX}23`, 'ASSISTANT', '', '2026-05-01T00:00:03.000Z', llmExtras(H12));
  await addMsg(
    H12,
    `${MSG_PREFIX}24`,
    'TOOL',
    JSON.stringify({
      toolName: 'help_search',
      success: true,
      result: 'Found: Providers (/settings?tab=providers)',
      arguments: { query: 'providers' },
      callId: 'call_prior_google',
    }),
    '2026-05-01T00:00:04.000Z',
  );
  await addMsg(
    H12,
    `${MSG_PREFIX}25`,
    'ASSISTANT',
    'Settings holds the provider list.',
    '2026-05-01T00:00:05.000Z',
    llmExtras(H12),
  );

  // `addMessage` stamps the chat row's `updatedAt`/`lastMessageAt` with the
  // WALL CLOCK, so restore the pinned values after the transcripts land.
  await repos.chats.update(H1, {
    updatedAt: HELP_UPDATED_AT[0],
    lastMessageAt: '2026-05-01T00:00:05.000Z',
  } as never);
  await repos.chats.update(H2, {
    updatedAt: HELP_UPDATED_AT[1],
    lastMessageAt: '2026-05-01T00:00:03.000Z',
  } as never);
  await repos.chats.update(H12, {
    updatedAt: HELP_UPDATED_AT[11],
    lastMessageAt: '2026-05-01T00:00:05.000Z',
  } as never);

  // ── 9. Help docs — v4's REAL syncHelpDocs over the curated subset ─────────
  // `HELP_DIR` is `join(process.cwd(), 'help')` captured at MODULE LOAD, so the
  // sync module must be imported AFTER the chdir. Every other import is already
  // resolved above, and the alias map is absolute, so nothing else moves.
  const helpDir = join(scratch, 'help');
  mkdirSync(helpDir, { recursive: true });
  for (const file of spec.helpDocFiles) {
    const src = join(V4_ROOT, 'help', file);
    if (!existsSync(src)) throw new Error(`help doc missing from the v4 checkout: ${src}`);
    copyFileSync(src, join(helpDir, file));
  }

  process.chdir(scratch);
  let syncResult: unknown;
  try {
    const { syncHelpDocs } = await import('@/lib/help/help-doc-sync');
    syncResult = await syncHelpDocs();
  } finally {
    process.chdir(V4_ROOT);
  }

  const helpDocRows = mainDb
    .prepare('SELECT "id", "path", "title", "url" FROM "help_docs" ORDER BY rowid')
    .all() as Array<{ id: string; path: string; title: string; url: string }>;
  if (helpDocRows.length !== spec.helpDocFiles.length) {
    throw new Error(
      `expected ${spec.helpDocFiles.length} help_docs rows, got ${helpDocRows.length}`,
    );
  }
  const chunkCount = (
    mainDb.prepare('SELECT COUNT(*) AS n FROM "help_doc_chunks"').get() as { n: number }
  ).n;
  if (chunkCount === 0) throw new Error('syncHelpDocs wrote no help_doc_chunks rows');
  const jobCount = (
    mainDb.prepare('SELECT COUNT(*) AS n FROM "background_jobs"').get() as { n: number }
  ).n;
  if (jobCount !== 0) {
    throw new Error(`background_jobs must be empty (no embedding profile), got ${jobCount}`);
  }

  // ── 10. The meta sidecar ──────────────────────────────────────────────────
  const meta = {
    note:
      'Companion to help-chat-main.db / help-chat-mount.db, written by ' +
      'harness/oracle/fixtures/build-help-chat-fixture.ts. Every id below is ' +
      'PINNED in the builder except the help_docs ids, which are minted by ' +
      "v4's real syncHelpDocs and baked into the committed database.",
    spec: 'harness/oracle/fixtures/help-chat-web.json',
    seedTimestamp: TS,
    provider: PROVIDER,
    model: `${MODEL} (absent from FALLBACK_PRICING → checkModelSupportsTools true)`,
    users: {
      [USER_A]: 'USER_A "friday" — owns every profile/character/chat below unless noted',
      [USER_B]: 'USER_B "saturday" — the ownership/404 arm; only a tool-LESS default profile',
      [USER_C]: 'USER_C "sunday" (name null) — no api key, no profile, no chat_settings',
    },
    apiKeys: {
      [KA]: 'KA — USER_A, ANTHROPIC, carried by P1/P2/P3',
      [KB]: 'KB — USER_B, ANTHROPIC, carried by PB1',
      [K_DANGLING]: 'NOT INSERTED — P4 references this id deliberately (no FK exists)',
    },
    connectionProfiles: {
      [P1]: 'P1 — USER_A "Primary Help", isDefault, tool-capable',
      [P2]: 'P2 — USER_A "No Tools", allowToolUse:false (excluded from toolCapableProfiles)',
      [P3]: 'P3 — USER_A "Text-Block Help", pseudoToolMode:text-block (H6 seat)',
      [P4]: 'P4 — USER_A "Dangling Key", apiKeyId points at a row that does not exist (H11 seat 0)',
      [PB1]: 'PB1 — USER_B "B No Tools", isDefault but allowToolUse:false',
      [PG1]: 'PG1 — USER_A "Google Help", provider GOOGLE, tool-capable (H12 seat) — the one plugin that KEEPS an id-less tool row',
    },
    characters: {
      [C1]: 'C1 Marigold — USER_A, defaultHelpToolsEnabled true, defaultConnectionProfileId P1, pronouns she/her/hers, no avatarUrl',
      [C2]: 'C2 Thistle — USER_A, defaultHelpToolsEnabled true, defaultConnectionProfileId NULL (the any-tool-capable-profile arm)',
      [C3]: 'C3 Quiet Bob — USER_A, defaultHelpToolsEnabled EXPLICIT false, no default profile',
      [CB1]: 'CB1 Rook — USER_B, defaultHelpToolsEnabled true, defaultConnectionProfileId PB1 (tool-less → hasToolCapableProfile false)',
      [CB2]: 'CB2 Wren — USER_B, defaultHelpToolsEnabled true, no default profile',
      [CC1]: 'CC1 Ash — USER_C, defaultHelpToolsEnabled EXPLICIT false',
    },
    files: {
      [F1]: `F1 marigold.png (image/png, 10 bytes, IMAGE/UPLOADED) linkedTo [C1], tags [${TAG_AVATAR}]`,
      [F2]: 'F2 thistle.png (image/png, 10 bytes, IMAGE/UPLOADED) linkedTo [C2], tags []',
      avatarTagNote:
        "v4's handleEligibility looks for the LITERAL string 'avatar' in file.tags, but " +
        'FileEntrySchema.tags is z.array(z.uuid()) and the base repository re-validates on ' +
        'READ, so a row with a non-UUID tag is dropped from every read. The literal arm is ' +
        'unreachable with valid data; F1 carries a pinned UUID tag instead and the avatar ' +
        'resolves through the images[0] fallback, exactly as it does on real data.',
    },
    chatSettings: {
      [CHAT_SETTINGS_A]: `USER_A only — cheapLLMSettings { strategy: PROVIDER_CHEAPEST, fallbackToLocal: false }; USER_B and USER_C have NO row`,
    },
    chats: {
      ...Object.fromEntries(
        Array.from({ length: 12 }, (_, i) => {
          const n = i + 1;
          const id = helpChatId(n);
          const seats = HELP_SEATS[i]
            .map(
              (s, si) =>
                `${CHARACTER_NAME[s.characterId]}(${s.characterId}) profile=${s.connectionProfileId ?? 'null'} participant=${helpParticipants[id][si]}`,
            )
            .join(' | ');
          const history =
            n === 1 || n === 12
              ? ' | 5 messages'
              : n === 2
                ? ' | 3 messages'
                : ' | no history';
          return [
            id,
            `H${n} — USER_A, chatType help, helpPageUrl ${HELP_PAGE_URLS[i] ?? 'NULL'}, updatedAt ${HELP_UPDATED_AT[i]}, seats: ${seats}${history}`,
          ];
        }),
      ),
      [HB1]: `HB1 — USER_B, chatType help, helpPageUrl /salon, updatedAt ${TS}, seat Rook(${CB1}) profile=${PB1} participant=${helpParticipants[HB1][0]}, no history`,
      [S1]: `S1 — USER_A, chatType salon, title "An Ordinary Salon", no participants (list-filter arm)`,
      [B1]: `B1 — USER_A, chatType brahma, title "A Console", consoleConnectionProfileId ${P1} (list-filter arm)`,
    },
    messages: {
      [`${MSG_PREFIX}01`]: 'H1 #1 SYSTEM "Help chat initiated for page: /salon/abc-123" @00:00:01Z',
      [`${MSG_PREFIX}02`]: 'H1 #2 USER "How do I add a character?" @00:00:02Z',
      [`${MSG_PREFIX}03`]: `H1 #3 ASSISTANT "" (empty tool-turn) participantId ${helpParticipants[H1][0]} @00:00:03Z`,
      [`${MSG_PREFIX}04`]: 'H1 #4 TOOL help_search result JSON @00:00:04Z',
      [`${MSG_PREFIX}05`]: `H1 #5 ASSISTANT "Use Aurora to add a character." participantId ${helpParticipants[H1][0]} @00:00:05Z`,
      [`${MSG_PREFIX}11`]: 'H2 #1 SYSTEM "Help chat initiated for page: /" @00:00:01Z',
      [`${MSG_PREFIX}12`]: 'H2 #2 USER "hi" @00:00:02Z',
      [`${MSG_PREFIX}13`]: `H2 #3 ASSISTANT "hello" participantId ${helpParticipants[H2][0]} @00:00:03Z`,
      [`${MSG_PREFIX}21`]: 'H12 #1 SYSTEM "Help chat initiated for page: /settings?tab=providers" @00:00:01Z',
      [`${MSG_PREFIX}22`]: 'H12 #2 USER "Which providers can I use?" @00:00:02Z',
      [`${MSG_PREFIX}23`]: `H12 #3 ASSISTANT "" (empty tool-turn) participantId ${helpParticipants[H12][0]} @00:00:03Z`,
      [`${MSG_PREFIX}24`]: 'H12 #4 TOOL help_search result JSON (id-less on read — the GOOGLE-keeps arm) @00:00:04Z',
      [`${MSG_PREFIX}25`]: `H12 #5 ASSISTANT "Settings holds the provider list." participantId ${helpParticipants[H12][0]} @00:00:05Z`,
    },
    helpDocs: {
      note:
        'Minted by v4\'s real syncHelpDocs() over a scratch help/ holding exactly the ' +
        'spec\'s helpDocFiles, byte-copied from the v4 checkout. No embedding profile ' +
        'existed at sync time, so background_jobs is EMPTY and no doc/chunk carries an ' +
        'embedding.',
      syncResult,
      chunkCount,
      byPath: Object.fromEntries(
        helpDocRows.map((r) => [r.path, { id: r.id, title: r.title, url: r.url }]),
      ),
    },
  };
  writeFileSync(metaOut, `${JSON.stringify(meta, null, 2)}\n`);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  // TRUNCATE journalling leaves a zero-byte `-journal` beside each database
  // once the last transaction commits; sweep them so only the two .db files
  // (and the meta sidecar) are left to commit.
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  process.stderr.write(
    `built help-chat fixture: main=${mainOut} mount=${mountOut} meta=${metaOut} ` +
      `(3 users, 6 profiles, 6 characters, 2 files, 15 chats, ` +
      `${helpDocRows.length} help_docs / ${chunkCount} chunks, 0 jobs)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`help-chat fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
