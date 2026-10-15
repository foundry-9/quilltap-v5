/**
 * The scenario-location fixture builder (P4.D295) — the test-pepper substrate
 * for `scenario_location_equivalence` (tier 2). A RUN-TIME builder: its output
 * pair (`/tmp/qt-scenario-location-{main,mount}.db`) is rebuilt by the
 * family's recipe through v4's REAL repositories at the round's pin, and both
 * differential sides read the SAME rebuilt pair (never a committed pair — the
 * round's §R.3 rule for lanes other than the keystone).
 *
 * ## What is in it, and which arm each row exists for
 *
 *   - Two users: Friday (the caller) and Other.
 *   - Characters (vaults provisioned by v4's own `characters.create`; the
 *     scenario files written straight into each vault's `Scenarios/`):
 *       ADA  — Friday's, live, vaulted: `# Heading`-titled files, a frontmatter
 *              one, an archived one, a spaced filename (`a b.md`, the `%20`
 *              row); `defaultScenarioId` = the path-derived id of
 *              `Scenarios/Arrival.md` (the default is a COLUMN).
 *       ZED  — Friday's, ARCHIVED: a tombstone — resolves and classifies as
 *              nothing (L7).
 *       OTTO — Other's: another user's vault (L7).
 *       NOVA — Friday's, VAULTLESS (pointer nulled): no shelf.
 *       BRAM — Friday's, live, vaulted, NO `Scenarios/` folder: an empty shelf.
 *   - Projects (official stores provisioned by `projects.create`):
 *       ALPHA     — `Scenarios/` healed through the location; two files that
 *                   claim `isDefault` (the multiple-default warning, L14), an
 *                   archived default claimant, a whitespace-only body (L13),
 *                   a body crossing unit 240 with whitespace runs, NBSP and an
 *                   emoji straddling units 239–240 (the excerpt + R-AA), a
 *                   mixed-case `B.md` (the `localeCompare` sort), `a b.md`.
 *       "beta" / "Beta" — created in THAT order: a case-insensitive name tie
 *                   (the catalogue's stable `localeCompare(base)` sort keeps
 *                   v4's `_findAll` rowid order — L-2); no `Scenarios/` folder.
 *       NAMELESS  — its `name` set to '' (raw): v4's row schema refuses it, so
 *                   every raw read drops it.
 *       BARE      — its `officialMountPointId` nulled and its store UNLINKED
 *                   (raw): a probe finds nothing; `ensure` MINTS a store (v4's
 *                   step 3 — v5 defers step 2's adoption of a linked store).
 *   - Groups: GUILD (healed `Scenarios/`, one file); UNNAMED (name '' — raw).
 *   - General: a database store + its `instance_settings` pointer, healed
 *     `Scenarios/` with two files (one default).
 *   - LINKED: a database store linked (NOT official) to ALPHA, holding a
 *     `Scenarios/` file — classifies as nothing.
 *   - ARBITRARY: an unlinked database store holding a `Scenarios/` file —
 *     classifies as nothing.
 *
 * Every pinned id is in `scenario-location.json`; minted mount ids (vaults,
 * official stores) are read back from the rows by both sides.
 *
 * Regenerate (Node 24, from the v4 checkout or the round's pin worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_FIXTURE_SL_MAIN=/tmp/qt-scenario-location-main.db \
 *   QT_FIXTURE_SL_MOUNT=/tmp/qt-scenario-location-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-scenario-location-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  otherUserId: string;
  seedTimestamp: string;
  ids: Record<string, string>;
}

/** A body that crosses unit 240 after the whitespace collapse with an emoji
 *  straddling units 239–240: "Lead" + one space (the NBSP/newline/tab run) =
 *  5 units, 234 more, then the surrogate pair at 239–240. */
export const LONG_BODY = `Lead  \n\t${'y'.repeat(234)}\u{1F3AD} and the rest   of\tthe scene.`;

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) throw new Error(`build under TZ=UTC (getTimezoneOffset=${offset})`);
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'scenario-location.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;
  const ID = spec.ids;

  const mainOut = process.env.QT_FIXTURE_SL_MAIN;
  const mountOut = process.env.QT_FIXTURE_SL_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_SL_MAIN and QT_FIXTURE_SL_MOUNT must point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-sl-fixture-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { CharacterSchema, ChatMetadataSchema } = await import('@/lib/schemas/types');
  const { UserSchema } = await import('@/lib/schemas/auth.types');
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
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
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
  const repos = getRepositories();
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');
  const maindb = getRawDatabase();
  if (!maindb) throw new Error('main DB handle unavailable');

  const { resolveScenarioLocation, characterScenarioIdForPath } = await import('@/lib/scenarios/location');
  const { buildScenarioFileContent } = await import('@/lib/mount-index/scenarios-common');
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');

  // Users.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'other', email: null, name: 'Other' } as never,
    { id: spec.otherUserId, createdAt: TS, updatedAt: TS } as never,
  );

  // Characters.
  const mkChar = async (id: string, name: string, userId: string): Promise<string> => {
    await repos.characters.create(
      { name, userId, controlledBy: 'llm', description: `${name}, on the shelf.` } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const row = (await repos.characters.findByIdRaw(id)) as { characterDocumentMountPointId?: string | null } | null;
    const mp = row?.characterDocumentMountPointId;
    if (!mp) throw new Error(`character ${name} has no vault`);
    return mp;
  };
  const adaMp = await mkChar(ID.ada, 'Ada', spec.userId);
  const zedMp = await mkChar(ID.zed, 'Zed', spec.userId);
  const ottoMp = await mkChar(ID.otto, 'Otto', spec.otherUserId);
  await mkChar(ID.nova, 'Nova', spec.userId);
  await mkChar(ID.bram, 'Bram', spec.userId);

  await writeDatabaseDocument(adaMp, 'Scenarios/Arrival.md', '# The Arrival\n\nThe train pulls in at dawn, steam on the platform.');
  await writeDatabaseDocument(
    adaMp,
    'Scenarios/b-note.md',
    '---\nname: B Note\ndescription: A small one\n---\n\nA note pinned to the board.',
  );
  await writeDatabaseDocument(adaMp, 'Scenarios/archived.md', '---\narchived: true\n---\n# Old Days\n\nGone now.');
  await writeDatabaseDocument(adaMp, 'Scenarios/a b.md', '# Spaced Out\n\nA file whose name has a space.');
  await writeDatabaseDocument(zedMp, 'Scenarios/z.md', '# Zed’s Last\n\nThe tombstone’s scene.');
  await writeDatabaseDocument(ottoMp, 'Scenarios/o.md', '# Otto’s\n\nAnother user’s scene.');
  maindb
    .prepare('UPDATE "characters" SET "defaultScenarioId" = ? WHERE "id" = ?')
    .run(characterScenarioIdForPath(adaMp, 'Scenarios/Arrival.md'), ID.ada);
  maindb.prepare('UPDATE "characters" SET "archivedAt" = ? WHERE "id" = ?').run(TS, ID.zed);
  maindb.prepare('UPDATE "characters" SET "characterDocumentMountPointId" = NULL WHERE "id" = ?').run(ID.nova);

  // Projects. `beta` is created before `Beta` — the tie's input order.
  const mkProject = async (id: string, name: string): Promise<string> => {
    const p = await repos.projects.create(
      { name, description: `${name}'s project.`, state: {} } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const mp = (p as { officialMountPointId?: string | null }).officialMountPointId;
    if (!mp) throw new Error(`project ${name} has no official store`);
    return mp;
  };
  const alphaMp = await mkProject(ID.alpha, 'Alpha');
  await mkProject(ID.betaLower, 'beta');
  const betaUpperMp = await mkProject(ID.betaUpper, 'Beta');
  await mkProject(ID.nameless, 'Nameless');
  await mkProject(ID.bare, 'Bare');

  const heal = async (scope: 'general' | 'project' | 'group', id: string | null) => {
    const location = await resolveScenarioLocation(scope, id, repos, spec.userId, { ensure: true });
    if (!location) throw new Error(`${scope} ${id} did not resolve`);
    return location.mountPointId;
  };
  await heal('project', ID.alpha);
  const file = (name: string | undefined, body: string, extra: { isDefault?: boolean; archived?: boolean; description?: string } = {}) =>
    buildScenarioFileContent({ name, body, ...extra });
  await writeDatabaseDocument(alphaMp, 'Scenarios/a.md', file('Ay', 'The first scene.', { isDefault: true }));
  await writeDatabaseDocument(alphaMp, 'Scenarios/B.md', file(undefined, 'The second scene, uppercase file.', { isDefault: true }));
  await writeDatabaseDocument(alphaMp, 'Scenarios/c.md', file('Sea', 'An archived claimant.', { isDefault: true, archived: true }));
  await writeDatabaseDocument(alphaMp, 'Scenarios/blank.md', '---\nname: Blank\n---\n\n   \n');
  await writeDatabaseDocument(alphaMp, 'Scenarios/long.md', file('Long', LONG_BODY, { description: 'Crosses the cut' }));
  await writeDatabaseDocument(alphaMp, 'Scenarios/a b.md', file('Spaced', 'A project file with a space.'));
  await writeDatabaseDocument(betaUpperMp, 'Scenarios/up.md', file('Upper', 'The capital beta scene.'));
  maindb.prepare('UPDATE "projects" SET "name" = ? WHERE "id" = ?').run('', ID.nameless);
  maindb.prepare('UPDATE "projects" SET "officialMountPointId" = NULL WHERE "id" = ?').run(ID.bare);
  // ...and UNLINK its store, so `ensure` MINTS a fresh one (step 3) on both
  // sides. v5's `ensure_official_store` defers v4's step 2 (adopting a linked
  // store) — a tracked deferral of that module, recorded by P4.D295, not this
  // family's subject.
  midb.prepare('DELETE FROM "project_doc_mount_links" WHERE "projectId" = ?').run(ID.bare);

  // Groups.
  const mkGroup = async (id: string, name: string): Promise<string> => {
    const g = await repos.groups.create(
      { name, description: `${name}'s group.`, state: {} } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const mp = (g as { officialMountPointId?: string | null }).officialMountPointId;
    if (!mp) throw new Error(`group ${name} has no official store`);
    return mp;
  };
  const guildMp = await mkGroup(ID.guild, 'Guild');
  await mkGroup(ID.unnamedGroup, 'Unnamed');
  await heal('group', ID.guild);
  await writeDatabaseDocument(guildMp, 'Scenarios/g.md', file('Guild Hall', 'The guild hall at noon.'));
  maindb.prepare('UPDATE "groups" SET "name" = ? WHERE "id" = ?').run('', ID.unnamedGroup);

  // General + the two non-owner stores.
  const mkStore = async (id: string, name: string) =>
    repos.docMountPoints.create(
      {
        name,
        basePath: '',
        mountType: 'database',
        storeType: 'documents',
        includePatterns: [],
        excludePatterns: [],
        enabled: true,
        lastScannedAt: null,
        scanStatus: 'idle',
        lastScanError: null,
        conversionStatus: 'idle',
        conversionError: null,
        fileCount: 0,
        chunkCount: 0,
        totalSizeBytes: 0,
      } as never,
      { id, createdAt: TS, updatedAt: TS },
    );
  await mkStore(ID.general, 'Quilltap General');
  maindb.exec('CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)');
  maindb
    .prepare('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)')
    .run('generalMountPointId', ID.general);
  await heal('general', null);
  await writeDatabaseDocument(ID.general, 'Scenarios/welcome.md', file('Welcome', 'The Grand Concourse at dusk.', { description: 'Gaslamps' }));
  await writeDatabaseDocument(ID.general, 'Scenarios/second.md', file('Second', 'The second general scene.', { isDefault: true }));

  await mkStore(ID.linked, 'Alpha Linked Notes');
  await repos.projectDocMountLinks.link(ID.alpha, ID.linked);
  await writeDatabaseDocument(ID.linked, 'Scenarios/x.md', file('Linked', 'A scene in a linked store.'));
  await mkStore(ID.arbitrary, 'Loose Papers');
  await writeDatabaseDocument(ID.arbitrary, 'Scenarios/y.md', file('Loose', 'A scene in an arbitrary store.'));

  closeMountIndexSQLiteClient();
  await closeDatabase();
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['-journal', '-wal', '-shm']) {
      if (existsSync(out + suffix)) rmSync(out + suffix);
    }
  }
  process.stderr.write(`built scenario-location fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`scenario-location fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
