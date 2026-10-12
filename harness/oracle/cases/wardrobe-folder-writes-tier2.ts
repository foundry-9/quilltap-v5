/**
 * Tier-2 oracle — WARDROBE FOLDER WRITES (the mount primitives of
 * `lib/database/repositories/vault-overlay/wardrobe-writes.ts`, P4.D279 C1 §4).
 *
 * Opens COPIES of the keystone fixture pair and drives v4's REAL
 * `listFoldersInMount` / `listFolderPathsInMount` / `createFolderInMount` /
 * `renameFolderInMount` / `deleteFolderInMount` / `updateManyInMount` /
 * `createInMount` / `deleteInMount` on Ada's vault (`resolveWardrobeMount(A)`),
 * mirroring `wardrobe-folder-writes.test.ts`'s beats over a REAL store: the
 * case-insensitive folder ensure, the nested create, the batch move with one
 * unknown id skipped, `requireAll` refusing before anything is written, the
 * hand-made `Hats: Summer` kept and a `../x` folder tamed to the root (the
 * `Wardrobe write normalised an incoming folder` DEBUG), the rename that STAMPS
 * an id-less item file before the store moves it (the `Loose Scarf` keeps its
 * path-derived id), a case-only rename, the sibling conflict, the 404, the two
 * refusals of `deleteFolderInMount` (strays — a note and a `.txt`; a same-named
 * subfolder) and its send-up of items + subfolders. The third 409 (`NOT_EMPTY`)
 * is unreachable by construction — every check precedes every move and the
 * items are re-projected before the rows drop — so it is recorded here as
 * unreachable rather than posed.
 *
 * After every op: the mount's `doc_mount_folders` paths and `doc_mount_file_links`
 * paths (LOWER-CASED — the store keeps the first spelling and the compare is
 * about structure), `readMountItems` (id / title / folder / archivedAt — the
 * writer mints `updatedAt`), the op's result or thrown message, and the captured
 * `Wardrobe …` DEBUG lines.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WKS_MAIN=/tmp/qt-wks-main.db QT_FIXTURE_WKS_MOUNT=/tmp/qt-wks-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/wardrobe-folder-writes-tier2.ts \
 *     > /tmp/oracle-wardrobe-folder-writes.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, rmSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  characterAId: string;
  missingId: string;
  items: Record<string, { id: string; title: string; types: string[]; folder: string }>;
}

/** The messages this family pins (v4's exact wording). */
const PINNED_MESSAGES = [
  'Wardrobe write normalised an incoming folder',
  'Wardrobe item created in folder',
  'Wardrobe items updated in folder',
  'Wardrobe folders listed',
  'Wardrobe folder ensured',
  'Wardrobe folder renamed',
  'Wardrobe folder deleted; contents sent up',
  'Wardrobe item deleted from folder',
  'Stamping ids into wardrobe files before a folder rename',
  'Wardrobe cycle check could not load peer tiers; checking the folder alone',
  'Recursive vault projection ensured subfolders',
];

/** New items this case creates (pinned ids). */
const STRAY_SCARF_ID = 'd9000000-0000-4000-8000-000000000051';
const RAIN_HAT_ID = 'd9000000-0000-4000-8000-000000000052';

function renderValue(v: unknown, remap: (s: string) => string): string {
  if (v === null || v === undefined) return 'null';
  if (typeof v === 'string') return remap(v);
  if (typeof v === 'boolean' || typeof v === 'number') return String(v);
  return remap(JSON.stringify(v));
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'wardrobe-keystone-tier2.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_WKS_MAIN;
  const mountFixture = process.env.QT_FIXTURE_WKS_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_WKS_MAIN and QT_FIXTURE_WKS_MOUNT must point at the seeded fixture .db files');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wks-folder-writes-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'wks-main-work.db');
  const mountWork = join(scratch, 'wks-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const { logger } = await import('@/lib/logger');
  let captured: string[] = [];
  let adaMount = '';
  const remap = (s: string): string => (adaMount ? s.split(adaMount).join('<ada.mount>') : s);
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message: string } };
      if (!PINNED_MESSAGES.includes(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error) fields.error = data.error.message;
      const rendered = Object.entries(fields)
        .map(([k, v]) => ` ${k}=${renderValue(v, remap)}`)
        .join('');
      captured.push(`${data.level.toUpperCase()} ${data.message}${rendered}`);
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const writes = await import('@/lib/database/repositories/vault-overlay/wardrobe-writes');
  const { hasStableWardrobeItemId } = await import('@/lib/database/repositories/vault-overlay/parsers');

  await initializeDatabase();
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');

  const mount = await writes.resolveWardrobeMount(spec.characterAId);
  if (!mount) throw new Error('Ada has no wardrobe mount');
  adaMount = mount.mountPointId;

  const folders = (): string[] =>
    (midb.prepare('SELECT path FROM doc_mount_folders WHERE mountPointId = ?').all(adaMount) as Array<{ path: string }>)
      .map((r) => r.path.toLowerCase())
      .sort();
  const docs = (): string[] =>
    (midb.prepare('SELECT relativePath FROM doc_mount_file_links WHERE mountPointId = ?').all(adaMount) as Array<{ relativePath: string }>)
      .map((r) => r.relativePath.toLowerCase())
      .sort();
  const items = async () =>
    (await writes.readMountItems(mount))
      .map((i) => ({ id: i.id, title: i.title, folder: i.folder ?? '', archivedAt: i.archivedAt ?? null }))
      .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  const docContent = (relativePathLower: string): string | null => {
    const row = midb
      .prepare(
        `SELECT d.content AS content FROM doc_mount_file_links l
           JOIN doc_mount_documents d ON d.fileId = l.fileId
          WHERE l.mountPointId = ? AND lower(l.relativePath) = ?`,
      )
      .get(adaMount, relativePathLower) as { content: string } | undefined;
    return row?.content ?? null;
  };

  const newItem = (id: string, title: string, folder: string, types: string[]) => ({
    id,
    characterId: spec.characterAId,
    title,
    description: null,
    imagePrompt: null,
    types,
    componentItemIds: [],
    appropriateness: null,
    isDefault: false,
    replace: false,
    migratedFromClothingRecordId: null,
    archivedAt: null,
    folder,
    createdAt: '2026-02-01T00:00:00.000Z',
    updatedAt: '2026-02-01T00:00:00.000Z',
  });

  const ops: Array<Record<string, unknown>> = [];
  const op = async (label: string, fn: () => Promise<unknown>, extra?: () => Promise<Record<string, unknown>>) => {
    captured = [];
    let result: unknown;
    try {
      result = { ok: await fn() };
    } catch (error) {
      result = { threw: remap(error instanceof Error ? error.message : String(error)) };
    }
    const record: Record<string, unknown> = {
      label,
      result,
      folders: folders(),
      docs: docs(),
      items: await items(),
    };
    if (extra) record.extra = await extra();
    record.lines = captured.slice();
    ops.push(record);
  };

  const I = spec.items;
  const moveResult = (moved: Array<{ id: string; folder?: string }>) => moved.map((i) => ({ id: i.id, folder: i.folder ?? '' }));

  await op('listFolders (initial)', () => writes.listFoldersInMount(mount));
  await op('listFolderPaths (initial)', async () => (await writes.listFolderPathsInMount(mount)).sort());
  // FIRST write on the mount: the id-less `Wardrobe/Loose/no-id.md` is still
  // unstamped (every projection stamps every item), so the rename must project
  // before the store moves the folder — the `Loose Scarf` keeps the id its OLD
  // path derived.
  await op(
    'renameFolder Loose→Odds (stamps the id-less file first)',
    () => writes.renameFolderInMount(mount, 'Loose', 'Odds'),
    async () => ({
      oddsDocs: docs()
        .filter((p) => p.startsWith('wardrobe/odds/'))
        .map((p) => ({ path: p, stableId: hasStableWardrobeItemId(docContent(p)) })),
      oldPathGone: docContent('wardrobe/loose/no-id.md') === null,
    }),
  );
  await op('createFolder formal (existing, case-insensitive)', () => writes.createFolderInMount(mount, 'formal'));
  await op('createFolder Rainy/Gear (nested, with ancestor)', () => writes.createFolderInMount(mount, 'Rainy/Gear'));
  await op("createFolder '' (the root)", () => writes.createFolderInMount(mount, ''));
  await op('updateMany: coat→Formal, composite→Hats: Summer, unknown skipped', async () =>
    moveResult(
      await writes.updateManyInMount(mount, [
        { id: I.rootCoat.id, patch: { folder: 'Formal' } },
        { id: I.composite.id, patch: { folder: 'Hats: Summer' } },
        { id: spec.missingId, patch: { folder: 'Formal' } },
      ]),
    ),
  );
  await op('updateMany requireAll with a borrowed id (nothing written)', () =>
    writes.updateManyInMount(
      mount,
      [
        { id: I.filedGown.id, patch: { folder: 'Formal' } },
        { id: I.groupCloak.id, patch: { folder: 'Formal' } },
        { id: I.groupCloak.id, patch: { folder: 'Formal' } },
      ],
      { requireAll: true },
    ),
  );
  await op('updateMany: a patch with no folder change (title only)', async () =>
    moveResult(await writes.updateManyInMount(mount, [{ id: I.filedGown.id, patch: { title: 'Evening Gown' } }])),
  );
  await op("createInMount with folder '../x' (tamed to the root)", async () => {
    const stored = await writes.createInMount(mount, newItem(STRAY_SCARF_ID, 'Stray Scarf', '../x', ['accessories']) as never);
    return { id: stored.id, folder: stored.folder, characterId: stored.characterId };
  });
  await op('createInMount into Rainy/Gear', async () => {
    const stored = await writes.createInMount(mount, newItem(RAIN_HAT_ID, 'Rain Hat', 'Rainy/Gear', ['accessories']) as never);
    return { id: stored.id, folder: stored.folder };
  });
  await op('renameFolder Formal→formal (case-only)', () => writes.renameFolderInMount(mount, 'Formal', 'formal'));
  await op('renameFolder formal→Rainy (sibling conflict)', () => writes.renameFolderInMount(mount, 'formal', 'Rainy'));
  await op('renameFolder Nope→X (404)', () => writes.renameFolderInMount(mount, 'Nope', 'X'));
  await op("renameFolder '' (the root)", () => writes.renameFolderInMount(mount, '', 'X'));
  await op('deleteFolder Notes (strays: a note and a .txt)', () => writes.deleteFolderInMount(mount, 'Notes'));
  await op('createFolder formal/formal (the same-name plant)', () => writes.createFolderInMount(mount, 'formal/formal'));
  await op('deleteFolder formal (same-named subfolder)', () => writes.deleteFolderInMount(mount, 'formal'));
  await op('deleteFolder formal/formal (empty)', () => writes.deleteFolderInMount(mount, 'formal/formal'));
  await op('deleteFolder Hats: Summer (one item sent to the root)', () => writes.deleteFolderInMount(mount, 'hats: summer'));
  await op('deleteFolder Rainy (item + subfolder sent up)', () => writes.deleteFolderInMount(mount, 'Rainy'));
  await op('deleteFolder Nope (404)', () => writes.deleteFolderInMount(mount, 'Nope'));
  await op("deleteFolder '' (the root)", () => writes.deleteFolderInMount(mount, ''));
  await op('deleteInMount stray scarf', () => writes.deleteInMount(mount, STRAY_SCARF_ID));
  await op('deleteInMount unknown', () => writes.deleteInMount(mount, spec.missingId));
  await op('listFolders (final)', () => writes.listFoldersInMount(mount));
  await op('listFolderPaths (final)', async () => (await writes.listFolderPathsInMount(mount)).sort());

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'wardrobe-folder-writes-tier2', ops }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-folder-writes-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
