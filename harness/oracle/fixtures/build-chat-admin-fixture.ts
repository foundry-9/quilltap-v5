/**
 * The committed chat-admin + tools server-surface fixture builder (P4.9E3A
 * deliverable) — the shared test-pepper substrate for
 * `chat_admin_routes_equivalence` (tier 2) and
 * `chat_regenerate_title_tier3_equivalence` (tier 3).
 *
 * Baked entirely through v4's REAL repositories (never hand-rolled SQL —
 * `project-store-fixture-needs-real-create`), with every id and timestamp pinned
 * so both differential sides read byte-identical rows; only the mutating cases
 * mint fresh ids, which the harness blanks.
 *
 * ## What is in it, and which arm each row exists for
 *
 *   1. ONE user + an api key + TWO connection profiles. `CONN` is the user's
 *      DEFAULT (`isDefault: true`) — `resolveMergedProfileId`'s middle tier — and
 *      `CONN_B` is the profile BEA carries in the source chat, so the merge's
 *      "prefer the source profile when it still resolves" branch has a live
 *      counterexample against the default.
 *   2. SIX characters, each vault provisioned by v4's own `characters.create`:
 *        - ARIA   (llm)  — TARGET cast. The `run-tool` execution context's
 *                          character (first active CHARACTER participant), and
 *                          the bulk-reattribute SOURCE participant.
 *        - BEA    (llm)  — SOURCE only → merges in. Carries TAG_C, so the
 *                          merge's tag fold has something to fold. Has a
 *                          `isDefault` wardrobe item (outfit mode `default`) AND
 *                          an equipped outfit in the SOURCE chat (mode
 *                          `previous_chat`, the merge's own default).
 *        - CLIO   (llm)  — in BOTH chats → the `skippedAlreadyPresent` arm.
 *        - DORIAN (llm)  — SOURCE only, the second merge candidate, so the
 *                          `characterIds` allowlist gate can exclude someone.
 *                          Carries TAG_C + TAG_D.
 *        - EVE    (user) — the operator's played character in TARGET (the
 *                          bulk-reattribute TARGET participant).
 *        - FLINT  (llm)  — SOURCE only, participant `status: 'removed'` → the
 *                          `p.status === 'removed'` filter's counterexample.
 *   3. FOUR tags: TAG_A already on the target chat, TAG_B free (the `add-tag`
 *      subject and, being already-absent, the `remove-tag` no-op subject),
 *      TAG_C on BEA+DORIAN and TAG_D on DORIAN (the merge tag fold).
 *   4. TARGET chat (`CHAT`) — participants EVE(user) / ARIA(llm) / CLIO(llm);
 *      `tags: [TAG_A]`; a PARTIAL `timestampConfig` (the P4.d18 normalization
 *      divergence lives here — deliberately NOT ported this round, see the order);
 *      seven messages spanning both roles across ARIA / CLIO / unattributed, plus
 *      one `system` event so the bulk rewrite must preserve a non-message row.
 *   5. SOURCE chat (`SOURCE_CHAT`) — BEA / CLIO / DORIAN / FLINT(removed), a
 *      rolling `contextSummary` (the recap bubble's body) and its own two
 *      messages.
 *   6. `EMPTY_CHAT` — no messages: `regenerate-title`'s "no messages" 400.
 *      `HELP_CHAT` — `chatType: 'help'`: the `titleHelpChat` arm.
 *   7. TWO memories whose `sourceMessageId` points at TARGET messages attributed
 *      to ARIA — bulk-reattribute deletes exactly these, and a third memory with
 *      NO source message proves the delete is scoped.
 *   8. Chat settings for the user (`cheapLLMSettings`) — `regenerate-title`'s
 *      gate and `buildCheapLLMConfig`'s source.
 *
 * No row lives in the llm-logs partition; the empty `background_jobs` table is
 * materialized so both sides' fresh copies share a schema (the enqueueing verbs
 * write into it).
 *
 * The minted character-vault ids are echoed to a JSON sidecar
 * (QT_FIXTURE_CA_MAIN + '.meta.json') that the oracle case and the Rust harness
 * both read.
 *
 * Regenerate (Node 24, from the v4 checkout) — the committed .db files land in
 * place:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   TZ=UTC \
 *   QT_FIXTURE_CA_MAIN=$W/crates/quilltap-web/tests/fixtures/chat-admin-main.db \
 *   QT_FIXTURE_CA_MOUNT=$W/crates/quilltap-web/tests/fixtures/chat-admin-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-chat-admin-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

// ── Pinned entity ids (shared verbatim with the oracle case + the Rust tests) ──
const APIKEY = 'a0000001-0000-4000-8000-000000000001';
const CONN = 'c0000001-0000-4000-8000-000000000001'; // the user's DEFAULT profile
const CONN_B = 'c0000002-0000-4000-8000-000000000002'; // BEA's source-chat profile

const ARIA = 'a1000000-0000-4000-8000-000000000001';
const BEA = 'a1000000-0000-4000-8000-000000000002';
const CLIO = 'a1000000-0000-4000-8000-000000000003';
const DORIAN = 'a1000000-0000-4000-8000-000000000004';
const EVE = 'a1000000-0000-4000-8000-000000000005';
const FLINT = 'a1000000-0000-4000-8000-000000000006';

const TAG_A = 'b1000000-0000-4000-8000-000000000001'; // already on the target chat
const TAG_B = 'b1000000-0000-4000-8000-000000000002'; // free (add-tag subject)
const TAG_C = 'b1000000-0000-4000-8000-000000000003'; // on BEA + DORIAN
const TAG_D = 'b1000000-0000-4000-8000-000000000004'; // on DORIAN only

const CHAT = 'c1000000-0000-4000-8000-000000000001'; // the merge TARGET
const SOURCE_CHAT = 'c1000000-0000-4000-8000-000000000002'; // the merge SOURCE
const EMPTY_CHAT = 'c1000000-0000-4000-8000-000000000003'; // no messages
const HELP_CHAT = 'c1000000-0000-4000-8000-000000000004'; // chatType 'help'

const P_EVE = 'e1000000-0000-4000-8000-000000000001';
const P_ARIA = 'e1000000-0000-4000-8000-000000000002';
const P_CLIO = 'e1000000-0000-4000-8000-000000000003';
const P_BEA = 'e1000000-0000-4000-8000-000000000011';
const P_CLIO_SRC = 'e1000000-0000-4000-8000-000000000012';
const P_DORIAN = 'e1000000-0000-4000-8000-000000000013';
const P_FLINT = 'e1000000-0000-4000-8000-000000000014';
const P_HELP = 'e1000000-0000-4000-8000-000000000021';

// TARGET messages. `role` × `participantId` is deliberately crossed so the three
// `roleFilter` values each select a DIFFERENT subset of ARIA's messages.
const M1 = 'd1000000-0000-4000-8000-000000000001'; // USER      / EVE
const M2 = 'd1000000-0000-4000-8000-000000000002'; // ASSISTANT / ARIA  (has a memory)
const M3 = 'd1000000-0000-4000-8000-000000000003'; // ASSISTANT / ARIA  (has a memory)
const M4 = 'd1000000-0000-4000-8000-000000000004'; // USER      / ARIA
const M5 = 'd1000000-0000-4000-8000-000000000005'; // ASSISTANT / CLIO
const M6 = 'd1000000-0000-4000-8000-000000000006'; // USER      / (unattributed)
const M7 = 'd1000000-0000-4000-8000-000000000007'; // ASSISTANT / (unattributed)
const M_SYS = 'd1000000-0000-4000-8000-000000000008'; // a `system` event
const M_SRC1 = 'd2000000-0000-4000-8000-000000000001';
const M_SRC2 = 'd2000000-0000-4000-8000-000000000002';

const MEM_M2 = 'f0000000-0000-4000-8000-000000000001'; // sourceMessageId = M2
const MEM_M3 = 'f0000000-0000-4000-8000-000000000002'; // sourceMessageId = M3
const MEM_FREE = 'f0000000-0000-4000-8000-000000000003'; // no source message

const W_BEA_DEFAULT = '00000000-0000-4000-8000-0000000000b1';
const W_BEA_SPARE = '00000000-0000-4000-8000-0000000000b2';
const W_DORIAN_DEFAULT = '00000000-0000-4000-8000-0000000000d1';
// EVE + CLIO each carry ONE distinctly-titled item so `run-tool`'s
// participant-picking is observable through `wardrobe_list` (which lists the
// CALLING character's own wardrobe). EVE is the DEFAULT pick — v4 takes the
// first active CHARACTER participant regardless of `controlledBy`, and EVE is
// the operator's played character sitting at displayOrder 0 — so the default
// pick, a named pick (CLIO) and an unmatched pick all produce different bytes.
const W_EVE_ONLY = '00000000-0000-4000-8000-0000000000e1';
const W_CLIO_ONLY = '00000000-0000-4000-8000-0000000000c1';

async function main(): Promise<void> {
  // Kept in step with the oracle case (which formats dates): build under TZ=UTC.
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`chat-admin fixture must be built under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'chat-admin-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_CA_MAIN;
  const mountOut = process.env.QT_FIXTURE_CA_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_CA_MAIN and QT_FIXTURE_CA_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ca-fixture-'));
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
  const { CharacterSchema, ChatMetadataSchema, FileEntrySchema, BackgroundJobSchema } =
    await import('@/lib/schemas/types');
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

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('files', FileEntrySchema);
  await ensureCollection('api_keys', ApiKeySchema);
  // Every enqueueing verb (reclassify-danger / render-conversation) writes here;
  // the table must exist before either side opens a copy.
  await ensureCollection('background_jobs', BackgroundJobSchema);

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

  // 1. User + api key + two connection profiles (CONN is the user DEFAULT).
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
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
      isDefault: true,
    } as never,
    { id: CONN, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'Second Desk',
      provider: 'OPENAI',
      modelName: 'gpt-4o-mini',
      apiKeyId: APIKEY,
    } as never,
    { id: CONN_B, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Tags.
  for (const [id, name] of [
    [TAG_A, 'Evening'],
    [TAG_B, 'Correspondence'],
    [TAG_C, 'Lamplighters'],
    [TAG_D, 'Far Bank'],
  ] as const) {
    await repos.tags.create(
      { userId: spec.userId, name, nameLower: name.toLowerCase(), quickHide: false } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 3. Characters (create owns vault provisioning).
  const vaults: Record<string, string> = {};
  const mkChar = async (id: string, name: string, controlledBy: string, extra: object = {}) => {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy, ...extra } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const raw = await repos.characters.findByIdRaw(id);
    const vault = raw?.characterDocumentMountPointId as string | null;
    if (!vault) throw new Error(`vault not minted for ${name}`);
    vaults[name.toLowerCase()] = vault;
    return vault;
  };
  await mkChar(ARIA, 'Aria', 'llm', {
    description: 'The Salon’s resident correspondent.',
    personality: 'Precise, wry, and fond of a well-turned postscript.',
  });
  await mkChar(BEA, 'Bea', 'llm', {
    description: 'A signal-keeper from the lower deck.',
    tags: [TAG_C],
  });
  await mkChar(CLIO, 'Clio', 'llm', { description: 'The archivist, present in both rooms.' });
  await mkChar(DORIAN, 'Dorian', 'llm', {
    description: 'A lamplighter who keeps to the far side of the river.',
    tags: [TAG_C, TAG_D],
  });
  await mkChar(EVE, 'Eve', 'user', { description: 'The operator’s voice at the table.' });
  await mkChar(FLINT, 'Flint', 'llm', { description: 'Dismissed from the lower deck.' });

  // 4. Wardrobe items — BEA + DORIAN each get an `isDefault` item so the merge's
  //    outfit modes (`default`, and `previous_chat`'s fallback) resolve to real
  //    slot contents rather than an empty set.
  const seedItem = async (
    characterId: string,
    id: string,
    title: string,
    types: string[],
    isDefault: boolean,
  ): Promise<void> => {
    await repos.wardrobe.create(
      {
        characterId,
        title,
        description: null,
        imagePrompt: null,
        types,
        componentItemIds: [],
        appropriateness: null,
        isDefault,
        replace: false,
        migratedFromClothingRecordId: null,
        archivedAt: null,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  await seedItem(BEA, W_BEA_DEFAULT, 'Signal-keeper’s coat', ['top'], true);
  await seedItem(BEA, W_BEA_SPARE, 'Deck boots', ['footwear'], false);
  await seedItem(DORIAN, W_DORIAN_DEFAULT, 'Lamplighter’s greatcoat', ['top'], true);
  await seedItem(EVE, W_EVE_ONLY, 'Operator’s travelling coat', ['top'], false);
  await seedItem(CLIO, W_CLIO_ONLY, 'Archivist’s sleeve-guards', ['accessories'], false);

  // 5. The chats.
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

  // The TARGET. `timestampConfig` is deliberately PARTIAL — v4 re-parses it
  // through `TimestampConfigSchema` at the repository write, so the stored row
  // already carries v4's materialized defaults; v5 reads the same bytes. (The
  // normalization on the UPDATE path is the P4.d18 divergence this round does
  // NOT port — see the order's ⚠ section.)
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Evening Post',
      chatType: 'salon',
      contextSummary: null,
      timestampConfig: { enabled: true, intervalMinutes: 45 },
      participants: [
        mkParticipant(P_EVE, EVE, 'user', 0),
        mkParticipant(P_ARIA, ARIA, 'llm', 1, { connectionProfileId: CONN }),
        mkParticipant(P_CLIO, CLIO, 'llm', 2),
      ],
      tags: [TAG_A],
    } as never,
    { id: CHAT, createdAt: TS, updatedAt: TS } as never,
  );

  // The SOURCE. BEA carries CONN_B so `resolveMergedProfileId`'s first tier is
  // distinguishable from the user default; DORIAN carries none, so it falls to
  // the default; FLINT is `removed`.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Lower Deck',
      chatType: 'salon',
      contextSummary:
        'Bea traced the signal failure to the eastern quay; Dorian agreed to walk the lamps at dusk.',
      participants: [
        mkParticipant(P_BEA, BEA, 'llm', 0, { connectionProfileId: CONN_B }),
        mkParticipant(P_CLIO_SRC, CLIO, 'llm', 1),
        mkParticipant(P_DORIAN, DORIAN, 'llm', 2),
        mkParticipant(P_FLINT, FLINT, 'llm', 3, { status: 'removed', removedAt: TS }),
      ],
      tags: [],
      equippedOutfit: {
        [BEA]: { top: [W_BEA_SPARE], bottom: [], footwear: [W_BEA_SPARE], accessories: [] },
      },
    } as never,
    { id: SOURCE_CHAT, createdAt: TS, updatedAt: TS } as never,
  );

  // `regenerate-title`'s "no messages in chat" 400.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'An Empty Room',
      chatType: 'salon',
      participants: [],
      tags: [],
    } as never,
    { id: EMPTY_CHAT, createdAt: TS, updatedAt: TS } as never,
  );

  // The help arm (`isHelpLikeChatType` → `titleHelpChat`).
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Help: getting started',
      chatType: 'help',
      participants: [mkParticipant(P_HELP, ARIA, 'llm', 0, { connectionProfileId: CONN })],
      tags: [],
    } as never,
    { id: HELP_CHAT, createdAt: TS, updatedAt: TS } as never,
  );

  // 6. Messages. Roles and participants are crossed so each `roleFilter` value
  //    selects a different subset of ARIA's rows:
  //      ASSISTANT → M2, M3   USER → M4   both → M2, M3, M4
  const add = async (chatId: string, event: object) =>
    repos.chats.addMessage(chatId, event as never);
  await add(CHAT, {
    type: 'message',
    id: M1,
    role: 'USER',
    content: 'The evening post is late again.',
    participantId: P_EVE,
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
  await add(CHAT, {
    type: 'message',
    id: M3,
    role: 'ASSISTANT',
    content: 'She reaches for the ledger. "I shall write to the quay myself."',
    participantId: P_ARIA,
    createdAt: '2026-05-01T10:02:00.000Z',
    attachments: [],
  });
  await add(CHAT, {
    type: 'message',
    id: M4,
    role: 'USER',
    content: 'A line spoken in Aria’s voice by the operator.',
    participantId: P_ARIA,
    createdAt: '2026-05-01T10:03:00.000Z',
    attachments: [],
  });
  await add(CHAT, {
    type: 'message',
    id: M5,
    role: 'ASSISTANT',
    content: 'Clio, without looking up: "The ledger is three winters out of date."',
    participantId: P_CLIO,
    createdAt: '2026-05-01T10:04:00.000Z',
    attachments: [],
  });
  await add(CHAT, {
    type: 'message',
    id: M6,
    role: 'USER',
    content: 'An unattributed aside from the floor.',
    participantId: null,
    createdAt: '2026-05-01T10:05:00.000Z',
    attachments: [],
  });
  await add(CHAT, {
    type: 'message',
    id: M7,
    role: 'ASSISTANT',
    content: 'An unattributed reply, author unknown.',
    participantId: null,
    createdAt: '2026-05-01T10:06:00.000Z',
    attachments: [],
  });
  // A non-`message` row the bulk rewrite must carry through untouched.
  await add(CHAT, {
    type: 'system',
    id: M_SYS,
    systemEventType: 'CONTEXT_SUMMARY',
    description: 'The Librarian folded the thread.',
    createdAt: '2026-05-01T10:07:00.000Z',
  });

  await add(SOURCE_CHAT, {
    type: 'message',
    id: M_SRC1,
    role: 'USER',
    content: 'What went dark on the eastern quay?',
    participantId: null,
    createdAt: '2026-05-01T09:00:00.000Z',
    attachments: [],
  });
  await add(SOURCE_CHAT, {
    type: 'message',
    id: M_SRC2,
    role: 'ASSISTANT',
    content: 'Bea: "Three lamps, and the signal with them."',
    participantId: P_BEA,
    createdAt: '2026-05-01T09:01:00.000Z',
    attachments: [],
  });
  await add(HELP_CHAT, {
    type: 'message',
    id: 'd3000000-0000-4000-8000-000000000001',
    role: 'USER',
    content: 'How do I add a second connection profile?',
    participantId: null,
    createdAt: '2026-05-01T11:00:00.000Z',
    attachments: [],
  });
  await add(HELP_CHAT, {
    type: 'message',
    id: 'd3000000-0000-4000-8000-000000000002',
    role: 'ASSISTANT',
    content: 'Open Settings → Connections and press Add Profile.',
    participantId: P_HELP,
    createdAt: '2026-05-01T11:01:00.000Z',
    attachments: [],
  });

  // Re-pin `updatedAt` — every `addMessage` bumps it.
  for (const id of [CHAT, SOURCE_CHAT, EMPTY_CHAT, HELP_CHAT]) {
    await repos.chats.update(id, { updatedAt: TS } as never);
  }

  // 7. Memories. Two hang off ARIA's messages (bulk-reattribute deletes exactly
  //    those); the third has no source message, proving the delete is scoped.
  const mkMemory = async (
    id: string,
    characterId: string,
    sourceMessageId: string | null,
    content: string,
  ) =>
    repos.memories.create(
      {
        characterId,
        aboutCharacterId: null,
        chatId: CHAT,
        projectId: null,
        content,
        summary: content,
        keywords: [],
        tags: [],
        importance: 0.6,
        embedding: null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: 0.6,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  await mkMemory(MEM_M2, ARIA, M2, 'The lamps were unlit the evening the post ran late.');
  await mkMemory(MEM_M3, ARIA, M3, 'I resolved to write to the quay myself.');
  await mkMemory(MEM_FREE, ARIA, null, 'The ledger is kept in the second drawer.');

  // 8. Chat settings (the `regenerate-title` gate + `buildCheapLLMConfig`).
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: {
        strategy: 'PROVIDER_CHEAPEST',
        fallbackToLocal: false,
        embeddingProvider: 'OPENAI',
      },
    } as never,
    { id: 'c8000000-0000-4000-8000-000000000001', createdAt: TS, updatedAt: TS } as never,
  );

  closeMountIndexSQLiteClient();
  await closeDatabase();

  // TRUNCATE journalling leaves a zero-byte sidecar behind; it is not part of
  // the committed fixture.
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }

  writeFileSync(mainOut + '.meta.json', JSON.stringify(vaults));
  process.stderr.write(
    `built chat-admin fixture: main=${mainOut} mount=${mountOut} vaults=${JSON.stringify(vaults)}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-admin fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
