/**
 * @jest-environment node
 *
 * P4.D164 tier-2 ORACLE for the subprompts PROMPT ASSEMBLY end to end (v4
 * `2f4254b42`): the compiler bake, the greeting head, and the fan-out's
 * REAL recompile — the family that proves the whole chain, where
 * `subprompts-storage.test.ts` (P4.D163) recorded the compiler as a seam.
 *
 * Drives v4's REAL `compileAllIdentityStacks` / `compileIdentityStackFor
 * Participant` / `buildChatContext` / `updateCharacterSubprompt` /
 * `deleteCharacterSubprompt` / `fanOutSubpromptChange` — nothing mocked but
 * the realtime bus (a no-op) and the job processor — over a FRESH copy of
 * the committed `subprompts-{main,mount}.db` pair per case, with the REAL
 * repositories, the real cipher binding, and the character-vault bridge
 * un-mocked. The fixture already carries two chats sharing character A
 * (`chatLlm` + `chatTwoSeats`), so the fan-out's recompile is observable as
 * two changed `compiledIdentityStacks` cells — no fixture extension.
 *
 * Emits one NDJSON line per case: { name, result, stacks, participants }
 * where `stacks` maps each named chat to its persisted
 * `compiledIdentityStacks` (or null — the whole envelope, `version` included,
 * the comparand) and `participants` maps it to `[[seatId, selectedSubpromptIds]]`.
 * The compile mints no clock and the cells carry no timestamps; the update's
 * minted `updatedAt` is reduced away (only `{id, title}` is kept).
 *
 * Run (Node 24, from the v4 checkout — a pinned worktree while v4 HEAD is past
 * the baseline; cp to a /tmp mirror, jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-subprompts-prompt-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/subprompts-prompt.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/subprompts.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/subprompts-main.db \
 *   QT_FIXTURE_SP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/subprompts-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-subprompts-prompt.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- subprompts-prompt
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  ids: Record<string, string>;
}

function applyMocks(): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  jest.doMock('@/lib/mount-index/character-vault', () =>
    jest.requireActual('@/lib/mount-index/character-vault'),
  );
  // The compiler runs REAL here (the point of this family); only the bus and
  // the job processor are quieted.
  jest.doMock('@/lib/realtime/bus', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/realtime/bus'),
    publishRealtime: () => undefined,
  }));
  jest.doMock('@/lib/background-jobs/processor', () => {
    const actual = jest.requireActual('@/lib/background-jobs/processor');
    return { __esModule: true, ...actual, ensureProcessorRunning: () => undefined };
  });
}

type Result = unknown;

async function attempt(fn: () => Promise<Result>): Promise<Result> {
  try {
    return await fn();
  } catch (error) {
    const e = error as { name?: string; message?: string };
    return { error: { name: e?.name ?? 'Error', message: e?.message ?? String(error) } };
  }
}

interface Mods {
  compiler: typeof import('@/lib/services/system-prompt-compiler/compiler');
  sp: typeof import('@/lib/subprompts/subprompts');
  fanout: typeof import('@/lib/subprompts/chat-fanout');
  init: typeof import('@/lib/chat/initialize');
  repos: ReturnType<typeof import('@/lib/repositories/factory').getRepositories>;
}

interface CaseSpec {
  name: string;
  /** The chats whose cells + seats are dumped after the op. */
  chats: string[];
  run: (m: Mods, I: Record<string, string>) => Promise<Result>;
}

async function chatOf(m: Mods, id: string): Promise<Record<string, unknown>> {
  const chat = (await m.repos.chats.findById(id)) as Record<string, unknown> | null;
  if (!chat) throw new Error(`chat ${id} missing from the fixture`);
  return chat;
}

async function compileAll(m: Mods, id: string): Promise<void> {
  await m.compiler.compileAllIdentityStacks(await chatOf(m, id) as never);
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks();

  const work = mkdtempSync(join(scratch, 'spp-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  await initializeDatabase();

  try {
    const { getRepositories } = await import('@/lib/repositories/factory');
    const m: Mods = {
      compiler: await import('@/lib/services/system-prompt-compiler/compiler'),
      sp: await import('@/lib/subprompts/subprompts'),
      fanout: await import('@/lib/subprompts/chat-fanout'),
      init: await import('@/lib/chat/initialize'),
      repos: getRepositories(),
    };
    const result = await attempt(() => c.run(m, spec.ids));
    const stacks: Record<string, unknown> = {};
    const participants: Record<string, unknown> = {};
    for (const id of c.chats) {
      const chat = await chatOf(m, id);
      stacks[id] = (chat.compiledIdentityStacks as unknown) ?? null;
      participants[id] = ((chat.participants as Array<Record<string, unknown>>) ?? []).map(
        (p) => [p.id, (p.selectedSubpromptIds as unknown) ?? null],
      );
    }
    return { name: c.name, result, stacks, participants };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`subprompts-prompt oracle must run under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'subprompts.json'), 'utf8'),
  ) as Spec;
  const I = spec.ids;

  const fixtures = {
    main: process.env.QT_FIXTURE_SP_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SP_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-subprompts-prompt-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;

  const A = I.charA;
  const reduced = (r: { id: string; title: string }) => ({ id: r.id, title: r.title });
  const greeting = async (m: Mods, ids: string[], scenario?: string) => {
    const subprompts = await m.sp.resolveSelectedSubprompts(A, ids);
    const ctx = await m.init.buildChatContext(A, undefined, scenario, undefined, subprompts);
    return { systemPrompt: ctx.systemPrompt, firstMessage: ctx.firstMessage };
  };

  const cases: CaseSpec[] = [
    // ── the compiler bake: the block present for the selecting seat, absent
    //    for the user seat, the removed seat, the pre-feature seat, and a seat
    //    whose character has no Subprompts/ at all ──────────────────────────
    { name: 'compile_all_chatLlm_block_present', chats: [I.chatLlm], run: async (m) => { await compileAll(m, I.chatLlm); return null; } },
    { name: 'compile_all_chatUser_no_cell', chats: [I.chatUser], run: async (m) => { await compileAll(m, I.chatUser); return null; } },
    { name: 'compile_all_chatRemoved_no_cell', chats: [I.chatRemoved], run: async (m) => { await compileAll(m, I.chatRemoved); return null; } },
    { name: 'compile_all_chatNoKey_cell_without_block', chats: [I.chatNoKey], run: async (m) => { await compileAll(m, I.chatNoKey); return null; } },
    { name: 'compile_all_chatTwoSeats_A_block_B_none', chats: [I.chatTwoSeats], run: async (m) => { await compileAll(m, I.chatTwoSeats); return null; } },
    { name: 'compile_participant_pLlm', chats: [I.chatLlm], run: async (m) => { await m.compiler.compileIdentityStackForParticipant(await chatOf(m, I.chatLlm) as never, I.pLlm); return null; } },
    { name: 'compile_participant_pUser_writes_nothing', chats: [I.chatUser], run: async (m) => { await m.compiler.compileIdentityStackForParticipant(await chatOf(m, I.chatUser) as never, I.pUser); return null; } },
    // ── the greeting head ─────────────────────────────────────────────────
    { name: 'greeting_A_terse_VERSE', chats: [], run: (m) => greeting(m, ['terse', 'VERSE']) },
    { name: 'greeting_A_none', chats: [], run: (m) => greeting(m, []) },
    { name: 'greeting_A_dangling_and_terse_with_scenario', chats: [], run: (m) => greeting(m, ['gone', 'terse'], 'A rainy quay.') },
    // ── the fan-out, REAL compile: an update rewrites both chats' A cells;
    //    a delete strips the id and recompiles without the block ───────────
    {
      name: 'fanout_update_terse_recompiles_both_chats',
      chats: [I.chatLlm, I.chatTwoSeats],
      run: async (m) => {
        await compileAll(m, I.chatLlm);
        await compileAll(m, I.chatTwoSeats);
        const updated = reduced(await m.sp.updateCharacterSubprompt(A, 'terse', { content: 'Two lines at most.' }));
        const fan = await m.fanout.fanOutSubpromptChange(A, 'terse');
        return { updated, fan };
      },
    },
    {
      name: 'fanout_delete_terse_strips_and_recompiles',
      chats: [I.chatLlm, I.chatTwoSeats],
      run: async (m) => {
        await compileAll(m, I.chatLlm);
        await compileAll(m, I.chatTwoSeats);
        const deleted = await m.sp.deleteCharacterSubprompt(A, 'terse');
        const fan = await m.fanout.fanOutSubpromptChange(A, 'terse', { removeSelection: true });
        return { deleted, fan };
      },
    },
    {
      name: 'fanout_delete_verse_strips_VERSE_recompiles_chatLlm',
      chats: [I.chatLlm, I.chatTwoSeats],
      run: async (m) => {
        await compileAll(m, I.chatLlm);
        await compileAll(m, I.chatTwoSeats);
        const deleted = await m.sp.deleteCharacterSubprompt(A, 'Verse');
        const fan = await m.fanout.fanOutSubpromptChange(A, 'verse', { removeSelection: true });
        return { deleted, fan };
      },
    },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    outLines.push(JSON.stringify(await runCase(spec, c, scratch, fixtures)));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`subprompts-prompt oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('subprompts-prompt tier-2 oracle', async () => {
  await main();
});
