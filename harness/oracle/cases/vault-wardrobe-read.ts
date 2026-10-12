/**
 * Read-differential oracle — the character vault WARDROBE read overlay.
 *
 * Opens the pre-seeded mount-index fixture and drives v4's REAL
 * `readCharacterVaultWardrobe` (lib/database/repositories/vault-overlay/vault-readers)
 * over each spec case, emitting the resulting `{ items } | null`. The Rust port
 * (db::vault_read_overlay::read_character_vault_wardrobe) reads the same fixture
 * and must produce the same result, exactly (no normalization — there is no clock
 * mint on this path; ids/timestamps come from the shared fixture).
 *
 * `9ecebf986`: the reader seeds nothing — component refs resolve against the
 * folder alone. P4.D279 (v4 `4a38222e6`, wardrobe folders): the read is
 * RECURSIVE through `isWardrobeItemDocumentPath` — store A now also holds a
 * nested item (`Formal/Evening/10-gown.md` → `folder: Formal/Evening`), a
 * one-deep item whose frontmatter `folder: Casual` key the path overrides
 * (`Formal/11-blazer.md` → `Formal`), two non-items under the reserved
 * `images/` (a `.webp` and a valid-looking `.md`), a nested
 * `Formal/instructions.md` (excluded at any depth), a `Notes/todo.md` with no
 * `types` (an item PATH the parser returns null for), and a composite whose
 * `componentItems` carry a cross-tier UUID (KEPT as written, bug 187) and an
 * unknown slug (dropped). Every item carries `folder` (`''` = the root).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_VAULT_WARDROBE_READ=/tmp/qt-vault-wardrobe-read-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-wardrobe-read.ts \
 *     > /tmp/oracle-vault-wardrobe-read.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, rmSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Case {
  mountPointId: string;
  characterId: string;
}
interface Spec {
  testPepperBase64: string;
  cases: Case[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'vault-wardrobe-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_VAULT_WARDROBE_READ;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_VAULT_WARDROBE_READ must point at the seeded fixture .db');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-vault-wardrobe-read-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'vault-wardrobe-read-mount-index-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { readCharacterVaultWardrobe } = await import(
    '@/lib/database/repositories/vault-overlay/vault-readers'
  );

  await initializeDatabase();

  const results: Array<unknown> = [];
  for (const c of spec.cases) {
    results.push(await readCharacterVaultWardrobe(c.mountPointId, c.characterId));
  }

  await closeDatabase();

  process.stdout.write(JSON.stringify({ results }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
