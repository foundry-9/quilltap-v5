/**
 * Tier-3 oracle case (P4.170 B.2) — the `WARDROBE_OUTFIT_ANNOUNCEMENT` job
 * body: v4's REAL `handleWardrobeOutfitAnnouncement`
 * (`lib/background-jobs/handlers/wardrobe-announcement.ts`), no LLM.
 *
 * Over ONE copy of the wardrobe-tools fixture pair (a chat whose seeded
 * `equippedOutfit` dresses the caller), the corpus's cases run IN ORDER — each
 * case's `plants` (raw SQL on the main copy) first, then ONE handler call with
 * a job `{ id, payload: { chatId, characterId } }` — and each case records the
 * `[WardrobeAnnouncement]` lines (level, message, context fields in order) and
 * the chat's `chat_messages` rows the call appended (the Aurora whisper).
 * Plants are cumulative, so the Rust side replays the same sequence on its own
 * copy (`wardrobe_announcement_job_tier3_equivalence.rs`).
 *
 * Run (Node 24, from the v4 checkout — the pinned worktree when v4 has drifted):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_WT_TOOLS_CORPUS=1 QT_FIXTURE_WT_MAIN=/tmp/qt-wann-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wann-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-tools-fixture.ts
 *   QT_FIXTURE_WT_MAIN=/tmp/qt-wann-main.db QT_FIXTURE_WT_MOUNT=/tmp/qt-wann-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/wardrobe-announcement-job.ts \
 *     > /tmp/oracle-wardrobe-announcement-job.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

interface Case {
  name: string;
  jobId: string;
  chatId: string;
  characterId: string;
  plants?: string[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'wardrobe-tools.json'), 'utf8'),
  ) as { testPepperBase64: string };
  const corpus = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'wardrobe-announcement-job.json'), 'utf8'),
  ) as { cases: Case[] };

  const fixtureMain = process.env.QT_FIXTURE_WT_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_WT_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_WT_MAIN + QT_FIXTURE_WT_MOUNT must point at the wardrobe-tools seed pair');
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-wann-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'main.db');
  const workMount = join(scratch, 'mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
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
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!data.message.startsWith('[WardrobeAnnouncement]')) return;
      const { service: _s, environment: _e, module: _m, ...fields } = data.context ?? {};
      captured.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite');
  const { handleWardrobeOutfitAnnouncement } = await import(
    '@/lib/background-jobs/handlers/wardrobe-announcement'
  );
  await initializeDatabase();
  const raw = getRawDatabase() as unknown as {
    exec: (s: string) => void;
    prepare: (s: string) => { all: (...a: unknown[]) => Array<Record<string, unknown>> };
  };

  const messages = (chatId: string) =>
    raw
      .prepare('SELECT role, content, systemSender, systemKind FROM chat_messages WHERE chatId = ? ORDER BY rowid')
      .all(chatId);

  for (const c of corpus.cases) {
    for (const sql of c.plants ?? []) raw.exec(sql);
    const before = messages(c.chatId).length;
    captured.length = 0;
    await handleWardrobeOutfitAnnouncement({
      id: c.jobId,
      payload: { chatId: c.chatId, characterId: c.characterId },
    } as never);
    const appended = messages(c.chatId).slice(before);
    process.stdout.write(JSON.stringify({ name: c.name, logs: captured, appended }) + '\n');
  }
  await closeDatabase();
}

main().catch((e) => {
  process.stderr.write(`${e instanceof Error ? e.stack : String(e)}\n`);
  process.exit(1);
});
