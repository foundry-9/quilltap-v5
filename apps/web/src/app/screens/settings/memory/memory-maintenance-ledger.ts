import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import {
  MAINTENANCE_PHASES,
  type MaintenanceLedgerStatus,
  type MaintenanceReason,
  type MaintenanceSummary,
} from '../../../core/core-contract';
import { PHASE_LABELS } from '../../../memory/maintenance-report';
import { fetchMaintenanceStatus, memoryKeys } from '../../../memory/memory.api';
import { readErrorText } from './api-error-text';

/** v4 `REASON_TEXT` (`memory-maintenance-ledger.tsx:26-30`). */
const REASON_TEXT: Record<MaintenanceReason, string> = {
  scheduled: 'daily round',
  watermark: 'nearing the cap',
  manual: 'by hand',
};

/** v4 `when` (`:32-35`) — an unparseable date shows as the raw string. */
function when(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
}

/**
 * One line on what the last run did, from its summary — v4 `summaryLine`
 * (`:38-50`), exported. No `toLocaleString` here (v4 has none).
 */
export function summaryLine(summary: MaintenanceSummary): string {
  if (summary.skippedReason) {
    return summary.skippedReason === 'archived'
      ? 'skipped: the character is archived'
      : 'skipped: character not found';
  }
  const parts: string[] = [];
  if (summary.fold) parts.push(`${summary.fold.folded} folded`);
  if (summary.consolidation) {
    const c = summary.consolidation;
    parts.push(
      c.skippedReason
        ? `consolidation skipped (${c.skippedReason})`
        : `${c.digestsCreated + c.digestsUpdated} digests, ${c.membersSuperseded} superseded`,
    );
  }
  if (summary.demote) {
    parts.push(`${summary.demote.demoted} archived (${summary.demote.demotable}/${summary.demote.cap} counted)`);
  }
  if (summary.retire) parts.push(`${summary.retire.retired} let go`);
  if (parts.length === 0) parts.push('nothing to do');
  return parts.join(' · ');
}

/**
 * The maintenance ledger — v4 `components/tools/memory-maintenance-ledger.tsx`
 * (`b045dd986`, `48790fb73`, at `83334b9a5`), `memoryMaintenanceStatus`: per
 * character, when each phase last ran and why, and what the last run did. The
 * ledger lives in the server's memory and is re-seeded from recent job rows at
 * boot; the `memories` realtime topic refreshes this read after a run commits
 * (the unscoped hint invalidates every `['memories', …]` key — R-AB).
 */
@Component({
  selector: 'qt-memory-maintenance-ledger',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (statusQuery.isLoading()) {
      <p class="qt-text-small qt-text-muted">Opening the ledger…</p>
    } @else if (statusQuery.isError()) {
      <p class="qt-text-small qt-text-destructive">{{ errorText() }}</p>
    } @else if (entries().length === 0) {
      <p class="qt-text-small qt-text-muted">
        The ledger is blank: no round has been recorded since the house last opened its doors.
      </p>
    } @else {
      <div class="space-y-3">
        @for (entry of entries(); track entry.characterId) {
          <div class="qt-card space-y-2">
            <div class="flex flex-wrap items-baseline justify-between gap-2">
              <p class="qt-text-label">{{ entry.name }}</p>
              @if (entry.lastLine; as line) {
                <p class="qt-text-xs qt-text-muted">{{ line }}</p>
              }
            </div>
            @if (entry.summary; as summary) {
              <p class="qt-text-small">{{ summary }}</p>
            }
            <ul class="grid gap-x-4 gap-y-1 sm:grid-cols-2">
              @for (p of entry.phases; track p.phase) {
                <li class="qt-text-xs flex justify-between gap-2">
                  <span class="qt-text-muted">{{ p.label }}</span>
                  <span>{{ p.text }}</span>
                </li>
              }
            </ul>
            @if (entry.demotion; as d) {
              <p class="qt-text-xs" [class.qt-text-muted]="d.muted" [class.qt-text-warning]="!d.muted">
                {{ d.text }}
              </p>
            }
          </div>
        }
      </div>
    }
  `,
})
export class MemoryMaintenanceLedger {
  private readonly core = inject(CoreClient);

  /** The counts list, for names (v4 `useCharacterMemoryCounts`). */
  readonly characters = input<ReadonlyArray<{ id: string; name: string }>>([]);

  protected readonly statusQuery = injectQuery(() => ({
    queryKey: memoryKeys.maintenanceStatus(),
    queryFn: (): Promise<MaintenanceLedgerStatus[]> => fetchMaintenanceStatus(this.core),
  }));

  protected readonly errorText = computed(() =>
    readErrorText(this.statusQuery.error(), 'Failed to read the maintenance ledger'),
  );

  protected readonly entries = computed(() => {
    const names = new Map(this.characters().map((c) => [c.id, c.name]));
    return (this.statusQuery.data() ?? [])
      .filter((e) => e.lastSummary || Object.keys(e.lastRun).length > 0)
      .map((e) => ({
        characterId: e.characterId,
        name: names.get(e.characterId) ?? e.characterId.slice(0, 8),
        lastLine: e.lastSummary
          ? `last ${e.lastSummary.dryRun ? 'rehearsal' : 'run'} ${when(e.lastSummary.finishedAt)} (${REASON_TEXT[e.lastSummary.reason]})`
          : null,
        summary: e.lastSummary ? summaryLine(e.lastSummary) : null,
        // All FIVE phases, the mirror included.
        phases: MAINTENANCE_PHASES.map((phase) => {
          const run = e.lastRun[phase];
          return {
            phase,
            label: PHASE_LABELS[phase],
            text: run ? `${when(run.at)} · ${REASON_TEXT[run.reason]}` : 'never',
          };
        }),
        // `48790fb73`: a demotion that found no excess reads muted, not as a warning.
        demotion:
          e.lastDemotion && !e.lastDemotion.effective
            ? e.lastDemotion.excess === 0
              ? { muted: true, text: 'Within the cap at the last demotion; automatic demotion looks again in an hour.' }
              : {
                  muted: false,
                  text: 'The last demotion moved too little of the excess to be worth repeating at once; automatic demotion rests for an hour after such a round.',
                }
            : null,
      }));
  });
}
