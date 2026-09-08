/**
 * Prompt-templates ROUTES fixture builder (P4.83) — the test-pepper substrate for
 * `prompt_templates_routes_equivalence`.
 *
 * A **clean provisioned instance** (v4's `initializeDatabase()` over an empty
 * scratch file: migrations run, no seeded content), because the point of the
 * family is what the FIRST list does — v4's `findAllForUser` lazily seeds the 21
 * built-in "Sample Prompts" before it reads. A fixture that already carried them
 * could never measure that.
 *
 * Baked through v4's REAL repositories with pinned ids/timestamps:
 *   - `userA` (the requesting user — `buildRequestContext` looks the row up, so
 *     it must exist or every case 500s with `User not found`) and a bare `userB`;
 *   - ONE user prompt template for `userA`, inserted BEFORE anything else, so
 *     the no-`ORDER BY` rowid order the modal lists in puts a user row FIRST and
 *     the seeded built-ins after it — which is what makes the order comparand
 *     discriminating rather than decorative.
 *
 * Every other shape the family needs is a PER-CASE seed applied by both sides
 * (the `settings-routes` `seedX` precedent), not baked here: a stale built-in, a
 * user template named like a built-in, and a schema-invalid plant. They are
 * per-case because each of them changes what the seeding pass does, and the
 * family needs the un-seeded baseline too.
 *
 * The DB lands in /tmp (env-pointed, uncommitted); the SPEC lands next to this
 * file and is the single shared source of ids/pepper for both differential
 * sides.
 *
 * Regenerate (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_PT_ROUTES_MAIN=/tmp/qt-pt-routes-fixture.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-prompt-templates-routes-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdirSync, mkdtempSync, rmSync, copyFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

const TEST_PEPPER_BASE64 = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const TS = '2020-01-01T00:00:00.000Z';

const SPEC = {
  testPepperBase64: TEST_PEPPER_BASE64,
  ts: TS,
  userA: '5e100000-0000-4000-8000-00000000a001',
  userB: '5e100000-0000-4000-8000-00000000a002',
  /** The baked user template (rowid 1 — the order anchor). */
  userTemplateId: '5e800000-0000-4000-8000-00000000b001',
  userTemplateName: 'My Own Prompt',
  /** Per-case: a built-in already on file under a catalogue name, OLD content. */
  staleBuiltInId: '5e800000-0000-4000-8000-00000000b002',
  /** Per-case: a USER template sharing that catalogue name. */
  userCloneId: '5e800000-0000-4000-8000-00000000b003',
  /** Per-case: a plant that fails `PromptTemplateSchema` (101-code-point name). */
  invalidRowId: '5e800000-0000-4000-8000-00000000b004',
  /** The catalogue name the stale/clone plants use. */
  collidingName: 'MODERN General',
  staleContent: 'OLD CONTENT — v4 never overwrites a sample prompt already on file.',
};

async function main(): Promise<void> {
  const out = process.env.QT_FIXTURE_PT_ROUTES_MAIN;
  if (!out) throw new Error('set QT_FIXTURE_PT_ROUTES_MAIN');

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER_BASE64;
  process.env.LOG_LEVEL = 'error';

  const scratch = mkdtempSync(join(tmpdir(), 'qt-prompt-templates-build-'));
  // The instance lock lives at `<QUILLTAP_DATA_DIR>/data/quilltap.lock`; v4's
  // backend opens it rather than creating the directory.
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main.db');
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'mount.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  await repos.users.create(
    { username: 'usera', email: null, name: 'User A' } as never,
    { id: SPEC.userA, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'userb', email: null, name: 'User B' } as never,
    { id: SPEC.userB, createdAt: TS, updatedAt: TS } as never,
  );

  // The user template — the FIRST prompt_templates row, so it holds rowid 1 and
  // the list's rowid order is measurable against any name/isBuiltIn sort.
  await repos.promptTemplates.create(
    {
      userId: SPEC.userA,
      name: SPEC.userTemplateName,
      content: 'A prompt of my very own.',
      description: null,
      isBuiltIn: false,
      category: null,
      modelHint: null,
      tags: [],
    } as never,
    { id: SPEC.userTemplateId, createdAt: TS, updatedAt: TS } as never,
  );

  await closeDatabase();
  copyFileSync(mainWork, out);
  rmSync(scratch, { recursive: true, force: true });

  const here = dirname(fileURLToPath(import.meta.url));
  writeFileSync(join(here, 'prompt-templates-routes.json'), JSON.stringify(SPEC, null, 2) + '\n');
  process.stderr.write(`built prompt-templates fixture: ${out}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`prompt-templates fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
