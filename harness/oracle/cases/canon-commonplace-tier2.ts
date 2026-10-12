/**
 * Tier-2 ORACLE for the Commonplace canon reads (v4 `d58548051`,
 * `lib/memory/cheap-llm-tasks/canon.ts`): `loadCommonplaceCanon`,
 * `loadCanonForSelfWithCommonplace` and `loadCanonForObserverAboutSubject`
 * (with and without `includeCommonplace`), each followed by the matching
 * REAL renderer, over a vault fixture built by
 * `build-canon-commonplace-fixture.ts`. Mirrors v4's own
 * `lib/memory/__tests__/canon-commonplace.test.ts:102-207` against the REAL
 * vault overlay instead of a mocked `readVaultTextFile`.
 *
 * Per op it emits the loader's returned value (a `CanonSource`, a `SelfCanon`
 * or the digest text — key presence as v4 builds it: `commonplace` only when
 * truthy), the rendered block, and the `[Canon]` log lines the op emitted,
 * captured by splicing a transport into the singleton logger (memory note
 * `v4-oracle-job-host-and-log-capture`).
 *
 * Run from the v4 server checkout under Node 24 (after the builder):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-canon-commonplace-main.db \
 *   QT_FIXTURE_MOUNT_OUT=/tmp/qt-canon-commonplace-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-canon-commonplace-fixture.ts
 *   QT_FIXTURE_CANON_MAIN=/tmp/qt-canon-commonplace-main.db \
 *   QT_FIXTURE_CANON_MOUNT=/tmp/qt-canon-commonplace-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/cases/canon-commonplace-tier2.ts \
 *     > /tmp/oracle-canon-commonplace.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

type Mount = 'vault' | 'none';
interface SubjectSpec {
  id: string;
  name: string;
  identity?: string | null;
  description?: string | null;
}
interface OpSpec {
  op: 'other' | 'self' | 'commonplace';
  name: string;
  mount: Mount;
  observer?: string;
  subject?: SubjectSpec | 'self';
  includeCommonplace?: boolean;
  character?: {
    id: string;
    name: string;
    manifesto: string | null;
    personality: string | null;
    description: string | null;
    identity: string | null;
  };
  holder?: string;
}
interface Spec {
  testPepperBase64: string;
  ops: OpSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'canon-commonplace-tier2.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_CANON_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_CANON_MOUNT;
  if (!fixtureMain || !fixtureMount) {
    throw new Error('QT_FIXTURE_CANON_MAIN and QT_FIXTURE_CANON_MOUNT must point at the built fixture');
  }

  // Read-only ops, but work on copies so the shared fixture stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-canon-commonplace-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'main.db');
  const workMount = join(scratch, 'mount.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  // DEBUG so `[Canon] Commonplace digest loaded into canon` reaches the
  // capture; the console transport is spliced out, so nothing reaches stdout.
  process.env.LOG_LEVEL = 'debug';

  const { logger } = await import('@/lib/logger');
  const captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as {
        level: string;
        message: string;
        context?: Record<string, unknown>;
        error?: { message: string };
      };
      if (!data.message.startsWith('[Canon]')) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error) fields.error = data.error.message;
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const canon = await import('@/lib/memory/cheap-llm-tasks/canon');

  await initializeDatabase();
  const repos = getRepositories();
  const mountOf = async (characterId: string, mount: Mount): Promise<string | null> => {
    if (mount === 'none') return null;
    const raw = await repos.characters.findByIdRaw(characterId);
    return raw?.characterDocumentMountPointId ?? null;
  };

  const out: string[] = [];
  for (const op of spec.ops) {
    captured.length = 0;
    let value: unknown;
    let block: string | null = null;
    if (op.op === 'other') {
      const observer = { characterId: op.observer!, mountPointId: await mountOf(op.observer!, op.mount) };
      const s = op.subject as SubjectSpec;
      const source = await canon.loadCanonForObserverAboutSubject(
        observer,
        { id: s.id, name: s.name, identity: s.identity ?? null, description: s.description ?? null },
        op.includeCommonplace === undefined ? undefined : { includeCommonplace: op.includeCommonplace },
      );
      value = source;
      block = canon.renderOtherCanonBlock(source);
    } else if (op.op === 'self') {
      const c = op.character!;
      const self = await canon.loadCanonForSelfWithCommonplace({
        ...c,
        mountPointId: await mountOf(c.id, op.mount),
      });
      value = self;
      block = canon.renderSelfCanonBlock(self);
    } else {
      const holder = { characterId: op.holder!, mountPointId: await mountOf(op.holder!, op.mount) };
      value = await canon.loadCommonplaceCanon(holder, op.subject as 'self' | { id: string; name: string });
    }
    out.push(
      JSON.stringify({
        name: op.name,
        // `undefined`-valued keys vanish in JSON exactly as v4's spread omits them.
        value: value ?? null,
        block,
        logs: captured.map((c) => ({ ...c })),
      })
    );
  }

  await closeDatabase();
  process.stdout.write(out.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`canon-commonplace oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
