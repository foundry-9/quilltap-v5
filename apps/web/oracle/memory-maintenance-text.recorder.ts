/**
 * The v4-side recorder behind
 * `src/app/screens/settings/memory/memory-maintenance-text.oracle.spec.ts`
 * (P4.D300, Tier 2 item 21).
 *
 * v4's Phase G (`b045dd986`, `48790fb73`) moves the memory SPA onto a handful
 * of pure values the Angular port transcribes: the settings documents'
 * defaults (`MEMORY_SETTING_DEFAULTS`, each its schema's REAL `.parse({})`),
 * the cap bounds, the importance band (`e0ac7493a`), the phase vocabulary, the
 * ledger's one-line `summaryLine` and the report's `PHASE_LABELS`. This
 * recorder drives v4's REAL modules — `lib/memory/settings-defaults.ts`,
 * `lib/memory/scoring/importance.ts`, `lib/memory/maintenance/types.ts`, and
 * the two client components that export the text (`components/tools/
 * memory-maintenance-ledger.tsx`, `components/memory/maintenance-report.tsx`,
 * loaded through `tsx`) — so the SPA's transcription is compared row for row.
 * Never a reimplementation: every `out` is v4's own value.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Run it from a pinned v4 worktree at `83334b9a5` (Node 24 at
 * `~/.nvm/versions/node/v24.13.1/bin`; stage under `/tmp/<order>/`, copy in):
 *
 * ```bash
 * PIN=${PIN:?a detached v4 worktree at 83334b9a5}
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/memory-maintenance-text.recorder.ts" "$PIN/"
 * OUT=/tmp/p4d300/memory-maintenance-text.ndjson
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx memory-maintenance-text.recorder.ts > "$OUT")
 * rm "$PIN/memory-maintenance-text.recorder.ts"
 * wc -l "$OUT"          # expect 29 (7 constants + 10 importanceBand + 12 summaryLine)
 * cp "$OUT" "$V5W/apps/web/src/testing/fixtures/memory-maintenance-text.ndjson"
 * ```
 *
 * A shorter file means the recorder errored and the redirect already truncated
 * the old one (the empty-file trap) — `rm -f` first, `wc -l` after.
 */

import { MEMORY_CAP_BOUNDS, MEMORY_SETTING_DEFAULTS } from '@/lib/memory/settings-defaults'
import { IMPORTANCE_HIGH, IMPORTANCE_LOW, importanceBand } from '@/lib/memory/scoring/importance'
import { MAINTENANCE_PHASES, type MaintenanceSummary } from '@/lib/memory/maintenance/types'
import { summaryLine } from '@/components/tools/memory-maintenance-ledger'
import { PHASE_LABELS } from '@/components/memory/maintenance-report'

const emit = (fn: string, id: string, args: unknown[], out: unknown): void => {
  console.log(JSON.stringify({ fn, id, args, out }))
}

// --- constants (7 rows) -----------------------------------------------------
emit('MEMORY_SETTING_DEFAULTS', 'maintenance', [], MEMORY_SETTING_DEFAULTS.maintenance)
emit('MEMORY_SETTING_DEFAULTS', 'consolidation', [], MEMORY_SETTING_DEFAULTS.consolidation)
emit('MEMORY_SETTING_DEFAULTS', 'extractionLimits', [], MEMORY_SETTING_DEFAULTS.extractionLimits)
emit('MEMORY_CAP_BOUNDS', 'bounds', [], MEMORY_CAP_BOUNDS)
emit('IMPORTANCE', 'high/low', [], { high: IMPORTANCE_HIGH, low: IMPORTANCE_LOW })
emit('MAINTENANCE_PHASES', 'phases', [], MAINTENANCE_PHASES)
emit('PHASE_LABELS', 'labels', [], PHASE_LABELS)

// --- importanceBand over the edge set (10 rows) -----------------------------
for (const v of [0, 0.1, 0.29, 0.2999, 0.3, 0.5, 0.6999, 0.7, 0.71, 1]) {
  emit('importanceBand', `v=${v}`, [v], importanceBand(v))
}

// --- summaryLine over every arm (12 rows) -----------------------------------
const base: MaintenanceSummary = {
  characterId: 'c1',
  reason: 'manual',
  dryRun: false,
  finishedAt: '2026-10-13T22:24:00.000Z',
  durationMs: 10,
  phases: [],
}
const fold = { eligible: 40, groups: 2, folded: 3, groupsFailed: 0, groupsDeferred: 0 }
const consolidation = { digestsCreated: 2, digestsUpdated: 1, membersSuperseded: 9, clustersFailed: 0, clustersDeferred: 0 }
const demote = { cap: 2000, hotTotal: 12500, demotable: 12100, protected: 400, excess: 10100, demoted: 1100, highestScoreDemoted: 0.2 }
const retire = { retentionDays: 90, retired: 4 }
const SUMMARIES: Array<[string, MaintenanceSummary]> = [
  ['nothing', base],
  ['skipped archived', { ...base, skippedReason: 'archived', fold }],
  ['skipped not found', { ...base, skippedReason: 'character-not-found' }],
  ['fold only', { ...base, fold }],
  ['consolidation counted', { ...base, consolidation }],
  ['consolidation skipped', { ...base, consolidation: { ...consolidation, skippedReason: 'no-llm' } }],
  ['consolidation skipped empty string', { ...base, consolidation: { ...consolidation, skippedReason: '' } }],
  ['demote big numbers', { ...base, demote }],
  ['retire only', { ...base, retire }],
  ['retire zero', { ...base, retire: { retentionDays: 30, retired: 0 } }],
  ['every part', { ...base, fold, consolidation, demote, retire }],
  ['every part, dry run', { ...base, dryRun: true, fold, consolidation, demote, retire }],
]
for (const [id, summary] of SUMMARIES) {
  emit('summaryLine', id, [summary], summaryLine(summary))
}
