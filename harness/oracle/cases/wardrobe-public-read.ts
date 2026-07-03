/**
 * Read-differential oracle — the PUBLIC wardrobe READ trio + shared-archetype tier.
 *
 * Opens the pre-seeded main + mount-index fixtures and drives v4's REAL
 * `repos.wardrobe.findByCharacterId` / `.findByIdForCharacter` (which fan out to
 * `getOverlaidWardrobeItems` + `findArchetypeById` -> `readGeneralWardrobe`) over
 * each case, emitting the resulting item list / single item / null. The Rust port
 * (db::wardrobe_read + archetype_wardrobe) reads the same fixtures and must produce
 * the same result exactly (no normalization -- every id/timestamp comes from the
 * shared fixture, no clock mint on a read path).
 *
 * Cases (see the spec's $comment):
 *   0. findByCharacterId(char, false)        -> active items only.
 *   1. findByCharacterId(char, true)         -> includes the archived item.
 *   2. findByIdForCharacter(char, composite) -> owned composite (componentItemIds
 *                                               resolved to archetype ids -- proves seeding).
 *   3. findByIdForCharacter(char, archeA)    -> archetype fallback (characterId null).
 *   4. findByIdForCharacter(char, no-such)   -> null.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WPR_MAIN=/tmp/qt-wpr-main.db QT_FIXTURE_WPR_MOUNT=/tmp/qt-wpr-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/wardrobe-public-read.ts \
 *     > /tmp/oracle-wpr.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  characterId: string;
  compositeId: string;
  archetypeAId: string;
  noSuchId: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'wardrobe-public-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_WPR_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WPR_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_WPR_MAIN and QT_FIXTURE_WPR_MOUNT must point at the seeded fixture .db files');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wpr-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'wpr-main-work.db');
  const mountWork = join(scratch, 'wpr-mount-work.db');
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

  await initializeDatabase();

  const repos = getRepositories();
  const wardrobe = repos.wardrobe;

  const results: Array<unknown> = [];
  // 0. active items only
  results.push(await wardrobe.findByCharacterId(spec.characterId, false));
  // 1. include archived
  results.push(await wardrobe.findByCharacterId(spec.characterId, true));
  // 2. owned composite by id (component refs resolved to archetype ids)
  results.push((await wardrobe.findByIdForCharacter(spec.characterId, spec.compositeId)) ?? null);
  // 3. archetype fallback
  results.push((await wardrobe.findByIdForCharacter(spec.characterId, spec.archetypeAId)) ?? null);
  // 4. unknown id -> null
  results.push((await wardrobe.findByIdForCharacter(spec.characterId, spec.noSuchId)) ?? null);

  await closeDatabase();

  process.stdout.write(JSON.stringify({ results }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
