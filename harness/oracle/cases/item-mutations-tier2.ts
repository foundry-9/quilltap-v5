/**
 * Oracle case — v4's wardrobe ITEM + FOLDER MUTATIONS (P4.D279 keystone C1
 * §11): the REAL `createItem` / `updateItem` / `setItemArchived` /
 * `moveItemsToFolder` / `componentLookupFor` / `validateComponentRefs`
 * (`lib/wardrobe/item-mutations.ts`) and `createWardrobeFolder` /
 * `renameWardrobeFolder` / `deleteWardrobeFolder`
 * (`lib/wardrobe/folder-mutations.ts`), each over a REAL
 * `resolveWardrobeLocation(...)`.
 *
 * Copies the baked fixtures (build-item-mutations-fixture.ts), then drives the
 * corpus `ops`. After EVERY op it records `{ op, result | error, items:
 * location.readItems(true), folders: location.listFolders(), logs }` where
 * `logs` are the `[ItemMutations]` / `[FolderMutations]` DEBUG lines the op
 * emitted (message + fields, every value stringified: strings as given,
 * booleans/numbers via String(), null as "null", arrays as JSON). v4's lines
 * are captured by splicing a sink into the singleton logger's transports. The
 * Rust port runs the same ops over a copy of the same fixture and diffs after
 * normalising minted ids / stamps (see the Rust test's header).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ITEM_MUTATIONS_MAIN=/tmp/qt-itemmut-main.db \
 *   QT_FIXTURE_ITEM_MUTATIONS_MOUNT=/tmp/qt-itemmut-mount.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/item-mutations-tier2.ts \
 *     > /tmp/oracle-item-mutations.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

type Scope = 'character' | 'general';

interface Op {
  op: string;
  scope: Scope;
  body?: Record<string, unknown>;
  lookup?: boolean;
  preserve?: Record<string, unknown>;
  target?: string;
  patch?: Record<string, unknown>;
  archived?: boolean;
  itemIds?: string[];
  folder?: string;
  path?: string;
  name?: string;
  ids?: string[];
  relativePath?: string;
  content?: string;
}

interface Spec {
  testPepperBase64: string;
  userId: string;
  characterId: string;
  ops: Op[];
}

interface LogLine {
  message: string;
  fields: Record<string, string>;
}

const CAPTURED_PREFIXES = ['[ItemMutations]', '[FolderMutations]'];

function stringify(v: unknown): string {
  if (v === null || v === undefined) return 'null';
  if (typeof v === 'string') return v;
  if (typeof v === 'boolean' || typeof v === 'number') return String(v);
  return JSON.stringify(v);
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'item-mutations-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const mainFixture = process.env.QT_FIXTURE_ITEM_MUTATIONS_MAIN;
  const mountFixture = process.env.QT_FIXTURE_ITEM_MUTATIONS_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error(
      'QT_FIXTURE_ITEM_MUTATIONS_MAIN and QT_FIXTURE_ITEM_MUTATIONS_MOUNT must point at the fixtures from build-item-mutations-fixture.ts',
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-itemmut-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'itemmut-main-work.db');
  const mountWork = join(scratch, 'itemmut-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // The compared lines are DEBUG — the logger must let them through.
  process.env.LOG_LEVEL = 'debug';

  // Capture v4's lines by splicing a sink into the singleton logger's shared
  // transports (children included); the `service` / `module` / `environment`
  // child fields are dropped (v5 renders them as the tracing target).
  const { logger } = await import('@/lib/logger');
  const logs: LogLine[] = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!CAPTURED_PREFIXES.some((p) => data.message.startsWith(p))) return;
      const { service: _s, module: _m, environment: _e, ...rest } = (data.context ?? {}) as Record<
        string,
        unknown
      >;
      const fields: Record<string, string> = {};
      for (const [k, v] of Object.entries(rest)) {
        if (v === undefined) continue;
        fields[k] = stringify(v);
      }
      logs.push({ message: data.message, fields });
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { resolveWardrobeLocation } = await import('@/lib/wardrobe/location');
  const {
    componentLookupFor,
    createItem,
    updateItem,
    setItemArchived,
    moveItemsToFolder,
    validateComponentRefs,
  } = await import('@/lib/wardrobe/item-mutations');
  const { createWardrobeFolder, renameWardrobeFolder, deleteWardrobeFolder } = await import(
    '@/lib/wardrobe/folder-mutations'
  );
  const { DocMountFileLinksRepository } = await import(
    '@/lib/database/repositories/doc-mount-file-links.repository'
  );
  const { sha256OfString } = await import('@/lib/utils/sha256');
  type WardrobeLocation = NonNullable<Awaited<ReturnType<typeof resolveWardrobeLocation>>>;
  type WardrobeItem = Awaited<ReturnType<WardrobeLocation['readItems']>>[number];

  await initializeDatabase();
  const repos = getRepositories();

  const locationFor = async (scope: Scope): Promise<WardrobeLocation> => {
    const location = await resolveWardrobeLocation(
      scope,
      scope === 'character' ? spec.characterId : null,
      repos,
      spec.userId,
      { ensure: true },
    );
    if (!location) throw new Error(`no ${scope} location`);
    return location;
  };

  const findByTitle = async (location: WardrobeLocation, title: string): Promise<WardrobeItem> => {
    const items = await location.readItems(true);
    const item = items.find((i) => i.title === title);
    if (!item) throw new Error(`target not found by title: ${title}`);
    return item;
  };

  const results: unknown[] = [];
  for (const op of spec.ops) {
    const location = await locationFor(op.scope);
    logs.length = 0;
    let result: unknown;
    try {
      switch (op.op) {
        case 'componentLookup': {
          const lookup = await componentLookupFor(repos, location, []);
          result = { kind: 'ok', ids: Array.from(lookup.keys()).sort() };
          break;
        }
        case 'validateRefs': {
          const lookup = await componentLookupFor(repos, location, []);
          const { components, missing } = validateComponentRefs(lookup, op.ids ?? []);
          result = { kind: 'ok', components: components.map((c) => c.id), missing };
          break;
        }
        case 'create': {
          const lookup = op.lookup ? await componentLookupFor(repos, location, []) : undefined;
          const item = await createItem(location, op.body as never, {
            lookup,
            preserve: op.preserve as never,
          });
          result = { kind: 'ok', item };
          break;
        }
        case 'update': {
          const current = await findByTitle(location, op.target!);
          const lookup = op.lookup ? await componentLookupFor(repos, location, []) : undefined;
          const updated = await updateItem(location, current, op.patch as never, { lookup });
          result = {
            kind: 'ok',
            found: updated !== null,
            item: updated,
            archivedAtSame: (updated?.archivedAt ?? null) === (current.archivedAt ?? null),
            updatedAtSame: updated?.updatedAt === current.updatedAt,
          };
          break;
        }
        case 'setArchived': {
          const current = await findByTitle(location, op.target!);
          const { item, changed } = await setItemArchived(location, current, op.archived === true);
          result = {
            kind: 'ok',
            changed,
            item,
            updatedAtSame: item?.updatedAt === current.updatedAt,
          };
          break;
        }
        case 'move': {
          const moved = await moveItemsToFolder(location, op.itemIds ?? [], op.folder ?? '');
          result = { kind: 'ok', moved };
          break;
        }
        case 'createFolder': {
          const folder = await createWardrobeFolder(location, op.path ?? '');
          result = { kind: 'ok', folder };
          break;
        }
        case 'renameFolder': {
          const folder = await renameWardrobeFolder(location, op.path ?? '', op.name ?? '');
          result = { kind: 'ok', folder };
          break;
        }
        case 'deleteFolder': {
          const { movedItems, movedFolders } = await deleteWardrobeFolder(location, op.path ?? '');
          result = { kind: 'ok', movedItems, movedFolders };
          break;
        }
        case 'plantNote': {
          // A file the wardrobe doesn't own, planted where a later delete must
          // refuse. linkDocumentContent directly (writeDatabaseDocument's
          // QUILLTAP_JOB_CHILD switch breaks initializeDatabase — the
          // document-store-oracle gotcha).
          const rel = op.relativePath ?? '';
          const content = op.content ?? '';
          const links = new DocMountFileLinksRepository();
          await links.linkDocumentContent({
            mountPointId: location.mountPointId,
            relativePath: rel,
            fileName: rel.includes('/') ? rel.slice(rel.lastIndexOf('/') + 1) : rel,
            folderId: null,
            fileType: 'markdown',
            content,
            contentSha256: sha256OfString(content),
            plainTextLength: content.length,
            fileSizeBytes: Buffer.byteLength(content, 'utf-8'),
          });
          result = { kind: 'ok' };
          break;
        }
        default:
          throw new Error(`unknown op ${op.op}`);
      }
    } catch (err) {
      result = { kind: 'error', message: errorMessage(err) };
    }
    const items = await location.readItems(true);
    const folders = await location.listFolders();
    results.push({ op: op.op, result, items, folders, logs: [...logs] });
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stdout.write(JSON.stringify({ case: 'item-mutations-tier2', results }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`item-mutations-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
