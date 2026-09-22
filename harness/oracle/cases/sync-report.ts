/**
 * Tier-1 oracle case — the `quilltap sync` REPORT RENDERER (v4
 * `packages/quilltap/lib/sync-report.js`, added at `23da0b322`).
 *
 * Drives v4's REAL module and emits one NDJSON row per probe. The renderer is
 * deliberately pure — actions and a colour switch in, lines out — so the whole
 * surface is a table: the fixed 8/6 column widths, the capped path column, the
 * em dash a side-less action takes, the trailing slash on a folder, the
 * parenthetical's `sha …, size` tail, the summary's conditional clauses, and
 * the 0/1/2 exit code.
 *
 * Colour is recorded BOTH ways. v4 emits raw ANSI escapes and pads INSIDE them,
 * so the coloured form is not simply the plain form with escapes wrapped around
 * it — a port that reasoned about the widths rather than reproducing them would
 * be wrong only in the coloured half, which is the half a human ever sees.
 *
 * The module is plain CommonJS in the launcher package rather than something
 * `@/lib` resolves, so it is loaded by absolute path through `createRequire`
 * against the checkout's cwd (the memory note: a case outside the v4 tree
 * cannot bare-import a v4 dependency).
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   npx tsx $V5W/harness/oracle/cases/sync-report.ts > /tmp/oracle-sync-report.ndjson
 */

import { createRequire } from 'module';
import { join } from 'path';

const require_ = createRequire(join(process.cwd(), 'noop.js'));
const report = require_(join(process.cwd(), 'packages/quilltap/lib/sync-report.js')) as {
  formatActionLine: (action: unknown, colour: boolean, pathWidth?: number) => string;
  formatActionLines: (actions: unknown[], colour: boolean) => string[];
  formatSummary: (summary: unknown, elapsedMs: number, dryRun: boolean) => string;
  exitCodeFor: (summary: unknown) => number;
  detailFor: (action: unknown) => string;
  displayPath: (action: unknown) => string;
  formatBytes: (n: number | undefined | null) => string;
};

const emit = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');

// ---------------------------------------------------------------------------
// formatBytes
// ---------------------------------------------------------------------------

const BYTE_INPUTS: Array<number | null> = [
  0,
  1,
  512,
  1023,
  1024,
  1536,
  10240,
  1048575,
  1048576,
  1572864,
  1073741823,
  1073741824,
  1610612736,
  1099511627776,
  // The `.toFixed` rounding boundary: 1023.95 KB renders as "1024.0 KB", not
  // "1.0 MB" — the scale is chosen before the rounding.
  1048524,
  null,
];

for (const n of BYTE_INPUTS) {
  emit({ kind: 'format-bytes', id: String(n), n, out: report.formatBytes(n as number) });
}

// ---------------------------------------------------------------------------
// action lines
// ---------------------------------------------------------------------------

const SHA = 'abcdef0123456789'.repeat(4);

interface Action {
  kind: string;
  side: string | null;
  relativePath: string;
  entryKind: string;
  reason?: string;
  sha256?: string;
  sizeBytes?: number;
  outcome?: string;
  error?: string;
  description?: string;
  lastModified?: string;
}

const ACTIONS: Array<{ id: string; action: Action }> = [
  {
    id: 'create-disk-with-sha-and-size',
    action: {
      kind: 'create', side: 'disk', relativePath: 'chapters/01.md', entryKind: 'file',
      sha256: SHA, sizeBytes: 2048, outcome: 'planned',
    },
  },
  {
    id: 'modify-store-with-a-reason',
    action: {
      kind: 'modify', side: 'store', relativePath: 'chapters/03.md', entryKind: 'file',
      reason: 'disk newer by 2h 14m', sha256: SHA, sizeBytes: 900, outcome: 'planned',
    },
  },
  {
    id: 'mkdir-disk-a-folder-gets-a-slash',
    action: { kind: 'mkdir', side: 'disk', relativePath: 'lore/maps', entryKind: 'folder', outcome: 'planned' },
  },
  {
    id: 'rmdir-store-a-folder-with-a-reason',
    action: {
      kind: 'rmdir', side: 'store', relativePath: 'drafts', entryKind: 'folder',
      reason: 'deleted on disk since last sync', outcome: 'planned',
    },
  },
  {
    id: 'conflict-has-no-side',
    action: {
      kind: 'conflict', side: null, relativePath: 'ch.md', entryKind: 'file',
      reason: 'both sides changed; --prefer to resolve', outcome: 'skipped',
    },
  },
  {
    id: 'skip-has-no-side-either',
    action: {
      kind: 'skip', side: null, relativePath: 'a.md', entryKind: 'file',
      reason: '--direction to-disk', outcome: 'skipped',
    },
  },
  {
    id: 'touch-disk',
    action: {
      kind: 'touch', side: 'disk', relativePath: 'ch.md', entryKind: 'file',
      reason: 'mtime 2026-09-20T10:00:00.000Z', outcome: 'planned',
    },
  },
  {
    id: 'describe-disk',
    action: {
      kind: 'describe', side: 'disk', relativePath: 'lore/harbour.png', entryKind: 'file',
      description: 'A map.', outcome: 'planned',
    },
  },
  {
    id: 'delete-store',
    action: {
      kind: 'delete', side: 'store', relativePath: 'gone.md', entryKind: 'file',
      reason: 'deleted on disk since last sync', outcome: 'planned',
    },
  },
  {
    id: 'a-failure-takes-the-error-not-the-reason',
    action: {
      kind: 'create', side: 'disk', relativePath: 'bad.md', entryKind: 'file',
      reason: 'this reason is not printed', sha256: SHA, sizeBytes: 12,
      outcome: 'failed', error: 'EACCES: permission denied',
    },
  },
  {
    id: 'a-failure-with-no-error-falls-back-to-the-reason',
    action: {
      kind: 'create', side: 'disk', relativePath: 'bad2.md', entryKind: 'file',
      reason: 'a reason', outcome: 'failed',
    },
  },
  {
    id: 'a-create-with-no-sha-carries-no-tail',
    action: { kind: 'create', side: 'disk', relativePath: 'nosha.md', entryKind: 'file', outcome: 'planned' },
  },
  {
    id: 'a-create-with-a-sha-and-no-size',
    action: { kind: 'create', side: 'disk', relativePath: 'nosize.md', entryKind: 'file', sha256: SHA, outcome: 'planned' },
  },
  {
    id: 'a-touch-with-a-sha-carries-no-tail',
    action: { kind: 'touch', side: 'disk', relativePath: 'x.md', entryKind: 'file', sha256: SHA, sizeBytes: 5, outcome: 'planned' },
  },
  {
    id: 'an-unknown-kind-takes-no-colour',
    action: { kind: 'wibble', side: 'disk', relativePath: 'x.md', entryKind: 'file', reason: 'r', outcome: 'planned' },
  },
  {
    id: 'a-path-longer-than-the-action-column',
    action: {
      kind: 'modify', side: 'store', entryKind: 'file',
      relativePath: 'a/very/deeply/nested/directory/structure/with/a/long/name.md',
      reason: 'store newer by 3d 4h', sha256: SHA, sizeBytes: 1048576, outcome: 'planned',
    },
  },
  {
    id: 'a-path-with-no-detail-is-not-padded',
    action: { kind: 'touch', side: 'store', relativePath: 'short.md', entryKind: 'file', outcome: 'planned' },
  },
];

for (const { id, action } of ACTIONS) {
  for (const colour of [false, true]) {
    for (const pathWidth of [0, 20, 44]) {
      emit({
        kind: 'action-line',
        id: `${id}/colour=${colour}/width=${pathWidth}`,
        action,
        colour,
        pathWidth,
        out: report.formatActionLine(action, colour, pathWidth),
      });
    }
  }
  emit({
    kind: 'action-parts',
    id,
    action,
    detailFor: report.detailFor(action),
    displayPath: report.displayPath(action),
  });
}

// ---------------------------------------------------------------------------
// the whole plan, so the shared path column is measured
// ---------------------------------------------------------------------------

const PLANS: Array<{ id: string; actions: Action[] }> = [
  { id: 'empty', actions: [] },
  {
    id: 'mixed',
    actions: [
      ACTIONS[0].action, ACTIONS[1].action, ACTIONS[2].action, ACTIONS[4].action,
      ACTIONS[6].action, ACTIONS[9].action,
    ],
  },
  {
    // Every action carries a detail and one path is far past the cap, so the
    // column is the CAP and not the longest path.
    id: 'one-outlier-caps-the-column',
    actions: [ACTIONS[1].action, ACTIONS[15].action],
  },
  {
    // No action carries a detail, so the width stays 0 and nothing is padded.
    id: 'no-details-means-no-padding',
    actions: [ACTIONS[11].action, ACTIONS[16].action],
  },
];

for (const { id, actions } of PLANS) {
  for (const colour of [false, true]) {
    emit({
      kind: 'action-lines',
      id: `${id}/colour=${colour}`,
      actions,
      colour,
      out: report.formatActionLines(actions, colour),
    });
  }
}

// ---------------------------------------------------------------------------
// summary + exit code
// ---------------------------------------------------------------------------

interface Summary {
  created: number;
  modified: number;
  deleted: number;
  touched: number;
  described: number;
  conflicts: number;
  skipped: number;
  failed: number;
}

const zero: Summary = {
  created: 0, modified: 0, deleted: 0, touched: 0,
  described: 0, conflicts: 0, skipped: 0, failed: 0,
};

const SUMMARIES: Array<{ id: string; summary: Summary; elapsedMs: number }> = [
  { id: 'nothing', summary: { ...zero }, elapsedMs: 812 },
  { id: 'everything', summary: { created: 3, modified: 1, deleted: 2, touched: 4, described: 5, conflicts: 6, skipped: 7, failed: 8 }, elapsedMs: 1234 },
  { id: 'one-conflict-is-singular', summary: { ...zero, conflicts: 1 }, elapsedMs: 50 },
  { id: 'two-conflicts-are-plural', summary: { ...zero, conflicts: 2 }, elapsedMs: 50 },
  { id: 'only-created', summary: { ...zero, created: 3 }, elapsedMs: 100 },
  { id: 'only-failed', summary: { ...zero, failed: 1 }, elapsedMs: 100 },
  { id: 'failed-and-conflicted', summary: { ...zero, failed: 1, conflicts: 1 }, elapsedMs: 100 },
  // The seconds render through `toFixed(1)`, so the rounding is part of it.
  { id: 'sub-millisecond', summary: { ...zero, touched: 1 }, elapsedMs: 0 },
  { id: 'rounds-up', summary: { ...zero, touched: 1 }, elapsedMs: 1950 },
  { id: 'rounds-to-a-tenth', summary: { ...zero, touched: 1 }, elapsedMs: 1249 },
  { id: 'long-run', summary: { ...zero, created: 1 }, elapsedMs: 3_723_000 },
];

for (const { id, summary, elapsedMs } of SUMMARIES) {
  for (const dryRun of [false, true]) {
    emit({
      kind: 'summary',
      id: `${id}/dryRun=${dryRun}`,
      summary,
      elapsedMs,
      dryRun,
      out: report.formatSummary(summary, elapsedMs, dryRun),
    });
  }
  emit({ kind: 'exit-code', id, summary, out: report.exitCodeFor(summary) });
}
