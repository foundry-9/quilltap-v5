/**
 * Tier-2 SEED fixture builder (P4.D279 keystone, C1 §6) — `applyDisplacement`
 * (v4 `lib/wardrobe/outfit-displacement.ts`, `9ecebf986`).
 *
 * Creates the spec chats (ids + createdAt/updatedAt pinned, one with a
 * pre-seeded `equippedOutfit` for the take-off ops) via v4's REAL
 * `ChatsRepository.create`, and the wear ledger in v4's MIGRATION shape
 * (`WARDROBE_WEAR_STATS_DDL` — the `build-chats-outfits-fixture.ts` idiom; never
 * through the repository, whose generateDDL shape lacks the `ON CONFLICT`
 * index every increment targets). The wardrobe ITEMS the ops wear live only in
 * the spec (the lookup is in-memory, `pool.byId`'s shape), so no vault is built.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-outfit-displacement-main.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-outfit-displacement-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  chats: Array<Record<string, unknown> & { id: string; createdAt: string; updatedAt: string }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'outfit-displacement-tier2.json'), 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the seed fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-outfit-displacement-fixture-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

  await initializeDatabase();
  {
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    const { WARDROBE_WEAR_STATS_DDL } = await import(
      '@/lib/database/backends/sqlite/wardrobe-wear-stats-ddl'
    );
    const raw = getRawDatabase();
    if (!raw) throw new Error('main DB handle unavailable for the wear ledger');
    for (const sql of WARDROBE_WEAR_STATS_DDL) raw.exec(sql);
  }

  const repo = new ChatsRepository();
  for (const c of spec.chats) {
    const { id, createdAt, updatedAt, ...data } = c;
    await repo.create(data as never, { id, createdAt, updatedAt });
  }

  await closeDatabase();
  process.stderr.write(`built outfit-displacement seed fixture: ${out} (${spec.chats.length} chats)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`outfit-displacement fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
