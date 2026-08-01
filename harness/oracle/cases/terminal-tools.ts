/**
 * Read-differential oracle — v4's Prospero terminal tools (`terminal_read` /
 * `terminal_list`, W4.1d batch 1).
 *
 * Opens a COPY of the pre-baked fixture and drives v4's REAL handler
 * (`executeTerminalReadTool` / `executeTerminalListTool`), validators
 * (`validateTerminalReadInput` / `validateTerminalListInput`), and formatters
 * (`formatTerminalReadResults` / `formatTerminalListResults`) for each case,
 * emitting each Output + formatted string verbatim. The Rust port
 * (`tools::terminal`) reads the same fixture and must produce identical Output
 * JSON + formatted strings — exactly (no normalization; nothing is mutated).
 *
 * Scrollback source: there is no live PTY in this process, so `ptyManager.get`
 * returns undefined and v4 falls to `tailFile(session.transcriptPath)`, reading
 * the exact bytes the fixture builder wrote. The Rust side reads the same file.
 *
 * `terminal_read` throw cases (session-not-found / wrong-chat): the handler
 * throws `TerminalToolError`; we capture `err.message`. Validator cases: the
 * dispatcher gates on `validate*` BEFORE execute and returns a fixed error
 * string; we record the validator's VERDICT as a boolean so the Rust side
 * proves the same gate decision. (Since v4 `e3593f75` the validators return
 * the parse — `TerminalReadInput | null` — not a boolean, so the verdict is
 * `!== null`; recording the raw return here rotted the oracle's `valid`
 * field to object/null and broke the harness parse.)
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TERMTOOLS=/tmp/qt-termtools.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/terminal-tools.ts \
 *     > /tmp/oracle-termtools.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface ReadCase {
  name: string;
  chatId: string;
  args: Record<string, unknown>;
  expectThrow?: string;
}
interface ReadValidateCase {
  name: string;
  args: Record<string, unknown>;
}
interface ListCase {
  name: string;
  chatId: string;
}
interface Spec {
  testPepperBase64: string;
  readCases: ReadCase[];
  readValidateCases: ReadValidateCase[];
  listCases: ListCase[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'terminal-tools-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_TERMTOOLS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_TERMTOOLS must point at the fixture from build-terminal-tools-fixture.ts'
    );
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-termtools-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'termtools-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const {
    executeTerminalReadTool,
    executeTerminalListTool,
    formatTerminalReadResults,
    formatTerminalListResults,
  } = await import('@/lib/tools/handlers/terminal-handler');
  const { validateTerminalReadInput } = await import('@/lib/tools/terminal-read-tool');
  const { validateTerminalListInput } = await import('@/lib/tools/terminal-list-tool');

  await initializeDatabase();

  const readResults: Array<{
    name: string;
    thrown?: string;
    output?: unknown;
    formatted?: string;
  }> = [];

  for (const c of spec.readCases) {
    const ctx = { userId: undefined, chatId: c.chatId, config: {} };
    try {
      const output = await executeTerminalReadTool(c.args as never, ctx as never);
      const formatted = formatTerminalReadResults(output);
      readResults.push({ name: c.name, output, formatted });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      readResults.push({ name: c.name, thrown: message });
    }
  }

  const readValidateResults = spec.readValidateCases.map((c) => ({
    name: c.name,
    valid: validateTerminalReadInput(c.args) !== null,
  }));

  const listResults: Array<{ name: string; valid: boolean; output: unknown; formatted: string }> =
    [];
  for (const c of spec.listCases) {
    const ctx = { userId: undefined, chatId: c.chatId, config: {} };
    // The list validator gates on the (empty) arguments object.
    const valid = validateTerminalListInput({}) !== null;
    const output = await executeTerminalListTool({} as never, ctx as never);
    const formatted = formatTerminalListResults(output);
    listResults.push({ name: c.name, valid, output, formatted });
  }

  await closeDatabase();

  process.stdout.write(
    JSON.stringify({
      case: 'terminal-tools',
      readResults,
      readValidateResults,
      listResults,
    }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`terminal-tools oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
