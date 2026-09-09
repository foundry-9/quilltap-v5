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
  // [P4.D168] Freeze the wall clock: the greeting's FORCED progressions report
  // reads `Date.now()`, and the Rust side injects the same constant. Without
  // this the elapsed/remaining spans differ on every run.
  const FIXED_NOW_MS = 1718452800000; // 2024-06-15T12:00:00Z
  const RealDate = Date;
  const FakeDate = class extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) {
        super(FIXED_NOW_MS);
      } else {
        // @ts-expect-error variadic forwarding
        super(...args);
      }
    }
    static now(): number {
      return FIXED_NOW_MS;
    }
  } as DateConstructor;
  (global as { Date: DateConstructor }).Date = FakeDate;

  const { buildChatContext } = await import('@/lib/chat/initialize');

  await initializeDatabase();

  const matrix: Array<{
    id: string;
    characterId: string;
    userCharacterId?: string;
    scenario?: string;
    selectedSystemPromptId?: string;
    /** P4.D164: the opener's selection, resolved through v4's REAL
     *  `resolveSelectedSubprompts` exactly as `handleCreate` does. */
    selectedSubpromptIds?: string[];
  }> = [
    { id: 'basic', characterId: spec.ariaId },
    { id: 'with_user_scenario', characterId: spec.ariaId, userCharacterId: spec.samId, scenario: 'A misty harbor at dawn.' },
    { id: 'selected_prompt', characterId: spec.ariaId, selectedSystemPromptId: spec.ariaSp2 },
    { id: 'default_partner', characterId: spec.bobId },
    // P4.D164 / v4 `2f4254b42`: the greeting's `## Additional Instructions`.
    { id: 'sp_two_no_scenario', characterId: spec.ariaId, selectedSubpromptIds: ['terse', 'scene'] },
    { id: 'sp_two_with_scenario_and_user', characterId: spec.ariaId, userCharacterId: spec.samId, scenario: 'A misty harbor at dawn.', selectedSubpromptIds: ['scene', 'TERSE'] },
    { id: 'sp_dangling_only_renders_nothing', characterId: spec.ariaId, selectedSubpromptIds: ['gone'] },
    { id: 'sp_empty_renders_nothing', characterId: spec.ariaId, selectedSubpromptIds: [] },
    { id: 'sp_with_selected_prompt', characterId: spec.ariaId, selectedSystemPromptId: spec.ariaSp2, selectedSubpromptIds: ['terse'] },
    // [P4.D168] Sam carries progressions; the greeting asks for the report
    // FORCED and reports every entry it parses — the complete `once` one
    // included — while the schema-refused entry is dropped and its siblings
    // survive. (MEASURED: at this call site `force` is inert either way. The
    // greeting passes no event loader, so `lastTurnMs` is null and rule 1
    // reports everything regardless; a mutation flipping `force` to `false`
    // stays green on BOTH sides. What these arms pin is that the opener reports
    // unconditionally, and where the block lands in the prompt.)
    { id: 'prog_forced_greeting', characterId: spec.samId },
    { id: 'prog_forced_with_scenario', characterId: spec.samId, scenario: 'A misty harbor at dawn.' },
    // Aria carries none: byte-identical to a pre-feature greeting.
    { id: 'prog_absent_on_aria', characterId: spec.ariaId },
  ];

  const { resolveSelectedSubprompts } = await import('@/lib/subprompts/subprompts');
  const rows: unknown[] = [];
  for (const c of matrix) {
    const subprompts = c.selectedSubpromptIds === undefined
      ? undefined
      : await resolveSelectedSubprompts(c.characterId, c.selectedSubpromptIds);
    const ctx = await buildChatContext(c.characterId, c.userCharacterId, c.scenario, c.selectedSystemPromptId, subprompts);
    rows.push({
      id: c.id,
      selectedSubpromptIds: c.selectedSubpromptIds ?? null,
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
