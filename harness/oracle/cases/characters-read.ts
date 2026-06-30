/**
 * Read-differential oracle — v4's `CharactersRepository` findBy* queries (Phase-2,
 * the store-backed capstone sub-unit 4c).
 *
 * Opens a COPY of the pre-baked fixture (main + mount-index) and drives v4's REAL
 * repository read methods for each spec query, emitting the hydrated result lists.
 * The Rust port (db::characters_read) reads the same fixture and must produce the
 * same lists — exactly, except physicalDescription's createdAt/updatedAt (minted at
 * hydration on each side, placeholdered by the harness).
 *
 * Id-taking queries (findById / findByIdRaw / findByIds) carry a targetName; we
 * resolve it to the minted id via a name->id lookup (both sides read the same
 * fixture, so the ids match). Every result is emitted as an array (findById /
 * findByIdRaw → [obj] | []).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARREAD_MAIN=/tmp/qt-charread-main.db \
 *   QT_FIXTURE_CHARREAD_MOUNT=/tmp/qt-charread-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-read.ts \
 *     > /tmp/oracle-charread.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Query {
  kind: string;
  targetName?: string;
  targetNames?: string[];
  userId?: string;
  imageId?: string;
  tagId?: string;
}
interface Spec {
  testPepperBase64: string;
  queries: Query[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'characters-read-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_CHARREAD_MAIN;
  const mountFixture = process.env.QT_FIXTURE_CHARREAD_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_CHARREAD_MAIN and QT_FIXTURE_CHARREAD_MOUNT must point at the fixtures from build-characters-read-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-charread-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'charread-main-work.db');
  const mountWork = join(scratch, 'charread-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  // name -> minted id map (both sides read the same fixture).
  const nameRows = (await rawQuery('SELECT id, name FROM characters')) as Array<{
    id: string;
    name: string;
  }>;
  const idByName = new Map<string, string>();
  for (const r of nameRows) idByName.set(r.name, r.id);
  const idFor = (name: string): string => {
    const id = idByName.get(name);
    if (!id) throw new Error(`no character named ${name}`);
    return id;
  };

  const toArray = (v: unknown): unknown[] => (v === null || v === undefined ? [] : [v]);

  const results: Array<{ kind: string; result: unknown[] }> = [];
  for (const q of spec.queries) {
    let result: unknown[];
    switch (q.kind) {
      case 'findByIdRaw':
        result = toArray(await repos.characters.findByIdRaw(idFor(q.targetName as string)));
        break;
      case 'findById':
        result = toArray(await repos.characters.findById(idFor(q.targetName as string)));
        break;
      case 'findAll':
        result = await repos.characters.findAll();
        break;
      case 'findByUserId':
        result = await repos.characters.findByUserId(q.userId as string);
        break;
      case 'findUserControlled':
        result = await repos.characters.findUserControlled(q.userId as string);
        break;
      case 'findLLMControlled':
        result = await repos.characters.findLLMControlled(q.userId as string);
        break;
      case 'findByIds':
        result = await repos.characters.findByIds(
          (q.targetNames as string[]).map(idFor)
        );
        break;
      case 'findByDefaultImageId':
        result = await repos.characters.findByDefaultImageId(q.imageId as string);
        break;
      case 'findByAvatarOverrideImageId':
        result = await repos.characters.findByAvatarOverrideImageId(q.imageId as string);
        break;
      case 'findByTag':
        result = await repos.characters.findByTag(q.tagId as string);
        break;
      default:
        throw new Error(`unknown query kind: ${q.kind}`);
    }
    results.push({ kind: q.kind, result });
  }

  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'characters-read', queries: results }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`characters-read oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
