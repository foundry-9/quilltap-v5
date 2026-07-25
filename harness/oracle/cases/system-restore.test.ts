/**
 * @jest-environment node
 *
 * P4.9G5 restore ORACLE — drives v4's REAL restore code over the COMMITTED
 * archive family (`crates/quilltap-web/tests/fixtures/restore-archives/`), the
 * same bytes the Rust side reads.
 *
 * ── PART 1: preview (`lib/backup/restore/preview.ts:20`) ─────────────────────
 * `previewRestore(zipPath)` is filesystem-only — it extracts, counts, and
 * cleans up, touching no database. Each case emits either the 41-key
 * `RestoreSummary` or the thrown message, verbatim: the preview route leaks
 * `error.message` to the client (`system/restore/route.ts:176`), so the
 * malformed-archive wording is part of the contract.
 *
 *   preview_full              the whole archive
 *   preview_legacy            + outfit-presets.json and the legacy
 *                             equippedOutfit shape (both parse-time folds)
 *   preview_minimal           every OPTIONAL data file absent — the [] fallbacks
 *   preview_missing_required  data/tags.json absent — `readJsonArrayFile` throws
 *   preview_malformed         no quilltap-backup-* root, no manifest.json
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-sysrestore-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/system-restore.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_RESTORE_ARCHIVES=$V5W/crates/quilltap-web/tests/fixtures/restore-archives \
 *   QT_ORACLE_OUT=/tmp/oracle-system-restore.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-restore
 */

import * as fs from 'fs';
import { join } from 'node:path';

interface PreviewCase {
  name: string;
  archive: string;
}

const PREVIEW_CASES: PreviewCase[] = [
  { name: 'preview_full', archive: 'restore-archive.zip' },
  { name: 'preview_legacy', archive: 'restore-archive-legacy.zip' },
  { name: 'preview_minimal', archive: 'restore-archive-minimal.zip' },
  { name: 'preview_missing_required', archive: 'restore-archive-missing-required.zip' },
  { name: 'preview_malformed', archive: 'restore-archive-malformed.zip' },
];

async function main(): Promise<void> {
  const archives = process.env.QT_RESTORE_ARCHIVES;
  if (!archives) throw new Error('QT_RESTORE_ARCHIVES must point at the committed archive dir');
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  process.env.LOG_LEVEL = 'error';

  const { previewRestore } = await import('@/lib/backup/restore/preview');

  const outLines: string[] = [];
  for (const c of PREVIEW_CASES) {
    const zipPath = join(archives, c.archive);
    if (!fs.existsSync(zipPath)) throw new Error(`missing archive fixture: ${zipPath}`);
    try {
      const preview = await previewRestore(zipPath);
      outLines.push(JSON.stringify({ name: c.name, preview }));
    } catch (error) {
      outLines.push(
        JSON.stringify({
          name: c.name,
          error: error instanceof Error ? error.message : String(error),
        }),
      );
    }
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`system-restore oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('system-restore oracle', async () => {
  await main();
});
