/**
 * P4.43 unit 7 oracle — conversation-summaries regeneration (v4
 * `lib/background-jobs/queue-service.ts` `enqueueRegenerateConversationSummaries`
 * + `lib/background-jobs/handlers/regenerate-conversation-summaries.ts`
 * `handleRegenerateConversationSummaries` + the status count from
 * `app/api/v1/system/conversation-summaries/route.ts`).
 *
 * All of these call `getRepositories()` + the vault-mirror bridge and touch no
 * LLM, so they run directly under the tsx real-DB path (like the store-backed
 * cases). We copy the committed seed fixture, run the op sequence via v4's REAL
 * functions, and emit ONE JSON object the Rust port is diffed against:
 *   - `statusBefore` / `statusAfter` — the `{success, inFlight}` count;
 *   - `enqueue1` / `enqueue2` — `{success, jobId:'<jobId>', message}` + `isNew`
 *     (enqueue2 must dedupe: isNew=false, "already in flight" message);
 *   - `jobs` — the background_jobs dump (type/status/maxAttempts/payload) after the
 *     two enqueues: exactly ONE REGENERATE_CONVERSATION_SUMMARIES row, maxAttempts 1;
 *   - the FIVE mount-index store tables AFTER the handler runs — the handler
 *     mirrors ONLY the summarized chats WITH participants (Council → both vaults;
 *     Cassia's Journal → one vault) and skips the whitespace / null / no-participant
 *     chats. Normalized identically to the vault-summary-mirror family (shared
 *     minted-id remap + timestamp placeholders, done Rust-side).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_CSR_MAIN=$V5/crates/quilltap-web/tests/fixtures/conversation-summaries-regen-main.db \
 *   QT_CSR_MOUNT=$V5/crates/quilltap-web/tests/fixtures/conversation-summaries-regen-mount.db \
 *     $N/npx tsx $V5/harness/oracle/cases/conversation-summaries-regen-tier2.ts \
 *     > /tmp/oracle-conversation-summaries-regen.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

// v4's exact route response strings (route.ts:49-51).
const MSG_NEW =
  'Conversation summaries are being re-mirrored into the character vaults in the background.';
const MSG_EXISTING = 'A summary regeneration is already in flight; the existing one will complete.';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'conversation-summaries-regen.json'), 'utf8')
  ) as Spec;

  const mainFixture = process.env.QT_CSR_MAIN;
  const mountFixture = process.env.QT_CSR_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_CSR_MAIN and QT_CSR_MOUNT must point at the fixtures from build-conversation-summaries-regen-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-csr-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'csr-main-work.db');
  const mountWork = join(scratch, 'csr-mount-work.db');
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
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { enqueueRegenerateConversationSummaries } = await import(
    '@/lib/background-jobs/queue-service'
  );
  const { handleRegenerateConversationSummaries } = await import(
    '@/lib/background-jobs/handlers/regenerate-conversation-summaries'
  );

  await initializeDatabase();
  const repos = getRepositories();
  const userId = spec.userId;

  // v4's route status logic (route.ts handleRegenerateStatus).
  const inFlight = async (): Promise<number> => {
    const [pending, processing] = await Promise.all([
      repos.backgroundJobs.findByUserId(userId, 'PENDING'),
      repos.backgroundJobs.findByUserId(userId, 'PROCESSING'),
    ]);
    return [...pending, ...processing].filter(
      (j: { type: string }) => j.type === 'REGENERATE_CONVERSATION_SUMMARIES'
    ).length;
  };
  const enqueueResponse = (r: { jobId: string; isNew: boolean }) => ({
    response: { success: true, jobId: '<jobId>', message: r.isNew ? MSG_NEW : MSG_EXISTING },
    isNew: r.isNew,
  });

  const statusBefore = { success: true, inFlight: await inFlight() };
  const enqueue1 = enqueueResponse(await enqueueRegenerateConversationSummaries(userId));
  const enqueue2 = enqueueResponse(await enqueueRegenerateConversationSummaries(userId));
  const statusAfter = { success: true, inFlight: await inFlight() };

  // background_jobs dump (main db).
  const maindb = getRawDatabase();
  if (!maindb) throw new Error('main DB handle unavailable');
  const jobs = canonicalizeRows({
    table: 'background_jobs',
    columns: ['type', 'status', 'maxAttempts', 'payload'],
    rawRows: maindb
      .prepare('SELECT type, status, maxAttempts, payload FROM background_jobs')
      .all() as Array<Record<string, unknown>>,
    orderBy: 'type',
  });

  // Run the handler (the backfill). It needs only job.userId + job.id.
  const job = { id: 'regen-oracle-job', userId } as never;
  await handleRegenerateConversationSummaries(job);

  // Dump the five mount-index store tables (like vault-summary-mirror).
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const dumpTable = (table: string, orderBy: string) => {
    const columns = (midb.pragma(`table_info(${table})`) as Array<{ name: string }>).map(
      (c) => c.name
    );
    const rawRows = midb.prepare(`SELECT * FROM ${table}`).all() as Array<Record<string, unknown>>;
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
      case: 'conversation-summaries-regen-tier2',
      statusBefore,
      enqueue1,
      enqueue2,
      statusAfter,
      jobs,
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
  process.stderr.write(`conversation-summaries-regen oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
