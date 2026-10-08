/**
 * Tier-2 oracle case (P4.D262 item 15) — the wear ledger's WRITE chokepoint,
 * v4 `3ee3b1342` "Wardrobe wear ledger (#81)".
 *
 * Drives v4's REAL code over ONE copy of the wardrobe-tools fixture pair (built
 * by the unchanged `build-wardrobe-tools-fixture.ts` with
 * `QT_WT_AVATAR_SPEC=fixtures/wardrobe-wear-commit-tier2.json`, which adds one
 * chat per scenario, each seeded with the caller's Blue Blouse):
 *   - `commit` ops → `getRepositories().wardrobeWear.commitEquippedOutfit`
 *     (the chokepoint itself — v4's spec `wardrobe-wear.repository.test.ts:
 *     237-330`, case for case, plus an earlier wear landing late);
 *   - `apply` ops → `applyOutfitSelections` (the seven shapes of
 *     `apply-outfit-selections.ledger.test.ts:109-198`, minus `llm_choose`,
 *     whose credit rides `outfit_llm_choose_tier3`);
 *   - `route` ops → the chat outfit route's `handleEquipSlot` (`set_all`'s
 *     `wornBundleIds` claim, the primitives' sources, the lost write's 500 —
 *     `outfit.test.ts`).
 * After each op it reads back the op's chat `equippedOutfit` and EVERY
 * `wardrobe_wear_stats` row (minus the minted `id` / `createdAt` /
 * `updatedAt`), and the log lines the ledger work emits, captured by splicing a
 * transport into the singleton logger (child loggers share its transports).
 *
 * Run (Node 24, from the v4 checkout or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_WT_AVATAR_SPEC=$V5/harness/oracle/fixtures/wardrobe-wear-commit-tier2.json \
 *   QT_FIXTURE_WT_MAIN=/tmp/qt-wwc-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wwc-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-wardrobe-tools-fixture.ts
 *   QT_FIXTURE_WWC_MAIN=/tmp/qt-wwc-main.db QT_FIXTURE_WWC_MOUNT=/tmp/qt-wwc-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/wardrobe-wear-commit-tier2.ts \
 *     > /tmp/oracle-wardrobe-wear-commit.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Op {
  name: string;
  kind: 'commit' | 'apply' | 'route';
  chat: string;
  who?: 'caller' | 'recipient';
  nextSlots?: Record<string, string[]>;
  wornBundles?: Array<{ id: string; leafIds: string[] }>;
  source?: string;
  at?: string;
  sourceChat?: string;
  selections?: Array<Record<string, unknown> & { who: 'caller' | 'recipient' }>;
  body?: Record<string, unknown> & { who: 'caller' | 'recipient' };
}

/** The messages this family compares (the ledger's write path, nothing else). */
const CAPTURED = new Set([
  'Committed equipped outfit',
  'Equipped outfit write failed; no wears credited',
  '[applyOutfitSelections] Failed to persist equipped outfit',
  '[Chats v1] Equipped outfit replaced (set_all)',
  '[Chats v1] Some claimed worn bundles were not credited',
  '[Chats v1] Error equipping wardrobe slot',
]);

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const base = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'wardrobe-tools.json'), 'utf8')) as {
    testPepperBase64: string;
    userId: string;
    callerCharacterId: string;
    recipientCharacterId: string;
  };
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'wardrobe-wear-commit-tier2.json'), 'utf8'),
  ) as { ops: Op[] };

  const fixtureMain = process.env.QT_FIXTURE_WWC_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_WWC_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_WWC_MAIN + QT_FIXTURE_WWC_MOUNT must point at the fixture pair (see header)');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-wwc-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'wwc-main.db');
  const workMount = join(scratch, 'wwc-mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = base.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // DEBUG so the chokepoint's `Committed equipped outfit` reaches the capture;
  // the console transport is spliced out below, so nothing reaches stdout.
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
  const { applyOutfitSelections } = await import('@/lib/wardrobe/apply-outfit-selections');
  const { handleEquipSlot } = await import('@/app/api/v1/chats/[id]/actions/outfit');

  await initializeDatabase();
  const repos = getRepositories();
  const who = (w: 'caller' | 'recipient' | undefined): string =>
    w === 'recipient' ? base.recipientCharacterId : base.callerCharacterId;

  const lines: string[] = [];
  for (const op of spec.ops) {
    captured.length = 0;
    let result: unknown = null;
    if (op.kind === 'commit') {
      try {
        const r = await repos.wardrobeWear.commitEquippedOutfit({
          chatId: op.chat,
          characterId: who(op.who),
          nextSlots: op.nextSlots as never,
          wornBundles: op.wornBundles,
          source: op.source as never,
          at: op.at,
        });
        result = r;
      } catch (error) {
        result = { error: error instanceof Error ? error.message : String(error) };
      }
    } else if (op.kind === 'apply') {
      const selections = (op.selections ?? []).map(({ who: w, ...rest }) => ({
        characterId: who(w),
        ...rest,
      }));
      await applyOutfitSelections(op.chat, selections as never, repos, {
        userId: base.userId,
        projectMountPointIds: [],
        sourceChatId: op.sourceChat ?? null,
        ...(op.source ? { source: op.source as never } : {}),
      });
    } else {
      const { who: w, ...body } = op.body!;
      const req = { json: async () => ({ characterId: who(w), ...body }) };
      const res = await handleEquipSlot(req as never, op.chat, {
        repos,
        user: { id: base.userId },
        session: {},
      } as never);
      result = { status: res.status, body: await res.json() };
    }
    const equippedOutfit = (await repos.chats.getEquippedOutfit(op.chat)) ?? null;
    const ledger = (await rawQuery(
      `SELECT "itemId", "wearerCharacterId", "wearCount", "firstWornAt", "lastWornAt", "lastWornChatId"
         FROM "wardrobe_wear_stats" ORDER BY "itemId", COALESCE("wearerCharacterId", '')`,
    )) as unknown[];
    lines.push(
      JSON.stringify({
        case: 'wardrobe-wear-commit',
        name: op.name,
        kind: op.kind,
        result,
        equippedOutfit,
        ledger,
        logs: captured.map((c) => ({ ...c })),
      }),
    );
  }

  await closeDatabase();
  process.stdout.write(lines.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`wardrobe-wear-commit oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
