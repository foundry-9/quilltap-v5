/**
 * Oracle case — v4's PUBLIC wardrobe write path (seam #7):
 * `repos.wardrobe.create` / `.update` / `.delete`.
 *
 * Copies the baked fixtures (a character + empty vault), then drives the REAL
 * public repo through the corpus op sequence. After each op it reads the target
 * character's `Wardrobe/` back via `readCharacterVaultWardrobe` (the verified-
 * equivalent read) and records the op's result tag + the read-back item list. The
 * Rust port composes its ported leaves over a copy of the same fixture and diffs
 * (harness normalizes each item's minted `updatedAt`).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WPUB_MAIN=/tmp/qt-wpub-main.db \
 *   QT_FIXTURE_WPUB_MOUNT=/tmp/qt-wpub-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-wardrobe-public.ts \
 *     > /tmp/oracle-vault-wardrobe-public.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CreateOp {
  op: 'create';
  readBack: string;
  data: Record<string, unknown>;
  options: { id: string; createdAt: string; updatedAt: string };
}
interface UpdateOp {
  op: 'update';
  readBack: string;
  id: string;
  characterId: string;
  patch: Record<string, unknown>;
}
interface DeleteOp {
  op: 'delete';
  readBack: string;
  id: string;
  characterId: string;
}
type AnyOp = CreateOp | UpdateOp | DeleteOp;

interface Spec {
  testPepperBase64: string;
  characterId: string;
  ops: AnyOp[];
}

function classify(err: unknown): string {
  const msg = err instanceof Error ? err.message : String(err);
  if (msg.includes('component cycle')) return 'cycle';
  if (msg.includes('mount is available')) return 'nomount';
  return `other:${msg}`;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'vault-wardrobe-public-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_WPUB_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WPUB_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_WPUB_MAIN and QT_FIXTURE_WPUB_MOUNT must point at the fixtures from build-vault-wardrobe-public-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wpub-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'wpub-main-work.db');
  const mountWork = join(scratch, 'wpub-mount-work.db');
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
  const { readCharacterVaultWardrobe } = await import(
    '@/lib/database/repositories/vault-overlay/vault-readers'
  );
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();
  const repos = getRepositories();

  // Resolve the read-back character's mount once (same for every op here).
  const owner = await repos.characters.findByIdRaw(spec.characterId);
  const mountPointId = owner?.characterDocumentMountPointId as string | undefined;
  if (!mountPointId) throw new Error('baked character has no vault mount');

  const readBack = async (characterId: string): Promise<unknown[]> => {
    const vault = await readCharacterVaultWardrobe(mountPointId, characterId);
    return vault?.items ?? [];
  };

  const results: unknown[] = [];
  for (const op of spec.ops) {
    let result: unknown;
    try {
      if (op.op === 'create') {
        await repos.wardrobe.create(op.data as never, op.options as never);
        result = { kind: 'ok' };
      } else if (op.op === 'update') {
        const item = await repos.wardrobe.update(op.id, op.patch as never, op.characterId);
        result = item ? { kind: 'ok' } : { kind: 'none' };
      } else {
        const deleted = await repos.wardrobe.delete(op.id, op.characterId);
        result = { kind: 'deleted', value: deleted };
      }
    } catch (err) {
      result = { kind: 'threw', reason: classify(err) };
    }
    const items = await readBack(op.readBack);
    results.push({ op: op.op, result, items });
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'vault-wardrobe-public', results }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`vault-wardrobe-public oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
