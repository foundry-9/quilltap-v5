/**
 * Tier-2/3-style oracle case — the seven wardrobe tool handlers (W4.1d batch 2;
 * v4 `lib/tools/handlers/wardrobe-*-handler.ts`).
 *
 * The handlers are pure DB paths (no LLM / no image gen — avatar generation is
 * gated OFF via `avatarGenerationEnabled: false`, and a `pendingWardrobeAnnouncements`
 * Set is supplied so no announcement job is enqueued), so they run directly under
 * the tsx real-DB path (no jest, no mocks), like the whisper / memory-cascade cases.
 *
 * We copy the pre-seeded two-database fixture, run the fixed op sequence via v4's
 * REAL handlers (`execute*` + `format*`), capture each op's Output + formatted
 * string, then read the mutated state back (both characters' wardrobes, the General
 * archetypes, and the chat's equipped outfit) for the Rust diff. Minted ids /
 * timestamps (create mints an id + createdAt/updatedAt; update/archive mint
 * updatedAt inside the content-addressed `.md`) are normalized identically on both
 * sides by the harness (positional UUID remap + ISO-timestamp collapse).
 *
 * P4.D262 (v4 `3ee3b1342` + `b3f937076`): the formatters run at ONE pinned
 * `nowMs` (`spec.nowMs`, v4's second parameter) so the relative "last worn"
 * dates compare; BEFORE the ops the wear ledger is PLANTED through v4's REAL
 * `incrementWears` (`spec.wearPlant` — the caller, the recipient, a departed
 * wearer, the unattributed row) and one item's picture pointer through v4's
 * REAL `wardrobe.update` (`spec.picturePlant`); the read-back gains every
 * ledger row (item ids a create minted are keyed by TITLE so both sides sort
 * alike); and AFTER the read-back the `pictureScenario` runs with the operator
 * switch ON (v4's REAL `chatSettings.updateForUser`), a usable default image
 * profile and the `background_jobs` table, reading the queued jobs back.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WT_MAIN=/tmp/qt-wt-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wt-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/wardrobe-tools.ts > /tmp/oracle-wardrobe-tools.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Op {
  name: string;
  tool: string;
  characterId?: string;
  args: Record<string, unknown>;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  callerCharacterId: string;
  recipientCharacterId: string;
  generalMountPointId: string;
  chatId: string;
  callerParticipantId: string;
  ops: Op[];
  nowMs: string;
  departedCharacterId: string;
  wearPlant: Array<{ itemId: string; wearer: string | null; chatId: string | null; at: string }>;
  picturePlant: { itemId: string; imageFileId: string };
  pictureScenario: { imageProfile: Record<string, unknown> & { id: string }; ops: Op[] };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'wardrobe-tools.json'), 'utf8'),
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_WT_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_WT_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error(
      'QT_FIXTURE_WT_MAIN + QT_FIXTURE_WT_MOUNT must point at the seed fixtures from build-wardrobe-tools-fixture.ts',
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wt-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'wt-main.db');
  const workMount = join(scratch, 'wt-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // [P4.D262] No job runner in this process: v4's `enqueueJob` lazily spawns the
  // job host, which would CLAIM the queued pictures (PENDING → PROCESSING) and
  // so defeat the PENDING-only dedupe mid-case. The Rust side has no runner
  // either; the host's own `shuttingDown` flag keeps `ensureProcessorRunning`
  // a no-op (`processor-host.ts:300-301`).
  (globalThis as unknown as Record<string, unknown>).__quilltapJobHost = {
    child: null,
    spawning: false,
    shuttingDown: true,
    childCrashed: false,
    restartTimestamps: [],
    signalHandlersInstalled: false,
    invalidationListeners: new Set(),
  };

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const wl = await import('@/lib/tools/handlers/wardrobe-list-handler');
  const wr = await import('@/lib/tools/handlers/wardrobe-read-handler');
  const wc = await import('@/lib/tools/handlers/wardrobe-create-handler');
  const wu = await import('@/lib/tools/handlers/wardrobe-update-handler');
  const wa = await import('@/lib/tools/handlers/wardrobe-archive-handler');
  const ww = await import('@/lib/tools/handlers/wardrobe-wear-handler');
  const wt = await import('@/lib/tools/handlers/wardrobe-take-off-handler');

  await initializeDatabase();
  const NOW_MS = Date.parse(spec.nowMs);

  // [P4.D262] The ledger + picture plants (both sides plant the same rows).
  {
    const repos = getRepositories();
    const wearerId = (w: string | null): string | null =>
      w === 'caller'
        ? spec.callerCharacterId
        : w === 'recipient'
          ? spec.recipientCharacterId
          : w === 'departed'
            ? spec.departedCharacterId
            : null;
    for (const w of spec.wearPlant) {
      await repos.wardrobeWear.incrementWears([
        { itemId: w.itemId, wearerCharacterId: wearerId(w.wearer), chatId: w.chatId, at: w.at } as never,
      ]);
    }
    await repos.wardrobe.update(
      spec.picturePlant.itemId,
      { imageFileId: spec.picturePlant.imageFileId } as never,
      spec.callerCharacterId,
    );
  }

  // A single per-turn announcement Set (as the orchestrator would supply). It
  // absorbs the archive/wear/take_off announcements so nothing is enqueued.
  const pendingWardrobeAnnouncements = new Set<string>();

  const runOp = async (op: Op): Promise<{ name: string; tool: string; output: unknown; formatted: string }> => {
    const characterId =
      op.characterId === 'recipient' ? spec.recipientCharacterId : spec.callerCharacterId;
    const base = { userId: spec.userId, chatId: spec.chatId, characterId };
    let output: unknown;
    let formatted: string;
    switch (op.tool) {
      case 'wardrobe_list':
        output = await wl.executeWardrobeListTool(op.args, base);
        formatted = wl.formatWardrobeListResults(output as never, NOW_MS);
        break;
      case 'wardrobe_read':
        output = await wr.executeWardrobeReadTool(op.args, base);
        formatted = wr.formatWardrobeReadResults(output as never, NOW_MS);
        break;
      case 'wardrobe_create':
        output = await wc.executeWardrobeCreateTool(op.args, base);
        formatted = wc.formatWardrobeCreateResults(output as never);
        break;
      case 'wardrobe_update':
        output = await wu.executeWardrobeUpdateTool(op.args, base);
        formatted = wu.formatWardrobeUpdateResults(output as never);
        break;
      case 'wardrobe_archive':
        output = await wa.executeWardrobeArchiveTool(op.args, {
          ...base,
          pendingWardrobeAnnouncements,
        });
        formatted = wa.formatWardrobeArchiveResults(output as never);
        break;
      case 'wardrobe_wear':
        output = await ww.executeWardrobeWearTool(op.args, {
          ...base,
          pendingWardrobeAnnouncements,
        });
        formatted = ww.formatWardrobeWearResults(output as never);
        break;
      case 'wardrobe_take_off':
        output = await wt.executeWardrobeTakeOffTool(op.args, {
          ...base,
          pendingWardrobeAnnouncements,
        });
        formatted = wt.formatWardrobeTakeOffResults(output as never);
        break;
      default:
        throw new Error(`unknown tool ${op.tool}`);
    }
    return { name: op.name, tool: op.tool, output, formatted };
  };

  const returns: unknown[] = [];
  for (const op of spec.ops) returns.push(await runOp(op));

  // Read the mutated state back (the "tables" diff, in read-back form).
  const repos = getRepositories();
  /** [P4.D262] Every ledger row minus the minted id/createdAt/updatedAt, keyed by
   *  the item's TITLE where the id was minted (a create), so both sides sort alike. */
  const wearReadback = async (): Promise<unknown[]> => {
    const { rawQuery } = await import('@/lib/database/manager');
    const rows = (await rawQuery(
      'SELECT "itemId", "wearerCharacterId", "wearCount", "firstWornAt", "lastWornAt", "lastWornChatId" FROM "wardrobe_wear_stats"',
    )) as Array<Record<string, unknown>>;
    const all = [
      ...(await repos.wardrobe.findByCharacterId(spec.callerCharacterId, true)),
      ...(await repos.wardrobe.findByCharacterId(spec.recipientCharacterId, true)),
    ] as Array<{ id: string; title: string }>;
    const fixed = new Set(JSON.stringify(spec).match(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g));
    const keyOf = (id: string): string =>
      fixed.has(id) ? id : (all.find((i) => i.id === id)?.title ?? id);
    return rows
      .map((r) => ({ ...r, itemId: keyOf(r.itemId as string) }))
      .sort((a, b) => {
        // Plain code-unit order (never `localeCompare` — ICU collation).
        const x = `${a.itemId}|${a.wearerCharacterId ?? ''}`;
        const y = `${b.itemId}|${b.wearerCharacterId ?? ''}`;
        return x < y ? -1 : x > y ? 1 : 0;
      });
  };
  const readback = {
    callerItems: await repos.wardrobe.findByCharacterId(spec.callerCharacterId, true),
    recipientItems: await repos.wardrobe.findByCharacterId(spec.recipientCharacterId, true),
    generalItems: await repos.wardrobe.findArchetypes(true),
    equippedOutfit: (await repos.chats.getEquippedOutfit(spec.chatId)) ?? null,
    pendingAnnouncements: Array.from(pendingWardrobeAnnouncements).sort(),
    wardrobeWear: await wearReadback(),
  };

  // [P4.D262] The picture scenario — the operator switch ON.
  const { ensureCollection } = await import('@/lib/database/manager');
  const { ChatSettingsSchema } = await import('@/lib/schemas/settings.types');
  const { ImageProfileSchema } = await import('@/lib/schemas/profile.types');
  await ensureCollection('chat_settings', ChatSettingsSchema);
  await ensureCollection('image_profiles', ImageProfileSchema);
  await repos.chatSettings.updateForUser(spec.userId, {
    wardrobeImageSettings: { imageProfileId: null, generateFromTools: true },
  } as never);
  {
    const { id: ipId, ...ipRest } = spec.pictureScenario.imageProfile;
    await repos.imageProfiles.create(
      { userId: spec.userId, parameters: {}, isDefault: true, isDangerousCompatible: false, tags: [], ...ipRest } as never,
      { id: ipId, createdAt: '2026-02-01T00:00:00.000Z', updatedAt: '2026-02-01T00:00:00.000Z' } as never,
    );
  }
  // v4 creates `background_jobs` lazily; force it so the reads below see it.
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');
  const pictures: unknown[] = [];
  for (const op of spec.pictureScenario.ops) pictures.push(await runOp(op));
  const { rawQuery } = await import('@/lib/database/manager');
  const jobs = await rawQuery(
    'SELECT "type", "status", "maxAttempts", "payload" FROM "background_jobs" ORDER BY rowid',
  );

  await closeDatabase();

  const lines: string[] = [];
  lines.push(JSON.stringify({ case: 'wardrobe-tools', returns }));
  lines.push(JSON.stringify({ case: 'wardrobe-tools', readback }));
  lines.push(JSON.stringify({ case: 'wardrobe-tools', pictures, jobs }));
  process.stdout.write(lines.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-tools oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
