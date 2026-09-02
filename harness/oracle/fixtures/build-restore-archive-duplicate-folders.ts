/**
 * P4.D145 restore-fixture builder — the ONE archive bug 114's drop arm needs.
 *
 * ── WHY A NEW ARCHIVE ────────────────────────────────────────────────────────
 * v4 `a5df98b3f` teaches the restore folder loop to drop a duplicate row
 * QUIETLY: a backup taken before `collapse-duplicate-folders-v1` ran can carry
 * many rows for one `(userId, projectId, path)`, the unique index rejects the
 * extras, and the first one restored is the survivor. No warning, no skipped
 * counter, `foldersRestored` simply not incremented.
 *
 * **None of the eleven committed archives can see it.** Measured 2026-09-02:
 * every one carries exactly ONE `folders` row (`/notes`), so there is no second
 * row for the index to reject — the arm is structurally invisible, the P4.D31
 * lesson exactly.
 *
 * ── WHY IT IS NOT BUILT BY `createBackup` ────────────────────────────────────
 * The same reason `build-restore-archive-legacy-profiles.ts` gives: a backup of
 * an instance that still HAS duplicates is not a thing a post-collapse v4 can
 * produce, because the collapse runs at boot and the index then forbids them.
 * The honest fixture is a DERIVATION: a v4-written archive with its
 * `data/folders.json` replaced by the rows a pre-collapse instance actually
 * held. Everything else in the zip — the layout, the manifest, every other data
 * file — is v4's own bytes, and the repackaging uses the same `zip -r <out>
 * <folder>` shell call `backup-service.ts:800` uses.
 *
 * ── THE SIX ROWS ─────────────────────────────────────────────────────────────
 *   1 `…0001` `/notes`         general   → RESTORED (first in, the survivor)
 *   2 `…0002` `/notes`         general   → dropped quietly (UNIQUE)
 *   3 `…0003` `/notes`         general   → dropped quietly (UNIQUE)
 *   4 `…0004` `/archive/`      general   → RESTORED (a different path)
 *   5 `…0005` `/notes`         project   → RESTORED. The index coalesces a NULL
 *                                          projectId to '', so "no project" is
 *                                          ONE value — but a real project id is
 *                                          a different one, and this row must
 *                                          survive. A `projectId`-blind index
 *                                          would drop it.
 *   6 `…0001` `/duplicate-id/` general   → dropped quietly (PRIMARY KEY). v4's
 *                                          predicate names
 *                                          `SQLITE_CONSTRAINT_PRIMARYKEY` in its
 *                                          own doc comment, so the whole
 *                                          constraint family takes the quiet
 *                                          path, not just UNIQUE.
 *
 * `foldersRestored` must therefore be 3, and `warnings` must gain NOTHING.
 *
 * The row order in the file IS the restore order (v4 and v5 both iterate the
 * parsed array), so "first in wins" is a property of the fixture, not luck.
 *
 * Run (Node 24, from the PINNED v4 worktree — see the lane record):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   PIN=/tmp/qt-v4-pin-p4d145-a5df98b3f
 *   cd "$PIN"
 *   QT_ARCHIVE_DIR=$V5W/crates/quilltap-web/tests/fixtures/restore-archives \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-restore-archive-duplicate-folders.ts
 *
 * (The transform needs no v4 code at all — it is `unzip`, a JSON rewrite and
 * `zip`. The pinned cwd is kept for consistency with the rest of the family.)
 */

import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, readdirSync, copyFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const BASE_ARCHIVE = 'restore-archive.zip';
const OUT_ARCHIVE = 'restore-archive-duplicate-folders.zip';

/** The user every committed archive belongs to (`system-data.json`). */
const USER_ID = 'e18e05bc-63e8-4539-8a85-719b7a508850';
/** `restore-archive.zip`'s own project ("The Voyage"). */
const PROJECT_ID = 'a3000000-0000-4000-8000-000000000001';
const STAMP = '2026-03-01T00:00:00.000Z';

function folder(
  id: string,
  path: string,
  name: string,
  projectId: string | null,
): Record<string, unknown> {
  return {
    id,
    userId: USER_ID,
    path,
    name,
    parentFolderId: null,
    projectId,
    createdAt: STAMP,
    updatedAt: STAMP,
  };
}

const FOLDERS: Array<Record<string, unknown>> = [
  folder('a9000000-0000-4000-8000-000000000001', '/notes', 'Notes', null),
  folder('a9000000-0000-4000-8000-000000000002', '/notes', 'Notes', null),
  folder('a9000000-0000-4000-8000-000000000003', '/notes', 'Notes', null),
  folder('a9000000-0000-4000-8000-000000000004', '/archive/', 'Archive', null),
  folder('a9000000-0000-4000-8000-000000000005', '/notes', 'Notes', PROJECT_ID),
  // Re-uses row 1's id on a different path: a PRIMARY KEY violation.
  folder('a9000000-0000-4000-8000-000000000001', '/duplicate-id/', 'Duplicate Id', null),
];

function main(): void {
  const dir = process.env.QT_ARCHIVE_DIR;
  if (!dir) throw new Error('set QT_ARCHIVE_DIR to the committed restore-archives directory');

  const base = join(dir, BASE_ARCHIVE);
  const scratch = mkdtempSync(join(tmpdir(), 'qt-duplicate-folders-'));
  try {
    execFileSync('unzip', ['-q', '-o', base, '-d', scratch]);
    const roots = readdirSync(scratch).filter((n) => n.startsWith('quilltap-backup-'));
    if (roots.length !== 1) {
      throw new Error(`expected exactly one backup root in ${BASE_ARCHIVE}, saw ${roots.length}`);
    }
    const root = roots[0];

    // Sanity: the base archive is the shape this derivation assumes — one
    // folder, and the project row row 5 points at.
    const foldersPath = join(scratch, root, 'data', 'folders.json');
    const existing = JSON.parse(readFileSync(foldersPath, 'utf8')) as Array<
      Record<string, unknown>
    >;
    if (existing.length !== 1) {
      throw new Error(`expected 1 folder in ${BASE_ARCHIVE}, saw ${existing.length}`);
    }
    const projects = JSON.parse(
      readFileSync(join(scratch, root, 'data', 'projects.json'), 'utf8'),
    ) as Array<{ id: string }>;
    if (!projects.some((p) => p.id === PROJECT_ID)) {
      throw new Error(`${BASE_ARCHIVE} no longer carries project ${PROJECT_ID}`);
    }

    writeFileSync(foldersPath, `${JSON.stringify(FOLDERS, null, 2)}\n`);

    const manifestPath = join(scratch, root, 'manifest.json');
    const manifest = JSON.parse(readFileSync(manifestPath, 'utf8')) as {
      counts: Record<string, number>;
    };
    manifest.counts.folders = FOLDERS.length;
    writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

    // The same call `backup-service.ts:800` makes, from the same kind of cwd.
    const staged = join(scratch, `${root}.zip`);
    execFileSync('zip', ['-q', '-r', staged, root], { cwd: scratch });
    const out = join(dir, OUT_ARCHIVE);
    copyFileSync(staged, out);
    process.stderr.write(`  ${OUT_ARCHIVE}  ${readFileSync(out).length} bytes\n`);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

main();
