/**
 * Read-differential oracle — v4's `executeSelfInventoryTool` +
 * `formatSelfInventoryResults` (W4.1d, the `self_inventory` tool).
 *
 * Opens a COPY of the pre-baked three-database fixture (main + mount-index +
 * llm-logs) and drives v4's REAL handler for each case, emitting the resulting
 * `SelfInventoryToolOutput` JSON AND the formatted markdown string. The Rust port
 * (`quilltap_core::tools::self_inventory`) reads the same fixtures and must produce
 * the same Output + formatted string — exactly, with no normalization (nothing is
 * mutated; the physicalDescription mint never surfaces).
 *
 * The `quilltap` section is fed the SAME host-env values the Rust `SelfInventoryEnv`
 * receives: `runtimeMode='local-dev'` + `clientShell={type:'browser'}` (achieved by
 * NOT setting the electron/docker/vm env vars) and only `quilltap.version` is
 * requested (release notes / changelog are out of the corpus, a documented
 * deferral). `isMountIndexDegraded()` is false (healthy fixture).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixtures:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SELFINV_MAIN=/tmp/qt-selfinv-main.db \
 *   QT_FIXTURE_SELFINV_MOUNT=/tmp/qt-selfinv-mount.db \
 *   QT_FIXTURE_SELFINV_LLMLOGS=/tmp/qt-selfinv-llmlogs.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/self-inventory.ts > /tmp/oracle-selfinv.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CaseSpec {
  name: string;
  characterId: string | null;
  sections?: string[];
  includeAutomaticImages?: boolean;
  withLoadedMemories?: boolean;
}

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  aria: { id: string };
  chat: { id: string };
  project: { id: string };
  loadedMemories: {
    semantic: Array<{ summary: string; importance: number; score: number; effectiveWeight: number }>;
    interCharacter: Array<{ aboutCharacterName: string; summary: string; importance: number }>;
    recap: string;
  };
  cases: CaseSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'self-inventory.json'), 'utf8')
  ) as Spec;

  const fxMain = process.env.QT_FIXTURE_SELFINV_MAIN;
  const fxMount = process.env.QT_FIXTURE_SELFINV_MOUNT;
  const fxLlm = process.env.QT_FIXTURE_SELFINV_LLMLOGS;
  if (
    !fxMain || !existsSync(fxMain) ||
    !fxMount || !existsSync(fxMount) ||
    !fxLlm || !existsSync(fxLlm)
  ) {
    throw new Error(
      'QT_FIXTURE_SELFINV_MAIN / _MOUNT / _LLMLOGS must point at the built fixtures'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-selfinv-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'selfinv-main-work.db');
  const mountWork = join(scratch, 'selfinv-mount-work.db');
  const llmWork = join(scratch, 'selfinv-llmlogs-work.db');
  copyFileSync(fxMain, mainWork);
  copyFileSync(fxMount, mountWork);
  copyFileSync(fxLlm, llmWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = llmWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';
  // Ensure a clean local-dev / browser host env for the quilltap section.
  process.env.NODE_ENV = 'development';
  delete process.env.QUILLTAP_ELECTRON;
  delete process.env.ELECTRON_SHELL_VERSION;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { executeSelfInventoryTool, formatSelfInventoryResults } = await import(
    '@/lib/tools/handlers/self-inventory-handler'
  );

  await initializeDatabase();

  const loadedMemoriesContext = {
    semantic: spec.loadedMemories.semantic,
    interCharacter: spec.loadedMemories.interCharacter,
    recap: spec.loadedMemories.recap,
  };

  const outputs: Array<{ name: string; output: unknown; formatted: string }> = [];
  for (const c of spec.cases) {
    const input: Record<string, unknown> = {};
    if (c.sections) input.sections = c.sections;
    if (c.includeAutomaticImages !== undefined) {
      input.includeAutomaticImages = c.includeAutomaticImages;
    }

    const context: Record<string, unknown> = {
      userId: spec.userId,
      chatId: spec.chat.id,
      characterId: c.characterId ?? undefined,
      projectId: spec.project.id,
      callingParticipantId: undefined,
    };
    if (c.withLoadedMemories) context.loadedMemories = loadedMemoriesContext;

    const output = await executeSelfInventoryTool(input, context as never);
    const formatted = formatSelfInventoryResults(output);
    outputs.push({ name: c.name, output, formatted });
  }

  await closeDatabase();

  for (const o of outputs) {
    process.stdout.write(JSON.stringify(o) + '\n');
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`self-inventory oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
