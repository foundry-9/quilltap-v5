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
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SELFINV_MAIN=/tmp/qt-selfinv-main.db \
 *   QT_FIXTURE_SELFINV_MOUNT=/tmp/qt-selfinv-mount.db \
 *   QT_FIXTURE_SELFINV_LLMLOGS=/tmp/qt-selfinv-llmlogs.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/self-inventory.ts > /tmp/oracle-selfinv.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CaseSpec {
  name: string;
  characterId: string | null;
  sections?: string[];
  includeAutomaticImages?: boolean;
  withLoadedMemories?: boolean;
  /** P4.D350: raw SQL run on the main working copy before the case (both sides). */
  plantSql?: string;
  /** P4.D350: raw SQL run after the case, restoring the plant (both sides). */
  restoreSql?: string;
  /**
   * P4.D378 R-F (item 13): group ids whose HYDRATED read (`groups.findById`)
   * must throw while the RAW row (`findByIdRaw`) stays whole — proven in-case,
   * on both sides, before the handler runs.
   */
  hydratedProbeGroupIds?: string[];
}

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  aria: { id: string };
  chat: { id: string };
  project: { id: string };
  loadedMemories: {
    semantic: Array<{ summary: string; importance: number; score: number; salience: number }>;
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
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
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
  // P4.D350: debug, so the two `self_inventory:` DEBUG lines (v4 `074e1e01a`)
  // reach the spliced transport below; every other line is dropped there.
  process.env.LOG_LEVEL = 'debug';
  // Ensure a clean local-dev / browser host env for the quilltap section.
  process.env.NODE_ENV = 'development';
  delete process.env.QUILLTAP_ELECTRON;
  delete process.env.ELECTRON_SHELL_VERSION;

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  // P4.D350: capture the handler's own DEBUG lines (`self_inventory: …`) per
  // case by splicing the logger's transports (memory note
  // `v4-oracle-job-host-and-log-capture`).
  const { logger } = await import('@/lib/logger');
  const captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!data.message.startsWith('self_inventory: ')) return;
      const { service: _s, module: _m, environment: _e, ...fields } = (data.context ?? {}) as Record<string, unknown>;
      captured.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });
  const { executeSelfInventoryTool, formatSelfInventoryResults } = await import(
    '@/lib/tools/handlers/self-inventory-handler'
  );

  await initializeDatabase();

  const loadedMemoriesContext = {
    semantic: spec.loadedMemories.semantic,
    interCharacter: spec.loadedMemories.interCharacter,
    recap: spec.loadedMemories.recap,
  };

  const { getRepositories } = await import('@/lib/repositories/factory');
  const repos = getRepositories();

  const outputs: Array<{
    name: string;
    output: unknown;
    formatted: string;
    logs: unknown[];
    probe?: unknown[];
  }> = [];
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

    if (c.plantSql) await rawQuery(c.plantSql);
    let probe: unknown[] | undefined;
    if (c.hydratedProbeGroupIds) {
      probe = [];
      for (const groupId of c.hydratedProbeGroupIds) {
        let hydratedThrew = false;
        try {
          await repos.groups.findById(groupId);
        } catch {
          hydratedThrew = true;
        }
        const raw = await repos.groups.findByIdRaw(groupId);
        probe.push({
          groupId,
          hydratedThrew,
          rawName: raw?.name ?? null,
          rawHasOfficialStore: Boolean(raw?.officialMountPointId),
        });
      }
    }
    captured.length = 0;
    const output = await executeSelfInventoryTool(input, context as never);
    const logs = captured.splice(0, captured.length);
    if (c.restoreSql) await rawQuery(c.restoreSql);
    const formatted = formatSelfInventoryResults(output);
    outputs.push(probe ? { name: c.name, output, formatted, logs, probe } : { name: c.name, output, formatted, logs });
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
