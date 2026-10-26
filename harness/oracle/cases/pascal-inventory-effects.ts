/**
 * Tier-2 oracle case (P4.D329 — v4 `e1872c94f` at `322a5488e`): the custom-tool
 * INVENTORY EFFECTS applier, `applyInventoryEffects`
 * (`lib/pascal/inventory-effects.ts`), through v4's REAL pool, refs, equip
 * chokepoints and notify.
 *
 * Runs the step matrix `fixtures/pascal-inventory-effects.json` over the
 * Inventory KEYSTONE pair (`fixtures/build-inventory-mutations-fixture.ts`,
 * reused read-only — it already carries an on-roster and an off-roster
 * character, General / project / vault objects, caps, kits and a project
 * chat). Every step runs in its OWN child process on a FRESH copy (v4's
 * module singletons never leak between steps). Per step the child emits ONE
 * line: `returned` (`JSON.stringify` of the applied list — KEY ORDER is the
 * contract), `pending` (the turn's map, entries sorted by key, items as ids;
 * `null` without one), `equipped` (the chat's `equippedInventory` cell after),
 * `messages` (the chat's `chat_messages` rows after — role / content /
 * systemSender / systemKind), `created` (the vault / store item documents the
 * step added, as parsed JSON), and `logs` (every line, off the `Logger`
 * prototype before the level check).
 *
 * The title lint is stubbed out exactly as `inventory-keystone-tier2.ts` does
 * (P4.D340's to port).
 *
 * Run (Node 24, from the worktree pinned at `322a5488e`), AFTER building the
 * keystone pair:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd /tmp/qt-v4-pin-p4d329-322a5488e
 *   QT_FIXTURE_OUT=/tmp/p4d329/qt-pie-main.db QT_FIXTURE_MOUNT_OUT=/tmp/p4d329/qt-pie-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-inventory-mutations-fixture.ts
 *   QT_FIXTURE_INV_MAIN=/tmp/p4d329/qt-pie-main.db QT_FIXTURE_INV_MOUNT=/tmp/p4d329/qt-pie-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/pascal-inventory-effects.ts \
 *     > /tmp/p4d329/oracle-pascal-inventory-effects.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';

type Json = any; // eslint-disable-line @typescript-eslint/no-explicit-any

interface Step {
  name: string;
  chat: string;
  character: string | null;
  withPending?: boolean;
  gestures: Array<{ index: number; inventory: Json }>;
}
interface Spec {
  userId: string;
  toolName: string;
  characters: Record<string, string>;
  chats: Record<string, string>;
  steps: Step[];
}

const here = dirname(fileURLToPath(import.meta.url));
const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'pascal-inventory-effects.json'), 'utf8')) as Spec;
const keystone = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'inventory-keystone.json'), 'utf8')) as {
  testPepperBase64: string;
};
const fixtureMain = process.env.QT_FIXTURE_INV_MAIN;
const fixtureMount = process.env.QT_FIXTURE_INV_MOUNT;
if (!fixtureMain || !fixtureMount || !existsSync(fixtureMain) || !existsSync(fixtureMount)) {
  throw new Error('QT_FIXTURE_INV_MAIN / QT_FIXTURE_INV_MOUNT must point at the keystone pair');
}

/** `inventory-keystone-tier2.ts`'s lint stub, verbatim in effect. */
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
  }
}

function tableExists(raw: Json, table: string): boolean {
  return !!raw.prepare(`SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?`).get(table);
}

async function runChild(index: number): Promise<void> {
  const step = spec.steps[index];
  const scratch = mkdtempSync(join(tmpdir(), 'qt-pie-step-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main.db');
  const mountWork = join(scratch, 'mount.db');
  copyFileSync(fixtureMain!, mainWork);
  copyFileSync(fixtureMount!, mountWork);
  process.env.ENCRYPTION_MASTER_PEPPER = keystone.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { Logger } = await import('@/lib/logger');
  const logs: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  let recording = false;
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (this: unknown, message: string, context?: Record<string, unknown>, ...rest: unknown[]) {
      if (recording) {
        const fields: Record<string, unknown> = {};
        for (const [k, v] of Object.entries(context ?? {})) if (v !== undefined) fields[k] = v;
        logs.push({ level, message, fields });
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  withoutTitleLint();
  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  await initializeDatabase();
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { applyInventoryEffects } = await import('@/lib/pascal/inventory-effects');
  const rawMain = getRawDatabase();
  const rawMount = getRawMountIndexDatabase();
  const knownDocs = new Set(
    (rawMount.prepare('SELECT "id" FROM "doc_mount_file_links"').all() as Array<{ id: string }>).map((r) => r.id),
  );

  const chatId = spec.chats[step.chat];
  const characterId = step.character === null ? null : spec.characters[step.character];
  const pending = step.withPending ? new Map<string, Json>() : undefined;

  recording = true;
  const returned = await applyInventoryEffects({
    userId: spec.userId,
    chatId,
    toolName: spec.toolName,
    characterId,
    gestures: step.gestures,
    ...(pending ? { pendingInventoryAnnouncements: pending } : {}),
  } as never);
  // v4's un-awaited post-write tails land before the dumps.
  await new Promise((r) => setTimeout(r, 300));
  recording = false;

  const chatRow = rawMain.prepare('SELECT "equippedInventory" FROM "chats" WHERE "id" = ?').get(chatId) as
    | { equippedInventory: string | null }
    | undefined;
  const messages = tableExists(rawMain, 'chat_messages')
    ? (rawMain
        .prepare('SELECT "role", "content", "systemSender", "systemKind" FROM "chat_messages" WHERE "chatId" = ? ORDER BY rowid')
        .all(chatId) as unknown[])
    : [];
  const created = (rawMount
    .prepare(
      'SELECT l."id" AS id, l."relativePath" AS path, d."content" AS content FROM "doc_mount_file_links" l JOIN "doc_mount_documents" d ON d."fileId" = l."fileId" ORDER BY l."relativePath"',
    )
    .all() as Array<{ id: string; path: string; content: string }>)
    .filter((d) => !knownDocs.has(d.id))
    .map((d) => ({ path: d.path, content: d.content }));
  const pendingOut = pending
    ? Array.from(pending.entries())
        .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        .map(([key, e]) => ({
          key,
          characterId: e.characterId,
          equipped: e.equipped.map((i: Json) => i.id),
          unequipped: e.unequipped.map((i: Json) => i.id),
          pickedUp: e.pickedUp.map((i: Json) => i.id),
          provenances: e.provenances,
        }))
    : null;

  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.stdout.write(
    JSON.stringify({
      step: step.name,
      returned: JSON.stringify(returned),
      pending: pendingOut,
      equipped: chatRow ? chatRow.equippedInventory : null,
      messages,
      created,
      logs,
    }) + '\n',
  );
  process.exit(0);
}

async function main(): Promise<void> {
  const only = process.env.QT_PIE_STEP_INDEX;
  if (only !== undefined) return runChild(Number(only));
  for (let i = 0; i < spec.steps.length; i++) {
    const child = spawnSync(process.execPath, [...process.execArgv, fileURLToPath(import.meta.url)], {
      env: { ...process.env, QT_PIE_STEP_INDEX: String(i) },
      encoding: 'utf8',
      maxBuffer: 256 * 1024 * 1024,
    });
    if (child.status !== 0 || !child.stdout.trim()) {
      process.stderr.write(`step #${i} ${spec.steps[i].name} failed:\n${child.stderr}\n`);
      process.exit(1);
    }
    process.stdout.write(child.stdout.trim().split('\n').pop() + '\n');
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`pascal-inventory-effects oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
