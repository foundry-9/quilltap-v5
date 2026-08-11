/**
 * P4.D63 — the character-archive EXTENSION of the committed groups+projects
 * fixture.
 *
 * The two add-member refusals (Shared contract rule 7) are byte-contractual
 * sentences, and neither is reachable without an archived character in THIS
 * family's fixture: the groups/projects routes read their own DBs, not the
 * characters family's.
 *
 * Extends in place for the same reason as
 * `extend-characters-fixture-archive.ts`: the main builder mints each
 * character's vault ids, so a rebuild would move ids nothing in this drift
 * touches. The `ALTER TABLE` is verbatim v4's own
 * `add-character-archive-fields-v1`.
 *
 * Adds **Edda** — archived, member of no group and no roster, so every
 * pre-existing case is unperturbed and she exists purely to be refused. (The
 * DIANA precedent in the main builder: a character referenced by one case
 * family only.)
 *
 * Idempotent: re-running detects Edda and exits.
 *
 * Run (Node 24, from the v4 checkout) AFTER `build-groups-projects-fixture.ts`:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_GROUPS_PROJECTS_MAIN=$W/crates/quilltap-web/tests/fixtures/groups-projects-main.db \
 *   QT_FIXTURE_GROUPS_PROJECTS_MOUNT=$W/crates/quilltap-web/tests/fixtures/groups-projects-mount.db \
 *     $N/node --import tsx $W/harness/oracle/fixtures/extend-groups-projects-fixture-archive.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

/** The archived character, pinned. */
export const EDDA = 'a1000000-0000-4000-8000-000000000005';
export const EDDA_ARCHIVED_AT = '2026-03-15T09:30:00.000Z';
const CONN = 'c0000001-0000-4000-8000-000000000001';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'groups-projects.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_GROUPS_PROJECTS_MAIN;
  const mountOut = process.env.QT_FIXTURE_GROUPS_PROJECTS_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_GROUPS_PROJECTS_MAIN and QT_FIXTURE_GROUPS_PROJECTS_MOUNT must point at the committed .db files',
    );
  }
  for (const p of [mainOut, mountOut]) {
    if (!existsSync(p)) throw new Error(`missing fixture ${p} — run build-groups-projects-fixture.ts first`);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-gp-archive-ext-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();
  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  if (await repos.characters.findByIdRaw(EDDA)) {
    process.stderr.write('groups-projects archive extension: already applied — nothing to do\n');
    closeMountIndexSQLiteClient();
    await closeDatabase();
    process.exit(0);
  }

  const maindb = getRawDatabase();
  if (!maindb) throw new Error('main DB handle unavailable');
  const existing = new Set(
    (maindb.prepare('PRAGMA table_info("characters")').all() as Array<{ name: string }>).map(
      (c) => c.name,
    ),
  );
  for (const col of ['archivedAt', 'archiveFileId', 'archivedAvatarFileId']) {
    if (!existing.has(col)) maindb.exec(`ALTER TABLE "characters" ADD COLUMN "${col}" TEXT`);
  }

  await repos.characters.create(
    {
      name: 'Edda',
      userId: spec.userId,
      title: null,
      description: 'The archived test character.',
      personality: null,
      aliases: [],
      controlledBy: 'llm',
      talkativeness: 0.5,
      defaultConnectionProfileId: CONN,
      isFavorite: false,
      npc: false,
      tags: [],
    } as never,
    { id: EDDA, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.characters.update(EDDA, { archivedAt: EDDA_ARCHIVED_AT } as never);

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stderr.write(
    `extended groups-projects fixture: +Edda (archived ${EDDA_ARCHIVED_AT}) → ${mainOut}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`groups-projects archive extension failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
