/**
 * Tier-2 fixture builder — the shared starting DB for the `chat_informs` repo
 * (P4.D205, v4 `e7d77bb60`), from the committed plaintext spec
 * (`chat-informs-tier2.json`).
 *
 * Same shape as build-chat-documents-fixture.ts: the `chat_informs` table is
 * created by v4's OWN `ensureCollection('chat_informs', ChatInformSchema)`, so
 * the DDL is identical to v4's generateDDL surface by construction — which is
 * also the surface v5 follows (see `crates/quilltap-core/src/db/chat_informs.rs`
 * on the two-DDL measurement). Seed rows go in through the real
 * `ChatInformsRepository.create` with id + timestamps pinned (CreateOptions),
 * so the starting state is fully deterministic.
 *
 * The seed's INSERTION ORDER carries a proof: `gamma` is written after `alpha`
 * but sorts before it (same `createdAt`, smaller `id`), so the rowid order and
 * the sorted order disagree and v4's `id.localeCompare` tiebreak is observable.
 *
 * The output file is the SEED-ONLY starting state. The op sequence under test is
 * applied later, by `cases/chat-informs-tier2.ts` (oracle) and the Rust harness,
 * each on its own fresh copy.
 *
 * Run from a v4 checkout pinned at the target under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd /tmp/qt-v4-pin-p4d205-f45a517a9
 *   QT_FIXTURE_OUT=/tmp/p4d205/qt-chat-informs-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chat-informs-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SeedRow {
  id: string;
  chatId: string;
  batchId: string;
  participantId: string;
  contentMarkdown: string;
  recordMessageId: string | null;
  createdAt: string;
  updatedAt: string;
  consumedAt: string | null;
  consumedByMessageId: string | null;
}

interface Spec {
  testPepperBase64: string;
  seed: SeedRow[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'chat-informs-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the fixture .db to write');
  }

  // Fresh output: drop any prior fixture so we never seed on top of stale state.
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chat-informs-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // writable path uses journal_mode = TRUNCATE
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { ChatInformsRepository } = await import(
    '@/lib/database/repositories/chat-informs.repository'
  );
  const { ChatInformSchema } = await import('@/lib/schemas/chat-inform.types');

  await initializeDatabase();
  await ensureCollection('chat_informs', ChatInformSchema);

  const repo = new ChatInformsRepository();
  for (const row of spec.seed) {
    await repo.create(
      {
        chatId: row.chatId,
        batchId: row.batchId,
        participantId: row.participantId,
        contentMarkdown: row.contentMarkdown,
        recordMessageId: row.recordMessageId,
        consumedAt: row.consumedAt,
        consumedByMessageId: row.consumedByMessageId,
      } as never,
      { id: row.id, createdAt: row.createdAt, updatedAt: row.updatedAt }
    );
  }

  await closeDatabase();

  process.stderr.write(
    `built chat_informs fixture: ${spec.seed.length} seed rows → ${out}\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-informs fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
