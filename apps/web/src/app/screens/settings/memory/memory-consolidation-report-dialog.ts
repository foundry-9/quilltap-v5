import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  afterNextRender,
  computed,
  inject,
  input,
  output,
} from '@angular/core';

import type {
  ConsolidationClusterReport,
  ConsolidationReport,
} from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';

/** v4 `SKIPPED_TEXT` (`memory-consolidation-report-dialog.tsx:12-16`). */
const SKIPPED_TEXT: Record<NonNullable<ConsolidationReport['skippedReason']>, string> = {
  'character-not-found': 'The house has no record of that character.',
  archived: 'That character has been archived, and the archive is not to be disturbed.',
  'no-llm':
    'No model is available to do the consolidating. Name a connection profile above, or configure the cheap LLM.',
};

/** v4 `bucketLabel` (`:18-27`). */
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

interface ClusterView {
  key: string;
  /** v4 `Cluster {i+1} &middot; {bucket} &middot; {kind}` (`:34-36`) — one string. */
  header: string;
  status: string;
  error: string | null;
  /** v4 `Notes to be folded ({n})` (`:43-45`). */
  membersLabel: string;
  members: MemberView[];
  /** v4 `Proposed digest{s}` (`:62-64`). */
  digestsLabel: string;
  digests: { id: string; content: string; meta: string }[];
  /** v4 `Contradictions noticed ({n})` (`:82-84`). */
  contradictionsLabel: string;
  contradictions: { key: string; line: string; ids: string }[];
}

/** v4 `ClusterBlock` (`:29-99`), every composed line computed as ONE string. */
function clusterView(cluster: ConsolidationClusterReport, index: number): ClusterView {
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
    // v4 `:70-72`: the JSX `{' '}` + line breaks collapse to single spaces.
    digests: cluster.digests.map((d) => ({
      id: d.id,
      content: d.content,
      meta:
        `${d.action === 'update' ? 'revises an existing digest' : 'new digest'} · ` +
        `${d.kind} · importance ${(d.importance * 100).toFixed(0)}% · from ` +
        `${d.memberIds.length} note${d.memberIds.length === 1 ? '' : 's'}`,
    })),
    contradictionsLabel: `Contradictions noticed (${cluster.contradictions.length})`,
    // v4 `:88-91`: `&bull; {note || fallback}{' '}<span>(older superseded by newer)</span>`.
    contradictions: cluster.contradictions.map((c, i) => ({
      key: `${c.olderId}-${c.newerId}-${i}`,
      line: `• ${c.note || 'A newer note displaces an older one.'} `,
      ids: `(${String(c.olderId).slice(0, 8)} superseded by ${String(c.newerId).slice(0, 8)})`,
    })),
  };
}

/**
 * The consolidation report (v4 `components/tools/memory-consolidation-report-
 * dialog.tsx` at `01a83539d`), shown after a dry run.
 *
 * **Transcribed as v4 writes it (P4.D276 R-H, recorded):** hand markup — no
 * `role="dialog"`, no backdrop-click close, no Escape handler; the header's
 * close icon and the footer `Close` are the only exits. **The portal:** v4
 * `createPortal(…, document.body)` so the overlay is not trapped under the
 * workspace toolbar's stacking context; the host reparents to `document.body`
 * after the first render (the `images/image-detail-modal.ts` idiom).
 */
@Component({
  selector: 'qt-memory-consolidation-report-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="qt-dialog-overlay p-4 z-[100]">
      <div class="qt-dialog max-w-3xl max-h-[90vh] overflow-hidden flex flex-col">
        <div class="qt-dialog-header">
          <div class="flex items-center justify-between">
            <h2 class="qt-dialog-title">{{ title() }}</h2>
            <button
              type="button"
              class="qt-text-secondary hover:text-foreground"
              aria-label="Close"
              (click)="close.emit()"
            >
              <qt-icon name="close" class="w-6 h-6" />
            </button>
          </div>
          <p class="qt-dialog-description">{{ description() }}</p>
        </div>

        <div class="flex-1 overflow-y-auto p-6 space-y-4">
          @if (skippedText(); as skipped) {
            <p class="qt-text-small qt-text-warning">{{ skipped }}</p>
          }

          <div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
            @for (stat of stats(); track stat.label) {
              <div class="qt-card text-center">
                <p class="text-2xl font-bold">{{ stat.value }}</p>
                <p class="qt-text-xs qt-text-muted">{{ stat.label }}</p>
              </div>
            }
          </div>
          <p class="qt-text-xs qt-text-muted">{{ noteLine() }}</p>

          @if (report().clusters.length === 0 && !report().skippedReason) {
            <p class="qt-text-small qt-text-muted">
              Not a single cluster worth folding. The shelves are in good order.
            </p>
          }

          @for (cluster of clusters(); track cluster.key) {
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
                    @for (c of cluster.contradictions; track c.key) {
                      <li class="qt-text-small">{{ c.line }}<span class="qt-text-xs qt-text-muted">{{ c.ids }}</span></li>
                    }
                  </ul>
                </div>
              }
            </div>
          }
        </div>

        <div class="qt-dialog-footer flex justify-end gap-2">
          <button type="button" class="qt-button qt-button-secondary" (click)="close.emit()">
            Close
          </button>
        </div>
      </div>
    </div>
  `,
})
export class MemoryConsolidationReportDialog {
  readonly report = input.required<ConsolidationReport>();
  readonly close = output<void>();

  /** v4 `:115-118` — the two JSX expressions concatenate with nothing between. */
  protected readonly title = computed(() => {
    const r = this.report();
    return (
      (r.dryRun ? 'Consolidation, Rehearsed' : 'Consolidation Report') +
      (r.characterName ? ` — ${r.characterName}` : '')
    );
  });

  /** v4 `:123-127`. */
  protected readonly description = computed(() =>
    this.report().dryRun
      ? 'Nothing has been written. This is what the Commonplace Book would do, were it let off the leash.'
      : 'The Commonplace Book has been at work.',
  );

  protected readonly skippedText = computed(() => {
    const reason = this.report().skippedReason;
    return reason ? SKIPPED_TEXT[reason] : null;
  });

  /** v4 `:135-140`. */
  protected readonly stats = computed(() => {
    const r = this.report();
    const s = r.stats;
    return [
      { label: 'Notes considered', value: s.candidates },
      { label: 'Clusters found', value: s.clustersFound },
      { label: 'Clusters attempted', value: s.clustersAttempted },
      {
        label: 'Digests proposed',
        // v4 `:139` VERBATIM — JS precedence is `(created + updated) || reduce`,
        // so a 0 (or NaN) sum falls to the clusters' own digest count.
        value:
          s.digestsCreated + s.digestsUpdated ||
          r.clusters.reduce((n, c) => n + c.digests.length, 0),
      },
    ];
  });

  /** v4 `:141-147` — one string; every clause carries its own trailing space. */
  protected readonly noteLine = computed(() => {
    const s = this.report().stats;
    return (
      (s.clustersDeferred > 0 ? `${s.clustersDeferred} further clusters wait for another run. ` : '') +
      (s.immatureSkipped > 0 ? `${s.immatureSkipped} notes are too young to fold. ` : '') +
      (s.noEmbeddingSkipped > 0 ? `${s.noEmbeddingSkipped} notes lack embeddings. ` : '') +
      (s.budgetExhausted ? 'The time budget ran out; the rest will keep. ' : '') +
      `${(s.durationMs / 1000).toFixed(1)}s.`
    );
  });

  protected readonly clusters = computed(() => this.report().clusters.map(clusterView));

  constructor() {
    const host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
    inject(DestroyRef).onDestroy(() => host.remove());
    afterNextRender(() => {
      if (typeof document !== 'undefined') {
        document.body.appendChild(host);
      }
    });
  }
}
