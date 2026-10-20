/**
 * Tier-2 oracle case (P4.D316 Tier 2 item 24) — v4's REAL `rollbackWardrobe`
 * (`lib/wardrobe/history/rollback.ts`, `ebb480edd`, at the pin `e8e17b2d9`),
 * driven ROUTE-FREE over seeded timelines with the pool passed both ways.
 *
 * Runs over ONE copy of the committed wardrobe-routes fixture pair (Aria's
 * vault, Quilltap General with an ARCHIVED item, chat1's project store holding
 * the `Project scarf`). Setup: the project's `allowAnyCharacter` is set false
 * through the REAL repository (its roster is empty), so Aria is OFF chat1's
 * project roster — T14: `loadToolPool` withholds the project tier (the scarf
 * is `missing`) while `loadWearablePool(..., { operator: true })` restores it.
 *
 * Each op (`fixtures/wardrobe-rollback-tier2.json`) clears that pair's
 * `chat_wardrobe_history` rows and every `background_jobs` row, plants the
 * timeline through the REAL repositories (`setEquippedOutfit` + `settle`
 * append), then the optional cursor / gap / live outfit, then calls
 * `rollbackWardrobe(repos, { chatId, pool, index, context, source })` with
 * `source: 'ui' | 'tool'` and either pool; `inTurn` hands it a pair of
 * `newWardrobeTurnContainers()` (the cursor waits in the Map — nothing is
 * settled, nothing enqueued). After each op it dumps the outcome (or the
 * thrown message), the chat's `equippedOutfit`, EVERY `wardrobe_wear_stats`
 * row (minus the minted id / timestamps — the commit carries `wornBundles:
 * []`, so only the newly worn leaves are credited), the pair's history rows
 * (ids / createdAt not dumped), the queued jobs, the turn's Set + Map, and
 * the captured log lines.
 *
 * Run (Node 24, from the v4 checkout or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server   # (or the round's pin)
 *   cp $V5W/crates/quilltap-web/tests/fixtures/wardrobe-routes-main.db /tmp/qt-wrb-main.db
 *   cp $V5W/crates/quilltap-web/tests/fixtures/wardrobe-routes-mount.db /tmp/qt-wrb-mount.db
 *   QT_FIXTURE_WRB_MAIN=/tmp/qt-wrb-main.db QT_FIXTURE_WRB_MOUNT=/tmp/qt-wrb-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/wardrobe-rollback-tier2.ts \
 *     > /tmp/qt-wrb-oracle.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Plant {
  slots: Record<string, string[]>;
  provenance?: Record<string, unknown>;
}

interface Op {
  name: string;
  chat: string;
  who: string;
  plants: Plant[];
  cursor?: number;
  dropIndex?: number;
  live?: Record<string, string[]>;
  rollback: { index?: number; source: 'ui' | 'tool'; pool: 'operator' | 'tool'; inTurn?: boolean };
}

/** The messages this family compares. */
const CAPTURED = new Set([
  'Rollback left a stale item off',
  'Rolled a wardrobe back',
  'Committed equipped outfit',
  'Equipped outfit write failed; no wears credited',
  'Wardrobe outfit announcement scheduled',
  'Failed to schedule wardrobe outfit announcement',
  'Failed to settle wardrobe history',
  '[WearablePool] Character off project roster — project wardrobe withheld',
  '[ProjectRoster] Tool access check',
]);

const SLOTS = ['top', 'bottom', 'footwear', 'accessories', 'hair'] as const;

// v4's job host would claim queued jobs mid-case (memory note
// `v4-oracle-job-host-and-log-capture`): hold it off before any import.
(globalThis as unknown as Record<string, unknown>).__quilltapJobHost = {
  child: null,
  spawning: false,
  shuttingDown: true,
  childCrashed: false,
  restartTimestamps: [],
  signalHandlersInstalled: false,
  invalidationListeners: new Set(),
};

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const base = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'wardrobe-routes.json'), 'utf8')) as {
    testPepperBase64: string;
    userId: string;
    ids: Record<string, string>;
  };
  const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'wardrobe-rollback-tier2.json'), 'utf8')) as {
    operatorProvenance: Record<string, unknown>;
    toolProvenance: Record<string, unknown>;
    ops: Op[];
  };
  const id = (k: string): string => base.ids[k] ?? k;
  const resolveProvenance = (p: Record<string, unknown>): Record<string, unknown> =>
    Object.fromEntries(
      Object.entries(p).map(([k, v]) => [k, k === 'actorCharacterId' ? id(v as string) : v]),
    );
  const fullSlots = (s: Record<string, string[]> | undefined): Record<string, string[]> => {
    const out: Record<string, string[]> = {};
    for (const slot of SLOTS) out[slot] = (s?.[slot] ?? []).map(id);
    return out;
  };

  const fixtureMain = process.env.QT_FIXTURE_WRB_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_WRB_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_WRB_MAIN + QT_FIXTURE_WRB_MOUNT must point at the fixture pair (see header)');
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-wrb-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'wrb-main.db');
  const workMount = join(scratch, 'wrb-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = base.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const { logger } = await import('@/lib/logger');
  const captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as {
        level: string;
        message: string;
        context?: Record<string, unknown>;
        error?: { message: string };
      };
      if (!CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error) fields.error = data.error.message;
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { rollbackWardrobe } = await import('@/lib/wardrobe/history/rollback');
  const { loadWearablePool } = await import('@/lib/wardrobe/pool');
  const { loadToolPool } = await import('@/lib/tools/handlers/wardrobe-handler-shared');
  const { newWardrobeTurnContainers } = await import('@/lib/wardrobe/turn-containers');

  await initializeDatabase();
  const { plantChatWardrobeHistoryTable } = await import(join(here, '..', 'lib', 'v4-migrations.ts'));
  await plantChatWardrobeHistoryTable();
  // The routes fixture predates the wear ledger: plant v4's own DDL (the Rust
  // side `test_support::ensure_wear_ledger_on`).
  const { WARDROBE_WEAR_STATS_DDL } = await import('@/lib/database/backends/sqlite/wardrobe-wear-stats-ddl');
  for (const statement of WARDROBE_WEAR_STATS_DDL) await rawQuery(statement);
  const repos = getRepositories();

  // T14's setup: chat1's project closes its (empty) roster.
  const project = await repos.projects.setAllowAnyCharacter(id('project'), false);
  if (!project || project.allowAnyCharacter !== false) throw new Error('setup: the roster did not close');

  const lines: string[] = [];
  for (const op of spec.ops) {
    const chatId = id(op.chat);
    const characterId = id(op.who);
    await rawQuery('DELETE FROM chat_wardrobe_history WHERE "chatId" = ? AND "characterId" = ?', [
      chatId,
      characterId,
    ]);
    await rawQuery('DELETE FROM "background_jobs"');
    for (const plant of op.plants) {
      await repos.chats.setEquippedOutfit(chatId, characterId, fullSlots(plant.slots) as never);
      await repos.wardrobeHistory.settle(chatId, characterId, {
        kind: 'append',
        provenance: resolveProvenance(plant.provenance ?? spec.toolProvenance),
      } as never);
    }
    if (op.cursor !== undefined) {
      await repos.wardrobeHistory.settle(chatId, characterId, { kind: 'cursor', index: op.cursor } as never);
    }
    if (op.dropIndex !== undefined) {
      await rawQuery(
        'DELETE FROM chat_wardrobe_history WHERE "chatId" = ? AND "characterId" = ? AND "entryIndex" = ?',
        [chatId, characterId, op.dropIndex],
      );
    }
    if (op.live) await repos.chats.setEquippedOutfit(chatId, characterId, fullSlots(op.live) as never);

    captured.length = 0;
    const rb = op.rollback;
    const pool =
      rb.pool === 'tool'
        ? await loadToolPool(repos, chatId, characterId)
        : await loadWearablePool(repos, characterId, undefined, { chatId, operator: true });
    const turn = rb.inTurn ? newWardrobeTurnContainers() : null;
    const provenance = resolveProvenance(rb.source === 'ui' ? spec.operatorProvenance : spec.toolProvenance);
    let outcome: unknown;
    try {
      outcome = await rollbackWardrobe(repos, {
        chatId,
        pool,
        ...(rb.index !== undefined ? { index: rb.index } : {}),
        context: { userId: base.userId, chatId, characterId, provenance, ...(turn ?? {}) } as never,
        source: rb.source,
      });
    } catch (error) {
      outcome = { thrown: error instanceof Error ? error.message : String(error) };
    }

    lines.push(
      JSON.stringify({
        case: 'wardrobe-rollback',
        name: op.name,
        outcome,
        equippedOutfit: (await repos.chats.getEquippedOutfit(chatId)) ?? null,
        ledger: await rawQuery(
          `SELECT "itemId", "wearerCharacterId", "wearCount", "firstWornAt", "lastWornAt", "lastWornChatId"
             FROM "wardrobe_wear_stats" ORDER BY "itemId", COALESCE("wearerCharacterId", '')`,
        ),
        history: ((await rawQuery(
          'SELECT "chatId", "characterId", "entryIndex", "slots", "slotsHash", "source", "actorCharacterId", "actorToolName", "isCurrent" FROM chat_wardrobe_history WHERE "chatId" = ? AND "characterId" = ? ORDER BY "entryIndex"',
          [chatId, characterId],
        )) as Array<Record<string, unknown>>).map((r) => ({ ...r, isCurrent: Number(r.isCurrent) })),
        jobs: await rawQuery(
          'SELECT "type", "status", "maxAttempts", "payload" FROM "background_jobs" ORDER BY rowid',
        ),
        set: turn ? Array.from(turn.pendingWardrobeAnnouncements).sort() : null,
        map: turn
          ? Array.from(turn.pendingWardrobeHistory.entries()).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
          : null,
        logs: captured.map((c) => ({ ...c })),
      }),
    );
  }

  await closeDatabase();
  process.stdout.write(lines.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-rollback oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
