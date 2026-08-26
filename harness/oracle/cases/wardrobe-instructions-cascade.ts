/**
 * P4.D119 tier-2 ORACLE — the dressing-instructions cascade and file helpers
 * (v4 `b86bb1a5`, `lib/wardrobe/wardrobe-instructions.ts`).
 *
 * Drives v4's REAL `resolveWardrobeInstructions` / `readWardrobeInstructionsFile`
 * / `writeWardrobeInstructionsFile` over a copy of the committed
 * `wardrobe-instructions-{main,mount}.db` pair. No mocks: the whole DB stack is
 * real, so what is compared is what a running instance would do.
 *
 * v4's own 179-line unit suite is the corpus floor and then some — it mocks
 * `readVaultTextFile` and asserts the PROBE ORDER through the mock's call list,
 * which a real-DB oracle cannot see. The dedupe-then-code-unit-sort is made
 * observable instead: two mounts in the SAME tier both carry a file, and WHICH
 * one wins is the sort's only externally visible consequence
 * (`group_tier_deduped_and_sorted` / `project_tier_deduped_and_sorted`).
 *
 * Mount ids are minted by the vault/store provisioning, so every case names
 * mounts by LABEL and the emitted result carries the label, not the UUID.
 *
 * Two phases in ONE process (a tsx oracle initializes the DB stack once):
 *   1. `cascadeCases` — resolve/read. Every case first DELETES
 *      `Wardrobe/instructions.md` from EVERY known mount, then writes its own
 *      seeds through the RAW document-store write, so the seeding never runs
 *      through the write helper under test.
 *   2. `writeOps` — an ORDERED sequence through `writeWardrobeInstructionsFile`,
 *      each emitting the five mount-index tables so the bytes on disk (and the
 *      absence of a file after a clear) are the comparand, not just a return
 *      value.
 *
 * Run (Node 24, from the v4 checkout or a pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WI_MAIN=$V5W/crates/quilltap-web/tests/fixtures/wardrobe-instructions-main.db \
 *   QT_FIXTURE_WI_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/wardrobe-instructions-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/cases/wardrobe-instructions-cascade.ts \
 *     > /tmp/oracle-wardrobe-instructions-cascade.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

const INSTRUCTIONS_PATH = 'Wardrobe/instructions.md';

interface CascadeCase {
  name: string;
  kind: 'resolve' | 'read';
  seed?: Record<string, string>;
  unprovisionGeneral?: boolean;
  character?: string | null;
  groups?: string[];
  projects?: string[];
  mount?: string;
}
interface WriteOp {
  name: string;
  kind: 'write' | 'read';
  mount: string;
  instructions?: string | null;
  preSeed?: string;
}
interface Spec {
  testPepperBase64: string;
  characterId: string;
  projectId: string;
  groupId: string;
  generalMountPointId: string;
  extraStores: Array<{ label: string; id: string; name: string }>;
  cascadeCases: CascadeCase[];
  writeOps: WriteOp[];
}

const MOUNT_TABLES: Array<{ key: string; table: string; orderBy: string }> = [
  { key: 'points', table: 'doc_mount_points', orderBy: 'name' },
  { key: 'files', table: 'doc_mount_files', orderBy: 'sha256' },
  { key: 'documents', table: 'doc_mount_documents', orderBy: 'contentSha256' },
  { key: 'links', table: 'doc_mount_file_links', orderBy: 'relativePath' },
  { key: 'folders', table: 'doc_mount_folders', orderBy: 'path' },
];

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'wardrobe-instructions.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_WI_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WI_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_WI_MAIN and QT_FIXTURE_WI_MOUNT must point at the fixture pair');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wi-cascade-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main.db');
  const mountWork = join(scratch, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { writeDatabaseDocument, deleteDatabaseDocument, DatabaseStoreError } = await import(
    '@/lib/mount-index/database-store'
  );
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
  const {
    resolveWardrobeInstructions,
    readWardrobeInstructionsFile,
    writeWardrobeInstructionsFile,
  } = await import('@/lib/wardrobe/wardrobe-instructions');

  await initializeDatabase();
  const repos = getRepositories();

  // --- label → minted mount id -------------------------------------------
  const character = await repos.characters.findByIdRaw(spec.characterId);
  const project = await repos.projects.findByIdRaw(spec.projectId);
  const group = await repos.groups.findByIdRaw(spec.groupId);
  const labels: Record<string, string> = {
    charA: character?.characterDocumentMountPointId as string,
    project: project?.officialMountPointId as string,
    group: group?.officialMountPointId as string,
    general: spec.generalMountPointId,
  };
  for (const s of spec.extraStores) labels[s.label] = s.id;
  for (const [k, v] of Object.entries(labels)) {
    if (!v) throw new Error(`fixture did not provide a mount for label ${k}`);
  }
  const byId = new Map(Object.entries(labels).map(([k, v]) => [v, k]));
  const ALL = Object.values(labels);

  const clearAll = async (): Promise<void> => {
    for (const mp of ALL) {
      try {
        await deleteDatabaseDocument(mp, INSTRUCTIONS_PATH);
      } catch (err) {
        if (!(err instanceof DatabaseStoreError && err.code === 'NOT_FOUND')) throw err;
      }
    }
  };
  /** Seed a file WITHOUT going through the helper under test. */
  const seedFile = async (mp: string, content: string): Promise<void> => {
    await ensureFolderPath(mp, 'Wardrobe');
    await writeDatabaseDocument(mp, INSTRUCTIONS_PATH, content);
  };

  const out: string[] = [];

  // --- phase 1: resolve / read -------------------------------------------
  for (const c of spec.cascadeCases) {
    await clearAll();
    for (const [label, content] of Object.entries(c.seed ?? {})) {
      await seedFile(labels[label], content);
    }
    if (c.unprovisionGeneral) {
      await rawQuery('DELETE FROM "instance_settings" WHERE "key" = ?', ['generalMountPointId']);
    }

    let result: unknown;
    if (c.kind === 'resolve') {
      const hit = await resolveWardrobeInstructions({
        characterMountPointId:
          c.character === null || c.character === undefined
            ? null
            : c.character === ''
              ? ''
              : labels[c.character],
        groupMountPointIds: (c.groups ?? []).map((l) => labels[l]),
        projectMountPointIds: (c.projects ?? []).map((l) => labels[l]),
      });
      result = hit
        ? { content: hit.content, tier: hit.tier, mount: byId.get(hit.mountPointId) ?? hit.mountPointId }
        : null;
    } else {
      result = await readWardrobeInstructionsFile(labels[c.mount as string]);
    }

    if (c.unprovisionGeneral) {
      await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
        'generalMountPointId',
        spec.generalMountPointId,
      ]);
    }
    out.push(JSON.stringify({ name: c.name, result }));
  }

  // --- phase 2: the write helper, an ordered sequence with table dumps ----
  await clearAll();
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const dumpTables = (): Record<string, unknown> => {
    const tables: Record<string, unknown> = {};
    for (const t of MOUNT_TABLES) {
      const columns = (midb.pragma(`table_info(${t.table})`) as Array<{ name: string }>).map(
        (c) => c.name,
      );
      const rawRows = midb.prepare(`SELECT * FROM ${t.table}`).all() as Array<
        Record<string, unknown>
      >;
      tables[t.key] = canonicalizeRows({ table: t.table, columns, rawRows, orderBy: t.orderBy });
    }
    return tables;
  };

  for (const op of spec.writeOps) {
    if (op.preSeed !== undefined) await seedFile(labels[op.mount], op.preSeed);
    let result: unknown = null;
    if (op.kind === 'write') {
      await writeWardrobeInstructionsFile(labels[op.mount], op.instructions ?? null);
      result = { wrote: true };
    } else {
      result = await readWardrobeInstructionsFile(labels[op.mount]);
    }
    out.push(JSON.stringify({ name: op.name, result, tables: dumpTables() }));
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stdout.write(out.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-instructions-cascade oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
