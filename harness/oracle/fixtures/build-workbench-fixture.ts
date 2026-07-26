/**
 * Builds the committed Pascal Workbench fixture (main + mount-index) via v4's
 * REAL repos + `storeMountFile`, so BOTH the jest workbench oracles
 * (`pascal-workbench.test.ts`, `pascal-workbench-route.test.ts`) and the Rust
 * differentials (`pascal_workbench_equivalence.rs`,
 * `pascal_workbench_route_equivalence.rs`) drive the same baked instance.
 *
 * The shape exists to exercise ATTACHMENT GROUPING — the part of the Workbench
 * with the most ways to be subtly wrong:
 *
 *   - **General store** (its id persisted into MAIN `instance_settings`), holding
 *     `Tools/gated_open.tool.json` / `Tools/gated_shut.tool.json` (P4.d19: one
 *     per availability clause, so the library's `gate` field takes all three of
 *     its values),
 *     `Tools/alpha.tool.json` (valid, range roll, no `title` → `displayTitle`
 *     derivation), `Tools/broken.tool.json` (not JSON → a library ERROR entry
 *     exercising `formatDefinitionIssues`), and `Tools/sub/nested.tool.json`
 *     (nested → rejected by `isRootToolFile`, listed by neither).
 *   - **Project "Ashfall"** (real `projects.create` → mints + links its official
 *     store) holding `Tools/ashfall_check.tool.json` (DICE roll, one parameter,
 *     two outcomes, explicit `title`).
 *   - **Group "Night Shift"** (real `groups.create` → mints its official store)
 *     holding `Tools/beta.tool.json` (`disabled: true`, whisper visibility),
 *     PLUS a second linked store ("Night Shift Annex", toolless) so one group
 *     carries an `official: true` and an `official: false` store at once.
 *   - The group ALSO links the PROJECT's store, so that one mount carries two
 *     badges and pins `attachmentsForMount`'s stable order (group before
 *     project).
 *   - **Imogen** — a character vault with `Tools/gamma.tool.json` (a
 *     metadata-gated table) and a real `metadata.json` fact sheet, which the
 *     route's `{ characterId }` metadata branch hydrates.
 *   - **Vaultless** — a character whose vault link is nulled after creation, so
 *     `destinations.characters` must omit it.
 *   - **Cracked** — a character whose vault `properties.json` keystone is
 *     DELETED. `surveyAttachments` reads `findAllRaw` deliberately, so this
 *     store still earns its character badge in the library (exactly the store an
 *     author would be repairing), while the route's `{ characterId }` branch
 *     surfaces the broken vault as a 422 rather than papering it with `{}`.
 *   - **"Loose Papers"** — an enabled store attached to nothing → `other`.
 *   - **"Shuttered"** — a DISABLED store carrying a tool → invisible to both the
 *     library and the destinations (`findEnabled` filters it).
 *
 * Every roll in the corpus is deterministic (`min === max`, or a dice form only
 * ever previewed through a fixed-byte source), so preview/audit rows diff
 * exactly across languages without a shared draw stream.
 *
 * Regenerate (v4 @ 231be14c, Node 24; the pinned detached worktree):
 *   cd /tmp/qt-v4-pin-231be14c
 *   QT_FIXTURE_CI_MAIN=<V5W>/crates/quilltap-web/tests/fixtures/workbench-main.db \
 *   QT_FIXTURE_CI_MOUNT=<V5W>/crates/quilltap-web/tests/fixtures/workbench-mount.db \
 *     npx tsx <V5W>/harness/oracle/fixtures/build-workbench-fixture.ts
 */

import { readFileSync, writeFileSync, existsSync, rmSync, mkdtempSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
  generalMountPointId: string;
  looseMountPointId: string;
  shutteredMountPointId: string;
  groupAnnexMountPointId: string;
  projectId: string;
  groupId: string;
  charImogenId: string;
  charVaultlessId: string;
  charCrackedId: string;
}

/** Valid, minimal, no `title` → the library derives "Alpha" from the name. */
const ALPHA = {
  name: 'alpha',
  description: 'The alpha tool.',
  roll: { min: 0.7, max: 0.7 },
  outcomes: [{ when: true, message: 'Alpha lands.', state: 'info' }],
};

/** Dice form, one parameter, two outcomes, an explicit title. */
const ASHFALL_CHECK = {
  name: 'ashfall_check',
  title: 'Ashfall Check',
  description: 'Test the ash for live embers.',
  roll: '1d20',
  parameters: { bonus: { type: 'integer', default: 0, min: -5, max: 5 } },
  outcomes: [
    { when: { gte: 12 }, message: 'Still warm.', state: 'success' },
    { when: true, message: 'Cold through.', state: 'failure' },
  ],
};

/** A tombstoned, whisper-default definition — `disabled` must survive to the library. */
const BETA = {
  name: 'beta',
  description: 'The beta tool.',
  disabled: true,
  defaultVisibility: 'whisper',
  roll: { min: 0.25, max: 0.25 },
  outcomes: [{ when: true, message: 'Beta, quietly.', state: 'info' }],
};

/** Metadata-gated: the route's `{ characterId }` branch decides which row fires. */
const GAMMA = {
  name: 'gamma',
  title: 'Gamma Burst',
  description: 'Consult the sheet, then the dice.',
  roll: { min: 0.9, max: 0.9 },
  outcomes: [
    {
      when: { gt: 0.5, metadata: { hasKey: { eq: true } } },
      message: 'The lock turns.',
      state: 'success',
    },
    { when: true, message: 'The lock holds.', state: 'failure' },
  ],
};

/**
 * The 616930db consult tool, so the library's `llm: boolean` badge has a TRUE
 * arm to pin — without it every entry reads `false` and a port that hardcoded
 * the field would pass.
 */
const ZETA = {
  name: 'zeta',
  title: 'The Consulted Table',
  description: 'Ask a separate model, then deal.',
  roll: { min: 0.4, max: 0.4 },
  llm: { prompt: 'Answer YES or NO.', errorMessage: 'The wire went dead.' },
  outcomes: [
    { when: { llm: { ok: true, eq: 'YES' } }, message: 'Granted.', state: 'success' },
    { when: true, message: 'Denied.', state: 'failure' },
  ],
};

/** Lives in the BROKEN vault — the library must still list it, badged. */
const DELTA = {
  name: 'delta',
  description: 'The delta tool.',
  roll: { min: 0.4, max: 0.4 },
  outcomes: [{ when: true, message: 'Delta, from a cracked vault.', state: 'info' }],
};

/**
 * P4.d19 (v4 `6864bf0e`): one definition per availability clause, so the
 * library's new `gate` field is exercised at all three of its values — the two
 * below plus every ungated entry's `null`.
 */
const GATED_OPEN = {
  name: 'gated_open',
  title: 'Offered To The Keyed',
  description: 'Only a keyed character is dealt this one.',
  availableWhen: { metadata: { hasKey: { eq: true } } },
  roll: { min: 0.4, max: 0.4 },
  outcomes: [{ when: true, message: 'The line hums.', state: 'success' }],
};

const GATED_SHUT = {
  name: 'gated_shut',
  description: 'A keyed character is kept from this one.',
  withheldWhen: { metadata: { hasKey: { eq: true } } },
  roll: { min: 0.4, max: 0.4 },
  outcomes: [{ when: true, message: 'The house obliges.', state: 'info' }],
};

/** Lives in the DISABLED store — neither view may see it. */
const EPSILON = {
  name: 'epsilon',
  description: 'The epsilon tool.',
  roll: { min: 0.1, max: 0.1 },
  outcomes: [{ when: true, message: 'Never dealt.', state: 'info' }],
};

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'workbench.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainOut = process.env.QT_FIXTURE_CI_MAIN;
  const mountOut = process.env.QT_FIXTURE_CI_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_CI_MAIN and QT_FIXTURE_CI_MOUNT must point at the .db files');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-workbench-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');
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
    ProjectDocMountLinkSchema,
    GroupDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
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
    ['group_doc_mount_links', GroupDocMountLinkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  const repos = getRepositories();
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');

  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  const { storeMountFile } = await import('@/lib/mount-index/store-file');
  const writeFile = async (
    mountPointId: string,
    relativePath: string,
    body: unknown,
  ): Promise<void> => {
    await storeMountFile({
      mountPointId,
      relativePath,
      data: Buffer.from(
        typeof body === 'string' ? body : JSON.stringify(body, null, 2),
        'utf8',
      ),
      originalMimeType: 'application/json',
      transcodeImages: false,
      extractText: false,
      enqueueEmbedding: false,
      // `metadata.json` is scaffold-seeded (`{}`) by characters.create; overwrite it.
      force: true,
    } as never);
  };

  const provisionStore = async (id: string, name: string, enabled = true): Promise<void> => {
    await repos.docMountPoints.create(
      {
        name,
        basePath: '',
        mountType: 'database',
        storeType: 'documents',
        includePatterns: [],
        excludePatterns: [],
        enabled,
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
  };

  // 1. The General store + its instance_settings pointer.
  await provisionStore(spec.generalMountPointId, 'Quilltap General');
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.generalMountPointId,
  ]);
  await writeFile(spec.generalMountPointId, 'Tools/alpha.tool.json', ALPHA);
  await writeFile(spec.generalMountPointId, 'Tools/broken.tool.json', '{ not json');
  await writeFile(spec.generalMountPointId, 'Tools/sub/nested.tool.json', ALPHA);
  await writeFile(spec.generalMountPointId, 'Tools/zeta.tool.json', ZETA);
  await writeFile(spec.generalMountPointId, 'Tools/gated_open.tool.json', GATED_OPEN);
  await writeFile(spec.generalMountPointId, 'Tools/gated_shut.tool.json', GATED_SHUT);

  // 2. The unattached + disabled stores.
  await provisionStore(spec.looseMountPointId, 'Loose Papers');
  await provisionStore(spec.shutteredMountPointId, 'Shuttered', false);
  await writeFile(spec.shutteredMountPointId, 'Tools/epsilon.tool.json', EPSILON);

  // 3. The project (create mints + links its official store).
  const project = await repos.projects.create(
    { name: 'Ashfall', state: {} } as never,
    { id: spec.projectId, createdAt: TS, updatedAt: TS } as never,
  );
  const projectStore = project.officialMountPointId as string;
  if (!projectStore) throw new Error('project store not minted');
  await writeFile(projectStore, 'Tools/ashfall_check.tool.json', ASHFALL_CHECK);

  // 4. The group: its official store, a second linked store, and a link onto the
  //    PROJECT's store (so one mount carries both badges, in stable order).
  const group = await repos.groups.create(
    { name: 'Night Shift', state: {} } as never,
    { id: spec.groupId, createdAt: TS, updatedAt: TS } as never,
  );
  const groupOfficial = group.officialMountPointId as string;
  if (!groupOfficial) throw new Error('group official store not minted');
  await writeFile(groupOfficial, 'Tools/beta.tool.json', BETA);
  await provisionStore(spec.groupAnnexMountPointId, 'Night Shift Annex');
  await repos.groupDocMountLinks.link(spec.groupId, spec.groupAnnexMountPointId);
  await repos.groupDocMountLinks.link(spec.groupId, projectStore);

  // 5. The three characters.
  const mkChar = async (id: string, name: string): Promise<string> => {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy: 'llm' } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const raw = await repos.characters.findByIdRaw(id);
    const vault = raw?.characterDocumentMountPointId as string | null;
    if (!vault) throw new Error(`character ${name} vault not minted`);
    return vault;
  };

  const imogenVault = await mkChar(spec.charImogenId, 'Imogen');
  await writeFile(imogenVault, 'Tools/gamma.tool.json', GAMMA);
  await writeFile(imogenVault, 'metadata.json', { hasKey: true, clearanceLevel: 3 });

  const vaultlessVault = await mkChar(spec.charVaultlessId, 'Vaultless');
  // Sever the link so the character genuinely has no vault (v4's `null` case).
  await rawQuery('UPDATE "characters" SET "characterDocumentMountPointId" = NULL WHERE "id" = ?', [
    spec.charVaultlessId,
  ]);

  const crackedVault = await mkChar(spec.charCrackedId, 'Cracked');
  await writeFile(crackedVault, 'Tools/delta.tool.json', DELTA);
  // Remove the keystone: the hydrating overlay now refuses this character, while
  // `findAllRaw` (what the survey deliberately uses) still sees the vault link.
  const midb2 = getRawMountIndexDatabase();
  if (!midb2) throw new Error('mount-index DB handle unavailable');
  // The visible location of a file is its LINK row; dropping the link is what a
  // deleted file looks like to every reader.
  midb2
    .prepare(`DELETE FROM "doc_mount_file_links" WHERE "mountPointId" = ? AND "relativePath" = ?`)
    .run(crackedVault, 'properties.json');

  closeMountIndexSQLiteClient();
  await closeDatabase();

  const meta = {
    projectStore,
    groupOfficial,
    imogenVault,
    vaultlessVault,
    crackedVault,
  };
  writeFileSync(mainOut + '.meta.json', JSON.stringify(meta));
  process.stderr.write(
    `built workbench fixture: main=${mainOut} mount=${mountOut} ` +
      `projectStore=${projectStore} groupOfficial=${groupOfficial} imogenVault=${imogenVault} ` +
      `vaultlessVault=${vaultlessVault} crackedVault=${crackedVault}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`workbench fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
