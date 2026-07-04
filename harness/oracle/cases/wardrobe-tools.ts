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
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_WT_MAIN=/tmp/qt-wt-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wt-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/wardrobe-tools.ts > /tmp/oracle-wardrobe-tools.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
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

  // A single per-turn announcement Set (as the orchestrator would supply). It
  // absorbs the archive/wear/take_off announcements so nothing is enqueued.
  const pendingWardrobeAnnouncements = new Set<string>();

  const returns: unknown[] = [];
  for (const op of spec.ops) {
    const characterId =
      op.characterId === 'recipient' ? spec.recipientCharacterId : spec.callerCharacterId;
    const base = { userId: spec.userId, chatId: spec.chatId, characterId };
    let output: unknown;
    let formatted: string;
    switch (op.tool) {
      case 'wardrobe_list':
        output = await wl.executeWardrobeListTool(op.args, base);
        formatted = wl.formatWardrobeListResults(output as never);
        break;
      case 'wardrobe_read':
        output = await wr.executeWardrobeReadTool(op.args, base);
        formatted = wr.formatWardrobeReadResults(output as never);
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
    returns.push({ name: op.name, tool: op.tool, output, formatted });
  }

  // Read the mutated state back (the "tables" diff, in read-back form).
  const repos = getRepositories();
  const readback = {
    callerItems: await repos.wardrobe.findByCharacterId(spec.callerCharacterId, true),
    recipientItems: await repos.wardrobe.findByCharacterId(spec.recipientCharacterId, true),
    generalItems: await repos.wardrobe.findArchetypes(true),
    equippedOutfit: (await repos.chats.getEquippedOutfit(spec.chatId)) ?? null,
    pendingAnnouncements: Array.from(pendingWardrobeAnnouncements).sort(),
  };

  await closeDatabase();

  const lines: string[] = [];
  lines.push(JSON.stringify({ case: 'wardrobe-tools', returns }));
  lines.push(JSON.stringify({ case: 'wardrobe-tools', readback }));
  process.stdout.write(lines.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-tools oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
