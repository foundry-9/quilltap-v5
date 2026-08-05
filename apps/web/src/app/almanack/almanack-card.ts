import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  inject,
  signal,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';
import type { Subscription } from 'rxjs';

import { apiUrl } from '../core/api-url';
import { CoreClient, coreErrorMessage } from '../core/core-client';
import type { AlmanackGenerateResult, AlmanackReportInfo } from '../core/core-contract';
import { formatDateTime } from '../shared/format-date';
import { formatBytes } from '../ui/format-bytes';
import { Icon } from '../ui/icon';
import { ProgressBar, type ProgressSegment } from '../ui/progress-bar';
import { ToastService } from '../ui/toast.service';
import { AlmanackReportDialog } from './almanack-report-dialog';
import { ALMANACK_PHASES, ALMANACK_TITLE } from './almanack-phases';

/**
 * The Almanack card — a port of v4 `components/tools/capabilities-report-card.tsx`
 * (rewritten at `0cde7fbc`, read at `f7f1a956`). Generates, lists, views and
 * deletes system reports.
 *
 * ## The generate flow, in v4's order
 *
 * Generation is a BLOCKING call that returns the whole rendered report in its
 * body, so live phase progress arrives on a side channel keyed by a
 * client-minted `progressId` — the same pattern the Green Room uses for chat
 * creation. v4's order is load-bearing and is carried exactly:
 *
 *   1. mint the `progressId`;
 *   2. **subscribe BEFORE dispatching**, so the first phase event cannot be
 *      missed. v4 says it plainly: the bus buffers anyway, but this keeps the
 *      bar honest from the very first tick;
 *   3. dispatch and await;
 *   4. the DISPATCH's own resolution — not the `done` frame — is what drives
 *      the UI: it toasts and opens the viewer on the returned content;
 *   5. on settle, tear the subscription down, clear the phase state, refetch.
 *
 * The progress reader is advisory throughout. If not one frame arrives, the
 * generate still completes and still opens its report; the bar simply never
 * leaves its indeterminate state.
 *
 * ## The channel (a divergence from v4, by standing decision)
 *
 * v4 reads a per-id SSE route (`GET ?action=capabilities-report-progress`).
 * v5 has ONE global event stream and no bespoke SSE routes (the D3 family), so
 * the `phase` frames arrive there scope-tagged by `progressId` and this card
 * filters them — `green-room.state.ts` is the precedent it copies. The card is
 * hosted inside a `qt-collapsible-card` and so can be collapsed and reopened
 * mid-run; that keeps working because the subscription lives with the card's
 * lifetime, not the panel's, and because the stream is already connected.
 */

/**
 * A typical run takes on the order of half a minute with warm caches. The
 * per-phase estimates come from the manifest's `estimatedShare`, so re-weighting
 * a phase there re-weights the bar without touching this file.
 */
const TYPICAL_RUN_MS = 30_000;

const SEGMENTS: ProgressSegment[] = ALMANACK_PHASES.map((phase) => ({
  key: phase.key,
  // Trim the trailing ellipsis: it reads well in a status line, badly in a
  // row of six-word segment labels.
  label: phase.label.replace(/…$/, ''),
  estimatedDurationMs: Math.round(phase.estimatedShare * TYPICAL_RUN_MS),
}));

/** The list query's key — invalidated after every generate and every delete. */
export const ALMANACK_REPORTS_QUERY_KEY = ['almanackReports'];

interface SelectedReport {
  id: string;
  filename: string;
  content: string;
}

@Component({
  selector: 'qt-almanack-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, ProgressBar, AlmanackReportDialog],
  template: `
    <!-- v4's rewritten card wraps its body in a nested "qt-card p-6" INSIDE the
         collapsible (card.tsx's root div) — a double-card its sibling Providers
         cards don't have. v5 deliberately drops the wrapper so the Almanack
         matches the api-keys / connection-profiles / cheap-llm cards beside it;
         a documented rendered-DOM divergence (recorded at the f7f1a956
         unification's §3 review). -->
    <div>
      <div class="flex items-start gap-4 mb-6">
        <div class="flex-1">
          <h2 class="qt-heading-2 text-foreground mb-1">{{ almanackTitle }}</h2>
          <p class="qt-text-small">
            A compendium of the establishment — every cog, ledger and shelf, catalogued
          </p>
        </div>
        <div class="flex-shrink-0 text-primary"><qt-icon name="file" class="w-8 h-8" /></div>
      </div>

      @if (error(); as message) {
        <div
          class="qt-bg-destructive/10 border qt-border-destructive qt-text-destructive px-4 py-3 rounded mb-4"
        >
          {{ message }}
        </div>
      }

      <div class="mb-6">
        <button
          type="button"
          class="qt-button qt-button-primary w-full flex items-center justify-center gap-2"
          [disabled]="generating()"
          (click)="generate()"
        >
          <qt-icon name="file" class="w-5 h-5" />
          {{ generating() ? 'Compiling the Almanack…' : 'Compile the Almanack' }}
        </button>

        @if (generating()) {
          <div class="mt-3">
            <qt-progress-bar
              [segments]="segments"
              [currentKey]="currentPhase()"
              [startedAt]="startedAt()"
              [indeterminate]="currentPhase() === null"
            />
            @if (phaseLabel(); as label) {
              <p class="qt-text-small mt-2">{{ label }}</p>
            }
          </div>
        }
      </div>

      <div>
        <h3 class="qt-heading-4 text-foreground mb-3">Previous Editions</h3>

        @if (reportsQuery.isPending() && !generating()) {
          <div class="text-center py-6 qt-text-secondary">
            <div class="qt-spinner h-6 w-6 mx-auto mb-2 animate-spin rounded-full"></div>
            Loading reports...
          </div>
        } @else if (reports().length === 0) {
          <div class="qt-card p-6 text-center">
            <qt-icon name="file" class="w-12 h-12 mx-auto mb-3 qt-text-secondary/50" />
            <p class="qt-text-small">No editions yet. Compile one to get started.</p>
          </div>
        } @else {
          <div class="space-y-2 max-h-[180px] overflow-y-auto">
            @for (report of reports(); track report.id) {
              <div
                class="qt-card p-4 flex items-center justify-between hover:qt-bg-muted/50 transition-colors"
              >
                <div class="flex-1 min-w-0">
                  <p class="qt-text-primary truncate">{{ report.filename }}</p>
                  <div class="flex gap-4 mt-1 qt-text-small">
                    <span>{{ formatDate(report.createdAt) }}</span>
                    <span>{{ formatSize(report.size) }}</span>
                  </div>
                </div>
                <div class="ml-4 flex items-center gap-2">
                  <button
                    type="button"
                    class="qt-button qt-button-icon qt-button-secondary p-2"
                    title="View Report"
                    [disabled]="loadingReportId() === report.id"
                    (click)="view(report)"
                  >
                    <qt-icon name="eye" class="w-4 h-4" />
                  </button>

                  <a
                    [href]="downloadHref(report.id)"
                    [download]="report.filename"
                    class="qt-button qt-button-icon qt-button-primary p-2"
                    title="Download Report"
                  >
                    <qt-icon name="download" class="w-4 h-4" />
                  </a>

                  <button
                    type="button"
                    class="qt-button qt-button-icon qt-button-destructive p-2"
                    title="Delete Report"
                    (click)="remove(report)"
                  >
                    <qt-icon name="trash" class="w-4 h-4" />
                  </button>
                </div>
              </div>
            }
          </div>
        }
      </div>

      @if (selected(); as report) {
        <qt-almanack-report-dialog
          [reportId]="report.id"
          [filename]="report.filename"
          [content]="report.content"
          (close)="selected.set(null)"
        />
      }
    </div>
  `,
})
export class AlmanackCard {
  private readonly core = inject(CoreClient);
  private readonly toast = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  protected readonly almanackTitle = ALMANACK_TITLE;
  protected readonly segments = SEGMENTS;

  protected readonly error = signal<string | null>(null);
  protected readonly loadingReportId = signal<string | null>(null);
  protected readonly generating = signal(false);
  protected readonly selected = signal<SelectedReport | null>(null);

  // Live progress.
  protected readonly currentPhase = signal<string | null>(null);
  protected readonly phaseLabel = signal<string | null>(null);
  protected readonly startedAt = signal<number | null>(null);

  /** The open progress subscription — v4's `abortRef`. */
  private progressSub: Subscription | null = null;

  protected readonly reportsQuery = injectQuery(() => ({
    queryKey: ALMANACK_REPORTS_QUERY_KEY,
    queryFn: async (): Promise<AlmanackReportInfo[]> => {
      const data = await this.core.dispatchData({ type: 'systemAlmanackList' });
      return (data['reports'] as AlmanackReportInfo[] | undefined) ?? [];
    },
  }));

  protected readonly reports = computed(() => this.reportsQuery.data() ?? []);

  constructor() {
    // Never leave a reader running behind a torn-down card (v4's unmount abort).
    inject(DestroyRef).onDestroy(() => this.progressSub?.unsubscribe());
  }

  protected formatDate(iso: string): string {
    return formatDateTime(iso);
  }

  protected formatSize(bytes: number): string {
    return formatBytes(bytes);
  }

  /**
   * The WEB-EDGE-ONLY download leg: the same `capabilities-report-get` action
   * with `download=true`, as a plain `<a download>` so the browser owns the save
   * dialog — v4's own shape.
   */
  protected downloadHref(reportId: string): string {
    return apiUrl(
      `/api/v1/system/tools?action=capabilities-report-get&reportId=${encodeURIComponent(
        reportId,
      )}&download=true`,
    );
  }

  /** The `phase` reader for one run — subscribed BEFORE the dispatch (see above). */
  private subscribeToPhases(progressId: string): Subscription {
    return this.core.events$.subscribe((frame) => {
      if (frame.progressId !== progressId) return;
      if (frame.kind === 'phase' && frame.key) {
        this.currentPhase.set(frame.key);
        this.phaseLabel.set(frame.label ?? null);
      } else if (frame.kind === 'done' || frame.kind === 'error') {
        this.currentPhase.set(null);
        this.phaseLabel.set(null);
      }
    });
  }

  protected async generate(): Promise<void> {
    if (this.generating()) return;

    // The same guard the Green Room's caller uses (`new-chat.state.ts:541-544`):
    // without `crypto.randomUUID` there is no id to scope frames by, so the run
    // simply goes without progress. It still completes — the dispatch's
    // resolution is what drives the UI — and the bar stays indeterminate.
    const progressId =
      typeof crypto !== 'undefined' && crypto.randomUUID ? crypto.randomUUID() : undefined;

    this.error.set(null);
    this.startedAt.set(Date.now());
    this.currentPhase.set(null);
    this.phaseLabel.set(null);
    this.generating.set(true);

    // Open the reader BEFORE the dispatch so the first phase event can't be
    // missed (v4 `:156-159`). This ordering is pinned by the spec.
    this.progressSub?.unsubscribe();
    this.progressSub = progressId ? this.subscribeToPhases(progressId) : null;

    try {
      const data = await this.core.dispatchData({ type: 'systemAlmanackGenerate', progressId });
      const result = data as unknown as AlmanackGenerateResult;
      this.toast.showSuccess('The Almanack has been bound');
      this.selected.set({
        id: result.reportId,
        filename: result.filename,
        content: result.content,
      });
    } catch (err) {
      const message = coreErrorMessage(err, 'Unknown error');
      this.error.set(message);
      this.toast.showError(message);
    } finally {
      this.progressSub?.unsubscribe();
      this.progressSub = null;
      this.currentPhase.set(null);
      this.phaseLabel.set(null);
      this.startedAt.set(null);
      this.generating.set(false);
      void this.queryClient.invalidateQueries({ queryKey: ALMANACK_REPORTS_QUERY_KEY });
    }
  }

  protected async view(report: AlmanackReportInfo): Promise<void> {
    try {
      this.loadingReportId.set(report.id);
      const data = await this.core.dispatchData({
        type: 'systemAlmanackGet',
        reportId: report.id,
      });
      this.selected.set({
        id: report.id,
        filename: report.filename,
        content: (data['content'] as string | undefined) ?? '',
      });
    } catch (err) {
      this.toast.showError(coreErrorMessage(err, 'Unknown error'));
    } finally {
      this.loadingReportId.set(null);
    }
  }

  /**
   * v4 gates on its promise-based `showConfirmation` modal; v5 has no such
   * component and `window.confirm` is the established idiom here (the photos /
   * wardrobe / character-edit precedent). The sentence is v4's, verbatim.
   */
  protected async remove(report: AlmanackReportInfo): Promise<void> {
    if (
      typeof window !== 'undefined' &&
      !window.confirm(`Are you sure you want to delete "${report.filename}"?`)
    ) {
      return;
    }
    try {
      await this.core.dispatchData({ type: 'systemAlmanackDelete', reportId: report.id });
      this.toast.showSuccess('Report deleted');
    } catch (err) {
      this.toast.showError(coreErrorMessage(err, 'Unknown error'));
    } finally {
      void this.queryClient.invalidateQueries({ queryKey: ALMANACK_REPORTS_QUERY_KEY });
    }
  }
}
