/**
 * Tier-2 oracle case — the stale-chat CACHE collapse (v4
 * `lib/background-jobs/maintenance/collapse-stale-chat-caches.ts`
 * `collapseStaleChatCaches`).
 *
 * Copies the seed fixture, drives v4's REAL `collapseStaleChatCaches(nowMs)`
 * (nowMs fixed by the spec so both ports agree on "stale"), then emits:
 *   - a `summary` line (the StaleChatCacheCollapseSummary);
 *   - a `chats` projection (id + the two cleared columns + updatedAt — proving
 *     the stale chat's caches are NULL, the active chat's survive, and neither
 *     updatedAt was bumped);
 *   - a `messages` projection (id/chatId + the five discardable columns + content
 *     — the stale chat's laden message goes all-NULL keeping content; the active
 *     chat's message survives);
 *   - a `chunks` projection (id/chatId/content + `embeddingNull` + the raw
 *     `updatedAt` — the stale chat's OLD embedded chunk is cold-tiered (NULL,
 *     stamp minted); its already-cold chunk is skipped by the `IS NOT NULL`
 *     guard; its two WARM chunks (one stamped EXACTLY at the retention cutoff,
 *     one inside the window) are spared by v4 f7cc887b's `updatedAt < cutoff`
 *     age guard and keep their seeded stamps; the active chat's chunk survives).
 *
 * The chunk `updatedAt` value on a CLEARED row is minted (v4 stamps
 * `new Date()`, the Rust sweep injects `nowMs`), so the Rust harness
 * placeholders any stamp that is not the row's own seeded value — which makes
 * "spared" and "cleared" a byte-level distinction rather than a boolean.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_RETENTION_CACHES=/tmp/qt-retention-caches.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/collapse-stale-chat-caches-tier2.ts \
 *     > /tmp/oracle-retention-caches.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  nowIso: string;
  seedTs: string;
  chunks: Array<{ id: string; updatedAt: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'retention-caches.json'), 'utf8'),
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_RETENTION_CACHES;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_RETENTION_CACHES must point at the seed fixture');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-retention-caches-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'retention-caches.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { collapseStaleChatCaches } = await import(
    '@/lib/background-jobs/maintenance/collapse-stale-chat-caches'
  );

  await initializeDatabase();

  const nowMs = new Date(spec.nowIso).getTime();
  const summary = await collapseStaleChatCaches(nowMs);

  const chats = await rawQuery<Array<Record<string, unknown>>>(
    'SELECT id, compressionCache, renderedMarkdown, updatedAt FROM chats ORDER BY id ASC',
    [],
  );
  const messages = await rawQuery<Array<Record<string, unknown>>>(
    `SELECT id, chatId, rawResponse, reasoningContent, reasoningSegments,
            renderedHtml, debugMemoryLogs, content
       FROM chat_messages ORDER BY id ASC`,
    [],
  );
  // P4.D25: each chunk now carries its OWN seeded `updatedAt` (the warmth
  // window is what is under test), so the projection emits the raw value and
  // the Rust harness placeholders anything that is not the row's seeded stamp.
  // A `<ts>` therefore means "this row was cold-tiered", exactly.
  const chunks = await rawQuery<Array<Record<string, unknown>>>(
    `SELECT id, chatId, content, updatedAt,
            CASE WHEN embedding IS NULL THEN 1 ELSE 0 END AS embeddingNull
       FROM conversation_chunks ORDER BY id ASC`,
    [],
  );

  process.stdout.write(JSON.stringify({ kind: 'summary', summary }) + '\n');
  process.stdout.write(JSON.stringify({ kind: 'chats', rows: chats ?? [] }) + '\n');
  process.stdout.write(JSON.stringify({ kind: 'messages', rows: messages ?? [] }) + '\n');
  process.stdout.write(JSON.stringify({ kind: 'chunks', rows: chunks ?? [] }) + '\n');

  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`collapse-stale-chat-caches oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
