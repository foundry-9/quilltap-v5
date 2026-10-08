/**
 * @jest-environment node
 *
 * Tier-2 ORACLE for the wardrobe ITEM-IMAGES module (P4.D263; v4
 * `lib/wardrobe/item-images.ts` + `lib/file-storage/wardrobe-image-bridge.ts`,
 * `7c8572869`), ported to `quilltap-core::services::{wardrobe_container,
 * wardrobe_image_bridge, wardrobe_item_images}`.
 *
 * Drives v4's REAL exports (`resolveWardrobeItemHome`,
 * `listWardrobeItemImages`, `toWardrobeImageSummary`, `addWardrobeItemImage`,
 * `setCurrentWardrobeItemImage`, `deleteWardrobeItemImage`,
 * `cleanupItemImages`, `carryItemImages`, `commitMovedImages`,
 * `dropSourceImageLinks`) against a REAL two-DB fixture: the DB stack is
 * doMocked back to the real modules (past jest.setup's global mocks) with the
 * real cipher binding — the `jest-real-db-oracle` shape of
 * `wardrobe-transfers.test.ts`.
 *
 * Per scenario: a FRESH module graph and a FRESH copy of the baked fixture;
 * symbolic picture refs resolved against the baked state; the op run with
 * v4's logger spied (every `[WardrobeImages]` / `[WardrobeImageBridge]` /
 * `[WardrobeContainer]` / cleanup-tag line, level + message + meta in key
 * order); then the main `files` table and the mount-index tables dumped, plus
 * every item's frontmatter pointer read back through v4's own resolver.
 * Emits one NDJSON line per scenario:
 * `{ name, result, logs, pointers, tables }`.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores
 * .claude/ paths, and this case reads its spec from `../fixtures/`):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-wii-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/wardrobe-item-images.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/wardrobe-item-images-tier2.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WII_MAIN=/tmp/qt-wii-main.db QT_FIXTURE_WII_MOUNT=/tmp/qt-wii-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-item-images-fixture.ts
 *   QT_FIXTURE_WII_MAIN=/tmp/qt-wii-main.db QT_FIXTURE_WII_MOUNT=/tmp/qt-wii-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-wardrobe-item-images.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=600000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- wardrobe-item-images
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

type Ref = { item?: string; nth?: number; doc?: string };
interface Scenario {
  name: string;
  op: string;
  scope?: string;
  container?: string | null;
  item: string;
  kind?: string;
  webp?: number;
  generationPrompt?: string | null;
  generationModel?: string | null;
  generationRevisedPrompt?: string | null;
  width?: number | null;
  height?: number | null;
  file?: Ref;
  mode?: 'move' | 'copy';
  destinationItem?: string;
  destination?: string;
  commit?: boolean;
  links?: Array<{ mount: string; nth?: number; leaf?: string }>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  characterId: string;
  archivedCharacterId: string;
  strangerCharacterId: string;
  projectId: string;
  groupId: string;
  generalMountPointId: string;
  items: Record<string, { id: string; home: string }>;
  webp: string[];
  copyDestinationItemId: string;
  scenarios: Scenario[];
}

const NOWHERE_ID = '99999999-0000-4000-8000-000000000000';

const MOUNT_TABLES: Array<{ key: string; table: string; skip?: string[] }> = [
  { key: 'points', table: 'doc_mount_points' },
  { key: 'folders', table: 'doc_mount_folders' },
  { key: 'links', table: 'doc_mount_file_links' },
  { key: 'mountFiles', table: 'doc_mount_files' },
  { key: 'documents', table: 'doc_mount_documents' },
  { key: 'blobs', table: 'doc_mount_blobs' },
];

const LOG_PREFIXES = [
  '[WardrobeImages]',
  '[WardrobeImageBridge]',
  '[WardrobeContainer]',
  '[Wardrobe v1]',
];

function sanitize(meta: unknown): unknown {
  if (meta === undefined) return null;
  return JSON.parse(
    // An `undefined` field is DROPPED, as winston drops it (§R.5).
    JSON.stringify(meta, (_k, v) => (v instanceof Error ? v.message : v)),
  );
}

async function runScenario(
  spec: Spec,
  scenario: Scenario,
  scratch: string,
  mainFixture: string,
  mountFixture: string,
): Promise<Record<string, unknown>> {
  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () =>
    jest.requireActual('@/lib/repositories/factory'),
  );
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  jest.doMock('@/lib/embedding/embedding-service', () =>
    jest.requireActual('@/lib/embedding/embedding-service'),
  );

  const work = mkdtempSync(join(scratch, 'sc-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const loggerModule = await import('@/lib/logger');
  const ii = await import('@/lib/wardrobe/item-images');

  await initializeDatabase();
  const repos = getRepositories();
  const raw = getRawDatabase();
  const midb = getRawMountIndexDatabase();
  if (!raw || !midb) throw new Error('DB handles unavailable');

  const containerId = (c: string | null | undefined): string | null => {
    switch (c) {
      case 'character':
        return spec.characterId;
      case 'archived':
        return spec.archivedCharacterId;
      case 'stranger':
        return spec.strangerCharacterId;
      case 'project':
        return spec.projectId;
      case 'group':
        return spec.groupId;
      case 'nowhere':
        return NOWHERE_ID;
      default:
        return null;
    }
  };
  const mountOf = async (m: string): Promise<string> => {
    if (m === 'general') return spec.generalMountPointId;
    if (m === 'character')
      return (await repos.characters.findByIdRaw(spec.characterId))
        ?.characterDocumentMountPointId as string;
    if (m === 'project')
      return (await repos.projects.findByIdRaw(spec.projectId))?.officialMountPointId as string;
    return (await repos.groups.findByIdRaw(spec.groupId))?.officialMountPointId as string;
  };
  const resolveRef = async (ref: Ref): Promise<string> => {
    if (ref.doc) {
      const id = spec.items[ref.doc].id;
      const row = raw
        .prepare(
          "SELECT f.id AS id FROM files f, json_each(f.linkedTo) j WHERE j.value = ? AND f.category = 'DOCUMENT'",
        )
        .get(id) as { id: string } | undefined;
      if (!row) throw new Error(`no DOCUMENT file for ${ref.doc}`);
      return row.id;
    }
    const list = await ii.listWardrobeItemImages(repos, spec.items[ref.item as string].id);
    const f = list[ref.nth ?? 0];
    if (!f) throw new Error(`no picture ${ref.nth} for ${ref.item}`);
    return f.id;
  };
  const homeFor = () =>
    ii.resolveWardrobeItemHome(
      repos,
      spec.userId,
      scenario.scope as never,
      containerId(scenario.container),
      spec.items[scenario.item].id,
    );

  // Resolve every symbolic ref BEFORE the logger is spied.
  const fileId = scenario.file ? await resolveRef(scenario.file) : null;
  const dropLinks: Array<{ mountPointId: string; leafName: string }> = [];
  for (const l of scenario.links ?? []) {
    const mountPointId = await mountOf(l.mount);
    if (l.leaf) {
      dropLinks.push({ mountPointId, leafName: l.leaf });
    } else {
      const list = await ii.listWardrobeItemImages(repos, spec.items[scenario.item].id);
      dropLinks.push({ mountPointId, leafName: list[l.nth ?? 0].originalFilename });
    }
  }
  const destinationMountPointId = scenario.destination ? await mountOf(scenario.destination) : null;

  const logs: Array<{ level: string; message: string; meta: unknown }> = [];
  const spies: jest.SpyInstance[] = [];
  for (const level of ['debug', 'info', 'warn', 'error'] as const) {
    spies.push(
      jest.spyOn(loggerModule.logger as never, level as never).mockImplementation(((
        message: string,
        meta?: unknown,
      ) => {
        if (typeof message === 'string' && LOG_PREFIXES.some((p) => message.startsWith(p))) {
          logs.push({ level, message, meta: sanitize(meta) });
        }
      }) as never),
    );
  }

  const errorOf = (e: unknown) => ({
    error: e instanceof Error ? e.name : 'unknown',
    message: e instanceof Error ? e.message : String(e),
  });

  let result: unknown;
  try {
    switch (scenario.op) {
      case 'home': {
        const home = await homeFor();
        result = home
          ? {
              scope: home.scope,
              characterId: home.characterId,
              itemId: home.item.id,
              imageFileId: home.item.imageFileId ?? null,
              containerItemIds: home.containerItems.map((i) => i.id).sort(),
            }
          : null;
        break;
      }
      case 'list': {
        const home = await homeFor();
        if (!home) throw new Error('no home');
        const images = await ii.listWardrobeItemImages(repos, home.item.id);
        result = {
          images: images.map(ii.toWardrobeImageSummary),
          itemImageFileId: home.item.imageFileId ?? null,
        };
        break;
      }
      case 'add': {
        const home = await homeFor();
        if (!home) throw new Error('no home');
        const { file, item } = await ii.addWardrobeItemImage(repos, home, {
          userId: spec.userId,
          kind: scenario.kind as never,
          content: Buffer.from(spec.webp[scenario.webp ?? 0], 'base64'),
          contentType: 'image/webp',
          width: scenario.width ?? null,
          height: scenario.height ?? null,
          generationPrompt: scenario.generationPrompt ?? null,
          generationModel: scenario.generationModel ?? null,
          generationRevisedPrompt: scenario.generationRevisedPrompt ?? null,
        });
        result = {
          file: ii.toWardrobeImageSummary(file),
          originalFilename: file.originalFilename,
          itemImageFileId: item?.imageFileId ?? null,
        };
        break;
      }
      case 'setCurrent': {
        const home = await homeFor();
        if (!home) throw new Error('no home');
        result = { current: await ii.setCurrentWardrobeItemImage(repos, home, fileId as string) };
        break;
      }
      case 'delete': {
        const home = await homeFor();
        if (!home) throw new Error('no home');
        result = { current: await ii.deleteWardrobeItemImage(repos, home, fileId as string) };
        break;
      }
      case 'cleanup': {
        await ii.cleanupItemImages(repos, spec.items[scenario.item].id, '[Wardrobe v1]', {
          characterId: spec.characterId,
        });
        result = { done: true };
        break;
      }
      case 'carry': {
        const sourceItemId = spec.items[scenario.item].id;
        const destinationItemId =
          scenario.destinationItem === 'copy'
            ? spec.copyDestinationItemId
            : spec.items[scenario.destinationItem as string].id;
        const { fileIdMap, pendingMove } = await ii.carryItemImages(repos, {
          mode: scenario.mode as 'move' | 'copy',
          sourceItemId,
          destinationItemId,
          destinationMountPointId: destinationMountPointId as string,
          userId: spec.userId,
        });
        if (scenario.commit) await ii.commitMovedImages(repos, sourceItemId, pendingMove);
        result = { fileIdMap: [...fileIdMap.entries()], pendingMove };
        break;
      }
      case 'drop': {
        await ii.dropSourceImageLinks(spec.items[scenario.item].id, dropLinks);
        result = { done: true };
        break;
      }
      default:
        throw new Error(`unknown op ${scenario.op}`);
    }
  } catch (e) {
    result = errorOf(e);
  }
  for (const s of spies) s.mockRestore();

  try {
    // Every item's frontmatter pointer, read back through v4's own resolver.
    const pointers: Record<string, unknown> = {};
    for (const [key, item] of Object.entries(spec.items)) {
      const [scope, container] =
        item.home === 'character'
          ? ['character', spec.characterId]
          : item.home === 'archived'
            ? ['character', spec.archivedCharacterId]
            : item.home === 'general'
              ? ['general', null]
              : item.home === 'project'
                ? ['project', spec.projectId]
                : ['group', spec.groupId];
      const home = await ii.resolveWardrobeItemHome(
        repos,
        spec.userId,
        scope as never,
        container as string | null,
        item.id,
      );
      pointers[key] = home ? (home.item.imageFileId ?? null) : '<gone>';
    }

    const dump = (db: { prepare: (s: string) => { all: () => unknown[] } }, table: string) => {
      const columns = (db.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>).map(
        (c) => c.name,
      );
      const rows = (db.prepare(`SELECT * FROM ${table} ORDER BY rowid`).all() as Array<
        Record<string, unknown>
      >).map((r) => {
        const out: Record<string, unknown> = {};
        for (const col of columns) out[col] = canonValue(r[col]);
        return out;
      });
      return { table, columns, rows };
    };
    const tables: Record<string, unknown> = { files: dump(raw as never, 'files') };
    for (const t of MOUNT_TABLES) tables[t.key] = dump(midb as never, t.table);

    return { name: scenario.name, result, logs, pointers, tables };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'wardrobe-item-images-tier2.json'), 'utf8'),
  ) as Spec;
  const mainFixture = process.env.QT_FIXTURE_WII_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WII_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_WII_MAIN and QT_FIXTURE_WII_MOUNT must point at the seed fixtures from build-wardrobe-item-images-fixture.ts',
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wii-oracle-'));
  scratchDirs.push(scratch);
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const scenario of spec.scenarios) {
    const payload = await runScenario(spec, scenario, scratch, mainFixture, mountFixture);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(
    `wardrobe-item-images oracle wrote ${outPath} (${outLines.length} scenarios)\n`,
  );
}

test('wardrobe-item-images tier-2 oracle', async () => {
  await main();
});

const scratchDirs: string[] = [];
afterAll(() => {
  for (const d of scratchDirs) rmSync(d, { recursive: true, force: true });
});
