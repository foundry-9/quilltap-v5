/**
 * Oracle case: v4's REAL read of the compressed `llm_logs` / `chat_messages` /
 * `conversation_chunks` columns over the committed `chat-compressed-*` triple
 * (P4.D203 — the drift ledger's failure (b)).
 *
 * The ledger measured that on a v4-4.10 instance v5's backup "SILENTLY contains
 * ZERO `llm_logs` rows": `services/backup/collect.rs`'s `F::Json` bind raised
 * `InvalidColumnType` on a compressed payload and `backup/mod.rs`'s
 * `.unwrap_or_default()` swallowed it. Nothing errored; the backup was simply
 * missing a partition.
 *
 * v4's `collectUserData` is module-private, but the call it makes for that
 * partition is `repos.llmLogs.findAll(10000)` — so this case drives that exact
 * REAL repository method, plus `repos.chats.find` + `getMessages` and
 * `repos.conversationChunks.findByChatId` for the other two compressed
 * surfaces. The Rust side runs `collect_user_data` and compares its own
 * `llm_logs` / `chats[].messages` / `conversation_chunks` arrays.
 *
 * Run (Node 24, from a pinned v4 worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
 *   cd /tmp/qt-v4-pin-p4d203-f45a517a9
 *   QT_FIXTURE_CZ_MAIN=$W/crates/quilltap-web/tests/fixtures/chat-compressed-main.db \
 *   QT_FIXTURE_CZ_MOUNT=$W/crates/quilltap-web/tests/fixtures/chat-compressed-mount.db \
 *   QT_FIXTURE_CZ_LLM=$W/crates/quilltap-web/tests/fixtures/chat-compressed-llmlogs.db \
 *     $N/npx tsx $W/harness/oracle/cases/chat-compressed-collect.ts \
 *       > /tmp/p4.d203/oracle-chat-compressed.ndjson
 *
 * The fixture is opened from a COPY in a scratch dir, so the committed bytes
 * are never touched.
 */

import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  chatId: string;
}

const emit = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'chat-compressed.json'), 'utf8'),
  ) as Spec;

  const src = {
    main: process.env.QT_FIXTURE_CZ_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CZ_MOUNT ?? '',
    llm: process.env.QT_FIXTURE_CZ_LLM ?? '',
  };
  for (const [k, v] of Object.entries(src)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cz-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = {
    main: join(scratch, 'main.db'),
    mount: join(scratch, 'mount.db'),
    llm: join(scratch, 'llm.db'),
  };
  copyFileSync(src.main, work.main);
  copyFileSync(src.mount, work.mount);
  copyFileSync(src.llm, work.llm);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work.main;
  process.env.SQLITE_MOUNT_INDEX_PATH = work.mount;
  process.env.SQLITE_LLM_LOGS_PATH = work.llm;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  try {
    const repos = getRepositories();

    // The llm-logs partition — `collectUserData`'s own call, verbatim.
    const logs = await repos.llmLogs.findAll(10000);
    emit({
      kind: 'llm_logs',
      count: logs.length,
      rows: (logs as Array<Record<string, unknown>>)
        .map((l) => ({
          id: l.id,
          type: l.type,
          provider: l.provider,
          modelName: l.modelName,
          request: l.request,
          response: l.response,
        }))
        .sort((a, b) => String(a.id).localeCompare(String(b.id))),
    });

    // The four compressed `chat_messages` columns, through the hydrating read.
    const events = (await repos.chats.getMessages(spec.chatId)) as Array<
      Record<string, unknown>
    >;
    emit({
      kind: 'chat_messages',
      count: events.length,
      rows: events.map((e) => ({
        id: e.id,
        type: e.type,
        content: e.content ?? null,
        opaqueContent: e.opaqueContent ?? null,
        context: e.context ?? null,
        description: e.description ?? null,
      })),
    });

    // `conversation_chunks.content`.
    const chunks = (await repos.conversationChunks.findByChatId(spec.chatId)) as Array<
      Record<string, unknown>
    >;
    emit({
      kind: 'conversation_chunks',
      count: chunks.length,
      rows: chunks.map((c) => ({
        id: c.id,
        interchangeIndex: c.interchangeIndex,
        content: c.content ?? null,
      })),
    });
  } finally {
    closeLLMLogsSQLiteClient();
    closeMountIndexSQLiteClient();
    await closeDatabase();
    rmSync(scratch, { recursive: true, force: true });
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-compressed-collect oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
