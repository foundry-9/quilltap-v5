/**
 * Tier-2 oracle case — the wardrobe tools' AVATAR TRIGGER (P4.123; v4
 * `wardrobe-{create,wear,take-off,archive}-handler.ts` →
 * `triggerAvatarGenerationIfEnabled` → the REAL `enqueueCharacterAvatarGeneration`).
 *
 * Same tsx / real-DB / no-jest / no-mock shape as `wardrobe-tools.ts`, over the
 * flag-ON fixture (`build-wardrobe-tools-fixture.ts` with `QT_WT_AVATAR_SPEC`).
 * Each scenario in `fixtures/wardrobe-tools-avatar-trigger.json` runs against its
 * OWN chat (so the trigger's pending-job dedup never crosses scenarios); after all
 * scenarios the whole `background_jobs` table and each chat's equipped outfit are
 * read back.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the flag-ON fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_WT_AVATAR_SPEC=~/source/quilltap-v5/harness/oracle/fixtures/wardrobe-tools-avatar-trigger.json \
 *   QT_FIXTURE_WTA_MAIN=/tmp/qt-wta-main.db QT_FIXTURE_WTA_MOUNT=/tmp/qt-wta-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/wardrobe-tools-avatar-trigger.ts \
 *     > /tmp/oracle-wardrobe-tools-avatar-trigger.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Op {
  tool: string;
  args: Record<string, unknown>;
}
interface Scenario {
  name: string;
  chatId: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const base = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'wardrobe-tools.json'), 'utf8'),
  ) as { testPepperBase64: string; userId: string; callerCharacterId: string };
  const av = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'wardrobe-tools-avatar-trigger.json'), 'utf8'),
  ) as { scenarios: Scenario[] };

  const fixtureMain = process.env.QT_FIXTURE_WTA_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_WTA_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_WTA_MAIN + QT_FIXTURE_WTA_MOUNT must point at the flag-ON fixtures');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wta-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'wta-main.db');
  const workMount = join(scratch, 'wta-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = base.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const wc = await import('@/lib/tools/handlers/wardrobe-create-handler');
  const wa = await import('@/lib/tools/handlers/wardrobe-archive-handler');
  const ww = await import('@/lib/tools/handlers/wardrobe-wear-handler');
  const wt = await import('@/lib/tools/handlers/wardrobe-take-off-handler');

  await initializeDatabase();

  const scenarios: unknown[] = [];
  for (const sc of av.scenarios) {
    const pendingWardrobeAnnouncements = new Set<string>();
    const ctx = {
      userId: base.userId,
      chatId: sc.chatId,
      characterId: base.callerCharacterId,
      pendingWardrobeAnnouncements,
    };
    const successes: boolean[] = [];
    for (const op of sc.ops) {
      let output: { success: boolean };
      switch (op.tool) {
        case 'wardrobe_create':
          output = await wc.executeWardrobeCreateTool(op.args, ctx);
          break;
        case 'wardrobe_archive':
          output = await wa.executeWardrobeArchiveTool(op.args, ctx);
          break;
        case 'wardrobe_wear':
          output = await ww.executeWardrobeWearTool(op.args, ctx);
          break;
        case 'wardrobe_take_off':
          output = await wt.executeWardrobeTakeOffTool(op.args, ctx);
          break;
        default:
          throw new Error(`unknown tool ${op.tool}`);
      }
      successes.push(output.success);
    }
    scenarios.push({
      name: sc.name,
      chatId: sc.chatId,
      successes,
      pendingAnnouncements: Array.from(pendingWardrobeAnnouncements).sort(),
    });
  }

  const repos = getRepositories();
  // Raw rows projected to the columns the trigger decides (payload parsed), so
  // the Rust side compares the same shape without a repository twin.
  const rawJobs = (await rawQuery('SELECT * FROM background_jobs', [])) as Array<
    Record<string, unknown>
  >;
  const jobs = rawJobs.map((r) => ({
    type: r.type,
    status: r.status,
    priority: r.priority,
    maxAttempts: r.maxAttempts,
    attempts: r.attempts,
    payload: JSON.parse(String(r.payload)),
  }));
  const equipped: Record<string, unknown> = {};
  for (const sc of av.scenarios) {
    equipped[sc.chatId] = (await repos.chats.getEquippedOutfit(sc.chatId)) ?? null;
  }
  await closeDatabase();

  const lines = [
    JSON.stringify({ case: 'wardrobe-tools-avatar-trigger', scenarios }),
    JSON.stringify({ case: 'wardrobe-tools-avatar-trigger', jobs, equipped }),
  ];
  process.stdout.write(lines.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-tools-avatar-trigger oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
