/**
 * Tier-2 differential ORACLE for the identity-stack compiler (P4.4 unit 2,
 * sub-unit 3).
 *
 * Opens a COPY of the pre-seeded fixtures, reads each chat, drives v4's REAL
 * compiler (lib/services/system-prompt-compiler/compiler.ts), and emits the
 * persisted `compiledIdentityStacks` value. The Rust port
 * (services::system_prompt_compiler) runs the same over its own copy and must
 * persist the same value exactly (all ids pinned — zero normalization).
 *
 * Row kinds:
 *   compileAll — `compileAllIdentityStacks` over the base chat.
 *   participant — `compileIdentityStackForParticipant` over one of the eight
 *     P4.D103 envelope chats, whose pre-seeded `compiledIdentityStacks` carries
 *     a current / legacy / older / newer stamp (v4 `a6870c5a`). The seeded
 *     stacks are SENTINEL strings, so a port that merges into or rewrites a
 *     stale map shows the sentinel in its output rather than being inferred.
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
  ariaP: string;
  samP: string;
  envelopeChats: Record<string, string>;
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
  const { compileAllIdentityStacks, compileIdentityStackForParticipant } = await import(
    '@/lib/services/system-prompt-compiler/compiler'
  );

  await initializeDatabase();
  const repos = getRepositories();

  const rows: unknown[] = [];

  const readCompiled = async (chatId: string): Promise<unknown> => {
    const after = await repos.chats.findById(chatId);
    return (after?.compiledIdentityStacks as unknown) ?? null;
  };

  const chat = await repos.chats.findById(spec.chatId);
  if (!chat) throw new Error('chat not found in fixture');
  await compileAllIdentityStacks(chat);
  rows.push({
    kind: 'compileAll',
    id: 'base-chat',
    chatId: spec.chatId,
    compiledIdentityStacks: await readCompiled(spec.chatId),
  });

  // P4.D103: the envelope arms. Aria is LLM-controlled (the merge path); Sam is
  // user-controlled, so building a stack for Sam returns null (the drop path).
  const arms: Array<[string, string, string]> = [
    ['merge-into-version-current', spec.envelopeChats.mergeIntoCurrent, spec.ariaP],
    ['merge-discards-legacy-bare-map', spec.envelopeChats.mergeIntoLegacy, spec.ariaP],
    ['merge-discards-older-stamp', spec.envelopeChats.mergeIntoOlder, spec.ariaP],
    ['merge-discards-newer-stamp', spec.envelopeChats.mergeIntoNewer, spec.ariaP],
    ['drop-removes-key-from-current', spec.envelopeChats.dropFromCurrent, spec.samP],
    ['drop-writes-nothing-when-key-absent', spec.envelopeChats.dropKeyAbsent, spec.samP],
    ['drop-clears-a-legacy-map', spec.envelopeChats.dropFromLegacy, spec.samP],
    ['drop-writes-nothing-on-a-null-column', spec.envelopeChats.dropFromNullColumn, spec.samP],
  ];
  for (const [id, chatId, participantId] of arms) {
    const target = await repos.chats.findById(chatId);
    if (!target) throw new Error(`envelope chat not found: ${chatId}`);
    await compileIdentityStackForParticipant(target, participantId);
    rows.push({
      kind: 'participant',
      id,
      chatId,
      participantId,
      compiledIdentityStacks: await readCompiled(chatId),
    });
  }

  await closeDatabase();

  for (const row of rows) process.stdout.write(JSON.stringify(row) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`identity-compiler oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
