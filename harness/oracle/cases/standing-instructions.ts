/**
 * Oracle case: standing instructions (P4.D103 unit 1, v4 `8f868109`).
 *
 * Drives v4's REAL `lib/chat/context/standing-instructions.ts` over a COPY of
 * the pre-seeded fixture pair:
 *   - `resolve` rows call `resolveStandingInstructions` + the one-shot
 *     `resolveStandingInstructionsSection`, emitting BOTH the resolved source
 *     list (kind/name/instructions, in order) and the rendered section, so a
 *     port that renders correctly from a wrongly-ordered resolve still reddens.
 *   - `render` rows call `renderStandingInstructionsSection` directly with
 *     hand-built sources the resolver can never produce (untrimmed bodies,
 *     all-whitespace entries, names carrying markdown/em dashes).
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SI_MAIN=/tmp/qt-si-main.db QT_FIXTURE_SI_MOUNT=/tmp/qt-si-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/cases/standing-instructions.ts \
 *     > /tmp/oracle-standing-instructions.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { readFileSync, existsSync, mkdtempSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  characters: Record<string, string>;
  projects: Record<string, string>;
  groups: Record<string, string>;
  missingProjectId: string;
  danglingGroupId: string;
}

type WireSource = { kind: 'project' | 'group'; name: string; instructions: string };

type Row =
  | {
      kind: 'resolve';
      id: string;
      projectId: string | null;
      characterId: string | null;
      sources: WireSource[];
      section: string | null;
    }
  | { kind: 'render'; id: string; sources: WireSource[]; out: string | null };

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'standing-instructions.json'), 'utf8'),
  ) as Spec;

  const mainFixture = process.env.QT_FIXTURE_SI_MAIN;
  const mountFixture = process.env.QT_FIXTURE_SI_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_SI_MAIN and QT_FIXTURE_SI_MOUNT must point at the seeded fixtures');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-si-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'si-main-work.db');
  const mountWork = join(scratch, 'si-mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const {
    resolveStandingInstructions,
    resolveStandingInstructionsSection,
    renderStandingInstructionsSection,
  } = await import('@/lib/chat/context/standing-instructions');

  await initializeDatabase();

  const rows: Row[] = [];

  const pushResolve = async (
    id: string,
    projectId: string | null | undefined,
    characterId: string | null | undefined,
  ): Promise<void> => {
    const sources = await resolveStandingInstructions({ projectId, characterId });
    const section = await resolveStandingInstructionsSection({ projectId, characterId });
    rows.push({
      kind: 'resolve',
      id,
      projectId: projectId ?? null,
      characterId: characterId ?? null,
      sources: sources as WireSource[],
      section,
    });
  };

  const { aria, bram, cleo, dane } = spec.characters;
  const { iota, kappa, lambda } = spec.projects;

  // Nothing at all → null section (the byte-identity contract).
  await pushResolve('nothing', null, null);
  await pushResolve('project-only-instructed', iota, null);
  await pushResolve('project-only-null-instructions', kappa, null);
  await pushResolve('project-only-whitespace-instructions', lambda, null);
  await pushResolve('project-missing-row', spec.missingProjectId, null);
  // Empty-string projectId is falsy in v4 — the project leg never runs.
  await pushResolve('project-empty-string-id', '', aria);

  // Groups only: membership insert order (Zephyr, Gamma, Beacon, Delta, Echo,
  // dangling) must come back Beacon / Gamma / Zephyr.
  await pushResolve('groups-only-sorted-against-insert-order', null, aria);
  // Same-name groups → the instructions tie-break (inserted B then A).
  await pushResolve('groups-same-name-instructions-tiebreak', null, bram);
  // No memberships at all.
  await pushResolve('groups-none', null, cleo);
  // ICU vs byte order: `apple` sorts before `Banana` under en-US collation.
  await pushResolve('groups-icu-collation-case', null, dane);
  await pushResolve('character-empty-string-id', iota, '');

  // Both legs: project ALWAYS precedes the sorted groups.
  await pushResolve('project-and-groups', iota, aria);
  await pushResolve('uninstructed-project-with-groups', kappa, aria);
  await pushResolve('instructed-project-uninstructed-character', iota, cleo);

  // --- Pure render rows -----------------------------------------------------
  const pushRender = (id: string, sources: WireSource[]): void => {
    rows.push({ kind: 'render', id, sources, out: renderStandingInstructionsSection(sources) });
  };

  pushRender('render-empty-list', []);
  pushRender('render-all-whitespace-drops-to-null', [
    { kind: 'group', name: 'Ghost', instructions: '   \n\t ' },
  ]);
  pushRender('render-mixed-whitespace-dropped', [
    { kind: 'project', name: 'Iota', instructions: '\n' },
    { kind: 'group', name: 'Gamma', instructions: 'Kept.' },
  ]);
  pushRender('render-untrimmed-body-trimmed', [
    { kind: 'group', name: 'Gamma', instructions: '\n\n  Trim me.  \n\n' },
  ]);
  pushRender('render-name-with-em-dash-and-markdown', [
    { kind: 'project', name: 'Iota — **bold** name', instructions: 'Body.' },
  ]);
  pushRender('render-multiline-body-kept-verbatim', [
    { kind: 'group', name: 'Gamma', instructions: 'One.\n\nTwo.\n- three' },
  ]);
  pushRender('render-group-before-project-order-is-caller-order', [
    { kind: 'group', name: 'Gamma', instructions: 'Group first.' },
    { kind: 'project', name: 'Iota', instructions: 'Project second.' },
  ]);
  pushRender('render-template-syntax-passed-through', [
    { kind: 'group', name: 'Gamma', instructions: '{{char}} greets {{user}}.' },
  ]);

  await closeDatabase();

  for (const row of rows) process.stdout.write(JSON.stringify(row) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`standing-instructions oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
