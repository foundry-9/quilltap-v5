/**
 * Tier-2 oracle case — the standalone two-table `vector_indices` repository.
 *
 * Drives v4's REAL `VectorIndicesRepository` over the committed op sequence
 * (saveMeta create/update, addEntry, addEntries, updateEntryEmbedding,
 * removeEntries, and a deleteByCharacterId wipe), then emits a RAW dump of BOTH
 * tables — `vector_indices` (ordered by `id`) and `vector_entries` (ordered by
 * the deterministic `embedding` hex) — with the embedding BLOB rendered as hex by
 * `canonicalizeRows`. The Rust harness applies the SAME minted-values
 * normalization (remap entry ids; placeholder timestamps) to this dump and to its
 * own, then diffs, so the remap is provably consistent.
 *
 * Two NDJSON lines are written, one per table, each tagged with `case` + `table`.
 *
 * Run from the v4 server checkout under Node 24, AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_VECTOR_INDICES=/tmp/qt-vector-indices-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vector-indices.ts \
 *     > /tmp/oracle-vector-indices.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  copyFileSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface EntryInput {
  id: string;
  characterId: string;
  embedding: number[];
}

type Op =
  | { kind: 'saveMeta'; characterId: string; dimensions: number }
  | { kind: 'deleteMetaByCharacterId'; characterId: string }
  | { kind: 'addEntry'; id: string; characterId: string; embedding: number[] }
  | { kind: 'addEntries'; entries: EntryInput[] }
  | { kind: 'updateEntryEmbedding'; id: string; embedding: number[] }
  | { kind: 'removeEntry'; id: string }
  | { kind: 'removeEntries'; ids: string[] }
  | { kind: 'removeEntriesByCharacterId'; characterId: string }
  | { kind: 'deleteByCharacterId'; characterId: string };

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'vector-indices-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_VECTOR_INDICES;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_VECTOR_INDICES must point at the seed fixture from build-vector-indices-fixture.ts'
    );
  }

  // Work on a fresh copy so the shared seed fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-vector-indices-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'vector-indices-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { VectorIndicesRepository } = await import(
    '@/lib/database/repositories/vector-indices.repository'
  );

  await initializeDatabase();
  const repo = new VectorIndicesRepository();

  for (const op of spec.ops) {
    switch (op.kind) {
      case 'saveMeta':
        await repo.saveMeta(op.characterId, op.dimensions);
        break;
      case 'deleteMetaByCharacterId':
        await repo.deleteMetaByCharacterId(op.characterId);
        break;
      case 'addEntry':
        await repo.addEntry({
          id: op.id,
          characterId: op.characterId,
          embedding: new Float32Array(op.embedding),
        });
        break;
      case 'addEntries':
        await repo.addEntries(
          op.entries.map((e) => ({
            id: e.id,
            characterId: e.characterId,
            embedding: new Float32Array(e.embedding),
          }))
        );
        break;
      case 'updateEntryEmbedding':
        await repo.updateEntryEmbedding(op.id, new Float32Array(op.embedding));
        break;
      case 'removeEntry':
        await repo.removeEntry(op.id);
        break;
      case 'removeEntries':
        await repo.removeEntries(op.ids);
        break;
      case 'removeEntriesByCharacterId':
        await repo.removeEntriesByCharacterId(op.characterId);
        break;
      case 'deleteByCharacterId':
        await repo.deleteByCharacterId(op.characterId);
        break;
      default:
        throw new Error(`unknown op: ${JSON.stringify(op)}`);
    }
  }

  // Dump both tables. Columns come from PRAGMA table_info (on-disk order); rows
  // from SELECT * through v4's own connected backend (so the registered BLOB
  // column hydrates as a Buffer -> hex via canonicalizeRows).
  const dumpTable = async (table: string, orderBy: string) => {
    const columns = (
      (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
    ).map((c) => c.name);
    const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<
      Record<string, unknown>
    >;
    return canonicalizeRows({ table, columns, rawRows, orderBy });
  };

  const metaDump = await dumpTable('vector_indices', 'id');
  const entriesDump = await dumpTable('vector_entries', 'embedding');

  await closeDatabase();

  process.stdout.write(
    JSON.stringify({ case: 'vector-indices-tier2', table: 'vector_indices', ...metaDump }) + '\n'
  );
  process.stdout.write(
    JSON.stringify({ case: 'vector-indices-tier2', table: 'vector_entries', ...entriesDump }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`vector-indices-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
