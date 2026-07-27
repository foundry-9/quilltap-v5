/**
 * The committed chat-dialogs server-surface fixture builder (P4.9E3B
 * deliverable) — the shared test-pepper substrate for
 * `chat_export_equivalence` (export + outfit-summary), `search_replace_equivalence`,
 * `message_reattribute_equivalence`, `tools_inventory_equivalence` (tier 2) and
 * `outfit_llm_choose_tier3_equivalence` (tier 3).
 *
 * Baked entirely through v4's REAL repositories (never hand-rolled SQL —
 * `project-store-fixture-needs-real-create`), with every id and timestamp pinned
 * so both differential sides read byte-identical rows.
 *
 * ## What is in it, and which arm each row exists for
 *
 *   1. ONE user (`Friday` — the export's `userName` fallback) + an api key +
 *      TWO connection profiles: `CONN` (the user DEFAULT, `allowWebSearch: true`
 *      — the tools `search_web` availability arm AND the llm_choose default
 *      profile) and `CONN_NOWEB` (`allowWebSearch` unset → false).
 *   2. FOUR characters, each vault provisioned by v4's own `characters.create`:
 *        - NORA (llm)  — the export primary; wardrobe: a default cloak + a hat.
 *        - PIP  (llm)  — second export speaker; the llm_choose subject (a
 *                        default coat, boots, scarf, and a COMPOSITE dock rig
 *                        bundling coat+boots); the merge-source character.
 *        - VERA (user) — a user-CONTROLLED character participant, so her lines
 *                        export under "Vera", not the operator's name.
 *        - WREN (llm)  — `canDressThemselves: false` + `canCreateOutfits: false`
 *                        (the tools wardrobe availability arms).
 *      Plus `MISSING_CHAR`, an id NEVER created — the export's broken-vault
 *      fallback arm (the participant exists, `findByIds` returns nothing for
 *      it, and the line falls back to the primary name).
 *   3. EXPORT_CHAT — Nora/Pip/Vera/void cast; a swipe group of three; a TOOL
 *      row and a role-SYSTEM row (the export filters role === 'SYSTEM' only);
 *      a `rawResponse` (→ `extra`); a type-'system' event (filtered upstream);
 *      `sillyTavernMetadata` (→ the JSONL header's `chat_metadata`).
 *      NOCHAR_CHAT — no participants (the `No character in chat` refusal).
 *   4. SR_CHAT — the search-replace + reattribute subject: messages carrying
 *      'lantern' in both cases (count is case-INSENSITIVE, replace is
 *      case-SENSITIVE — MEM_CASE and R5 pin the asymmetry), one message (R4)
 *      with TWO sourced memories (reattribute deletes exactly those), and
 *      memories spanning chat scope / character scope / summary-only matches.
 *   5. TOOLS_CHAT_PROJ (project + linked doc stores + chat imageProfileId +
 *      CONN on the primary + multi-character), TOOLS_CHAT_NOSTORES (project
 *      whose links are REMOVED + CONN_NOWEB), TOOLS_CHAT_BARE (no project,
 *      single Wren, no connection profile) — the nine availability arms.
 *   6. MERGE_TARGET (Vera + Nora, `scenarioText` for the llm_choose prompt) and
 *      MERGE_SOURCE (Pip; `equippedOutfit` for Pip incl. the composite — ALSO
 *      the outfit-summary subject, with a bare `NORA` entry whose slots are
 *      empty and a slot id whose item doesn't cover the slot).
 *   7. Chat settings (`cheapLLMSettings`) — the llm_choose config source.
 *
 * No row lives in the llm-logs partition; the empty `background_jobs` table is
 * materialized so fresh copies share a schema.
 *
 * Regenerate (Node 24, from the v4 checkout) — the committed .db files land in
 * place:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   TZ=UTC \
 *   QT_FIXTURE_CD_MAIN=$W/crates/quilltap-web/tests/fixtures/chat-dialogs-main.db \
 *   QT_FIXTURE_CD_MOUNT=$W/crates/quilltap-web/tests/fixtures/chat-dialogs-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-chat-dialogs-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

// ── Pinned entity ids (shared verbatim with the oracle cases + the Rust tests) ──
const APIKEY = 'aa000001-0000-4000-8000-000000000001';
const CONN = 'cc000001-0000-4000-8000-000000000001'; // user DEFAULT, allowWebSearch
const CONN_NOWEB = 'cc000002-0000-4000-8000-000000000002';

const NORA = 'a2000000-0000-4000-8000-000000000001';
const PIP = 'a2000000-0000-4000-8000-000000000002';
const VERA = 'a2000000-0000-4000-8000-000000000003';
const WREN = 'a2000000-0000-4000-8000-000000000004';
const MISSING_CHAR = 'a2000000-0000-4000-8000-000000000099'; // never created

// Wardrobe items (NORA a*, PIP b*).
const N_DEF = '00000000-0000-4000-8000-0000000000a1'; // top, isDefault
const N_HAT = '00000000-0000-4000-8000-0000000000a2'; // accessories
const P_DEF = '00000000-0000-4000-8000-0000000000b1'; // top, isDefault
const P_BOOTS = '00000000-0000-4000-8000-0000000000b2'; // footwear
const P_SCARF = '00000000-0000-4000-8000-0000000000b3'; // accessories
const P_SET = '00000000-0000-4000-8000-0000000000b4'; // composite [P_DEF, P_BOOTS]

const EXPORT_CHAT = 'c2000000-0000-4000-8000-000000000001';
const NOCHAR_CHAT = 'c2000000-0000-4000-8000-000000000002';
const SR_CHAT = 'c2000000-0000-4000-8000-000000000003';
const TOOLS_CHAT_PROJ = 'c2000000-0000-4000-8000-000000000004';
const TOOLS_CHAT_NOSTORES = 'c2000000-0000-4000-8000-000000000005';
const TOOLS_CHAT_BARE = 'c2000000-0000-4000-8000-000000000006';
const MERGE_TARGET = 'c2000000-0000-4000-8000-000000000007';
const MERGE_SOURCE = 'c2000000-0000-4000-8000-000000000008';

const PROJECT_1 = 'aaaa0000-0000-4000-8000-000000000001';
const PROJECT_2 = 'aaaa0000-0000-4000-8000-000000000002';
const IMAGE_PROFILE_ID = 'ee000000-0000-4000-8000-000000000001'; // never resolved

// EXPORT_CHAT participants.
const P_NORA = 'e2000000-0000-4000-8000-000000000001';
const P_PIP = 'e2000000-0000-4000-8000-000000000002';
const P_VERA = 'e2000000-0000-4000-8000-000000000003';
const P_VOID = 'e2000000-0000-4000-8000-000000000004';
// SR_CHAT participants.
const P3_NORA = 'e2000000-0000-4000-8000-000000000011';
const P3_PIP = 'e2000000-0000-4000-8000-000000000012';
const P3_VERA = 'e2000000-0000-4000-8000-000000000013';
// Tools chats.
const P4_NORA = 'e2000000-0000-4000-8000-000000000021';
const P4_PIP = 'e2000000-0000-4000-8000-000000000022';
const P5_NORA = 'e2000000-0000-4000-8000-000000000031';
const P6_WREN = 'e2000000-0000-4000-8000-000000000041';
// Merge chats.
const P7_VERA = 'e2000000-0000-4000-8000-000000000051';
const P7_NORA = 'e2000000-0000-4000-8000-000000000052';
const P8_PIP = 'e2000000-0000-4000-8000-000000000061';

// EXPORT_CHAT messages.
const X1 = 'd4000000-0000-4000-8000-000000000001'; // USER / VERA
const X2 = 'd4000000-0000-4000-8000-000000000002'; // ASSISTANT / NORA (+rawResponse)
const X3 = 'd4000000-0000-4000-8000-000000000003'; // ASSISTANT / PIP
const X4 = 'd4000000-0000-4000-8000-000000000004'; // ASSISTANT / VOID (fallback name)
const X5 = 'd4000000-0000-4000-8000-000000000005'; // USER / unattributed → 'Friday'
const X6A = 'd4000000-0000-4000-8000-000000000006'; // swipe 0 / NORA
const X6B = 'd4000000-0000-4000-8000-000000000007'; // swipe 1 / NORA
const X6C = 'd4000000-0000-4000-8000-000000000008'; // swipe 2 / NORA
const X7 = 'd4000000-0000-4000-8000-000000000009'; // role SYSTEM → filtered
const X8 = 'd4000000-0000-4000-8000-00000000000a'; // role TOOL → exported
const X_SYS = 'd4000000-0000-4000-8000-00000000000b'; // type 'system' → filtered upstream
// SR_CHAT messages.
const R1 = 'd5000000-0000-4000-8000-000000000001'; // 'lantern' ×2 / NORA
const R2 = 'd5000000-0000-4000-8000-000000000002'; // 'lantern' ×1 / PIP
const R3 = 'd5000000-0000-4000-8000-000000000003'; // no match / VERA
const R4 = 'd5000000-0000-4000-8000-000000000004'; // 'lantern' ×1 / NORA, 2 sourced memories
const R5 = 'd5000000-0000-4000-8000-000000000005'; // 'Lantern' (capital) — counted, NOT replaced

// Memories.
const MEM_R4A = 'f2000000-0000-4000-8000-000000000001'; // NORA / SR_CHAT / source R4 / 'lantern'
const MEM_R4B = 'f2000000-0000-4000-8000-000000000002'; // NORA / SR_CHAT / source R4 / summary-only match
const MEM_N1 = 'f2000000-0000-4000-8000-000000000003'; // NORA / no chat / 'lantern'
const MEM_P1 = 'f2000000-0000-4000-8000-000000000004'; // PIP / SR_CHAT / 'lantern' keyword-only match
const MEM_CASE = 'f2000000-0000-4000-8000-000000000005'; // NORA / SR_CHAT / 'Lantern' — counted, not replaced

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`chat-dialogs fixture must be built under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'chat-dialogs-web.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_CD_MAIN;
  const mountOut = process.env.QT_FIXTURE_CD_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_CD_MAIN and QT_FIXTURE_CD_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cd-fixture-'));
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
  // doc_mount_blobs has hand-written DDL — trigger CREATE via a read.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  // 1. User + api key + connection profiles.
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
      allowWebSearch: true,
    } as never,
    { id: CONN, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.connections.create(
    {
      userId: spec.userId,
      name: 'No Web Desk',
      provider: 'OPENAI',
      modelName: 'gpt-4o-mini',
      apiKeyId: APIKEY,
    } as never,
    { id: CONN_NOWEB, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Characters (create owns vault provisioning).
  const mkChar = async (id: string, name: string, controlledBy: string, extra: object = {}) => {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy, ...extra } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  await mkChar(NORA, 'Nora', 'llm', {
    description: 'A correspondent of the northern quays.',
    personality: 'Meticulous, dry, unhurried.',
  });
  await mkChar(PIP, 'Pip', 'llm', {
    description: 'A dockhand with an eye for weather.',
    personality: 'Quick, cheerful, superstitious.',
  });
  await mkChar(VERA, 'Vera', 'user', { description: 'The operator’s voice at the rail.' });
  await mkChar(WREN, 'Wren', 'llm', {
    description: 'Keeps the ledger and nothing else.',
    canDressThemselves: false,
    canCreateOutfits: false,
  });
  // Verify the wardrobe flags actually landed (create must not strip them).
  {
    const wren = await repos.characters.findById(WREN);
    if (wren?.canDressThemselves !== false || wren?.canCreateOutfits !== false) {
      throw new Error('WREN wardrobe flags did not persist through characters.create');
    }
  }

  // 3. Wardrobe items.
  const seedItem = async (
    characterId: string,
    id: string,
    title: string,
    types: string[],
    isDefault: boolean,
    componentItemIds: string[] = [],
    description: string | null = null,
  ): Promise<void> => {
    await repos.wardrobe.create(
      {
        characterId,
        title,
        description,
        imagePrompt: null,
        types,
        componentItemIds,
        appropriateness: null,
        isDefault,
        replace: false,
        migratedFromClothingRecordId: null,
        archivedAt: null,
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  await seedItem(NORA, N_DEF, 'Nora’s travelling cloak', ['top'], true);
  await seedItem(NORA, N_HAT, 'Nora’s signal hat', ['accessories'], false);
  await seedItem(PIP, P_DEF, 'Pip’s patched coat', ['top'], true);
  await seedItem(PIP, P_BOOTS, 'Pip’s tar boots', ['footwear'], false);
  await seedItem(PIP, P_SCARF, 'Pip’s red scarf', ['accessories'], false);
  await seedItem(
    PIP,
    P_SET,
    'Pip’s dock rig',
    ['top', 'footwear'],
    false,
    [P_DEF, P_BOOTS],
    'The whole working outfit in one piece.',
  );

  // 4. Projects. PROJECT_1 keeps its official store (linked doc stores present);
  //    PROJECT_2 has every link REMOVED (the has-project-but-no-stores arm).
  await repos.projects.create(
    { name: 'The Quay Ledger', description: 'Tools with stores.', characterRoster: [], state: {} } as never,
    { id: PROJECT_1, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.projects.create(
    { name: 'The Bare Project', description: 'Tools without stores.', characterRoster: [], state: {} } as never,
    { id: PROJECT_2, createdAt: TS, updatedAt: TS } as never,
  );
  {
    const links1 = await repos.projectDocMountLinks.findByProjectId(PROJECT_1);
    if (links1.length === 0) {
      throw new Error('PROJECT_1 has no doc mount links — expected the official store link');
    }
    const links2 = await repos.projectDocMountLinks.findByProjectId(PROJECT_2);
    for (const link of links2) {
      await repos.projectDocMountLinks.unlink(PROJECT_2, link.mountPointId);
    }
    const links2After = await repos.projectDocMountLinks.findByProjectId(PROJECT_2);
    if (links2After.length !== 0) {
      throw new Error('PROJECT_2 still has doc mount links after unlinking');
    }
  }

  // 5. Chats.
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
      title: 'The Evening Watch',
      chatType: 'salon',
      contextSummary: null,
      sillyTavernMetadata: { note_prompt: 'Keep to the quay.', imported_from: 'quilltap' },
      participants: [
        mkParticipant(P_NORA, NORA, 'llm', 0, { connectionProfileId: CONN }),
        mkParticipant(P_PIP, PIP, 'llm', 1),
        mkParticipant(P_VERA, VERA, 'user', 2),
        mkParticipant(P_VOID, MISSING_CHAR, 'llm', 3),
      ],
      tags: [],
    } as never,
    { id: EXPORT_CHAT, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'An Empty Watch',
      chatType: 'salon',
      participants: [],
      tags: [],
    } as never,
    { id: NOCHAR_CHAT, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Lantern Ledger',
      chatType: 'salon',
      participants: [
        mkParticipant(P3_NORA, NORA, 'llm', 0, { connectionProfileId: CONN }),
        mkParticipant(P3_PIP, PIP, 'llm', 1),
        mkParticipant(P3_VERA, VERA, 'user', 2),
      ],
      tags: [],
    } as never,
    { id: SR_CHAT, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Tools, Fully Provisioned',
      chatType: 'salon',
      projectId: PROJECT_1,
      imageProfileId: IMAGE_PROFILE_ID,
      participants: [
        mkParticipant(P4_NORA, NORA, 'llm', 0, { connectionProfileId: CONN }),
        mkParticipant(P4_PIP, PIP, 'llm', 1),
      ],
      tags: [],
    } as never,
    { id: TOOLS_CHAT_PROJ, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Tools, Storeless Project',
      chatType: 'salon',
      projectId: PROJECT_2,
      participants: [
        mkParticipant(P5_NORA, NORA, 'llm', 0, { connectionProfileId: CONN_NOWEB }),
      ],
      tags: [],
    } as never,
    { id: TOOLS_CHAT_NOSTORES, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Tools, Bare Room',
      chatType: 'salon',
      participants: [mkParticipant(P6_WREN, WREN, 'llm', 0)],
      tags: [],
    } as never,
    { id: TOOLS_CHAT_BARE, createdAt: TS, updatedAt: TS } as never,
  );

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Rain Watch',
      chatType: 'salon',
      scenarioText: 'A rain-slick quay at dusk; the mail packet is overdue.',
      participants: [
        mkParticipant(P7_VERA, VERA, 'user', 0),
        mkParticipant(P7_NORA, NORA, 'llm', 1, { connectionProfileId: CONN }),
      ],
      tags: [],
    } as never,
    { id: MERGE_TARGET, createdAt: TS, updatedAt: TS } as never,
  );

  // MERGE_SOURCE doubles as the outfit-summary subject: Pip's equipped state
  // includes the composite (expands to coat+boots; boots dropped from `top` by
  // the slot-coverage filter), a direct boots entry, and N_HAT — an id outside
  // Pip's wardrobe, unresolvable, silently dropped. The bare NORA entry proves
  // the empty-slot shape survives.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Lower Quay',
      chatType: 'salon',
      contextSummary: 'Pip kept watch on the lower quay while the lamps failed.',
      participants: [mkParticipant(P8_PIP, PIP, 'llm', 0)],
      tags: [],
      equippedOutfit: {
        [PIP]: { top: [P_SET], bottom: [], footwear: [P_BOOTS], accessories: [N_HAT] },
        [NORA]: { top: [], bottom: [], footwear: [], accessories: [] },
      },
    } as never,
    { id: MERGE_SOURCE, createdAt: TS, updatedAt: TS } as never,
  );

  // 6. Messages.
  const add = async (chatId: string, event: object) =>
    repos.chats.addMessage(chatId, event as never);

  await add(EXPORT_CHAT, {
    type: 'message',
    id: X1,
    role: 'USER',
    content: 'Vera leans on the rail. "Any sign of the packet?"',
    participantId: P_VERA,
    createdAt: '2026-05-01T10:00:00.000Z',
    attachments: [],
  });
  await add(EXPORT_CHAT, {
    type: 'message',
    id: X2,
    role: 'ASSISTANT',
    content: 'Nora folds her glass. "None, and the tide has turned."',
    participantId: P_NORA,
    createdAt: '2026-05-01T10:01:00.000Z',
    attachments: [],
    rawResponse: { model: 'mock-model', finishReason: 'stop' },
    tokenCount: 18,
  });
  await add(EXPORT_CHAT, {
    type: 'message',
    id: X3,
    role: 'ASSISTANT',
    content: 'Pip whistles low. "Weather coming in off the bar."',
    participantId: P_PIP,
    createdAt: '2026-05-01T10:02:00.000Z',
    attachments: [],
  });
  await add(EXPORT_CHAT, {
    type: 'message',
    id: X4,
    role: 'ASSISTANT',
    content: 'A voice from a vanished ledger speaks anyway.',
    participantId: P_VOID,
    createdAt: '2026-05-01T10:03:00.000Z',
    attachments: [],
  });
  await add(EXPORT_CHAT, {
    type: 'message',
    id: X5,
    role: 'USER',
    content: 'An unattributed aside from the operator.',
    participantId: null,
    createdAt: '2026-05-01T10:04:00.000Z',
    attachments: [],
  });
  // The swipe group (insertion order == swipeIndex order, v4's normal shape).
  for (const [id, idx, content] of [
    [X6A, 0, 'Nora: "We wait." (first draft)'],
    [X6B, 1, 'Nora: "We wait for the flood." (second draft)'],
    [X6C, 2, 'Nora: "We wait, and we write." (third draft)'],
  ] as Array<[string, number, string]>) {
    await add(EXPORT_CHAT, {
      type: 'message',
      id,
      role: 'ASSISTANT',
      content,
      participantId: P_NORA,
      swipeGroupId: 'swipe-g1',
      swipeIndex: idx,
      createdAt: `2026-05-01T10:0${5 + idx}:00.000Z`,
      attachments: [],
    });
  }
  await add(EXPORT_CHAT, {
    type: 'message',
    id: X7,
    role: 'SYSTEM',
    content: 'A SYSTEM-role line the export must drop.',
    participantId: null,
    createdAt: '2026-05-01T10:08:00.000Z',
    attachments: [],
  });
  await add(EXPORT_CHAT, {
    type: 'message',
    id: X8,
    role: 'TOOL',
    content: '{"tool":"rng","result":7}',
    participantId: P_NORA,
    createdAt: '2026-05-01T10:09:00.000Z',
    attachments: [],
  });
  await add(EXPORT_CHAT, {
    type: 'system',
    id: X_SYS,
    systemEventType: 'CONTEXT_SUMMARY',
    description: 'The Librarian folded the thread.',
    createdAt: '2026-05-01T10:10:00.000Z',
  });

  await add(SR_CHAT, {
    type: 'message',
    id: R1,
    role: 'ASSISTANT',
    content: 'The lantern by the quay went dark. The lantern keeper never came.',
    participantId: P3_NORA,
    createdAt: '2026-05-01T11:00:00.000Z',
    attachments: [],
  });
  await add(SR_CHAT, {
    type: 'message',
    id: R2,
    role: 'ASSISTANT',
    content: 'No lantern tonight, then.',
    participantId: P3_PIP,
    createdAt: '2026-05-01T11:01:00.000Z',
    attachments: [],
  });
  await add(SR_CHAT, {
    type: 'message',
    id: R3,
    role: 'USER',
    content: 'Nothing on the water.',
    participantId: P3_VERA,
    createdAt: '2026-05-01T11:02:00.000Z',
    attachments: [],
  });
  await add(SR_CHAT, {
    type: 'message',
    id: R4,
    role: 'ASSISTANT',
    content: 'A second dark lantern, then, and the ledger says why.',
    participantId: P3_NORA,
    createdAt: '2026-05-01T11:03:00.000Z',
    attachments: [],
  });
  await add(SR_CHAT, {
    type: 'message',
    id: R5,
    role: 'ASSISTANT',
    content: 'The Lantern Office denies everything.',
    participantId: P3_NORA,
    createdAt: '2026-05-01T11:04:00.000Z',
    attachments: [],
  });

  // Re-pin `updatedAt` — every `addMessage` bumps it.
  for (const id of [EXPORT_CHAT, SR_CHAT]) {
    await repos.chats.update(id, { updatedAt: TS } as never);
  }

  // 7. Memories.
  const mkMemory = async (
    id: string,
    characterId: string,
    chatId: string | null,
    sourceMessageId: string | null,
    content: string,
    summary: string,
    keywords: string[] = [],
  ) =>
    repos.memories.create(
      {
        characterId,
        aboutCharacterId: null,
        chatId,
        projectId: null,
        content,
        summary,
        keywords,
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
  await mkMemory(
    MEM_R4A,
    NORA,
    SR_CHAT,
    R4,
    'The second lantern went dark before the ledger closed.',
    'A dark lantern, recorded.',
  );
  await mkMemory(
    MEM_R4B,
    NORA,
    SR_CHAT,
    R4,
    'She resolved to write to the harbour office.',
    'A letter about the lantern, planned.',
  );
  await mkMemory(
    MEM_N1,
    NORA,
    null,
    null,
    'Every lantern on the north quay burns whale oil.',
    'Quay fuel, noted.',
    ['lantern', 'oil'],
  );
  await mkMemory(
    MEM_P1,
    PIP,
    SR_CHAT,
    null,
    'The weather turned before the watch ended.',
    'Weather, turned.',
    ['lantern-watch'],
  );
  await mkMemory(
    MEM_CASE,
    NORA,
    SR_CHAT,
    null,
    'The Lantern Office keeps its own counsel.',
    'An office, unhelpful.',
  );

  // 8. A group with VERA as member (the `group-stores` read: Vera is the
  //    user-CONTROLLED participant in SR_CHAT and EXPORT_CHAT, so both chats
  //    surface The Watch's official store; TOOLS_CHAT_BARE has no
  //    user-controlled character and stays empty).
  const watch = await repos.groups.create(
    {
      name: 'The Watch',
      description: 'Vera’s standing watch.',
      color: '#223344',
      icon: 'lantern',
      state: {},
    } as never,
    { id: 'dd000000-0000-4000-8000-000000000001', createdAt: TS, updatedAt: TS } as never,
  );
  if (watch.id !== 'dd000000-0000-4000-8000-000000000001') {
    throw new Error(`Watch group id drift: ${watch.id}`);
  }
  await repos.groupCharacterMembers.addMember('dd000000-0000-4000-8000-000000000001', VERA);

  // 9. Chat settings (the llm_choose cheap-LLM config source).
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: {
        strategy: 'PROVIDER_CHEAPEST',
        fallbackToLocal: false,
        embeddingProvider: 'OPENAI',
      },
    } as never,
    { id: 'c8000000-0000-4000-8000-000000000002', createdAt: TS, updatedAt: TS } as never,
  );

  closeMountIndexSQLiteClient();
  await closeDatabase();

  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }

  process.stderr.write(`built chat-dialogs fixture: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-dialogs fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
