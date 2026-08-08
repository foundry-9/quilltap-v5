/**
 * Tier-3 SEED fixture builder — the chat-message orchestrator (Phase-3 Unit-3
 * wave 3, final unit; v4 `processMessage` / `executeTurnChain`).
 *
 * Bakes the seed state both differential sides start from, across BOTH databases:
 *   - one connection profile (`repos.connections.create`, pinned id) that every
 *     character defaults to (the responder resolution reads it);
 *   - the participant characters via v4's REAL `repos.characters.create` (pinned
 *     ids, full vault provisioning in the mount-index DB — buildContext's system
 *     prompt + next-speaker talkativeness read through the vault overlay), each
 *     carrying `defaultConnectionProfileId`;
 *   - one `chat_settings` row (danger DETECT_ONLY so no reroute, cheapLLM present,
 *     compression + autoDetectRng + answer-confirmation off);
 *   - the corpus chats via `repos.chats.create` (pinned ids/timestamps, CHARACTER
 *     participants only), each with its seed opener message(s) via
 *     `repos.chats.addMessages` (the summary-fold chat carries 11 interchanges so
 *     the finalizer-deferred summary check crosses the fold threshold).
 *
 * Both the jest oracle and the Rust test copy BOTH fixture files and run the
 * orchestrator sequence (which mints new ids/timestamps), so the seed rows are
 * byte-identical on both sides and only the send-path effect differs.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-orch-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-orch-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-orchestrator-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  description?: string;
  aliases?: string[];
  controlledBy: string;
  talkativeness: number;
  connectionProfileId: string;
  /** Application-state slim column (not vault-managed) — drives opaque-anywhere. */
  systemTransparency?: boolean;
  /** W4.10a: a Carina answerer (reachable by anyone via `@Name:` / `ask_carina`). */
  canBeCarina?: boolean;
}
interface ParticipantSpec {
  id: string;
  character: string;
  controlledBy?: string;
  status?: string;
  /** W4.10a: per-participant connection-profile override (wins over the
   *  character default) — lets a case pin a distinct provider/model. */
  connectionProfileId?: string;
}
interface MessageSpec {
  id: string;
  role: string;
  content: string;
  participantId: string | null;
  /** Staff-whisper sender (systemSender != null → a whisper). */
  systemSender?: string | null;
  systemKind?: string | null;
  opaqueContent?: string | null;
  targetParticipantIds?: string[] | null;
  attachments?: string[];
}
interface ChatSpec {
  id: string;
  chatType: string;
  contextSummary: string | null;
  participants: ParticipantSpec[];
  messages: MessageSpec[];
  /** W4.1g: per-chat disabled tool ids (the wire slate excludes them). */
  disabledTools?: string[];
  /** W4.4: the Chat level of the agent-mode cascade (nullable — null = not set). */
  agentModeEnabled?: boolean;
  /** W4.4: seeded agent turn count (the reset gate zeroes it on a new user turn). */
  agentTurnCount?: number;
  /** W4.2u: the operator Concierge flip (`'OFF'` = off-duty → danger resolves OFF). */
  conciergeOverride?: string | null;
  /** W4.2u: the classification label (dangerous chat → the first-branch reroute). */
  isDangerousChat?: boolean;
  /** P4.d2: the "nothing to add" per-chat toggle (b90cd1f5). NULL/absent = on. */
  turnSkippingEnabled?: boolean;
  /** P4.D60 (Bug 50/51): the impersonation overlay seed — participant ids the
   * human is impersonating this session (their `controlledBy` stays 'llm'). */
  impersonatingParticipantIds?: string[];
  /** P4.D60 (Bug 51): the seat the human is currently speaking as. */
  activeTypingParticipantId?: string | null;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  connectionProfiles: Array<{
    id: string;
    name: string;
    provider: string;
    modelName: string;
    /** W4.4a4: `'courier'` for the manual/clipboard transport. */
    transport?: string;
    /** W4.2u: the uncensored-reroute target carries these. */
    apiKeyId?: string;
    isDangerousCompatible?: boolean;
  }>;
  chatSettings: {
    id: string;
    // finding #27: `defaultCheapProfileId` selects a configured cheap profile
    // (priority 1 of `getCheapLLMProvider`), so the summary/fold cheap-LLM call
    // runs on THAT profile — not `getCheapestModel(responding.provider)`.
    cheapLLMSettings: { strategy: string; fallbackToLocal: boolean; defaultCheapProfileId?: string };
    compressionEnabled: boolean;
    autoDetectRng: boolean;
    answerConfirmationEnabled: boolean;
    dangerMode: string;
    /** W4.4: the Global level of the agent-mode cascade. */
    agentModeSettings?: { maxTurns: number; defaultEnabled: boolean };
  };
  characters: CharacterSpec[];
  chats: ChatSpec[];
  /**
   * W4.10a: `apiKeyId → SYNTHETIC key value` seeded into the `api_keys` table so
   * the danger-reroute path resolves the key off the REAL table (the Rust
   * `DbApiKeys` resolver reads the same rows). The provider is derived from the
   * connection profile that references each key. Never a real key.
   */
  apiKeys?: Record<string, string>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'orchestrator-tier3.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_FIXTURE_OUT;
  const outMount = process.env.QT_FIXTURE_MOUNT_OUT;
  if (!outMain || !outMount) {
    throw new Error('QT_FIXTURE_OUT and QT_FIXTURE_MOUNT_OUT must point at the fixture .db files');
  }
  for (const base of [outMain, outMount]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-orch-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, ensureCollection, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  const repos = getRepositories();

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) {
      midb.exec(sql);
    }
  }

  // [40319484] `gcOrphanedFileRow` runs inside every content-addressed rewrite
  // and deletes from `doc_mount_blobs` unconditionally, so a mount index that
  // lacks the table now throws `no such table` on the SECOND write to any path.
  // v4's blobs repository creates it lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts:113`); copied verbatim so the fixture carries
  // exactly the table a real instance has (it is in `fresh_schema.json` too).
  midb.exec(`
    CREATE TABLE IF NOT EXISTS "doc_mount_blobs" (
      "id" TEXT PRIMARY KEY,
      "fileId" TEXT NOT NULL,
      "sha256" TEXT NOT NULL,
      "sizeBytes" INTEGER NOT NULL,
      "storedMimeType" TEXT NOT NULL,
      "data" BLOB NOT NULL,
      "createdAt" TEXT NOT NULL,
      "updatedAt" TEXT NOT NULL,
      FOREIGN KEY ("fileId") REFERENCES "doc_mount_files" ("id") ON DELETE CASCADE
    )
  `);
  midb.exec(
    'CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")'
  );

  // Connection profiles (the uncensored-reroute target carries `apiKeyId` +
  // `isDangerousCompatible`).
  for (const cp of spec.connectionProfiles) {
    await repos.connections.create(
      {
        userId: spec.userId,
        name: cp.name,
        provider: cp.provider,
        modelName: cp.modelName,
        ...(cp.transport !== undefined ? { transport: cp.transport } : {}),
        ...(cp.apiKeyId !== undefined ? { apiKeyId: cp.apiKeyId } : {}),
        ...(cp.isDangerousCompatible !== undefined
          ? { isDangerousCompatible: cp.isDangerousCompatible }
          : {}),
      } as never,
      { id: cp.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  // W4.10a: seed the `api_keys` table (MAIN db) with the SYNTHETIC reroute keys so
  // `DbApiKeys` / v4's `findApiKeyByIdAndUserId` resolve the key material off the
  // REAL table. The id IS the referencing profile's `apiKeyId` (a raw INSERT since
  // `createApiKey` mints its own id); the provider is the referencing profile's.
  if (spec.apiKeys && Object.keys(spec.apiKeys).length > 0) {
    const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
    await ensureCollection('api_keys', ApiKeySchema);
    for (const [apiKeyId, keyValue] of Object.entries(spec.apiKeys)) {
      const owner = spec.connectionProfiles.find((cp) => cp.apiKeyId === apiKeyId);
      const provider = owner?.provider ?? 'OPENROUTER';
      await rawQuery(
        'INSERT INTO api_keys (id, userId, label, provider, key_value, isActive, lastUsed, createdAt, updatedAt) ' +
          'VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)',
        [
          apiKeyId,
          spec.userId,
          'reroute-key',
          provider,
          keyValue,
          1,
          null,
          spec.seedTimestamp,
          spec.seedTimestamp,
        ]
      );
    }
  }

  // Characters (full vault provisioning, pinned ids + talkativeness + default cp).
  for (const c of spec.characters) {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        description: c.description ?? null,
        aliases: c.aliases ?? [],
        controlledBy: c.controlledBy,
        talkativeness: c.talkativeness,
        defaultConnectionProfileId: c.connectionProfileId,
        ...(c.systemTransparency !== undefined ? { systemTransparency: c.systemTransparency } : {}),
        ...(c.canBeCarina !== undefined ? { canBeCarina: c.canBeCarina } : {}),
      } as never,
      { id: c.id }
    );
  }

  // Chat settings (danger DETECT_ONLY, cheapLLM, compression off, no autoDetectRng,
  // confirmation off).
  await repos.chatSettings.create(
    {
      userId: spec.userId,
      cheapLLMSettings: spec.chatSettings.cheapLLMSettings,
      contextCompressionSettings: { enabled: spec.chatSettings.compressionEnabled },
      autoDetectRng: spec.chatSettings.autoDetectRng,
      answerConfirmationSettings: { enabled: spec.chatSettings.answerConfirmationEnabled },
      dangerousContentSettings: { mode: spec.chatSettings.dangerMode },
      ...(spec.chatSettings.agentModeSettings !== undefined
        ? { agentModeSettings: spec.chatSettings.agentModeSettings }
        : {}),
    } as never,
    { id: spec.chatSettings.id }
  );

  // Chats + their opener messages.
  for (const chat of spec.chats) {
    const participants = chat.participants.map((p) => ({
      id: p.id,
      type: 'CHARACTER',
      characterId: p.character,
      controlledBy: p.controlledBy ?? 'llm',
      status: p.status ?? 'active',
      ...(p.connectionProfileId !== undefined
        ? { connectionProfileId: p.connectionProfileId }
        : {}),
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }));
    await repos.chats.create(
      {
        userId: spec.userId,
        title: `Fixture ${chat.id}`,
        participants,
        chatType: chat.chatType,
        contextSummary: chat.contextSummary,
        ...(chat.disabledTools !== undefined ? { disabledTools: chat.disabledTools } : {}),
        ...(chat.agentModeEnabled !== undefined ? { agentModeEnabled: chat.agentModeEnabled } : {}),
        ...(chat.agentTurnCount !== undefined ? { agentTurnCount: chat.agentTurnCount } : {}),
        ...(chat.conciergeOverride !== undefined ? { conciergeOverride: chat.conciergeOverride } : {}),
        ...(chat.isDangerousChat !== undefined ? { isDangerousChat: chat.isDangerousChat } : {}),
        ...(chat.turnSkippingEnabled !== undefined
          ? { turnSkippingEnabled: chat.turnSkippingEnabled }
          : {}),
        ...(chat.impersonatingParticipantIds !== undefined
          ? { impersonatingParticipantIds: chat.impersonatingParticipantIds }
          : {}),
        ...(chat.activeTypingParticipantId !== undefined
          ? { activeTypingParticipantId: chat.activeTypingParticipantId }
          : {}),
      } as never,
      { id: chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
    if (chat.messages.length > 0) {
      await repos.chats.addMessages(
        chat.id,
        chat.messages.map((m) => ({
          id: m.id,
          type: 'message',
          role: m.role,
          content: m.content,
          participantId: m.participantId ?? undefined,
          createdAt: spec.seedTimestamp,
          attachments: m.attachments ?? [],
          ...(m.systemSender !== undefined ? { systemSender: m.systemSender } : {}),
          ...(m.systemKind !== undefined ? { systemKind: m.systemKind } : {}),
          ...(m.opaqueContent !== undefined ? { opaqueContent: m.opaqueContent } : {}),
          ...(m.targetParticipantIds !== undefined
            ? { targetParticipantIds: m.targetParticipantIds }
            : {}),
        })) as never
      );
    } else {
      await repos.chats.getMessageCount(chat.id);
    }
  }

  // Force the tables the Rust port reads/writes but owns no DDL for into
  // existence (empty). v4 creates them lazily on first repository access.
  //   - background_jobs: the summary-check title-update / memory-extraction rows;
  //   - memories / vector_indices / vector_entries: buildContext's memory search
  //     reads `memories` (empty → no results) and the gate reads the vector store.
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');
  await repos.memories.findAll();
  await repos.vectorIndices.findMetaByCharacterId(spec.characters[0].id);
  await repos.vectorIndices.findEntriesByCharacterId(spec.characters[0].id);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built orchestrator fixture: ${outMain} + ${outMount} ` +
      `(${spec.characters.length} characters, ${spec.chats.length} chats)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`orchestrator fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
