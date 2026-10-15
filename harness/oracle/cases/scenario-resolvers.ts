/**
 * Read-differential ORACLE for the per-kind scenario reads (P4.4 unit 2,
 * sub-unit 1; REWRITTEN by P4.D295 — L-8).
 *
 * v4 `545e299bb` deleted the per-tier resolvers this case used to drive
 * (`resolveGeneralScenarioBody` / `resolveProjectScenarioBody`, from the deleted
 * `lib/mount-index/{general,project}-scenarios.ts`) for ONE chokepoint, the
 * scenario location (`lib/scenarios/location.ts`). This case drives its REAL
 * replacements over the SAME seeded fixtures: General's read through
 * `resolveScenarioLocation('general', null, …).read(path)` (the scenario BODY,
 * or null), and a ref through `lookupScenarioRef({ mountPointId, path }, …)` —
 * plus the path rows the pre-span resolvers never had: `..`, a nested path,
 * empty, a `%`-escape decoded, and a malformed `%` (v4 THROWS `URIError` —
 * R-Z). The fixture's "project" store has no owner row, so a ref into it is
 * REFUSED (`That store does not hold scenarios`) — the location's classify.
 * The Rust port (`scenarios::location`) reads the SAME baked fixtures and must
 * produce the same result exactly.
 *
 * Run (Node 24, from the v4 checkout or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=${V5:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SCEN_MAIN=/tmp/qt-scen-main.db QT_FIXTURE_SCEN_MOUNT=/tmp/qt-scen-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/scenario-resolvers.ts > /tmp/oracle-scen.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, rmSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  generalMountPointId: string;
  projectMountPointId: string;
}

/** The location only reads the user for a character; any id serves here. */
const USER = 'f0000000-0000-4000-8000-00000000000f';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'scenario-resolvers.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_SCEN_MAIN;
  const mountFixture = process.env.QT_FIXTURE_SCEN_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_SCEN_MAIN and QT_FIXTURE_SCEN_MOUNT must point at the seeded fixtures');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-scen-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'scen-main-work.db');
  const mountWork = join(scratch, 'scen-mount-work.db');
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
  const { resolveScenarioLocation, lookupScenarioRef } = await import('@/lib/scenarios/location');

  await initializeDatabase();
  const repos = getRepositories();
  const general = await resolveScenarioLocation('general', null, repos, USER);
  if (!general) throw new Error('the fixture General store did not resolve');

  const rows: Array<Record<string, unknown>> = [];
  const threw = (e: unknown) => ({ threw: e instanceof Error ? `${e.name}: ${e.message}` : String(e) });
  const gen = async (id: string, path: string) => {
    try {
      const s = await general.read(path);
      rows.push({ id, op: 'general', path, body: s ? s.body : null });
    } catch (e) {
      rows.push({ id, op: 'general', path, ...threw(e) });
    }
  };
  const ref = async (id: string, mountPointId: string, path: string) => {
    try {
      const r = await lookupScenarioRef({ mountPointId, path }, repos, USER);
      rows.push({
        id,
        op: 'ref',
        mountPointId,
        path,
        status: r.status,
        ...(r.status === 'refused' && { reason: r.reason }),
        ...(r.status === 'missing' && { missingPath: r.path }),
        ...(r.status === 'ok' && { body: r.scenario.body, scenarioPath: r.scenario.path }),
      });
    } catch (e) {
      rows.push({ id, op: 'ref', mountPointId, path, ...threw(e) });
    }
  };

  // The pre-span matrix, through the location.
  await gen('general_bare', 'welcome');
  await gen('general_full', 'Scenarios/welcome.md');
  await gen('general_nomd', 'Scenarios/welcome');
  await gen('general_leading_slash', '/welcome');
  await gen('general_missing', 'nonexistent');
  await gen('general_empty', 'empty');
  // The rows the pre-span resolvers never had (a bad path is NotFound on a
  // location read; a malformed escape THROWS).
  await gen('general_dotdot', '../welcome.md');
  await gen('general_nested', 'Scenarios/a/welcome.md');
  await gen('general_blank_path', '   ');
  await gen('general_percent_decoded', 'welc%6Fme');
  await gen('general_malformed_percent', 'welc%E0me');
  // Refs: General by ref; the ownerless "project" store refuses.
  await ref('ref_general_ok', spec.generalMountPointId, 'Scenarios/welcome.md');
  await ref('ref_general_missing', spec.generalMountPointId, 'nope');
  await ref('ref_general_empty_body_missing', spec.generalMountPointId, 'empty');
  await ref('ref_general_bad_path', spec.generalMountPointId, 'Scenarios/../x.md');
  await ref('ref_general_percent_decoded', spec.generalMountPointId, 'Scenarios%2Fwelcome.md');
  await ref('ref_general_malformed_percent', spec.generalMountPointId, '%E0');
  await ref('ref_ownerless_store_refused', spec.projectMountPointId, 'Scenarios/tavern.md');

  await closeDatabase();

  for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`scenario-resolvers oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
