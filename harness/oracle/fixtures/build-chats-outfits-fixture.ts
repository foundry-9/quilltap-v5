/**
 * Tier-2 SEED fixture builder — the chats equipped-outfit ops (Phase-2, the chats
 * repo — sub-unit 6: getEquippedOutfit / getEquippedOutfitForCharacter /
 * setEquippedOutfit / removeEquippedItemFromAllChats).
 *
 * Creates the spec chats (ids + chat createdAt/updatedAt pinned) via v4's REAL
 * `repos.chats.create`. Some chats carry a pre-seeded `equippedOutfit` (slots in
 * SCHEMA field order top/bottom/footwear/accessories, characterId keys in sorted
 * order — see the module-header seam note in `chats_outfits.rs`). The outfit ops
 * mint NO timestamps/ids and never bump the CHAT's updatedAt (v4 `_update`
 * preserves it), so the chat-level timestamps stay at the pinned seed sentinel and
 * the whole `chats` dump is diffed EXACTLY (zero normalization).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-choutfit-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-outfits-fixture.ts
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
  const specPath = join(here, 'chats-outfits-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_OUT;
  if (!out) {
    throw new Error('QT_FIXTURE_OUT must point at the seed fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-choutfit-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

  await initializeDatabase();

  const repo = new ChatsRepository();
  for (const c of spec.chats) {
    const { id, createdAt, updatedAt, ...data } = c;
    await repo.create(data as never, { id, createdAt, updatedAt });
  }

  await closeDatabase();
  process.stderr.write(`built chats outfits seed fixture: ${out} (${spec.chats.length} chats)\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chats outfits fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
