/**
 * Tier-3 oracle case — the outfit-change side effects (P4.D279 keystone C1 §7;
 * v4 `lib/wardrobe/outfit-change-effects.ts`, NEW at `4a38222e6`).
 *
 * Drives the REAL module — `notifyWardrobeChanged`, `refreshAvatarForOutfit`,
 * `recordPendingWardrobeAnnouncement`, `scheduleWardrobeAnnouncement`,
 * `flushPendingWardrobeAnnouncements` — through the scripted matrix in
 * `fixtures/outfit-change-effects-tier3.json`, over a copy of the fixture
 * `build-outfit-change-effects-fixture.ts` bakes. Same tsx / real-DB / no-jest /
 * no-mock shape as `wardrobe-tools-avatar-trigger.ts`; the real
 * `triggerAvatarGenerationIfEnabled` and the real
 * `enqueueWardrobeOutfitAnnouncement` run underneath.
 *
 * One shared pair of per-turn containers (`newWardrobeTurnContainers()` — the
 * Set AND the history Map) stands for the orchestrator's; steps with
 * `withSet: false` (and the bare `schedule` / `refresh` ops) run without them,
 * and `withMap: false` passes the Set alone (v4's "a caller that predates the
 * factory" branch). `dropBackgroundJobs` is destructive — every later schedule
 * takes the catch leg.
 *
 * P4.D316 (`05005c562`, re-aimed at the pin `e8e17b2d9`): every notify /
 * record ctx carries a REQUIRED `provenance` (default the step character's own
 * `character` provenance) and an optional `historyIntent`;
 * `recordPendingWardrobeAnnouncement` takes `repos` FIRST (the unchanged case
 * passed `(ctx, sourceContext)` and v4 read the ctx AS `repos` — a silent
 * garble); `dress` writes a slots object through the REAL chats repository
 * (no effects); `seedMap` / `clearMap` touch the turn Map directly. The
 * `chat_wardrobe_history` table is planted on the per-case copy through
 * P4.D315's `plantChatWardrobeHistoryTable` and dumped per step (ids /
 * createdAt not dumped), beside the Map's contents.
 *
 * Emitted per step (one NDJSON line): the set's contents (sorted), the whole
 * `background_jobs` table projected to the columns both sides decide (`null`
 * once the table is gone), and the module's OWN logger lines — only the two
 * messages `outfit-change-effects.ts` writes, with their fields. The lines of
 * the layers beneath (`[WardrobeAnnouncement] …`, `Background job enqueued`,
 * the avatar trigger's `Avatar image profile resolved`, the repository's
 * fallback ERRORs) are not this family's — the avatar-trigger and queue
 * families own them.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUTFIT_EFFECTS_MAIN=/tmp/qt-outfit-effects-main.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/outfit-change-effects-tier3.ts \
 *     > /tmp/oracle-outfit-change-effects.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Step {
  name: string;
  op:
    | 'notify'
    | 'record'
    | 'refresh'
    | 'schedule'
    | 'flush'
    | 'flushNoSet'
    | 'dropBackgroundJobs'
    | 'equip'
    | 'dress'
    | 'seedMap'
    | 'clearMap';
  chat?: string;
  character?: 'A' | 'B';
  withSet?: boolean;
  /** P4.D316: `false` passes the Set WITHOUT its history Map. */
  withMap?: boolean;
  provenance?: Record<string, unknown>;
  historyIntent?: Record<string, unknown>;
  /** `dress` steps: the slots object written as-is. */
  slots?: Record<string, string[]>;
  context?: string;
  /** `equip` steps (P4.D282): the route body minus `characterId`. */
  body?: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  characterAId: string;
  characterBId: string;
  characterUId: string;
  chats: Record<string, { id: string }>;
  steps: Step[];
}

/** The module's own lines (message → compared). P4.D316 adds the settle
 * guard's WARN (unreachable through v4's real repository — its presence is
 * asserted ABSENT on both sides) and the operator resolver's DEBUG. */
const CAPTURED = new Set([
  'Wardrobe outfit announcement scheduled',
  'Failed to schedule wardrobe outfit announcement',
  'Failed to settle wardrobe history',
  'Resolved operator for an outfit change',
]);

interface LogLine {
  level: string;
  message: string;
  fields: Record<string, unknown>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'outfit-change-effects-tier3.json'), 'utf8'),
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_OUTFIT_EFFECTS_MAIN;
  if (!fixtureMain || !existsSync(fixtureMain)) {
    throw new Error('QT_FIXTURE_OUTFIT_EFFECTS_MAIN must point at the built fixture');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-outfit-effects-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'outfit-effects-main.db');
  copyFileSync(fixtureMain, workMain);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'mount-index.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // The module's success line is DEBUG: the logger must let it through to the
  // capture; the console transport is spliced out, so nothing reaches stdout.
  process.env.LOG_LEVEL = 'debug';

  const { logger } = await import('@/lib/logger');
  const captured: LogLine[] = [];
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

  // No job runner in this process: v4's `enqueueJob` lazily spawns the job
  // host, which CLAIMS queued jobs (PENDING → PROCESSING) and so defeats the
  // announcement's PENDING-only dedupe mid-matrix. The Rust side has no runner
  // either; the host's own `shuttingDown` flag keeps `ensureProcessorRunning`
  // a no-op (the avatar-trigger family's idiom).
  (globalThis as unknown as Record<string, unknown>).__quilltapJobHost = {
    child: null,
    spawning: false,
    shuttingDown: true,
    childCrashed: false,
    restartTimestamps: [],
    signalHandlersInstalled: false,
    invalidationListeners: new Set(),
  };

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const effects = await import('@/lib/wardrobe/outfit-change-effects');
  const { handleEquipSlot } = await import('@/app/api/v1/chats/[id]/actions/outfit');
  const { plantChatWardrobeHistoryTable } = await import(join(here, '..', 'lib', 'v4-migrations.ts'));

  await initializeDatabase();
  await plantChatWardrobeHistoryTable();
  const repos = getRepositories();

  const dumpHistory = async (): Promise<unknown> =>
    ((await rawQuery(
      'SELECT "chatId", "characterId", "entryIndex", "slots", "slotsHash", "source", "actorCharacterId", "actorToolName", "isCurrent" FROM chat_wardrobe_history ORDER BY "chatId", "characterId", "entryIndex"',
      [],
    )) as Array<Record<string, unknown>>).map((r) => ({ ...r, isCurrent: Number(r.isCurrent) }));

  let jobsTableGone = false;
  const dumpJobs = async (): Promise<unknown> => {
    if (jobsTableGone) return null;
    const rows = (await rawQuery('SELECT * FROM background_jobs', [])) as Array<
      Record<string, unknown>
    >;
    const jobs = rows.map((r) => ({
      type: r.type,
      status: r.status,
      userId: r.userId,
      priority: r.priority,
      maxAttempts: r.maxAttempts,
      attempts: r.attempts,
      payload: JSON.parse(String(r.payload)),
    }));
    // Minted ids and clock timestamps are not dumped; the rows sort by what is.
    jobs.sort((a, b) => {
      const ka = `${a.type}\u0000${JSON.stringify(a.payload)}`;
      const kb = `${b.type}\u0000${JSON.stringify(b.payload)}`;
      return ka < kb ? -1 : ka > kb ? 1 : 0;
    });
    return jobs;
  };

  const characterOf = (c: 'A' | 'B' | undefined): string => {
    if (c === 'A') return spec.characterAId;
    if (c === 'B') return spec.characterBId;
    throw new Error('step needs a character');
  };
  const chatOf = (key: string | undefined): string => {
    const chat = key ? spec.chats[key] : undefined;
    if (!chat) throw new Error(`unknown chat ${key}`);
    return chat.id;
  };

  // The one shared pair of turn containers; `withSet: false` steps run without
  // them, `withMap: false` with the Set alone.
  const { pendingWardrobeAnnouncements: turnSet, pendingWardrobeHistory: turnMap } =
    effects.newWardrobeTurnContainers();
  const ctxOf = (step: Step) => ({
    userId: spec.userId,
    chatId: chatOf(step.chat),
    characterId: characterOf(step.character),
    provenance: (step.provenance ?? {
      source: 'character',
      actorCharacterId: characterOf(step.character),
    }) as never,
    ...(step.historyIntent ? { historyIntent: step.historyIntent as never } : {}),
    ...(step.withSet
      ? step.withMap === false
        ? { pendingWardrobeAnnouncements: turnSet }
        : { pendingWardrobeAnnouncements: turnSet, pendingWardrobeHistory: turnMap }
      : {}),
  });
  const lines: string[] = [];
  for (const step of spec.steps) {
    captured.length = 0;
    let status: number | undefined;
    switch (step.op) {
      case 'notify':
        await effects.notifyWardrobeChanged(repos, ctxOf(step), step.context ?? '');
        break;
      case 'record':
        await effects.recordPendingWardrobeAnnouncement(repos, ctxOf(step), step.context ?? '');
        break;
      case 'dress':
        await repos.chats.setEquippedOutfit(
          chatOf(step.chat),
          characterOf(step.character),
          step.slots as never,
        );
        break;
      case 'seedMap':
        turnMap.set(characterOf(step.character), step.historyIntent as never);
        break;
      case 'clearMap':
        turnMap.clear();
        break;
      case 'refresh':
        await effects.refreshAvatarForOutfit(
          repos,
          {
            userId: spec.userId,
            chatId: chatOf(step.chat),
            characterId: characterOf(step.character),
          },
          step.context ?? '',
        );
        break;
      case 'schedule':
        await effects.scheduleWardrobeAnnouncement(step.context ?? '', {
          userId: spec.userId,
          chatId: chatOf(step.chat),
          characterId: characterOf(step.character),
        });
        break;
      case 'flush':
        await effects.flushPendingWardrobeAnnouncements({
          userId: spec.userId,
          chatId: chatOf(step.chat),
          pendingWardrobeAnnouncements: turnSet,
          pendingWardrobeHistory: turnMap,
        });
        break;
      case 'flushNoSet':
        await effects.flushPendingWardrobeAnnouncements({
          userId: spec.userId,
          chatId: chatOf(step.chat),
        });
        break;
      case 'equip': {
        // P4.D282 — the chat outfit route's REAL handler: its
        // `notifyWardrobeChanged(…, '[Chats v1] outfit-equip')` carries no
        // turn set, so the announcement is enqueued at once.
        const req = { json: async () => ({ characterId: characterOf(step.character), ...step.body }) };
        const res = await handleEquipSlot(req as never, chatOf(step.chat), {
          repos,
          user: { id: spec.userId },
          session: {},
        } as never);
        status = res.status;
        break;
      }
      case 'dropBackgroundJobs':
        await rawQuery('DROP TABLE background_jobs', []);
        jobsTableGone = true;
        break;
      default:
        throw new Error(`unknown op ${(step as Step).op}`);
    }
    lines.push(
      JSON.stringify({
        case: 'outfit-change-effects-tier3',
        step: step.name,
        set: Array.from(turnSet).sort(),
        map: Array.from(turnMap.entries()).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
        history: await dumpHistory(),
        jobs: await dumpJobs(),
        logs: captured.slice(),
        ...(status !== undefined ? { status } : {}),
      }),
    );
  }

  await closeDatabase();
  process.stdout.write(lines.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`outfit-change-effects-tier3 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
