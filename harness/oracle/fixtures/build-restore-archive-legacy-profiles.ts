/**
 * P4.D126 restore-fixture builder — the ONE archive bug 103 needs.
 *
 * ── WHY A NEW ARCHIVE ────────────────────────────────────────────────────────
 * v4 `e000d6bfc` (bug 103) seeds the two `connection_profiles` columns an OLDER
 * archive cannot carry. A key absent from the archive is absent from the INSERT,
 * so SQLite applies the table DEFAULT — the right answer for a brand-new row and
 * the wrong one for a profile whose owner made a choice before the column
 * existed.
 *
 * **None of the ten committed archives can see it.** Measured 2026-08-26: every
 * one of them carries exactly one connection profile, `OPENAI_COMPATIBLE`, with
 * `supportsImageUpload: false` STORED and `multiCharacterPrefill` ABSENT. So:
 *
 *   - the `supportsImageUpload` seeding arm is invisible — the key is present,
 *     and the provider is not in the historic capability map anyway;
 *   - the `multiCharacterPrefill` arm is invisible on a FRESH target, because
 *     `generateDDL` declares that column with **no DEFAULT** (only the migration
 *     `add-profile-multi-character-prefill-field-v1` adds `DEFAULT 1`), so
 *     omitting the column and writing an explicit NULL land the same cell. It
 *     is visible only on a migration-vintage instance — which is where
 *     `restore_vintage_state` pins it, not here.
 *
 * ── WHY IT IS NOT BUILT BY `createBackup` ────────────────────────────────────
 * Every other archive in the family is written by v4's REAL backup writer, and
 * that is exactly why none of them can express this: a modern instance HAS both
 * columns, so v4's writer emits both keys (or omits `multiCharacterPrefill`
 * only when it is NULL). **An archive older than a column is not a thing v4 can
 * still produce.** The honest fixture is therefore a DERIVATION: a v4-written
 * archive with its `data/connection-profiles.json` replaced by records in the
 * shapes an older Quilltap actually wrote. Everything else in the zip — the
 * layout, the manifest, every other data file — is v4's own bytes, and the
 * repackaging uses the same `zip -r <out> <folder>` shell call
 * `backup-service.ts:800` uses.
 *
 * Both engines then read the SAME derived bytes, which is the claim the restore
 * family exists to make.
 *
 * ── THE SIX PROFILES ─────────────────────────────────────────────────────────
 * Mirrors v4's own `restore-field-fidelity.test.ts` 4.9 block, plus the two arms
 * a state diff can carry that a mock cannot:
 *
 *   1 `Carried Both`      OPENAI    sIU: true STORED, mCP: false STORED
 *                         → neither key is touched (v4 case 1)
 *   2 `Prefill Predates`  ANTHROPIC sIU: true STORED, mCP ABSENT
 *                         → mCP becomes an explicit null (v4 case 2)
 *   3 `Both Predate`      ANTHROPIC both ABSENT
 *                         → sIU seeded TRUE from the historic map (v4 case 3)
 *   4 `Never Capable`     OLLAMA    both ABSENT
 *                         → sIU seeded FALSE (v4 case 4)
 *   5 `Stored False`      GOOGLE    sIU: false STORED, mCP: null STORED
 *                         → NEITHER is touched, though both are falsy. This is
 *                           the arm that separates v4's `=== undefined` test
 *                           from a truthiness test: a `!value` seeding condition
 *                           would flip a GOOGLE profile whose owner deliberately
 *                           turned vision OFF back on.
 *   6 `Lowercase Legacy`  "openai"  both ABSENT
 *                         → sIU seeded TRUE. v4's map is matched
 *                           case-INSENSITIVELY (`(provider ?? '').toUpperCase()`)
 *                           because `ProviderEnum` is an open string and nothing
 *                           guarantees the stored casing, least of all in an
 *                           archive old enough to be missing the column.
 *
 * `supportsImageUpload` is never written as an explicit `null`: v4's schema is
 * `z.boolean().default(false)`, so a stored null fails the parse rather than
 * reaching the seeding — not a shape any Quilltap ever wrote.
 *
 * Run (Node 24, from the PINNED v4 worktree — see the lane record):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   PIN=/tmp/qt-v4-pin-p4d126-8872d7efc
 *   cd "$PIN"
 *   QT_ARCHIVE_DIR=$V5W/crates/quilltap-web/tests/fixtures/restore-archives \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-restore-archive-legacy-profiles.ts
 *
 * (The transform needs no v4 code at all — it is `unzip`, a JSON rewrite and
 * `zip`. The pinned cwd is kept for consistency with the rest of the family.)
 */

import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, readdirSync, copyFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const BASE_ARCHIVE = 'restore-archive-minimal.zip';
const OUT_ARCHIVE = 'restore-archive-legacy-profiles.zip';

/** The user every committed archive belongs to (`system-data.json`). */
const USER_ID = 'e18e05bc-63e8-4539-8a85-719b7a508850';
const STAMP = '2026-03-01T00:00:00.000Z';

/** A base record with everything the schema requires and nothing it defaults. */
function profile(
  id: string,
  name: string,
  provider: string,
  extra: Record<string, unknown>,
): Record<string, unknown> {
  return {
    id,
    userId: USER_ID,
    name,
    provider,
    modelName: 'legacy-model',
    createdAt: STAMP,
    updatedAt: STAMP,
    ...extra,
  };
}

const PROFILES: Array<Record<string, unknown>> = [
  profile('cd000001-0000-4000-8000-000000000001', 'Carried Both', 'OPENAI', {
    supportsImageUpload: true,
    multiCharacterPrefill: false,
  }),
  profile('cd000001-0000-4000-8000-000000000002', 'Prefill Predates', 'ANTHROPIC', {
    supportsImageUpload: true,
  }),
  profile('cd000001-0000-4000-8000-000000000003', 'Both Predate', 'ANTHROPIC', {}),
  profile('cd000001-0000-4000-8000-000000000004', 'Never Capable', 'OLLAMA', {}),
  profile('cd000001-0000-4000-8000-000000000005', 'Stored False', 'GOOGLE', {
    supportsImageUpload: false,
    multiCharacterPrefill: null,
  }),
  profile('cd000001-0000-4000-8000-000000000006', 'Lowercase Legacy', 'openai', {}),
];

function main(): void {
  const dir = process.env.QT_ARCHIVE_DIR;
  if (!dir) throw new Error('set QT_ARCHIVE_DIR to the committed restore-archives directory');

  const base = join(dir, BASE_ARCHIVE);
  const scratch = mkdtempSync(join(tmpdir(), 'qt-legacy-profiles-'));
  try {
    execFileSync('unzip', ['-q', '-o', base, '-d', scratch]);
    const roots = readdirSync(scratch).filter((n) => n.startsWith('quilltap-backup-'));
    if (roots.length !== 1) {
      throw new Error(`expected exactly one backup root in ${BASE_ARCHIVE}, saw ${roots.length}`);
    }
    const root = roots[0];

    // Sanity: the base archive is the shape this derivation assumes.
    const profilesPath = join(scratch, root, 'data', 'connection-profiles.json');
    const existing = JSON.parse(readFileSync(profilesPath, 'utf8')) as Array<
      Record<string, unknown>
    >;
    if (existing.length !== 1) {
      throw new Error(`expected 1 profile in ${BASE_ARCHIVE}, saw ${existing.length}`);
    }

    writeFileSync(profilesPath, `${JSON.stringify(PROFILES, null, 2)}\n`);

    const manifestPath = join(scratch, root, 'manifest.json');
    const manifest = JSON.parse(readFileSync(manifestPath, 'utf8')) as {
      counts: Record<string, number>;
    };
    manifest.counts.connectionProfiles = PROFILES.length;
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
