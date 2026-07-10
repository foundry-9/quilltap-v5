/**
 * Read-differential ORACLE for buildChatContext (P4.4 unit 2, sub-unit 2).
 *
 * Opens the pre-seeded main + mount-index fixtures and drives v4's REAL
 * `buildChatContext` (lib/chat/initialize.ts) over a matrix, emitting the
 * computed fields (systemPrompt, firstMessage, the resolved characterId/name,
 * and the resolved userCharacter id/name). The Rust port
 * (services::chat_initialize::build_chat_context) reads the SAME fixtures and
 * must produce the same values exactly (no normalization — this path mints no
 * clock/id).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CCTX_MAIN=/tmp/qt-cctx-main.db QT_FIXTURE_CCTX_MOUNT=/tmp/qt-cctx-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/chat-context-init.ts > /tmp/oracle-cctx.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  ariaId: string;
  samId: string;
  bobId: string;
  ariaSp2: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'chat-context-init.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_CCTX_MAIN;
  const mountFixture = process.env.QT_FIXTURE_CCTX_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_CCTX_MAIN and QT_FIXTURE_CCTX_MOUNT must point at the seeded fixtures');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cctx-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'cctx-main-work.db');
  const mountWork = join(scratch, 'cctx-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { buildChatContext } = await import('@/lib/chat/initialize');

  await initializeDatabase();

  const matrix: Array<{
    id: string;
    characterId: string;
    userCharacterId?: string;
    scenario?: string;
    selectedSystemPromptId?: string;
  }> = [
    { id: 'basic', characterId: spec.ariaId },
    { id: 'with_user_scenario', characterId: spec.ariaId, userCharacterId: spec.samId, scenario: 'A misty harbor at dawn.' },
    { id: 'selected_prompt', characterId: spec.ariaId, selectedSystemPromptId: spec.ariaSp2 },
    { id: 'default_partner', characterId: spec.bobId },
  ];

  const rows: unknown[] = [];
  for (const c of matrix) {
    const ctx = await buildChatContext(c.characterId, c.userCharacterId, c.scenario, c.selectedSystemPromptId);
    rows.push({
      id: c.id,
      systemPrompt: ctx.systemPrompt,
      firstMessage: ctx.firstMessage,
      characterId: ctx.character.id,
      characterName: ctx.character.name,
      userCharacterId: ctx.userCharacter?.id ?? null,
      userCharacterName: ctx.userCharacter?.name ?? null,
    });
  }

  await closeDatabase();

  for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-context-init oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
