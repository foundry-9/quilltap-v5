/**
 * Tier-2/3 oracle case — the Pascal wardrobe-effects applier (P4.D306; v4
 * `lib/pascal/wardrobe-effects.ts`, NEW at `f0db8cbb8`).
 *
 * tsx, real DB, no jest, NO mocks: v4's REAL `applyWardrobeEffects` over its
 * REAL `loadToolPool` (`buildWearablePool`), `wearItem`, `takeOffWearable`,
 * `applyDisplacement` (the wear ledger's chokepoint) and
 * `notifyWardrobeChanged` (the avatar trigger — a no-op, the chat's flag is
 * off — then the announcement: the step's set, or an enqueue). The step
 * matrix is `fixtures/pascal-wardrobe-effects.json`; every step runs over the
 * SAME copy of the built fixture in order, planting its starting outfit first
 * with one raw UPDATE (the Rust side plants the identical bytes).
 *
 * Emitted per step: the applier's return (`IndexedAppliedEffect[]`, bytes),
 * the character's `equippedOutfit` after, every `wardrobe_wear_stats` row
 * (minus the clock columns), the step's set (sorted; `null` when none was
 * passed), the `background_jobs` table projected as the tier-3 outfit family
 * does, and the lines that are this unit's: the module's own (context
 * `pascal.wardrobe-effects` — which on the no-set path includes the
 * scheduler's, since the notify hands that context down) and the
 * chokepoint's `[applyDisplacement] Outfit change committed` (mode / item /
 * slot / source per commit). The logger's shared transports are spliced, so
 * nothing reaches stdout.
 *
 * P4.D316 (v4 `05005c562` + `4ae20977f` at the pin `e8e17b2d9`): a `withSet`
 * step hands the applier BOTH turn containers (`newWardrobeTurnContainers()` —
 * the unchanged case passed a Set with no Map, so every in-turn step settled AT
 * ONCE, the "predates the factory" branch, unlike production); `withSet: false`
 * steps settle the history at once with the `pascal` provenance (the
 * outside-a-turn entrances' shape). `chat_wardrobe_history` is planted on the
 * per-case copy (P4.D315's helper) and dumped per step beside the Map.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MAIN=/tmp/qt-pwe-main.db \
 *   QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MOUNT=/tmp/qt-pwe-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/pascal-wardrobe-effects.ts > /tmp/oracle-pascal-wardrobe-effects.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Step {
  name: string;
  start: string;
  characterId?: string | null;
  withSet?: boolean;
  gestures: Array<{ index: number; wardrobe: Record<string, unknown> }>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  characterId: string;
  chatId: string;
  toolName: string;
  states: Record<string, Record<string, string[]>>;
  steps: Step[];
}

const MODULE_CONTEXT = 'pascal.wardrobe-effects';
const COMMITTED = '[applyDisplacement] Outfit change committed';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'pascal-wardrobe-effects.json'), 'utf8'),
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error(
      'QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MAIN + _MOUNT must point at the fixtures from build-pascal-wardrobe-effects-fixture.ts',
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pwe-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'pwe-main.db');
  const workMount = join(scratch, 'pwe-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  // No job runner in this process: `enqueueJob` would lazily spawn the host,
  // which would claim the queued announcement mid-case. The host's own
  // `shuttingDown` flag keeps `ensureProcessorRunning` a no-op.
  (globalThis as unknown as Record<string, unknown>).__quilltapJobHost = {
    child: null,
    spawning: false,
    shuttingDown: true,
    childCrashed: false,
    restartTimestamps: [],
    signalHandlersInstalled: false,
    invalidationListeners: new Set(),
  };

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
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error) fields.error = data.error.message;
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { applyWardrobeEffects } = await import('@/lib/pascal/wardrobe-effects');

  await initializeDatabase();
  const { plantChatWardrobeHistoryTable } = await import(join(here, '..', 'lib', 'v4-migrations.ts'));
  await plantChatWardrobeHistoryTable();
  const { newWardrobeTurnContainers } = await import('@/lib/wardrobe/outfit-change-effects');
  const repos = getRepositories();
  const dumpHistory = async (): Promise<unknown[]> =>
    ((await rawQuery(
      'SELECT "chatId", "characterId", "entryIndex", "slots", "slotsHash", "source", "actorCharacterId", "actorToolName", "isCurrent" FROM chat_wardrobe_history ORDER BY "chatId", "characterId", "entryIndex"',
      [],
    )) as Array<Record<string, unknown>>).map((r) => ({ ...r, isCurrent: Number(r.isCurrent) }));

  const dumpJobs = async (): Promise<unknown[]> => {
    const rows = (await rawQuery(
      'SELECT "type", "status", "userId", "priority", "maxAttempts", "attempts", "payload" FROM "background_jobs" ORDER BY rowid',
    )) as Array<Record<string, unknown>>;
    return rows
      .map((r) => ({
        ...r,
        payload: typeof r.payload === 'string' ? JSON.parse(r.payload as string) : r.payload,
      }))
      .sort((a, b) => {
        const x = `${a.type}\0${JSON.stringify(a.payload)}`;
        const y = `${b.type}\0${JSON.stringify(b.payload)}`;
        return x < y ? -1 : x > y ? 1 : 0;
      });
  };
  const dumpWear = async (): Promise<unknown[]> => {
    const rows = (await rawQuery(
      'SELECT "itemId", "wearerCharacterId", "wearCount", "lastWornChatId" FROM "wardrobe_wear_stats"',
    )) as Array<Record<string, unknown>>;
    return rows.sort((a, b) => {
      // Plain code-unit order (never `localeCompare`).
      const x = `${a.itemId}|${a.wearerCharacterId ?? ''}`;
      const y = `${b.itemId}|${b.wearerCharacterId ?? ''}`;
      return x < y ? -1 : x > y ? 1 : 0;
    });
  };

  const lines: string[] = [];
  for (const step of spec.steps) {
    const start = spec.states[step.start];
    if (!start) throw new Error(`step ${step.name}: unknown start state ${step.start}`);
    await rawQuery('UPDATE "chats" SET "equippedOutfit" = ? WHERE "id" = ?', [
      JSON.stringify({ [spec.characterId]: start }),
      spec.chatId,
    ]);

    const characterId = step.characterId === undefined ? spec.characterId : step.characterId;
    const containers = step.withSet === false ? null : newWardrobeTurnContainers();
    const set = containers ? containers.pendingWardrobeAnnouncements : null;
    captured.length = 0;
    const returned = await applyWardrobeEffects({
      userId: spec.userId,
      chatId: spec.chatId,
      toolName: spec.toolName,
      characterId,
      gestures: step.gestures as never,
      ...(containers ?? {}),
    });

    const logs = captured
      .filter((l) => l.fields.context === MODULE_CONTEXT || l.message.startsWith(COMMITTED))
      .map((l) => ({ level: l.level, message: l.message, fields: l.fields }));

    lines.push(
      JSON.stringify({
        step: step.name,
        returned: JSON.stringify(returned),
        equipped: (await repos.chats.getEquippedOutfitForCharacter(spec.chatId, spec.characterId)) ?? null,
        wear: await dumpWear(),
        set: set ? Array.from(set).sort() : null,
        map: containers
          ? Array.from(containers.pendingWardrobeHistory.entries()).sort(([a], [b]) =>
              a < b ? -1 : a > b ? 1 : 0,
            )
          : null,
        history: await dumpHistory(),
        jobs: await dumpJobs(),
        logs,
      }),
    );
  }

  await closeDatabase();
  process.stdout.write(lines.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`pascal-wardrobe-effects oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
