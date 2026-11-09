/**
 * Tier-2 (real-DB) oracle — P4.D378 item 12: v4's REAL `assembleCorePacket`
 * (`lib/services/aurora-notifications/core-whisper.ts`) at `624d0d25c`, whose
 * group tier (`assembleGroupCoreFiles`) walks the shared `groupMountsFor`
 * (v4 `0c47f4539`): membership order, a store shared by two groups credited
 * to the first, the character's own vault filtered out, a membership whose
 * group row is ABSENT listed with name `''` (sorting FIRST).
 *
 * Opens a COPY of the pair built by `fixtures/build-core-packet-fixture.ts` and
 * runs `core-packet.json::cases` (one character per row), emitting per row the
 * packet's `files[]` (`path`, `body`, `sourceLabel` — `null` when absent) and
 * `approxTokens`, or `packet: null`. A row with `hydratedProbeGroupIds` also
 * emits the R-F probe (the hydrated `groups.findById` throws, the raw row is
 * whole), measured in-case before the packet is assembled.
 *
 * Run (Node 24, from the v4 checkout), AFTER building the pair:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_COREPACKET_MAIN=/tmp/qt-corepacket-main.db \
 *   QT_FIXTURE_COREPACKET_MOUNT=/tmp/qt-corepacket-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/cases/core-packet.ts > /tmp/oracle-core-packet.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CaseSpec {
  name: string;
  characterId: string;
  budget: number;
  hydratedProbeGroupIds?: string[];
}

interface Spec {
  testPepperBase64: string;
  cases: CaseSpec[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'core-packet.json'), 'utf8')
  ) as Spec;

  const fxMain = process.env.QT_FIXTURE_COREPACKET_MAIN;
  const fxMount = process.env.QT_FIXTURE_COREPACKET_MOUNT;
  if (!fxMain || !existsSync(fxMain) || !fxMount || !existsSync(fxMount)) {
    throw new Error('QT_FIXTURE_COREPACKET_MAIN / _MOUNT must point at the built pair');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-corepacket-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'corepacket-main-work.db');
  const mountWork = join(scratch, 'corepacket-mount-work.db');
  copyFileSync(fxMain, mainWork);
  copyFileSync(fxMount, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = join(scratch, 'llm-logs.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { assembleCorePacket } = await import('@/lib/services/aurora-notifications/core-whisper');

  await initializeDatabase();
  const repos = getRepositories();

  const rows: unknown[] = [];
  for (const c of spec.cases) {
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

    const packet = await assembleCorePacket(c.characterId, c.budget);
    const out = packet
      ? {
          files: packet.files.map((f) => ({
            path: f.path,
            body: f.body,
            sourceLabel: f.sourceLabel ?? null,
          })),
          approxTokens: packet.approxTokens,
        }
      : null;
    rows.push(probe ? { name: c.name, packet: out, probe } : { name: c.name, packet: out });
  }

  await closeDatabase();
  for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`core-packet oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
