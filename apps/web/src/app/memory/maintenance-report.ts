import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import type {
  ConsolidationClusterReport,
  ConsolidationReport,
  MaintenancePhase,
  MaintenanceReport,
  PhaseSkipReason,
} from '../core/core-contract';

/**
 * What a maintenance run did (or, rehearsed, would do), phase by phase — v4
 * `components/memory/maintenance-report.tsx` (`b045dd986`, relabelled
 * `48790fb73`, at `83334b9a5`). One view serves the Maintenance card's dialog
 * and the memory list's; the consolidation phase's clusters render as the old
 * consolidation report did (the cluster block MOVED here from the deleted
 * report dialog, as v4 moved `ClusterBlock` — one renderer, P4.D300 L-14).
 *
 * Every composed line is computed as ONE string (v4's JSX `{' '}` and line
 * breaks collapse to single spaces), so the template carries no whitespace
 * Angular could drop.
 */

/** v4 `PHASE_LABELS` (`:12-18`). */
export const PHASE_LABELS: Record<MaintenancePhase, string> = {
  'fold-near-duplicates': 'Fold near-duplicates',
  consolidate: 'Consolidate',
  demote: 'Demote over the cap',
  retire: 'Retire old archive notes',
  mirror: 'Refresh the Commonplace files',
};

/** v4 `SKIP_TEXT` (`:20-27`). */
export const SKIP_TEXT: Record<PhaseSkipReason, string> = {
  'not-requested': 'not asked for',
  'maintenance-disabled': 'automatic maintenance is off',
  'consolidation-disabled': 'consolidation is off',
  'phase-disabled': 'switched off above',
  'no-retention-window': 'no archive retention window is set',
  'nothing-touched': 'nothing changed that it mirrors',
};

/** v4 `REPORT_SKIP_TEXT` (`:29-32`). */
export const REPORT_SKIP_TEXT: Record<NonNullable<MaintenanceReport['skippedReason']>, string> = {
  'character-not-found': 'The character could not be found.',
  archived: 'The character rests in the archive; nothing was done.',
};

/** v4 `CONSOLIDATION_SKIP_TEXT` (`:34-36`). */
export const CONSOLIDATION_SKIP_TEXT: Record<NonNullable<ConsolidationReport['skippedReason']>, string> = {
  'no-llm':
    'No model is available to do the consolidating. Name a connection profile, or configure the cheap LLM.',
};

/** v4 `bucketLabel` (`:47-56`). */
function bucketLabel(cluster: ConsolidationClusterReport): string {
  switch (cluster.bucket.kind) {
    case 'self':
      return 'About themselves';
    case 'other':
      return `About ${cluster.bucket.subjectName}`;
    default:
      return 'About no one in particular';
  }
}

interface MemberView {
  id: string;
  /** v4 `&bull; {content}` — one string. */
  line: string;
  standalone: boolean;
}

export interface ClusterView {
  key: string;
  /** v4 `Cluster {i+1} &middot; {bucket} &middot; {kind}` — one string. */
  header: string;
  status: string;
  error: string | null;
  /** v4 `Notes to be folded ({n})`. */
  membersLabel: string;
  members: MemberView[];
  /** v4 `Proposed digest{s}`. */
  digestsLabel: string;
  digests: { id: string; content: string; meta: string }[];
  /** v4 `Contradictions noticed ({n})`. */
  contradictionsLabel: string;
  contradictions: { key: string; line: string; ids: string }[];
}

/** v4 `ClusterBlock` (`:58-118`), every composed line computed as ONE string. */
export function clusterView(cluster: ConsolidationClusterReport, index: number): ClusterView {
  const standalones = new Set(cluster.keepStandalone);
  return {
    key: `${cluster.bucket.kind}-${cluster.memberIds[0] ?? index}-${index}`,
    header: `Cluster ${index + 1} · ${bucketLabel(cluster)} · ${cluster.clusterKind}`,
    status: cluster.status,
    error: cluster.error || null,
    membersLabel: `Notes to be folded (${cluster.memberIds.length})`,
    members: cluster.memberIds.map((id, i) => ({
      id,
      line: `• ${cluster.memberContents[i]}`,
      standalone: standalones.has(id),
    })),
    digestsLabel: `Proposed digest${cluster.digests.length === 1 ? '' : 's'}`,
    digests: cluster.digests.map((d) => ({
      id: d.id,
      content: d.content,
      meta:
        `${d.action === 'update' ? 'revises an existing digest' : 'new digest'} · ` +
        `${d.kind} · importance ${(d.importance * 100).toFixed(0)}% · from ` +
        `${d.memberIds.length} note${d.memberIds.length === 1 ? '' : 's'}`,
    })),
    contradictionsLabel: `Contradictions noticed (${cluster.contradictions.length})`,
    // v4: `&bull; {note || fallback}{' '}<span>(older superseded by newer)</span>`.
    contradictions: cluster.contradictions.map((c, i) => ({
      key: `${c.olderId}-${c.newerId}-${i}`,
      line: `• ${c.note || 'A newer note displaces an older one.'} `,
      ids: `(${String(c.olderId).slice(0, 8)} superseded by ${String(c.newerId).slice(0, 8)})`,
    })),
  };
}

interface Stat {
  label: string;
  value: number | string;
}

/** v4 `ConsolidationSection` (`:120-147`). */
function consolidationView(report: ConsolidationReport): {
  skipped: string | null;
  stats: Stat[];
  noteLine: string;
  empty: boolean;
  clusters: ClusterView[];
} {
  const s = report.stats;
  return {
    skipped: report.skippedReason ? CONSOLIDATION_SKIP_TEXT[report.skippedReason] : null,
    stats: [
      { label: 'Notes considered', value: s.candidates },
      // `48790fb73`: "Clusters worth folding" over `clustersQualified` (was
      // "Clusters found" over `clustersFound`).
      { label: 'Clusters worth folding', value: s.clustersQualified },
      { label: 'Clusters attempted', value: s.clustersAttempted },
      {
        label: 'Digests proposed',
        // v4 VERBATIM — JS precedence is `(created + updated) || reduce`.
        value:
          s.digestsCreated + s.digestsUpdated ||
          report.clusters.reduce((n, c) => n + c.digests.length, 0),
      },
    ],
    noteLine:
      (s.clustersDeferred > 0 ? `${s.clustersDeferred} further clusters wait for another run. ` : '') +
      (s.immatureSkipped > 0 ? `${s.immatureSkipped} notes are too young to fold. ` : '') +
      (s.noEmbeddingSkipped > 0 ? `${s.noEmbeddingSkipped} notes lack embeddings. ` : '') +
      (s.budgetExhausted ? 'The time budget ran out; the rest will keep. ' : '') +
      `${(s.durationMs / 1000).toFixed(1)}s.`,
    empty: report.clusters.length === 0 && !report.skippedReason,
    clusters: report.clusters.map(clusterView),
  };
}

/** One character's report: the phase ledger, then each phase that ran (v4 `:150-235`). */
@Component({
  selector: 'qt-maintenance-report-view',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (view(); as v) {
      @if (v.skipped !== null) {
        <p class="qt-text-small qt-text-warning">{{ v.skipped }}</p>
      } @else {
        <div class="space-y-5">
          <ul class="space-y-1">
            @for (o of v.phases; track o.phase) {
              <li class="qt-text-small flex items-baseline justify-between gap-3">
                <span [class.qt-text-muted]="!o.ran">{{ o.label }}</span>
                <span class="qt-text-xs qt-text-muted">{{ o.text }}</span>
              </li>
            }
          </ul>

          @if (v.fold; as f) {
            <section class="space-y-2">
              <h3 class="qt-text-label">{{ labels['fold-near-duplicates'] }}</h3>
              <div class="grid grid-cols-3 gap-3">
                @for (stat of f.stats; track stat.label) {
                  <div class="qt-card text-center">
                    <p class="text-2xl font-bold">{{ stat.value }}</p>
                    <p class="qt-text-xs qt-text-muted">{{ stat.label }}</p>
                  </div>
                }
              </div>
              @if (f.tail) {
                <p class="qt-text-xs qt-text-muted">{{ f.tail }}</p>
              }
            </section>
          }

          @if (v.consolidation; as c) {
            <section class="space-y-2">
              <h3 class="qt-text-label">{{ labels.consolidate }}</h3>
              <div class="space-y-3">
                @if (c.skipped; as skipped) {
                  <p class="qt-text-small qt-text-warning">{{ skipped }}</p>
                }
                <div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
                  @for (stat of c.stats; track stat.label) {
                    <div class="qt-card text-center">
                      <p class="text-2xl font-bold">{{ stat.value }}</p>
                      <p class="qt-text-xs qt-text-muted">{{ stat.label }}</p>
                    </div>
                  }
                </div>
                <p class="qt-text-xs qt-text-muted">{{ c.noteLine }}</p>
                @if (c.empty) {
                  <p class="qt-text-small qt-text-muted">
                    Not a single cluster worth folding. The shelves are in good order.
                  </p>
                }
                @for (cluster of c.clusters; track cluster.key) {
                  <div class="qt-card space-y-3">
                    <div class="flex items-center justify-between gap-2">
                      <p class="qt-text-label">{{ cluster.header }}</p>
                      <span class="qt-text-xs qt-text-muted">{{ cluster.status }}</span>
                    </div>

                    @if (cluster.error; as error) {
                      <p class="qt-text-small qt-text-destructive">{{ error }}</p>
                    }

                    <div>
                      <p class="qt-text-label-xs qt-text-muted mb-1">{{ cluster.membersLabel }}</p>
                      <ul class="space-y-1">
                        @for (member of cluster.members; track member.id) {
                          <li class="qt-text-small"><span [class.qt-text-warning]="member.standalone">{{ member.line }}</span>@if (member.standalone) {<span class="qt-text-xs qt-text-muted"> (kept on the shelf as it is)</span>}</li>
                        }
                      </ul>
                    </div>

                    @if (cluster.digests.length > 0) {
                      <div>
                        <p class="qt-text-label-xs qt-text-muted mb-1">{{ cluster.digestsLabel }}</p>
                        <ul class="space-y-2">
                          @for (digest of cluster.digests; track digest.id) {
                            <li class="border-l-2 qt-border-default pl-3">
                              <p class="qt-text-small">{{ digest.content }}</p>
                              <p class="qt-text-xs qt-text-muted">{{ digest.meta }}</p>
                            </li>
                          }
                        </ul>
                      </div>
                    }

                    @if (cluster.contradictions.length > 0) {
                      <div>
                        <p class="qt-text-label-xs qt-text-warning mb-1">{{ cluster.contradictionsLabel }}</p>
                        <ul class="space-y-1">
                          @for (x of cluster.contradictions; track x.key) {
                            <li class="qt-text-small">{{ x.line }}<span class="qt-text-xs qt-text-muted">{{ x.ids }}</span></li>
                          }
                        </ul>
                      </div>
                    }
                  </div>
                }
              </div>
            </section>
          }

          @if (v.demote; as d) {
            <section class="space-y-2">
              <h3 class="qt-text-label">{{ labels.demote }}</h3>
              <div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
                @for (stat of d.stats; track stat.label) {
                  <div class="qt-card text-center">
                    <p class="text-2xl font-bold">{{ stat.value }}</p>
                    <p class="qt-text-xs qt-text-muted">{{ stat.label }}</p>
                  </div>
                }
              </div>
              <p class="qt-text-xs qt-text-muted">{{ d.tail }}</p>
            </section>
          }

          @if (v.retire; as line) {
            <section class="space-y-1">
              <h3 class="qt-text-label">{{ labels.retire }}</h3>
              <p class="qt-text-small">{{ line }}</p>
            </section>
          }

          @if (v.mirror; as line) {
            <section class="space-y-1">
              <h3 class="qt-text-label">{{ labels.mirror }}</h3>
              <p class="qt-text-small">{{ line }}</p>
            </section>
          }
        </div>
      }
    }
  `,
})
export class MaintenanceReportView {
  readonly report = input.required<MaintenanceReport>();

  protected readonly labels = PHASE_LABELS;

  protected readonly view = computed(() => {
    const report = this.report();
    if (report.skippedReason) {
      return { skipped: REPORT_SKIP_TEXT[report.skippedReason] } as const;
    }
    const { fold, demote, retire, mirror, consolidation, dryRun } = report;
    return {
      skipped: null,
      phases: report.phases.map((o) => ({
        phase: o.phase,
        label: PHASE_LABELS[o.phase],
        ran: o.status === 'ran',
        text:
          o.status === 'ran'
            ? `${dryRun ? 'rehearsed' : 'ran'} · ${(o.durationMs / 1000).toFixed(1)}s`
            : `skipped — ${SKIP_TEXT[o.skippedBecause ?? 'not-requested']}`,
      })),
      fold: fold
        ? {
            stats: [
              { label: 'Notes compared', value: fold.eligible },
              { label: 'Sets found', value: fold.groups.length },
              { label: dryRun ? 'Would fold' : 'Folded', value: fold.folded },
            ] as Stat[],
            tail:
              fold.deferred > 0 || fold.groupsDeferred > 0 || fold.groupsFailed > 0
                ? (fold.deferred > 0 ? `${fold.deferred} notes wait for a later comparison. ` : '') +
                  (fold.groupsDeferred > 0 ? `${fold.groupsDeferred} sets wait for the next run. ` : '') +
                  (fold.groupsFailed > 0
                    ? `${fold.groupsFailed} sets could not be folded and were left as they were.`
                    : '')
                : null,
          }
        : null,
      consolidation: consolidation ? consolidationView(consolidation) : null,
      demote: demote
        ? {
            stats: [
              { label: 'Cap', value: demote.cap.toLocaleString() },
              { label: 'Counted', value: demote.demotable.toLocaleString() },
              // `48790fb73`: "Exempt" (was "Protected") over `protected`.
              { label: 'Exempt', value: demote.protected.toLocaleString() },
              { label: dryRun ? 'Would archive' : 'Archived', value: demote.demoted.toLocaleString() },
            ] as Stat[],
            tail:
              (demote.excess > 0
                ? `${demote.excess.toLocaleString()} counted notes over the cap; the least protected go to the archive first.`
                : 'Within the cap. Nothing need go to the archive.') +
              ' Hand-written notes and digests are never counted and never moved.',
          }
        : null,
      retire: retire
        ? retire.retired === 0
          ? `No superseded archive note is older than ${retire.retentionDays} days.`
          : `${retire.retired.toLocaleString()} superseded archive note${retire.retired === 1 ? '' : 's'} older than ${retire.retentionDays} days ${dryRun ? 'would be' : 'were'} let go.`
        : null,
      mirror:
        mirror && mirror.subjects.length > 0
          ? `${mirror.subjects.length} subject file${mirror.subjects.length === 1 ? '' : 's'} ` +
            `${dryRun ? 'would be refreshed' : mirror.deferredToCommit ? 'refreshed once the run is committed' : 'refreshed'}.`
          : null,
    };
  });
}

/** A user-wide dry run (LLM-free phases only): one row per character (v4 `:238-271`). */
@Component({
  selector: 'qt-maintenance-reports-table',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (rows().length === 0) {
      <p class="qt-text-small qt-text-muted">No character has any memories to tend.</p>
    } @else {
      <div class="max-h-80 overflow-y-auto border qt-border-default rounded-lg">
        <table class="w-full text-sm">
          <thead class="qt-bg-muted sticky top-0">
            <tr>
              <th class="text-left px-3 py-2 qt-text-secondary font-medium">Character</th>
              <th class="text-right px-3 py-2 qt-text-secondary font-medium">Would fold</th>
              <th class="text-right px-3 py-2 qt-text-secondary font-medium" title="Counted notes against the cap">Counted / cap</th>
              <th class="text-right px-3 py-2 qt-text-secondary font-medium">Would archive</th>
              <th class="text-right px-3 py-2 qt-text-secondary font-medium">Would let go</th>
            </tr>
          </thead>
          <tbody>
            @for (r of rows(); track r.characterId) {
              <tr>
                <td class="px-3 py-2 text-foreground truncate max-w-[160px]">{{ r.name }}</td>
                <td class="text-right px-3 py-2">{{ r.fold }}</td>
                <td class="text-right px-3 py-2" [class.qt-text-destructive]="r.over">{{ r.counted }}</td>
                <td class="text-right px-3 py-2">{{ r.archive }}</td>
                <td class="text-right px-3 py-2">{{ r.letGo }}</td>
              </tr>
            }
          </tbody>
        </table>
      </div>
    }
  `,
})
export class MaintenanceReportsTable {
  readonly reports = input.required<MaintenanceReport[]>();

  protected readonly rows = computed(() =>
    this.reports()
      .filter((r) => !r.skippedReason)
      .map((r) => ({
        characterId: r.characterId,
        name: r.characterName ?? 'Unnamed',
        fold: r.fold ? String(r.fold.folded) : '—',
        over: !!r.demote && r.demote.excess > 0,
        counted: r.demote
          ? `${r.demote.demotable.toLocaleString()} / ${r.demote.cap.toLocaleString()}`
          : '—',
        archive: r.demote ? String(r.demote.demoted) : '—',
        letGo: r.retire ? String(r.retire.retired) : '—',
      })),
  );
}
