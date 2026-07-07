/**
 * Tier-2 oracle case — W4.6b conversation-summary vault mirror + delete sweep
 * (v4 `lib/file-storage/conversation-summary-vault-bridge.ts`).
 *
 * These are module functions (not repo methods) that call `getRepositories()` +
 * the mount-index store primitives, so — like the store-backed cases — they run
 * directly under the tsx real-DB path (no jest, no model mock: the summary write
 * touches no LLM). We copy the shared seed fixture, run the fixed op sequence via
 * v4's REAL functions, then dump the five mount-index store tables so the Rust port
 * can be diffed against it.
 *
 * OP SEQUENCE (all fields pinned — see the spec):
 *   1-3. `writeConversationSummaryToVaults(...)` x3 (create, rename-in-place, distinct).
 *   4.   `repos.chats.delete(chat1, { syncVaults: true })`  — sweep chat1 from both vaults.
 *   5.   `repos.chats.delete(chat2, { syncVaults: false })` — chat2's files REMAIN.
 *
 * NORMALIZATION (done identically on both dumps by the Rust harness): every
 * mount-index row id is minted (vault provisioning + summary writes), so the harness
 * remaps ids to first-seen tokens in natural-key order ACROSS all five tables (FKs
 * verify by relationship) and placeholders timestamps. The link chunkCount is pinned
 * (reindex artifact). doc_mount_documents.content is diffed byte-for-byte.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_VSM_MAIN=/tmp/qt-vsm-main.db \
 *   QT_FIXTURE_VSM_MOUNT=/tmp/qt-vsm-mount.db \
 *     $N/npx tsx $V5/harness/oracle/cases/vault-summary-mirror-tier2.ts \
 *     > /tmp/oracle-vault-summary-mirror.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface WriteOp {
  chatId: string;
  chatTitle: string;
  summary: string;
  summaryGeneration: number;
  participantCharacterIds: string[];
  messageCount: number;
  firstMessageAt: string | null;
  lastMessageAt: string | null;
  updatedAt: string;
}
interface DeleteOp {
  chatId: string;
  syncVaults: boolean;
}
interface Spec {
  testPepperBase64: string;
  writes: WriteOp[];
  deletes: DeleteOp[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'vault-summary-mirror-tier2.json'), 'utf8')
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_VSM_MAIN;
  const mountFixture = process.env.QT_FIXTURE_VSM_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_VSM_MAIN and QT_FIXTURE_VSM_MOUNT must point at the fixtures from build-vault-summary-mirror-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vsm-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'vsm-main-work.db');
  const mountWork = join(scratch, 'vsm-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { writeConversationSummaryToVaults } = await import(
    '@/lib/file-storage/conversation-summary-vault-bridge'
  );

  await initializeDatabase();
  const repos = getRepositories();

  // 1-3: write (create / rename-in-place / distinct) the summary into both vaults.
  for (const w of spec.writes) {
    await writeConversationSummaryToVaults({
      chatId: w.chatId,
      chatTitle: w.chatTitle,
      summary: w.summary,
      summaryGeneration: w.summaryGeneration,
      participantCharacterIds: w.participantCharacterIds,
      messageCount: w.messageCount,
      firstMessageAt: w.firstMessageAt,
      lastMessageAt: w.lastMessageAt,
      updatedAt: w.updatedAt,
    });
  }

  // 4-5: delete the chats — one with syncVaults on (sweep), one off (leave files).
  for (const d of spec.deletes) {
    await repos.chats.delete(d.chatId, { syncVaults: d.syncVaults });
  }

  // MOUNT-INDEX db: the store tables, read directly through the raw handle.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable (degraded open?)');
  const dumpTable = (table: string, orderBy: string) => {
    const columns = (midb.pragma(`table_info(${table})`) as Array<{ name: string }>).map(
      (c) => c.name
    );
    const rawRows = midb.prepare(`SELECT * FROM ${table}`).all() as Array<
      Record<string, unknown>
    >;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  const points = dumpTable('doc_mount_points', 'name');
  const folders = dumpTable('doc_mount_folders', 'path');
  const files = dumpTable('doc_mount_files', 'sha256');
  const documents = dumpTable('doc_mount_documents', 'contentSha256');
  const links = dumpTable('doc_mount_file_links', 'relativePath');

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      case: 'vault-summary-mirror-tier2',
      points,
      folders,
      files,
      documents,
      links,
    }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`vault-summary-mirror-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
