/**
 * Tier-2 differential ORACLE for the identity-stack compiler (P4.4 unit 2,
 * sub-unit 3).
 *
 * Opens a COPY of the pre-seeded fixtures, reads the chat, drives v4's REAL
 * `compileAllIdentityStacks` (lib/services/system-prompt-compiler/compiler.ts),
 * and emits the persisted `compiledIdentityStacks` map. The Rust port
 * (services::system_prompt_compiler::compile_all_identity_stacks) runs the same
 * over its own copy and must persist the same map exactly (all ids pinned — zero
 * normalization).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_IDC_MAIN=/tmp/qt-idc-main.db QT_FIXTURE_IDC_MOUNT=/tmp/qt-idc-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/identity-compiler.ts > /tmp/oracle-idc.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  chatId: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'identity-compiler.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_IDC_MAIN;
  const mountFixture = process.env.QT_FIXTURE_IDC_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_IDC_MAIN and QT_FIXTURE_IDC_MOUNT must point at the seeded fixtures');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-idc-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'idc-main-work.db');
  const mountWork = join(scratch, 'idc-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { compileAllIdentityStacks } = await import(
    '@/lib/services/system-prompt-compiler/compiler'
  );

  await initializeDatabase();
  const repos = getRepositories();

  const chat = await repos.chats.findById(spec.chatId);
  if (!chat) throw new Error('chat not found in fixture');
  await compileAllIdentityStacks(chat);

  const after = await repos.chats.findById(spec.chatId);
  const compiled = (after?.compiledIdentityStacks as Record<string, string> | null | undefined) ?? null;

  await closeDatabase();

  process.stdout.write(JSON.stringify({ compiledIdentityStacks: compiled }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`identity-compiler oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
