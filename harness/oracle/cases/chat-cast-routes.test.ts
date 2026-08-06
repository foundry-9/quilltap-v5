/**
 * @jest-environment node
 *
 * P4.9E1A chat CAST + AVATAR-OVERRIDE route-surface ORACLE: drives v4's REAL
 * chat action handlers (`actions/participants.ts`, `actions/avatars.ts`,
 * `actions/toggle-avatar-generation.ts`) AND the `PUT` bag entrance
 * (`helpers.ts` `processChatUpdates`) over a FRESH copy of the committed
 * chat-cast fixture per case, emitting each response body plus post-mutation
 * table dumps so the Rust port (`api::chat_cast::*`, `api::salon::chat_update`)
 * diffs byte-for-byte.
 *
 * The clock is frozen to NOW_MS for every case so the minted participant
 * `updatedAt`, `removedAt`, message `createdAt` and job timestamps are
 * deterministic — the Rust side tokenizes anything that is not the seed
 * sentinel, so the freeze is belt-and-braces rather than load-bearing.
 *
 * **Must run under `TZ=UTC`** — the Host / Prospero announcement writers format
 * dates, and v5's formatter is UTC (the house convention).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-cast-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/chat-cast-routes.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/chat-cast.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CAST_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-cast-main.db \
 *   QT_FIXTURE_CAST_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-cast-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-chat-cast.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- chat-cast-routes
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  ids: Record<string, string>;
}

// Shared frozen clock (matches NOW_MS in the Rust test).
const NOW_MS = 1_777_939_200_000; // 2026-05-05T00:00:00.000Z

function mockRequest(url: string, method: string, body?: unknown): unknown {
  return {
    method,
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body ?? {}),
  };
}

function applyMocks(spec: Spec): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  // jest.setup stubs the character-vault bridge to a single "mock-vault-mount";
  // un-mock it so each character resolves its own real vault (the wardrobe
  // default-outfit resolution reads through it).
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  jest.doMock('@/lib/mount-index/character-vault', () =>
    jest.requireActual('@/lib/mount-index/character-vault'),
  );
  // The chat route pulls the markdown renderer (unified/ESM) transitively.
  jest.doMock('@/lib/services/markdown-renderer.service', () => ({
    __esModule: true,
    renderMarkdownToHtml: async () => null,
    canPreRenderMessage: () => false,
  }));
  // Processor-off (the P4.6y lesson, per `cost-background-routes`): `enqueueJob`
  // kicks v4's in-process job processor, which CLAIMS the freshly-queued
  // CHARACTER_AVATAR_GENERATION rows and flips them PENDING → PROCESSING mid-case.
  // That is v4's runtime, not `handleToggleAvatarGeneration`; the port runs no
  // processor, so leaving it on would diff v4's scheduler against nothing.
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: spec.userId } }),
  }));
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });
}

// ---------------------------------------------------------------------------
// Table dumps (the Rust harness mirrors each one exactly)
// ---------------------------------------------------------------------------

/** The raw `chats` row for one chat, JSON columns parsed. */
async function readChatRow(chatId: string): Promise<unknown> {
  const { rawQuery } = await import('@/lib/database/manager');
  const rows = (await rawQuery(
    'SELECT id, participants, impersonatingParticipantIds, activeTypingParticipantId, ' +
      'tags, equippedOutfit, avatarGenerationEnabled, compiledIdentityStacks, updatedAt ' +
      'FROM chats WHERE id = ?',
    [chatId],
  )) as Array<Record<string, unknown>>;
  const row = rows[0];
  if (!row) return null;
  const j = (v: unknown) => (typeof v === 'string' && v.length > 0 ? JSON.parse(v) : v ?? null);
  return {
    id: row.id,
    participants: j(row.participants),
    impersonatingParticipantIds: j(row.impersonatingParticipantIds),
    activeTypingParticipantId: row.activeTypingParticipantId ?? null,
    tags: j(row.tags),
    equippedOutfit: j(row.equippedOutfit),
    avatarGenerationEnabled: row.avatarGenerationEnabled ?? null,
    compiledIdentityStacks: j(row.compiledIdentityStacks),
    updatedAt: row.updatedAt,
  };
}

/**
 * Every chat message event (the Host / Prospero announcements + status events),
 * re-sorted deterministically.
 *
 * `getMessages` is `ORDER BY createdAt ASC`, but the two sides' `createdAt`
 * values are NOT comparable: the oracle freezes its clock, so every announcement
 * of a case shares one timestamp and SQLite's tie-break decides the order, while
 * the Rust side's real clock advances and orders by insertion. Both are then
 * normalized to `<ts>` anyway, so the timestamp cannot participate in the sort.
 * Re-sorting by `(type, systemKind, body)` on BOTH sides makes the diff compare
 * the SET of announcements — which is the actual claim — rather than an artifact
 * of the freeze.
 */
async function readMessages(chatId: string): Promise<unknown> {
  const { getRepositories } = await import('@/lib/repositories/factory');
  const rows = (await getRepositories().chats.getMessages(chatId)) as Array<
    Record<string, unknown>
  >;
  const key = (m: Record<string, unknown>) =>
    `${String(m.type ?? '')}\u0000${String(m.systemKind ?? '')}\u0000${String(
      m.content ?? m.description ?? '',
    )}`;
  return [...rows].sort((a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0));
}

/** The `avatarOverrides` array of every character, in row order. */
async function readAvatarOverrides(): Promise<unknown> {
  const { rawQuery } = await import('@/lib/database/manager');
  const rows = (await rawQuery(
    'SELECT id, avatarOverrides FROM characters ORDER BY id',
  )) as Array<Record<string, unknown>>;
  return rows.map((r) => ({
    id: r.id,
    avatarOverrides:
      typeof r.avatarOverrides === 'string' && r.avatarOverrides.length > 0
        ? JSON.parse(r.avatarOverrides)
        : [],
  }));
}

/** Every background job, oldest first (the avatar-generation enqueues). */
async function readJobs(): Promise<unknown> {
  const { rawQuery } = await import('@/lib/database/manager');
  // ORDER BY the payload's characterId, NOT the row id: the id is a fresh uuid
  // on both sides, so ordering by it is ordering by randomness.
  const rows = (await rawQuery(
    'SELECT id, userId, type, status, priority, attempts, maxAttempts, payload ' +
      "FROM background_jobs ORDER BY type, json_extract(payload, '$.characterId')",
  )) as Array<Record<string, unknown>>;
  return rows.map((r) => ({
    ...r,
    payload: typeof r.payload === 'string' ? JSON.parse(r.payload) : r.payload,
  }));
}

interface CaseSpec {
  name: string;
  run: () => Promise<{ status: number; body: unknown; tables?: unknown }>;
}

async function loadRoute(path: string): Promise<Record<string, (...a: unknown[]) => Promise<unknown>>> {
  return (await import(path)) as never;
}

async function respond(r: unknown): Promise<{ status: number; body: unknown }> {
  const resp = r as { status: number; json: () => Promise<unknown> };
  return { status: resp.status, body: await resp.json() };
}

const CHAT_ROUTE = '@/app/api/v1/chats/[id]/route';

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec);

  const work = mkdtempSync(join(scratch, 'cast-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  const RealDate = Date;
  const iso = new RealDate(NOW_MS).toISOString();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  global.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      if (a.length === 0) super(iso);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return NOW_MS;
    }
  } as unknown as DateConstructor;

  try {
    const out = await c.run();
    return {
      name: c.name,
      status: out.status,
      body: out.body,
      ...(out.tables !== undefined ? { tables: out.tables } : {}),
    };
  } finally {
    global.Date = RealDate;
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`chat-cast-routes oracle must run under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'chat-cast.json'), 'utf8'),
  ) as Spec;
  const I = spec.ids;

  const fixtures = {
    main: process.env.QT_FIXTURE_CAST_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CAST_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cast-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const B = 'http://localhost/api/v1';
  const post = async (chatId: string, action: string, body: unknown) =>
    (await loadRoute(CHAT_ROUTE)).POST(
      mockRequest(`${B}/chats/${chatId}?action=${action}`, 'POST', body),
      { params: Promise.resolve({ id: chatId }) },
    );
  const get = async (chatId: string, query: string) =>
    (await loadRoute(CHAT_ROUTE)).GET(mockRequest(`${B}/chats/${chatId}?${query}`, 'GET'), {
      params: Promise.resolve({ id: chatId }),
    });
  const put = async (chatId: string, body: unknown) =>
    (await loadRoute(CHAT_ROUTE)).PUT(mockRequest(`${B}/chats/${chatId}`, 'PUT', body), {
      params: Promise.resolve({ id: chatId }),
    });

  /** The standard post-mutation dump for a participant case. */
  const castTables = async (chatId: string) => ({
    chat: await readChatRow(chatId),
    messages: await readMessages(chatId),
  });

  const addCase = (name: string, chatId: string, body: unknown, dump = true): CaseSpec => ({
    name,
    run: async () => {
      const { status, body: out } = await respond(await post(chatId, 'add-participant', body));
      return dump ? { status, body: out, tables: await castTables(chatId) } : { status, body: out };
    },
  });
  const updateCase = (name: string, chatId: string, body: unknown, dump = true): CaseSpec => ({
    name,
    run: async () => {
      const { status, body: out } = await respond(await post(chatId, 'update-participant', body));
      return dump ? { status, body: out, tables: await castTables(chatId) } : { status, body: out };
    },
  });
  const removeCase = (name: string, chatId: string, body: unknown, dump = true): CaseSpec => ({
    name,
    run: async () => {
      const { status, body: out } = await respond(await post(chatId, 'remove-participant', body));
      return dump ? { status, body: out, tables: await castTables(chatId) } : { status, body: out };
    },
  });
  const rebuildCase = (name: string, chatId: string, body: unknown, dump = true): CaseSpec => ({
    name,
    run: async () => {
      const { status, body: out } = await respond(
        await post(chatId, 'rebuild-system-prompt', body),
      );
      return dump ? { status, body: out, tables: await castTables(chatId) } : { status, body: out };
    },
  });
  const bagCase = (name: string, chatId: string, body: unknown, dump = true): CaseSpec => ({
    name,
    run: async () => {
      const { status, body: out } = await respond(await put(chatId, body));
      return dump ? { status, body: out, tables: await castTables(chatId) } : { status, body: out };
    },
  });

  const cases: CaseSpec[] = [
    // ── ?action=add-participant ───────────────────────────────────────────
    addCase('add_fresh_defaults', I.chatMain, { type: 'CHARACTER', characterId: I.dora }),
    addCase('add_explicit_fields', I.chatQuiet, {
      type: 'CHARACTER',
      characterId: I.dora,
      connectionProfileId: I.connB,
      imageProfileId: I.imageProfile,
      displayOrder: 7,
      hasHistoryAccess: false,
      joinScenario: 'arrives by airship, soaked through',
      controlledBy: 'llm',
    }),
    addCase('add_history_access_suppresses_scenario', I.chatQuiet, {
      type: 'CHARACTER',
      characterId: I.dora,
      joinScenario: 'arrives by airship, soaked through',
      hasHistoryAccess: true,
    }),
    addCase('add_outfit_none', I.chatQuiet, {
      type: 'CHARACTER',
      characterId: I.dora,
      outfitSelection: { characterId: I.dora, mode: 'none' },
    }),
    addCase('add_outfit_manual', I.chatQuiet, {
      type: 'CHARACTER',
      characterId: I.dora,
      outfitSelection: {
        characterId: I.dora,
        mode: 'manual',
        slots: { top: [I.wDoraTop], bottom: [], footwear: [], accessories: [] },
      },
    }),
    addCase('add_user_controlled', I.chatQuiet, {
      type: 'CHARACTER',
      characterId: I.dora,
      controlledBy: 'user',
    }),
    addCase('add_stale_profile_falls_back', I.chatQuiet, {
      type: 'CHARACTER',
      characterId: I.dora,
      connectionProfileId: I.missing,
    }),
    // ── P4.D39 / v4 `8bb1a958`: `default` mode spans all three tiers ──────
    // GAIL owns no wardrobe at all. Before the fix she joined undressed; now
    // she wears the whole shared tier (G_COAT + G_LIVERY in top, G_HAT in
    // accessories), layered by `createdAt`.
    addCase('add_outfit_default_empty_vault_wears_shared', I.chatQuiet, {
      type: 'CHARACTER',
      characterId: I.gail,
      outfitSelection: { characterId: I.gail, mode: 'default' },
    }),
    // DORA owns two defaults. The shared coat is OLDER than both, so it layers
    // innermost; her own coat follows; the shared hat fills a slot she owns
    // nothing in; her boots stand alone.
    addCase('add_outfit_default_layers_shared_under_own', I.chatQuiet, {
      type: 'CHARACTER',
      characterId: I.dora,
      outfitSelection: { characterId: I.dora, mode: 'default' },
    }),
    addCase('add_reactivates_removed', I.chatMain, { type: 'CHARACTER', characterId: I.eris }),
    // ERIS holds a NOT-default copy of the shared livery: it shadows the shared
    // item by id, so she wears the shared coat + hat and her own cape but NOT
    // the livery. The arm that only exists because tiers merge on the FULL
    // pools and `isDefault` is filtered last.
    addCase('add_reactivates_removed_with_outfit', I.chatMain, {
      type: 'CHARACTER',
      characterId: I.eris,
      controlledBy: 'user',
      outfitSelection: { characterId: I.eris, mode: 'default' },
    }),
    addCase(
      'add_already_present',
      I.chatMain,
      { type: 'CHARACTER', characterId: I.bram },
      false,
    ),
    addCase(
      'add_already_present_deactivated',
      I.chatSolo,
      { type: 'CHARACTER', characterId: I.cleo },
      false,
    ),
    addCase('add_character_missing', I.chatMain, { type: 'CHARACTER', characterId: I.missing }, false),
    addCase('add_chat_missing', I.missing, { type: 'CHARACTER', characterId: I.dora }, false),

    // ── ?action=update-participant ────────────────────────────────────────
    updateCase('update_display_order', I.chatMain, {
      participantId: I.pBram,
      displayOrder: 5,
    }),
    updateCase('update_talkativeness_set', I.chatMain, {
      participantId: I.pBram,
      talkativeness: 0.4,
    }),
    updateCase('update_talkativeness_null', I.chatMain, {
      participantId: I.pBram,
      talkativeness: null,
    }),
    updateCase('update_talkativeness_absent', I.chatMain, {
      participantId: I.pBram,
      hasHistoryAccess: true,
    }),
    updateCase('update_image_profile_set', I.chatMain, {
      participantId: I.pBram,
      imageProfileId: I.imageProfile,
    }),
    updateCase('update_image_profile_null', I.chatMain, {
      participantId: I.pBram,
      imageProfileId: null,
    }),
    updateCase('update_join_scenario_set', I.chatMain, {
      participantId: I.pBram,
      joinScenario: 'was already at the table',
    }),
    updateCase('update_join_scenario_null', I.chatMain, {
      participantId: I.pBram,
      joinScenario: null,
    }),
    updateCase('update_selected_prompt_change', I.chatMain, {
      participantId: I.pBram,
      selectedSystemPromptId: I.bramPromptAlt,
    }),
    updateCase('update_selected_prompt_null', I.chatMain, {
      participantId: I.pBram,
      selectedSystemPromptId: null,
    }),
    updateCase('update_selected_prompt_unchanged', I.chatMain, {
      participantId: I.pBram,
      selectedSystemPromptId: I.bramPrompt,
    }),
    updateCase('update_connection_profile_change', I.chatMain, {
      participantId: I.pBram,
      connectionProfileId: I.connB,
    }),
    updateCase('update_connection_profile_same', I.chatMain, {
      participantId: I.pBram,
      connectionProfileId: I.connA,
    }),
    updateCase(
      'update_connection_profile_missing',
      I.chatMain,
      { participantId: I.pBram, connectionProfileId: I.missing },
      false,
    ),
    updateCase(
      'update_image_profile_missing',
      I.chatMain,
      { participantId: I.pBram, imageProfileId: I.missing },
      false,
    ),
    updateCase('update_controlled_by_user_no_typist', I.chatQuiet, {
      participantId: I.qBram,
      controlledBy: 'user',
    }),
    updateCase('update_controlled_by_user_typist_set', I.chatMain, {
      participantId: I.pBram,
      controlledBy: 'user',
    }),
    updateCase('update_controlled_by_llm_promotes', I.chatMain, {
      participantId: I.pAria,
      controlledBy: 'llm',
    }),
    updateCase('update_controlled_by_llm_not_typist', I.chatMain, {
      participantId: I.pGail,
      controlledBy: 'llm',
    }),
    // Bug 23 (`bd419ae9`): the `controlledBy` early return is GONE, so a `status`
    // in the same patch now falls through and is applied (and the whole-chat
    // identity stacks recompile).
    updateCase('update_controlled_by_with_status_falls_through', I.chatMain, {
      participantId: I.pBram,
      controlledBy: 'user',
      status: 'silent',
    }),
    updateCase('update_status_silent', I.chatMain, { participantId: I.pBram, status: 'silent' }),
    updateCase('update_status_exit_silent', I.chatQuiet, {
      participantId: I.qCleo,
      status: 'active',
    }),
    updateCase('update_status_absent', I.chatMain, { participantId: I.pCleo, status: 'absent' }),
    updateCase('update_status_removed', I.chatMain, { participantId: I.pCleo, status: 'removed' }),
    updateCase('update_is_active_false', I.chatMain, { participantId: I.pBram, isActive: false }),
    updateCase('update_is_active_true_noop', I.chatMain, { participantId: I.pBram, isActive: true }),
    updateCase(
      'update_participant_missing',
      I.chatMain,
      { participantId: I.missing, displayOrder: 3 },
      false,
    ),
    updateCase(
      'update_chat_missing',
      I.missing,
      { participantId: I.pBram, displayOrder: 3 },
      false,
    ),
    updateCase(
      'update_invalid_status',
      I.chatMain,
      { participantId: I.pBram, status: 'lurking' },
      false,
    ),
    updateCase(
      'update_talkativeness_out_of_range',
      I.chatMain,
      { participantId: I.pBram, talkativeness: 4 },
      false,
    ),

    // ── ?action=remove-participant ────────────────────────────────────────
    removeCase('remove_ok', I.chatMain, { participantId: I.pCleo }),
    removeCase('remove_impersonating_promotes', I.chatMain, { participantId: I.pAria }),
    removeCase('remove_impersonating_not_typist', I.chatMain, { participantId: I.pGail }),
    removeCase('remove_last_character', I.chatSolo, { participantId: I.sBram }, false),
    removeCase('remove_participant_missing', I.chatMain, { participantId: I.missing }, false),
    removeCase('remove_chat_missing', I.missing, { participantId: I.pCleo }, false),

    // ── ?action=rebuild-system-prompt ─────────────────────────────────────
    rebuildCase('rebuild_ok', I.chatMain, { participantId: I.pBram }),
    rebuildCase('rebuild_user_controlled', I.chatMain, { participantId: I.pAria }, false),
    rebuildCase('rebuild_participant_missing', I.chatMain, { participantId: I.missing }, false),
    rebuildCase('rebuild_chat_missing', I.missing, { participantId: I.pBram }, false),
    rebuildCase('rebuild_no_participant_id', I.chatMain, {}, false),

    // ── the avatar-override family ────────────────────────────────────────
    {
      name: 'get_avatars',
      run: async () => respond(await get(I.chatMain, 'action=get-avatars')),
    },
    {
      name: 'get_avatars_chat_missing',
      run: async () => respond(await get(I.missing, 'action=get-avatars')),
    },
    {
      name: 'set_avatar_new',
      run: async () => {
        const { status, body } = await respond(
          await post(I.chatMain, 'set-avatar', { characterId: I.dora, imageId: I.imgA }),
        );
        return { status, body, tables: { overrides: await readAvatarOverrides() } };
      },
    },
    {
      name: 'set_avatar_replace',
      run: async () => {
        const { status, body } = await respond(
          await post(I.chatMain, 'set-avatar', { characterId: I.bram, imageId: I.imgB }),
        );
        return { status, body, tables: { overrides: await readAvatarOverrides() } };
      },
    },
    {
      name: 'set_avatar_character_missing',
      run: async () =>
        respond(await post(I.chatMain, 'set-avatar', { characterId: I.missing, imageId: I.imgA })),
    },
    {
      name: 'set_avatar_image_missing',
      run: async () =>
        respond(await post(I.chatMain, 'set-avatar', { characterId: I.bram, imageId: I.missing })),
    },
    {
      name: 'remove_avatar_ok',
      run: async () => {
        const { status, body } = await respond(
          await post(I.chatMain, 'remove-avatar', { characterId: I.bram }),
        );
        return { status, body, tables: { overrides: await readAvatarOverrides() } };
      },
    },
    {
      name: 'remove_avatar_no_override',
      run: async () => {
        const { status, body } = await respond(
          await post(I.chatMain, 'remove-avatar', { characterId: I.dora }),
        );
        return { status, body, tables: { overrides: await readAvatarOverrides() } };
      },
    },
    {
      name: 'remove_avatar_character_missing',
      run: async () =>
        respond(await post(I.chatMain, 'remove-avatar', { characterId: I.missing })),
    },
    {
      name: 'toggle_avatar_generation_on',
      run: async () => {
        const { status, body } = await respond(
          await post(I.chatMain, 'toggle-avatar-generation', {}),
        );
        return {
          status,
          body,
          tables: { chat: await readChatRow(I.chatMain), jobs: await readJobs() },
        };
      },
    },
    {
      name: 'toggle_avatar_generation_off',
      run: async () => {
        const { status, body } = await respond(
          await post(I.chatQuiet, 'toggle-avatar-generation', {}),
        );
        return {
          status,
          body,
          tables: { chat: await readChatRow(I.chatQuiet), jobs: await readJobs() },
        };
      },
    },
    {
      name: 'toggle_avatar_generation_chat_missing',
      run: async () => respond(await post(I.missing, 'toggle-avatar-generation', {})),
    },

    // ── the chat-PUT bag entrance (processChatUpdates) ────────────────────
    // Each of these mirrors an action-verb case above; the Rust harness asserts
    // the two entrances land IDENTICAL `chats` state.
    bagCase('bag_update_participant', I.chatMain, {
      updateParticipant: { participantId: I.pBram, displayOrder: 5 },
    }),
    bagCase('bag_add_participant', I.chatMain, {
      addParticipant: { type: 'CHARACTER', characterId: I.dora },
    }),
    bagCase('bag_remove_participant', I.chatMain, { removeParticipantId: I.pCleo }),
    bagCase('bag_remove_last_participant', I.chatSolo, { removeParticipantId: I.sBram }, false),
    bagCase('bag_update_and_chat_fields', I.chatMain, {
      chat: { title: 'The Salon, Reconvened' },
      updateParticipant: { participantId: I.pBram, status: 'silent' },
    }),
    bagCase(
      'bag_update_participant_missing',
      I.chatMain,
      { updateParticipant: { participantId: I.missing, displayOrder: 5 } },
      false,
    ),
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`chat-cast-routes oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('chat-cast-routes oracle', async () => {
  await main();
});
