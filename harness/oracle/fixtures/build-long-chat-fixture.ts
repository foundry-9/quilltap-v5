/**
 * Long-chat fixture builder (P4.6h — Salon virtualization / dogfood finding #3b).
 *
 * Bakes ONE chat with ~300 messages of realistic mixed shape through v4's REAL
 * repositories (`repos.chats.addMessages`), so the SPA e2e (`salon-scroll.spec.ts`)
 * can prove the virtualized list renders a large history in bounded time, lands at
 * the bottom, and keeps only a window of rows in the DOM.
 *
 * This is a SEPARATE fixture from the shared Salon fixture — the salon
 * differentials' oracles are pinned to `salon-main.db`/`salon-mount.db`, so this
 * MUST NOT reuse or mutate them. It shares only the committed test pepper + user
 * id (so the e2e's `.dbkey` + user-id rewrite work identically).
 *
 * Everything is deterministic (fixed ids, timestamps derived from a base by index,
 * canned content) so the committed .db is reproducible.
 *
 * Regenerate (Node 24, from the v4 checkout) + re-copy the committed .db files:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   W=$V5/.claude/worktrees/salon-virtualization-ced41b
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SALON_LONG_MAIN=$W/crates/quilltap-web/tests/fixtures/salon-long-main.db \
 *   QT_FIXTURE_SALON_LONG_MOUNT=$W/crates/quilltap-web/tests/fixtures/salon-long-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/build-long-chat-fixture.ts
 */

import { mkdtempSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// The committed test substrate (matches env.ts TEST_PEPPER / FIXTURE_USER).
const TEST_PEPPER_BASE64 = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const USER_ID = 'e18e05bc-63e8-4539-8a85-719b7a508850';
const BASE_TS = '2026-02-01T00:00:00.000Z';
const CHAT_ID = '10c00000-0000-4000-8000-000000000001';
const CONN_ID = 'c0000001-0000-4000-8000-000000000001';
const API_KEY_ID = 'a0000001-0000-4000-8000-000000000001';

// Characters + their participant ids (Cleo is the user's avatar; Aria + Bram LLM).
const CHARS = {
  aria: { id: 'a71a0000-0000-4000-8000-000000000001', name: 'Aria', controlledBy: 'llm' },
  bram: { id: 'b7a10000-0000-4000-8000-000000000002', name: 'Bram', controlledBy: 'llm' },
  cleo: { id: 'c1e00000-0000-4000-8000-000000000003', name: 'Cleo', controlledBy: 'user' },
} as const;
const PART = {
  aria: 'aaaa0000-0000-4000-8000-000000000001',
  bram: 'bbbb0000-0000-4000-8000-000000000002',
  cleo: 'cccc0000-0000-4000-8000-000000000003',
} as const;

const TOTAL_MESSAGES = 300;

interface GeneratedMessage {
  id: string;
  role: 'USER' | 'ASSISTANT';
  content: string;
  participantId: string | null;
  createdAt: string;
  targetParticipantIds?: string[];
  systemSender?: string;
  systemKind?: string;
}

/** Markdown-heavy assistant bodies — rotated so re-measure exercises varied heights. */
const ASSISTANT_BODIES = [
  `## The Ridge Ahead\n\nThree obstacles stand between us and the summit:\n\n- the scree field\n- a frozen traverse\n- the final chimney\n\nWe move at **first light**.`,
  `*She adjusts her goggles against the glare.* "The barometer is falling, Bram. We have not much daylight left, and the wind is turning."`,
  `Here is the cipher I mentioned:\n\n\`\`\`\nrot13(rot13("Quilltap")) === "Quilltap"\n\`\`\`\n\nApply it twice and you are precisely back where you began.`,
  `> The mountain does not care for our schedule.\n\nAnd yet *we* must keep to it. Onward, before the light goes.`,
  `The **Aurora** protocol (see \`docs/aurora.md\`) routes every whisper through the Salon's commonplace book. It is, in a word, *ingenious* — and a little alarming.`,
  `A short list of what we packed:\n\n1. Three days of rations\n2. The good rope, not the frayed one\n3. Bram's insufferable optimism\n\nIt will have to be enough.`,
];

/** User bodies — a couple carry roleplay actions to exercise the dialogue pass. */
const USER_BODIES = [
  `Good. What is our first move at dawn?`,
  `Tell me more about that cipher — I don't trust anything I can't undo.`,
  `*I check the ropes one more time, then nod.* Ready when you are.`,
  `How much daylight do we truly have left?`,
];

function tsAt(index: number): string {
  // One second apart, ascending — deterministic and strictly ordered.
  return new Date(Date.parse(BASE_TS) + index * 1000).toISOString();
}

/**
 * Generate the ~300-message flow: mostly alternating user/assistant, with a
 * whisper every ~20 turns and a RUN of consecutive staff announcements every ~15
 * turns (so the render-item announcement packing is genuinely exercised).
 */
function generateMessages(): GeneratedMessage[] {
  const out: GeneratedMessage[] = [];
  let i = 0;
  let assistantToggle = 0;
  while (out.length < TOTAL_MESSAGES) {
    const id = `f1000000-0000-4000-8000-${String(out.length + 1).padStart(12, '0')}`;
    const at = tsAt(i);

    // Every 15th slot: a short run of 2 staff announcements (packing).
    if (i > 0 && i % 15 === 0 && out.length + 2 <= TOTAL_MESSAGES) {
      out.push({
        id,
        role: 'ASSISTANT',
        content: 'Aria has nothing further to add this round.',
        participantId: null,
        createdAt: at,
        systemSender: 'host',
        systemKind: 'turn-pass',
      });
      out.push({
        id: `f1000000-0000-4000-8000-${String(out.length + 1).padStart(12, '0')}`,
        role: 'ASSISTANT',
        content: 'Bram passes the turn.',
        participantId: null,
        createdAt: tsAt(i + 1),
        systemSender: 'host',
        systemKind: 'turn-pass',
      });
      i += 2;
      continue;
    }

    // Every 20th slot: a private whisper from Aria to the user's character.
    if (i > 0 && i % 20 === 0) {
      out.push({
        id,
        role: 'ASSISTANT',
        content: `*She lowers her voice so only you can hear.* "Keep an eye on Bram — he is bluffing about the weather."`,
        participantId: PART.aria,
        createdAt: at,
        targetParticipantIds: [PART.cleo],
      });
      i += 1;
      continue;
    }

    // Otherwise alternate a user turn and an assistant turn.
    if (out.length % 2 === 0) {
      out.push({
        id,
        role: 'USER',
        content: USER_BODIES[out.length % USER_BODIES.length],
        participantId: PART.cleo,
        createdAt: at,
      });
    } else {
      const speaker = assistantToggle++ % 2 === 0 ? PART.aria : PART.bram;
      out.push({
        id,
        role: 'ASSISTANT',
        content: ASSISTANT_BODIES[out.length % ASSISTANT_BODIES.length],
        participantId: speaker,
        createdAt: at,
      });
    }
    i += 1;
  }
  return out.slice(0, TOTAL_MESSAGES);
}

async function main(): Promise<void> {
  const mainOut = process.env.QT_FIXTURE_SALON_LONG_MAIN;
  const mountOut = process.env.QT_FIXTURE_SALON_LONG_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_SALON_LONG_MAIN and QT_FIXTURE_SALON_LONG_MOUNT must point at the .db files',
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-long-chat-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER_BASE64;
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
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
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
  await ensureCollection('characters', CharacterSchema);

  // Materialize the doc-mount tables the character-vault scaffold writes to
  // (mirrors build-salon-fixture.ts).
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

  const repos = getRepositories();
  const TS = BASE_TS;

  // 1. User.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: USER_ID, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Default chat settings (mirrors the salon fixture's knobs).
  await repos.chatSettings.create(
    {
      userId: USER_ID,
      cheapLLMSettings: { strategy: 'PROVIDER_CHEAPEST', fallbackToLocal: true },
      contextCompressionSettings: { enabled: false },
      autoDetectRng: false,
      answerConfirmationSettings: { enabled: false },
      dangerousContentSettings: { mode: 'DETECT_ONLY' },
    } as never,
    { id: '5e000000-0000-4000-8000-000000000010', createdAt: TS, updatedAt: TS } as never,
  );

  // 3. One api key + one OPENAI_COMPATIBLE connection (baseUrl rewritten by the e2e).
  await ensureCollection('api_keys', ApiKeySchema);
  const apiKeyCol = await getCollection('api_keys');
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: API_KEY_ID,
      userId: USER_ID,
      label: 'mock-key',
      provider: 'OPENAI_COMPATIBLE',
      key_value: 'sk-synthetic-mock-key',
      isActive: true,
      createdAt: TS,
      updatedAt: TS,
    }) as never,
  );
  await repos.connections.create(
    {
      userId: USER_ID,
      name: 'Local Mock',
      provider: 'OPENAI_COMPATIBLE',
      modelName: 'mock-model',
      baseUrl: 'http://127.0.0.1:1/v1',
      apiKeyId: API_KEY_ID,
    } as never,
    { id: CONN_ID, createdAt: TS, updatedAt: TS } as never,
  );

  // 4. Characters (each mints a linked vault).
  for (const c of Object.values(CHARS)) {
    await repos.characters.create(
      {
        name: c.name,
        userId: USER_ID,
        description: null,
        aliases: [],
        controlledBy: c.controlledBy,
        talkativeness: 0.5,
        defaultConnectionProfileId: CONN_ID,
      } as never,
      { id: c.id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // 5. The chat + its participants.
  await repos.chats.create(
    {
      userId: USER_ID,
      title: 'The Long Expedition',
      chatType: 'salon',
      contextSummary: null,
      participants: [
        { id: PART.aria, type: 'CHARACTER', characterId: CHARS.aria.id, controlledBy: 'llm', status: 'active', createdAt: TS, updatedAt: TS },
        { id: PART.bram, type: 'CHARACTER', characterId: CHARS.bram.id, controlledBy: 'llm', status: 'active', createdAt: TS, updatedAt: TS },
        { id: PART.cleo, type: 'CHARACTER', characterId: CHARS.cleo.id, controlledBy: 'user', status: 'active', createdAt: TS, updatedAt: TS },
      ],
    } as never,
    { id: CHAT_ID, createdAt: TS, updatedAt: TS } as never,
  );

  // 6. ~300 messages through the REAL addMessages path.
  const messages = generateMessages();
  await repos.chats.addMessages(
    CHAT_ID,
    messages.map((m) => ({
      id: m.id,
      type: 'message',
      role: m.role,
      content: m.content,
      participantId: m.participantId ?? undefined,
      createdAt: m.createdAt,
      attachments: [],
      ...(m.targetParticipantIds ? { targetParticipantIds: m.targetParticipantIds } : {}),
      ...(m.systemSender ? { systemSender: m.systemSender } : {}),
      ...(m.systemKind ? { systemKind: m.systemKind } : {}),
    })) as never,
  );

  // Force the empty tables the read paths touch into existence — the Salon list,
  // chatGet (avatars → `files`), the turn query, and the delete-cascade each read
  // one of these, and an unmaterialized table surfaces as `no such table` at
  // runtime rather than an empty result.
  await ensureCollection('memories', (await import('@/lib/schemas/memory.types')).MemorySchema);
  await repos.backgroundJobs.findByUserId(USER_ID, 'PENDING');
  await repos.files.findByUserId(USER_ID);
  await repos.tags.findByIds([]);
  await repos.conversationChunks.findByChatId(CHAT_ID);
  await repos.vectorIndices.findMetaByCharacterId(CHARS.aria.id);
  await repos.vectorIndices.findEntriesByCharacterId(CHARS.aria.id);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built long-chat fixture: main=${mainOut} mount=${mountOut} (${messages.length} messages)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`long-chat fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
