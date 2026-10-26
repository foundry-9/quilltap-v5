/**
 * Tier-2 oracle case (P4.D324, the Inventory keystone — v4 `4fbdad186` at
 * `322a5488e`): location, pool, dependents, the equipped set, the mutations,
 * the pictures, the transfers and equip / arrival defaults — one op list
 * (`inventory-keystone.json`'s `ops`) over the ONE keystone pair
 * (`build-inventory-mutations-fixture.ts`), each op tagged with its family.
 *
 * Every op runs in its OWN child process on a FRESH copy of the pair (v4's
 * module singletons — the index cache, the realtime bus, the job host — never
 * leak between ops), through v4's REAL code. Per op the child emits ONE line:
 *   - `result` (`JSON.stringify` — KEY ORDER is the contract) or `error`
 *     (`{name, message, code?, status?, details?}`);
 *   - `logs`: every line v4 logged, recorded off the `Logger` prototype before
 *     the level check (`{level, message, fields: [[k, v]…]}`);
 *   - `delta`: per dumped table, the rows that differ from the fixture's (whole
 *     rows, BLOB data as its length) and the fixture row ids now missing.
 * The Rust side computes its own delta against the same fixture and both are
 * normalised identically (minted ids / timestamps), so neither side's minting
 * order matters.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the pair:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_INV_MAIN=/tmp/qt-inv-main.db QT_FIXTURE_INV_MOUNT=/tmp/qt-inv-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/inventory-keystone-tier2.ts \
 *     > /tmp/oracle-inv-keystone.ndjson
 *
 * v4 runs WITHOUT its title lint ({@link withoutTitleLint} — R-7: the keystone
 * ports the mutations without the lint calls; the lint is P4.D340's): the
 * lint's pool reads, census lines and refusals are not the keystone's to
 * match.
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';

type Json = any; // eslint-disable-line @typescript-eslint/no-explicit-any

interface Spec {
  testPepperBase64: string;
  userId: string;
  webp: string[];
  ops: Array<Record<string, Json> & { name: string; family: string; kind: string }>;
}

/** The tables a delta covers: [db, table, key column]. */
const TABLES: Array<[string, string, string]> = [
  ['mount', 'doc_mount_documents', 'id'],
  ['mount', 'doc_mount_files', 'id'],
  ['mount', 'doc_mount_file_links', 'id'],
  ['mount', 'doc_mount_folders', 'id'],
  ['mount', 'doc_mount_blobs', 'id'],
  ['main', 'files', 'id'],
  ['main', 'chats', 'id'],
];

const here = dirname(fileURLToPath(import.meta.url));
const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'inventory-keystone.json'), 'utf8')) as Spec;
const fixtureMain = process.env.QT_FIXTURE_INV_MAIN;
const fixtureMount = process.env.QT_FIXTURE_INV_MOUNT;
if (!fixtureMain || !fixtureMount || !existsSync(fixtureMain) || !existsSync(fixtureMount)) {
  throw new Error('QT_FIXTURE_INV_MAIN / QT_FIXTURE_INV_MOUNT must point at the keystone pair');
}

function canon(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (Buffer.isBuffer(v) || v instanceof Uint8Array) return `<blob ${v.length}>`;
  return v;
}

async function dumpAll(rawMain: Json, rawMount: Json): Promise<Record<string, Array<Record<string, unknown>>>> {
  const out: Record<string, Array<Record<string, unknown>>> = {};
  for (const [db, table] of TABLES) {
    const raw = db === 'main' ? rawMain : rawMount;
    const cols = (raw.prepare(`PRAGMA table_info("${table}")`).all() as Array<{ name: string }>).map((c) => c.name);
    const rows = raw.prepare(`SELECT * FROM "${table}"`).all() as Array<Record<string, unknown>>;
    out[table] = rows.map((r) => {
      const o: Record<string, unknown> = {};
      for (const c of cols) o[c] = canon(r[c]);
      return o;
    });
  }
  return out;
}

/**
 * Run v4 without its Inventory title lint — `lib/inventory/title-lint.ts` and
 * the document-write layer `lib/inventory/title-lint-documents.ts`. tsx
 * compiles v4 to CommonJS, so every `import` of a module is a `require` that
 * hits `require.cache`: each lint module's cache entry is replaced, BEFORE any
 * module that imports it loads, by its real exports with the call-site entry
 * points overridden — `lintInventoryTitle` answers a clean verdict (`{ rows:
 * [] }`: no pool read, no census, no refusal), `writeTitleLintRows` writes
 * nothing, the database store's `lintInventoryDocumentChange` answers a no-op
 * finisher. Checked: each override must be what a fresh `require` returns.
 */
function withoutTitleLint(): void {
  const req = createRequire(fileURLToPath(import.meta.url));
  const overrides: Record<string, Record<string, unknown>> = {
    '@/lib/inventory/title-lint': {
      lintInventoryTitle: async () => ({ rows: [] }),
      writeTitleLintRows: () => undefined,
    },
    '@/lib/inventory/title-lint-documents': {
      lintInventoryDocumentChange: async () => () => undefined,
    },
  };
  for (const [specifier, stub] of Object.entries(overrides)) {
    const path = req.resolve(specifier);
    const real = req(specifier) as Record<string, unknown>;
    const entry = req.cache[path];
    if (!entry) throw new Error(`no require.cache entry for ${specifier}`);
    entry.exports = { ...real, ...stub };
    for (const name of Object.keys(stub)) {
      if ((req(specifier) as Record<string, unknown>)[name] !== stub[name]) {
        throw new Error(`the ${specifier} override did not take`);
      }
    }
  }
}

async function runChild(index: number): Promise<void> {
  const op = spec.ops[index];
  const scratch = mkdtempSync(join(tmpdir(), 'qt-inv-keystone-op-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main.db');
  const mountWork = join(scratch, 'mount.db');
  copyFileSync(fixtureMain!, mainWork);
  copyFileSync(fixtureMount!, mountWork);
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { Logger } = await import('@/lib/logger');
  let logs: Array<{ level: string; message: string; fields: Array<[string, unknown]> }> = [];
  let recording = false;
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (this: unknown, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      if (recording) {
        logs.push({
          level,
          message,
          fields: Object.entries(context ?? {}).filter(([, v]) => v !== undefined),
        });
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  withoutTitleLint();
  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  await initializeDatabase();
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const repos = getRepositories();
  const userId = spec.userId;
  const rawMain = getRawDatabase();
  const rawMount = getRawMountIndexDatabase();
  const before = await dumpAll(rawMain, rawMount);

  const location = await import('@/lib/inventory/location');
  const pool = await import('@/lib/inventory/pool');
  const dependents = await import('@/lib/inventory/dependents');
  const equippedSet = await import('@/lib/inventory/equipped-set');
  const mutations = await import('@/lib/inventory/item-mutations');
  const images = await import('@/lib/inventory/item-images');
  const transfers = await import('@/lib/inventory/transfers');
  const equip = await import('@/lib/inventory/equip');
  const defaults = await import('@/lib/inventory/apply-defaults');
  const types = await import('@/lib/schemas/inventory.types');

  const loc = async (s: Json) => {
    const l = await location.resolveInventoryLocation(s.scope, s.id ?? null, repos, userId, { ensure: !!s.ensure });
    if (!l) throw new Error(`no location for ${s.scope}:${s.id}`);
    return l;
  };
  const locSummary = (l: Json) =>
    l && { scope: l.scope, id: l.id, mountPointId: l.mountPointId, characterId: l.characterId, origin: l.origin };
  const poolSummary = (p: Json) => ({
    reachable: p.resolved(),
    byId: Array.from(p.byId.keys()),
    own: p.own.map((r: Json) => r.id),
    consultedMounts: p.consultedMounts,
    unreadMounts: p.unreadMounts,
  });
  const recordOf = async (s: Json, itemId: string) => {
    const l = await loc(s);
    const r = await l.findItem(itemId);
    if (!r) throw new Error(`no record ${itemId}`);
    return { l, r };
  };
  const chatOf = async (chatId: string) => {
    const c = await repos.chats.findById(chatId);
    if (!c) throw new Error(`no chat ${chatId}`);
    return c as Json;
  };
  const ctxOf = (op: Json) => ({ userId, provenance: op.provenance ?? 'operator', announce: false });
  const pictureId = async (itemId: string, n: number) => (await images.listInventoryItemImages(repos, itemId))[n]?.id ?? `no-picture-${n}`;

  recording = true;
  let result: unknown = null;
  let error: Json = null;
  try {
    switch (op.kind) {
      case 'resolveLocation':
        result = locSummary(
          await location.resolveInventoryLocation(op.scope, op.id ?? null, repos, userId, { ensure: !!op.ensure }),
        );
        break;
      case 'readItems':
        result = await (await loc(op.at)).readItems(!!op.includeArchived);
        break;
      case 'listFolders':
        result = await (await loc(op.at)).listFolders();
        break;
      case 'loadPool':
        result = poolSummary(
          await pool.loadInventoryPool(repos, { characterId: op.characterId, chatId: op.chatId, projectId: op.projectId }),
        );
        break;
      case 'loadPoolForLocation':
        result = poolSummary(await pool.loadPoolForLocation(repos, await loc(op.at), op.context ?? {}));
        break;
      case 'findDependents':
        result = await dependents.findDependents(op.archetypeId);
        break;
      case 'findRecordsAnywhere':
        result = Array.from((await dependents.findRecordsAnywhere(op.ids)).entries());
        break;
      case 'findKitsStranded': {
        const t = await loc(op.target);
        result = await dependents.findKitsStrandedByMove(op.itemId, { mountPointId: t.mountPointId, scope: t.scope });
        break;
      }
      case 'readEquippedSet': {
        const chat = await chatOf(op.chatId);
        const p = await pool.loadInventoryPool(repos, { characterId: op.characterId, chatId: chat.id, projectId: chat.projectId ?? null });
        result = equippedSet.readEquippedSet(chat, op.characterId, p);
        break;
      }
      case 'create':
        result = await mutations.createInventoryItem(repos, await loc(op.at), types.createInventorySchema.parse(op.body) as never);
        break;
      case 'instantiate': {
        const body = types.instantiateInventorySchema.parse(op.body) as Json;
        result = await mutations.instantiate(repos, await loc(op.at), body.archetypeId, {
          folder: body.folder,
          overrides: body.overrides,
        });
        break;
      }
      case 'update': {
        const { l, r } = await recordOf(op.at, op.itemId);
        result = await mutations.updateInventoryItem(repos, l, r, types.updateInventorySchema.parse(op.body) as never, op.context ?? {});
        break;
      }
      case 'clearOverrides': {
        const { l, r } = await recordOf(op.at, op.itemId);
        result = await mutations.clearOverrides(l, r, op.fields);
        break;
      }
      case 'setArchived': {
        const { l, r } = await recordOf(op.at, op.itemId);
        result = await mutations.setInventoryItemArchived(l, r, op.archived);
        break;
      }
      case 'detach': {
        const { l, r } = await recordOf(op.at, op.itemId);
        result = await mutations.detach(repos, l, r);
        break;
      }
      case 'detachAll':
        result = await mutations.detachAll(op.archetypeId);
        break;
      case 'delete': {
        const { l, r } = await recordOf(op.at, op.itemId);
        result = await mutations.deleteInventoryItem(repos, l, r, { force: !!op.force });
        break;
      }
      case 'duplicate': {
        const { l, r } = await recordOf(op.at, op.itemId);
        result = await mutations.duplicateInventoryItem(repos, l, r);
        break;
      }
      case 'moveToFolder':
        result = await mutations.moveInventoryItemsToFolder(await loc(op.at), op.itemIds, op.folder);
        break;
      case 'copyToFolder':
        result = await mutations.copyInventoryItemsToFolder(repos, await loc(op.at), op.itemIds, op.folder);
        break;
      case 'createFolder':
        result = await mutations.createInventoryFolder(await loc(op.at), op.path);
        break;
      case 'renameFolder':
        result = await mutations.renameInventoryFolder(await loc(op.at), op.path, op.newName);
        break;
      case 'deleteFolder':
        result = await mutations.deleteInventoryFolder(await loc(op.at), op.path);
        break;
      case 'listImages': {
        const home = await images.resolveInventoryItemHome(repos, userId, op.at.scope, op.at.id ?? null, op.itemId);
        result = home && (await images.listInventoryImagesWithFallthrough(repos, home.item));
        break;
      }
      case 'addImage': {
        const home = await images.resolveInventoryItemHome(repos, userId, op.at.scope, op.at.id ?? null, op.itemId);
        if (!home) throw new Error('no home');
        const added = await images.addInventoryItemImage(repos, home, {
          userId,
          kind: op.imageKind,
          content: Buffer.from(spec.webp[op.webp], 'base64'),
          contentType: 'image/webp',
          width: null,
          height: null,
          generationPrompt: null,
          generationModel: null,
          generationRevisedPrompt: null,
        });
        result = { fileId: added.file.id, record: added.record };
        break;
      }
      case 'setCurrentImage': {
        const home = await images.resolveInventoryItemHome(repos, userId, op.at.scope, op.at.id ?? null, op.itemId);
        if (!home) throw new Error('no home');
        result = await images.setCurrentInventoryItemImage(repos, home, op.fileId ?? (await pictureId(op.itemId, op.picture)));
        break;
      }
      case 'deleteImage': {
        const home = await images.resolveInventoryItemHome(repos, userId, op.at.scope, op.at.id ?? null, op.itemId);
        if (!home) throw new Error('no home');
        result = await images.deleteInventoryItemImage(repos, home, op.fileId ?? (await pictureId(op.itemId, op.picture)));
        break;
      }
      case 'transfer': {
        const source = await loc(op.source);
        const destination = await loc(op.destination);
        const record = await source.findItem(op.itemId);
        if (!record) throw new Error('no source record');
        result = await transfers.transferInventoryItem(repos, { action: op.action, source, destination, record, userId });
        break;
      }
      case 'equip':
        result = await equip.equipItems(repos, await chatOf(op.chatId), op.characterId, op.ids, ctxOf(op), { lenient: !!op.lenient });
        break;
      case 'unequip':
        result = await equip.unequipItems(repos, await chatOf(op.chatId), op.characterId, op.ids, ctxOf(op));
        break;
      case 'pickUp':
        result = await equip.pickUpItems(repos, await chatOf(op.chatId), op.characterId, op.archetypeIds, ctxOf(op), {
          title: op.title,
          equip: op.equip,
        });
        break;
      case 'dropEverywhere':
        result = await equip.dropCarriedEverywhere(repos, op.characterId, op.itemId, op.also ?? []);
        break;
      case 'applyDefaults':
        await defaults.applyInventoryDefaults(repos, op.chatId, op.characterIds, {
          source: op.source,
          userId,
          sourceChatId: op.sourceChatId ?? null,
        });
        result = null;
        break;
      case 'resetDefaults':
        result = await defaults.resetInventoryToDefaults(repos, await chatOf(op.chatId), op.characterId, userId);
        break;
      default:
        throw new Error(`unknown op kind ${op.kind}`);
    }
  } catch (e) {
    const err = e as Json;
    error = {
      name: err?.name ?? 'Error',
      message: err?.message ?? String(e),
      ...(err?.code !== undefined ? { code: err.code } : {}),
      ...(err?.status !== undefined ? { status: err.status } : {}),
      ...(err?.details !== undefined ? { details: err.details } : {}),
    };
  }
  recording = false;

  const after = await dumpAll(rawMain, rawMount);
  const delta: Record<string, { changed: unknown[]; removed: string[] }> = {};
  for (const [, table, key] of TABLES) {
    const was = new Map(before[table].map((r) => [String(r[key]), JSON.stringify(r)]));
    const now = new Set(after[table].map((r) => String(r[key])));
    delta[table] = {
      changed: after[table].filter((r) => was.get(String(r[key])) !== JSON.stringify(r)),
      removed: Array.from(was.keys()).filter((k) => !now.has(k)),
    };
  }
  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stdout.write(
    JSON.stringify({
      index,
      name: op.name,
      family: op.family,
      ...(error ? { error } : { result: result === undefined ? 'undefined' : JSON.stringify(result) }),
      logs,
      delta,
    }) + '\n',
  );
  process.exit(0);
}

async function main(): Promise<void> {
  const only = process.env.QT_INV_OP_INDEX;
  if (only !== undefined) return runChild(Number(only));
  for (let i = 0; i < spec.ops.length; i++) {
    const child = spawnSync(process.execPath, [...process.execArgv, fileURLToPath(import.meta.url)], {
      env: { ...process.env, QT_INV_OP_INDEX: String(i) },
      encoding: 'utf8',
      maxBuffer: 256 * 1024 * 1024,
    });
    if (child.status !== 0 || !child.stdout.trim()) {
      process.stderr.write(`op #${i} ${spec.ops[i].name} failed:\n${child.stderr}\n`);
      process.exit(1);
    }
    process.stdout.write(child.stdout.trim().split('\n').pop() + '\n');
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`inventory-keystone-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
