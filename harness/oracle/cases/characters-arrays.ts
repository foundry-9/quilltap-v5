/**
 * Tier-2 oracle case — v4's `CharactersRepository` array / sub-array ops (Phase-2,
 * the store-backed capstone sub-unit 4b).
 *
 * Drives v4's REAL repository methods over the baked fixture character:
 * `addSystemPrompt` / `updateSystemPrompt` / `setDefaultSystemPrompt` /
 * `deleteSystemPrompt`, `addScenario` / `updateScenario` / `removeScenario`,
 * `addPartnerLink` / `removePartnerLink`, and the `setFavorite` /
 * `setControlledBy` / `setCanBeCarina` setters. Each sub-array op internally does
 * findById (read overlay) -> mutate -> update (write overlay). We do NOT set
 * QUILLTAP_JOB_CHILD, so reindexSingleFile runs; the differential pins chunkCount
 * and excludes doc_mount_chunks.
 *
 * The id-taking prompt/scenario ops carry a targetName / targetTitle in the spec;
 * we resolve it to the current item's id via `findById` right before the op (the
 * id is path-derived, so both sides agree). A character spans two DBs, so we dump
 * BOTH the main slim `characters` row and the mount-index store tables.
 *
 * NORMALIZATION (done identically on both dumps by the Rust harness): the
 * shared-id remap across all six tables (FKs verify by relationship) + timestamp
 * placeholders; chunkCount pinned.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARARR_MAIN=/tmp/qt-chararr-main.db \
 *   QT_FIXTURE_CHARARR_MOUNT=/tmp/qt-chararr-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/characters-arrays.ts \
 *     > /tmp/oracle-chararr.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  op: string;
  name?: string;
  content?: string;
  isDefault?: boolean;
  title?: string;
  targetName?: string;
  targetTitle?: string;
  data?: Record<string, unknown>;
  partnerId?: string;
  value?: unknown;
}
interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'characters-arrays-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_CHARARR_MAIN;
  const mountFixture = process.env.QT_FIXTURE_CHARARR_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_CHARARR_MAIN and QT_FIXTURE_CHARARR_MOUNT must point at the fixtures from build-characters-arrays-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-chararr-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'chararr-main-work.db');
  const mountWork = join(scratch, 'chararr-mount-work.db');
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
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();
  const repos = getRepositories();

  const idRow = (await rawQuery('SELECT id FROM characters LIMIT 1')) as Array<{ id: string }>;
  if (idRow.length === 0) throw new Error('fixture has no character row');
  const characterId = idRow[0].id;

  const resolvePromptId = async (name: string): Promise<string> => {
    const c = await repos.characters.findById(characterId);
    const p = (c?.systemPrompts ?? []).find((x) => x.name === name);
    if (!p) throw new Error(`prompt not found for targetName: ${name}`);
    return p.id;
  };
  const resolveScenarioId = async (title: string): Promise<string> => {
    const c = await repos.characters.findById(characterId);
    const s = (c?.scenarios ?? []).find((x) => x.title === title);
    if (!s) throw new Error(`scenario not found for targetTitle: ${title}`);
    return s.id;
  };

  for (const op of spec.ops) {
    switch (op.op) {
      case 'addSystemPrompt':
        await repos.characters.addSystemPrompt(characterId, {
          name: op.name as string,
          content: op.content as string,
          isDefault: op.isDefault as boolean,
        } as never);
        break;
      case 'updateSystemPrompt': {
        const id = await resolvePromptId(op.targetName as string);
        await repos.characters.updateSystemPrompt(characterId, id, op.data as never);
        break;
      }
      case 'setDefaultSystemPrompt': {
        const id = await resolvePromptId(op.targetName as string);
        await repos.characters.setDefaultSystemPrompt(characterId, id);
        break;
      }
      case 'deleteSystemPrompt': {
        const id = await resolvePromptId(op.targetName as string);
        await repos.characters.deleteSystemPrompt(characterId, id);
        break;
      }
      case 'addScenario':
        await repos.characters.addScenario(characterId, {
          title: op.title as string,
          content: op.content as string,
        });
        break;
      case 'updateScenario': {
        const id = await resolveScenarioId(op.targetTitle as string);
        await repos.characters.updateScenario(characterId, id, op.data as never);
        break;
      }
      case 'removeScenario': {
        const id = await resolveScenarioId(op.targetTitle as string);
        await repos.characters.removeScenario(characterId, id);
        break;
      }
      case 'addPartnerLink':
        await repos.characters.addPartnerLink(
          characterId,
          op.partnerId as string,
          op.isDefault as boolean
        );
        break;
      case 'removePartnerLink':
        await repos.characters.removePartnerLink(characterId, op.partnerId as string);
        break;
      case 'setFavorite':
        await repos.characters.setFavorite(characterId, op.value as boolean);
        break;
      case 'setControlledBy':
        await repos.characters.setControlledBy(characterId, op.value as 'llm' | 'user');
        break;
      case 'setCanBeCarina':
        await repos.characters.setCanBeCarina(characterId, op.value as boolean);
        break;
      default:
        throw new Error(`unknown op: ${op.op}`);
    }
  }

  // MAIN db: the slim characters row.
  const charColumns = (
    (await rawQuery('PRAGMA table_info(characters)')) as Array<{ name: string }>
  ).map((c) => c.name);
  const charRows = (await rawQuery('SELECT * FROM characters')) as Array<
    Record<string, unknown>
  >;
  const characters = canonicalizeRows({
    table: 'characters',
    columns: charColumns,
    rawRows: charRows,
    orderBy: 'name',
  });

  // MOUNT-INDEX db: the store tables.
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable (degraded open?)');
  const dumpTable = (table: string, orderBy: string) => {
    const columns = (
      midb.pragma(`table_info(${table})`) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = midb
      .prepare(`SELECT * FROM ${table}`)
      .all() as Array<Record<string, unknown>>;
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
      case: 'characters-arrays-tier2',
      characters,
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
  process.stderr.write(`characters-arrays-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
