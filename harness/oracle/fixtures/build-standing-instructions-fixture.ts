/**
 * Fixture builder for the standing-instructions differential (P4.D103 unit 1,
 * v4 `8f868109`).
 *
 * Bakes, via v4's REAL repositories (all ids/timestamps pinned so both sides
 * read identical rows):
 *   - one user + four characters (Aria / Bram / Cleo / Dane),
 *   - three projects: Iota (instructions, carrying a `{{char}}` placeholder so
 *     the consumer's template processing is observable), Kappa (no
 *     instructions), Lambda (whitespace-only instructions — the trim arm),
 *   - ten groups whose MEMBERSHIP INSERT ORDER deliberately fights the name
 *     sort, so a port that keeps table order reddens:
 *       Aria  → Zephyr, Gamma, Beacon (all instructed; sorted Beacon/Gamma/
 *               Zephyr), Delta (no instructions), Echo (whitespace-only),
 *               and a DANGLING membership whose group row does not exist,
 *       Bram  → "Mirror" twice (SAME name, different instructions, inserted
 *               second-then-first) — the `|| a.instructions.localeCompare(...)`
 *               tie-break, plus Solo (whitespace-only) so Bram's own
 *               non-Mirror leg contributes nothing,
 *       Cleo  → no memberships at all,
 *       Dane  → "Banana" and "apple" — byte order puts `B` (0x42) before `a`
 *               (0x61); ICU en-US `localeCompare` puts `apple` first. The one
 *               row that separates `locale_compare` from `str::cmp`.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SI_MAIN=/tmp/qt-si-main.db QT_FIXTURE_SI_MOUNT=/tmp/qt-si-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-standing-instructions-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { randomUUID } from 'node:crypto';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  characters: Record<string, string>;
  projects: Record<string, string>;
  groups: Record<string, string>;
  missingProjectId: string;
  danglingGroupId: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'standing-instructions.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_SI_MAIN;
  const mountOut = process.env.QT_FIXTURE_SI_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_SI_MAIN and QT_FIXTURE_SI_MOUNT must both point at the .db files to write',
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-si-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { GroupSchema } = await import('@/lib/schemas/group.types');
  const { ProjectSchema } = await import('@/lib/schemas/project.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    GroupCharacterMemberSchema,
    GroupDocMountLinkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('groups', GroupSchema);
  await ensureCollection('projects', ProjectSchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['group_character_members', GroupCharacterMemberSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger it via a read.
  const { DocMountBlobsRepository } = await import(
    '@/lib/database/repositories/doc-mount-blobs.repository'
  );
  await new DocMountBlobsRepository().findByFileId('00000000-0000-4000-8000-000000000000');

  const repos = getRepositories();
  const TS = spec.seedTimestamp;

  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  for (const [key, id] of Object.entries(spec.characters)) {
    const name = key[0].toUpperCase() + key.slice(1);
    await repos.characters.create(
      {
        name,
        userId: spec.userId,
        title: null,
        description: `${name} the test character.`,
        personality: null,
        aliases: [],
        controlledBy: 'llm',
        talkativeness: 0.5,
        isFavorite: false,
        npc: false,
        tags: [],
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  }

  // --- Projects -------------------------------------------------------------
  const project = async (id: string, name: string, instructions: string | null) => {
    const p = await repos.projects.create(
      { name, description: null, instructions, state: {} } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    if (p.id !== id) throw new Error(`${name} project id drift: ${p.id}`);
  };
  await project(
    spec.projects.iota,
    'Iota',
    'Expedition standing orders: {{char}} keeps the journal, and {{user}} signs each entry.',
  );
  await project(spec.projects.kappa, 'Kappa', null);
  await project(spec.projects.lambda, 'Lambda', '   \n\t  ');

  // --- Groups ---------------------------------------------------------------
  const group = async (id: string, name: string, instructions: string | null) => {
    const g = await repos.groups.create(
      { name, description: null, instructions, state: {} } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    if (g.id !== id) throw new Error(`${name} group id drift: ${g.id}`);
  };
  await group(spec.groups.zephyr, 'Zephyr', 'Zephyr rule: end every scene on a question.');
  await group(spec.groups.gamma, 'Gamma', 'Gamma rule: no exclamation marks.');
  await group(spec.groups.beacon, 'Beacon', 'Beacon rule: mention the light once.  ');
  await group(spec.groups.delta, 'Delta', null);
  await group(spec.groups.echo, 'Echo', '   ');
  await group(spec.groups.mirrorSecond, 'Mirror', 'Mirror rule B: speak last.');
  await group(spec.groups.mirrorFirst, 'Mirror', 'Mirror rule A: speak first.');
  await group(spec.groups.solo, 'Solo', '\n\n');
  await group(spec.groups.bananaUpper, 'Banana', 'Banana rule: yellow only.');
  await group(spec.groups.appleLower, 'apple', 'apple rule: red only.');

  // --- Memberships (INSERT ORDER fights every sort) -------------------------
  const { aria, bram, cleo, dane } = spec.characters;
  for (const g of [
    spec.groups.zephyr,
    spec.groups.gamma,
    spec.groups.beacon,
    spec.groups.delta,
    spec.groups.echo,
  ]) {
    await repos.groupCharacterMembers.addMember(g, aria);
  }
  // A dangling membership: the join row exists, the group row does not.
  midb
    .prepare(
      'INSERT INTO group_character_members (id, groupId, characterId, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?)',
    )
    .run(randomUUID(), spec.danglingGroupId, aria, TS, TS);

  await repos.groupCharacterMembers.addMember(spec.groups.mirrorSecond, bram);
  await repos.groupCharacterMembers.addMember(spec.groups.mirrorFirst, bram);
  await repos.groupCharacterMembers.addMember(spec.groups.solo, bram);

  void cleo; // Cleo is deliberately a member of nothing.

  await repos.groupCharacterMembers.addMember(spec.groups.bananaUpper, dane);
  await repos.groupCharacterMembers.addMember(spec.groups.appleLower, dane);

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(`built standing-instructions fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`standing-instructions fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
