/**
 * Oracle case (W4.1d batch 1) — the three help / agent-mode tool handlers.
 *
 * Drives v4's REAL handlers over the committed corpus:
 *   - help_settings: executeHelpSettingsTool + formatHelpSettingsResults, reading
 *     a COPY of the baked fixture (chat_settings + the four profile repos).
 *   - help_navigate: executeHelpNavigateTool + formatHelpNavigateResults (pure).
 *   - submit_final_response: executeSubmitFinalResponseTool +
 *     formatSubmitFinalResponseResults (pure).
 *
 * Emits, per op, the tool Output (`output`) and the formatted string
 * (`formatted`) for a field-by-field diff against the Rust port (tools::help).
 * The settings reads mint nothing, so the Rust port reading the same fixture
 * matches exactly (no normalization).
 *
 * Run (Node 24, from the v4 checkout), AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_HELP=/tmp/qt-help.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/help-tools.ts \
 *     > /tmp/oracle-help-tools.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SettingsOp {
  user: 'A' | 'B';
  args: unknown;
}
interface NavigateOp {
  args: unknown;
}
interface SubmitOp {
  chatId: string;
  args: unknown;
}
interface Spec {
  testPepperBase64: string;
  userA: string;
  userB: string;
  settingsOps: SettingsOp[];
  navigateOps: NavigateOp[];
  submitOps: SubmitOp[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'help-tools-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_HELP;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_HELP must point at the fixture from build-help-tools-fixture.ts');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-help-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'help-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { executeHelpSettingsTool, formatHelpSettingsResults } = await import(
    '@/lib/tools/handlers/help-settings-handler'
  );
  const { executeHelpNavigateTool, formatHelpNavigateResults } = await import(
    '@/lib/tools/handlers/help-navigate-handler'
  );
  const { executeSubmitFinalResponseTool, formatSubmitFinalResponseResults } = await import(
    '@/lib/tools/handlers/submit-final-response-handler'
  );

  await initializeDatabase();

  const settings: Array<{ output: unknown; formatted: string }> = [];
  for (const op of spec.settingsOps) {
    const userId = op.user === 'A' ? spec.userA : spec.userB;
    const output = await executeHelpSettingsTool(op.args, { userId });
    const formatted = formatHelpSettingsResults(output);
    settings.push({ output, formatted });
  }

  const navigate: Array<{ output: unknown; formatted: string }> = [];
  for (const op of spec.navigateOps) {
    const output = await executeHelpNavigateTool(op.args, { userId: spec.userA });
    const formatted = formatHelpNavigateResults(output);
    navigate.push({ output, formatted });
  }

  const submit: Array<{ output: unknown; formatted: string }> = [];
  for (const op of spec.submitOps) {
    const output = await executeSubmitFinalResponseTool(op.args, { chatId: op.chatId });
    const formatted = formatSubmitFinalResponseResults(output);
    submit.push({ output, formatted });
  }

  await closeDatabase();

  process.stdout.write(
    JSON.stringify({ case: 'help-tools', settings, navigate, submit }) + '\n'
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`help-tools oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
