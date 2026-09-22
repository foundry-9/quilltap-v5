/**
 * Builder for the committed `chat-compressed-{main,mount,llmlogs}.db` triple
 * (P4.D203 — v4 `186eb09cb` + `f45a517a9`).
 *
 * The ONE new committed fixture this round's §R.12 authorizes for P4.D203. It
 * exists because every other family that could prove the ledger's failure (b)
 * — "Create Backup succeeds with non-zero `llm_logs`" — reads a committed
 * fixture that §R.12 forbids rebuilding at the target pin.
 *
 * Built at the TARGET pin through v4's REAL repositories, so every cell of 512
 * bytes or more is a genuine brotli BLOB written by v4's own `documentToRow`:
 *
 *   main       — one user, one character, one chat, and messages covering all
 *                four registered `chat_messages` columns on both sides of the
 *                512-byte floor, plus one `conversation_chunks` row over it.
 *   mount      — the empty mount-index schema (the backup collector reads it).
 *   llmlogs    — two logs through v4's REAL `LLMLogsRepository`, both with
 *                serialized `request`/`response` over the floor. Written HERE
 *                rather than in the oracle case because jest's setup mocks the
 *                logging service (`jest.setup.ts:379`), so a jest oracle writes
 *                ZERO `llm_logs` rows.
 *
 * ⚠ This builder must be run from a worktree pinned at the target sha, and its
 * output is COMMITTED. Rebuilding it at a different pin re-encodes every cell
 * — read `docs/developer/porting/drift-ledger.md` §5.1 first.
 *
 * Run (Node 24, from a pinned v4 worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
 *   cd /tmp/qt-v4-pin-p4d203-f45a517a9
 *   QT_FIXTURE_CZ_MAIN=$W/crates/quilltap-web/tests/fixtures/chat-compressed-main.db \
 *   QT_FIXTURE_CZ_MOUNT=$W/crates/quilltap-web/tests/fixtures/chat-compressed-mount.db \
 *   QT_FIXTURE_CZ_LLM=$W/crates/quilltap-web/tests/fixtures/chat-compressed-llmlogs.db \
 *     $N/npx tsx $W/harness/oracle/fixtures/build-chat-compressed-fixture.ts
 */

import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  characterId: string;
  chatId: string;
  participantId: string;
  chunkId: string;
  seedTimestamp: string;
  messages: Array<Record<string, unknown>>;
  logs: Array<Record<string, unknown>>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'chat-compressed.json'), 'utf8'),
  ) as Spec;

  const mainOut = process.env.QT_FIXTURE_CZ_MAIN;
  const mountOut = process.env.QT_FIXTURE_CZ_MOUNT;
  const llmOut = process.env.QT_FIXTURE_CZ_LLM;
  if (!mainOut || !mountOut || !llmOut) {
    throw new Error('QT_FIXTURE_CZ_{MAIN,MOUNT,LLM} must point at the .db files to write');
  }
  for (const out of [mainOut, mountOut, llmOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cz-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.SQLITE_LLM_LOGS_PATH = llmOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { CharacterSchema, ChatMetadataSchema, ConversationChunkSchema } = await import(
    '@/lib/schemas/types'
  );
  const { UserSchema } = await import('@/lib/schemas/auth.types');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  // The chats table through the CURRENT schema, so the committed fixture is not
  // born one column behind: without this the repo's own lazy DDL omits
  // `transcriptVersion` and every write logs "Failed to bump transcript
  // version" (the same vintage gap the P4.94 widen closed for five other
  // committed pairs).
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('conversation_chunks', ConversationChunkSchema);

  const repos = getRepositories();

  await repos.users.create(
    { username: 'operator', email: null, name: 'The Operator' } as never,
    { id: spec.userId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never,
  );

  // No character row: `characters.create` calls `ensureCharacterVault`, which
  // needs the whole mount-index collection set this fixture has no use for.
  // The backup collector iterates characters for memories and simply yields
  // none — the arms under test are `llm_logs`, `chats`/`messages` and
  // `conversation_chunks`.

  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'The Compressed Interchange',
      participants: [
        {
          id: spec.participantId,
          type: 'CHARACTER',
          characterId: spec.characterId,
          controlledBy: 'llm',
          createdAt: spec.seedTimestamp,
          updatedAt: spec.seedTimestamp,
        },
      ],
    } as never,
    { id: spec.chatId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
  );

  // Every message goes through `addMessages`, so v4's real `documentToRow`
  // decides TEXT-vs-BLOB per cell exactly as production would.
  await repos.chats.addMessages(spec.chatId, spec.messages as never);

  await repos.conversationChunks.create(
    {
      chatId: spec.chatId,
      interchangeIndex: 0,
      content: (spec as unknown as { chunkContent: string }).chunkContent,
      participantNames: ['Lorian'],
      messageIds: [],
      embedding: null,
    } as never,
    { id: spec.chunkId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
  );

  // The llm-logs partition, through v4's REAL repository — a jest oracle
  // cannot do this (jest.setup mocks the logging service), which is why the
  // rows are baked here.
  for (const log of spec.logs) {
    const { id, ...data } = log as Record<string, unknown>;
    await repos.llmLogs.create(
      { userId: spec.userId, ...data } as never,
      { id: id as string, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never,
    );
  }

  closeLLMLogsSQLiteClient();
  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `built chat-compressed fixture: main=${mainOut} mount=${mountOut} llm=${llmOut} ` +
      `(1 chat, ${spec.messages.length} messages, 1 chunk, ${spec.logs.length} llm-logs)\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-compressed fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
