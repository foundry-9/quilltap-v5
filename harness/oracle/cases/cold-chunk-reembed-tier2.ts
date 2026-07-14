/**
 * Tier-2 oracle case — cold-chunk RE-EMBED enqueue (v4
 * `lib/scriptorium/cold-chunk-reembed.ts` `maybeEnqueueColdChunkReembed`).
 *
 * Copies the seed fixture, drives v4's REAL `maybeEnqueueColdChunkReembed` for
 * each of the three chats (debounce reset first — a fresh scan each), then emits:
 *   - a `counts` line ({ cold, warm, chunkless } return values), and
 *   - a `jobs` projection of the enqueued `EMBEDDING_GENERATE` rows: count +
 *     the flattened payload/priority/maxAttempts/type/status rows sorted by
 *     entityId (the job id + timestamps are minted → omitted).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_COLD_REEMBED=/tmp/qt-cold-reembed.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/cold-chunk-reembed-tier2.ts \
 *     > /tmp/oracle-cold-reembed.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  coldChatId: string;
  warmChatId: string;
  chunklessChatId: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'cold-reembed.json'), 'utf8'),
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_COLD_REEMBED;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_COLD_REEMBED must point at the seed fixture');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cold-reembed-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'cold-reembed.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { maybeEnqueueColdChunkReembed, _resetColdChunkReembedDebounceForTesting } = await import(
    '@/lib/scriptorium/cold-chunk-reembed'
  );

  await initializeDatabase();
  _resetColdChunkReembedDebounceForTesting();

  const cold = await maybeEnqueueColdChunkReembed(spec.userId, spec.coldChatId);
  // A second immediate scan is debounced (within the 5-minute window).
  const debounced = await maybeEnqueueColdChunkReembed(spec.userId, spec.coldChatId);
  const warm = await maybeEnqueueColdChunkReembed(spec.userId, spec.warmChatId);
  const chunkless = await maybeEnqueueColdChunkReembed(spec.userId, spec.chunklessChatId);

  // `status` is a nondeterministic artifact (the processor may claim a job), so
  // it is omitted; `type`/`priority`/`maxAttempts`/payload are the port's proof.
  const rawJobs =
    (await rawQuery<Array<Record<string, unknown>>>(
      'SELECT type, priority, maxAttempts, payload FROM background_jobs',
      [],
    )) ?? [];
  const jobs = rawJobs
    .map((j) => {
      const payload = JSON.parse(String(j.payload));
      return {
        type: j.type,
        priority: j.priority,
        maxAttempts: j.maxAttempts,
        entityType: payload.entityType,
        entityId: payload.entityId,
        chatId: payload.chatId,
        profileId: payload.profileId,
      };
    })
    .sort((a, b) => String(a.entityId).localeCompare(String(b.entityId)));

  // No-profile arm: with every embedding profile removed, a fresh scan of the
  // still-cold chat enqueues nothing (and adds no jobs — asserted by dumping
  // BEFORE this step).
  _resetColdChunkReembedDebounceForTesting();
  await rawQuery('DELETE FROM embedding_profiles', []);
  const noProfile = await maybeEnqueueColdChunkReembed(spec.userId, spec.coldChatId);

  process.stdout.write(
    JSON.stringify({ kind: 'counts', cold, debounced, warm, chunkless, noProfile }) + '\n',
  );
  process.stdout.write(JSON.stringify({ kind: 'jobs', count: jobs.length, rows: jobs }) + '\n');

  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`cold-chunk-reembed oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
