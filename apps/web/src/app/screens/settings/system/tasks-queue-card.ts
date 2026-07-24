import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import { CoreDispatchError } from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';
import { TaskDetailsModal } from './task-details-modal';
import { TaskItem } from './task-item';
import {
  controlJob,
  controlTasksQueue,
  deleteJob,
  fetchJob,
  fetchTasksQueue,
  formatTokens,
  setJobConcurrency,
  tasksQueueKeys,
  type FullJobDetail,
  type QueueData,
} from './tasks-queue.api';

const MIN_CONCURRENCY = 1;
const MAX_CONCURRENCY = 32;
const DEFAULT_CONCURRENCY = 4;
const POLL_MS = 5000;

function errText(err: unknown, fallback: string): string {
  if (err instanceof CoreDispatchError) return err.message || fallback;
  if (err instanceof Error) return err.message || fallback;
  return fallback;
}

/**
 * The Tasks Queue card (v4 `components/tools/tasks-queue/index.tsx` + its hook):
 * the background-job queue for memory extraction and other LLM tasks. Summary
 * stats + a detailed breakdown, per-job rows with pause/resume/view/delete, a
 * processor start/stop control, a 5 s auto-refresh poll (off by default), and
 * the "Simultaneous Labours" concurrency slider (1–32, default 4 — the house
 * register kept verbatim).
 *
 * Over the six §1 verbs P4.9G1 delivers — so its live behaviour and e2e beat
 * are ACTIVATE-AT-UNIFY; the component compiles and renders against the contract
 * today.
 */
@Component({
  selector: 'qt-tasks-queue-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, TaskItem, TaskDetailsModal],
  template: `
    <div>
      <!-- Simultaneous Labours (concurrency) -->
      <div class="qt-card p-4 mb-6">
        <label for="maxConcurrentJobs" class="qt-text-body text-foreground font-medium">
          Simultaneous Labours — {{ displayConcurrency() }}
        </label>
        <input
          id="maxConcurrentJobs"
          type="range"
          [min]="min"
          [max]="max"
          step="1"
          class="w-full cursor-pointer mt-2"
          [value]="displayConcurrency()"
          (mousedown)="onDragStart()"
          (touchstart)="onDragStart()"
          (input)="onDragInput($event)"
          (mouseup)="onDragCommit()"
          (touchend)="onDragCommit()"
        />
        <p class="qt-text-small qt-text-muted mt-1">
          How many background errands the engine may undertake at once. Four suits most households; a
          stouter machine may shoulder up to two-and-thirty. Mind that a single ravenous task type
          may then monopolise the works.
        </p>
      </div>

      @if (error(); as msg) {
        <div class="qt-bg-destructive/10 border qt-border-destructive qt-text-destructive px-4 py-3 rounded mb-4">
          {{ msg }}
        </div>
      }

      <!-- Filters -->
      <div class="flex items-center gap-3 mb-4 flex-wrap">
        <button
          type="button"
          class="qt-button qt-button-secondary flex items-center gap-2"
          [disabled]="query.isFetching()"
          (click)="query.refetch()"
        >
          <qt-icon name="refresh" [class]="'w-4 h-4 ' + (query.isFetching() ? 'animate-spin' : '')" />
          Refresh
        </button>
        <button
          type="button"
          class="qt-button qt-button-primary flex items-center gap-2"
          [disabled]="controlLoading() || running() || !activeTotal()"
          (click)="control('start')"
        >
          <qt-icon name="play" class="w-4 h-4" />
          Start Queue
        </button>
        <button
          type="button"
          class="qt-button qt-button-secondary flex items-center gap-2"
          [disabled]="controlLoading() || !running() || !activeTotal()"
          (click)="control('stop')"
        >
          <qt-icon name="stop" class="w-4 h-4" />
          Stop Queue
        </button>
        <label class="flex items-center gap-2 qt-text-small cursor-pointer">
          <input
            type="checkbox"
            class="rounded qt-border-default text-primary focus:ring-ring"
            [checked]="autoRefresh()"
            (change)="autoRefresh.set($any($event.target).checked)"
          />
          Auto-refresh (5s)
        </label>
        @if (data(); as d) {
          <span
            [class]="
              'qt-text-small flex items-center gap-1.5 ml-auto ' +
              (running() ? 'qt-text-success' : 'qt-text-secondary')
            "
          >
            <span
              [class]="
                'w-2 h-2 rounded-full ' + (running() ? 'bg-success animate-pulse' : 'qt-bg-muted-foreground')
              "
            ></span>
            {{ running() ? 'Queue Running' : 'Queue Stopped' }}
          </span>
        }
      </div>

      @if (data(); as d) {
        <div class="grid grid-cols-3 gap-4 mb-6">
          <div class="qt-card p-3 text-center">
            <div class="qt-heading-2 text-foreground">{{ d.stats.activeTotal }}</div>
            <div class="qt-text-xs">Active Jobs</div>
          </div>
          <div class="qt-card p-3 text-center">
            <div class="qt-heading-2 text-foreground">~{{ totalTokens() }}</div>
            <div class="qt-text-xs">Est. Tokens</div>
          </div>
          <div class="qt-card p-3 text-center">
            <div class="qt-heading-2 qt-text-success">{{ d.stats.completed }}</div>
            <div class="qt-text-xs">Completed</div>
          </div>
        </div>

        <div class="flex flex-wrap gap-4 qt-text-small mb-4">
          <span><span class="qt-text-info font-medium">{{ d.stats.processing }}</span> processing</span>
          <span><span class="qt-text-warning font-medium">{{ d.stats.pending }}</span> pending</span>
          <span><span class="qt-text-destructive font-medium">{{ d.stats.failed }}</span> failed</span>
          @if (d.stats.paused > 0) {
            <span><span class="qt-text-warning font-medium">{{ d.stats.paused }}</span> paused</span>
          }
          @if (d.stats.dead > 0) {
            <span><span class="qt-text-secondary font-medium">{{ d.stats.dead }}</span> dead</span>
          }
        </div>
      }

      <h3 class="qt-heading-4 text-foreground mb-3">Queue Items</h3>

      @if (query.isPending() && !data()) {
        <div class="text-center py-6 qt-text-secondary">
          <div class="animate-spin rounded-full h-6 w-6 border-b-2 qt-border-primary mx-auto mb-2"></div>
          Loading queue...
        </div>
      } @else if (jobs().length === 0) {
        <div class="qt-card p-6 text-center">
          <qt-icon name="check" class="w-12 h-12 mx-auto mb-3 qt-text-secondary/50" />
          <p class="qt-text-secondary">Queue is empty. All tasks completed!</p>
        </div>
      } @else {
        <div class="space-y-2 max-h-[300px] overflow-y-auto">
          @for (job of jobs(); track job.id) {
            <qt-task-item
              [job]="job"
              [busy]="jobActionLoading() === job.id"
              (view)="viewJob($event)"
              (pause)="pauseJob($event)"
              (resume)="resumeJob($event)"
              (delete)="removeJob($event)"
            />
          }
        </div>
      }

      @if (selectedJob(); as job) {
        <qt-task-details-modal
          [job]="job"
          [busy]="jobActionLoading() === job.id"
          (close)="selectedJob.set(null)"
          (delete)="removeJob($event)"
        />
      }
    </div>
  `,
})
export class TasksQueueCard {
  private readonly core = inject(CoreClient);

  protected readonly min = MIN_CONCURRENCY;
  protected readonly max = MAX_CONCURRENCY;

  protected readonly autoRefresh = signal(false);
  protected readonly controlLoading = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly selectedJob = signal<FullJobDetail | null>(null);
  protected readonly jobActionLoading = signal<string | null>(null);
  private readonly dragConcurrency = signal<number | null>(null);

  protected readonly query = injectQuery(() => ({
    queryKey: tasksQueueKeys.all,
    queryFn: () => fetchTasksQueue(this.core),
    refetchInterval: this.autoRefresh() ? POLL_MS : (false as const),
  }));

  protected readonly data = computed<QueueData | undefined>(() => this.query.data());
  protected readonly jobs = computed(() => this.data()?.jobs ?? []);
  protected readonly running = computed(() => this.data()?.processorStatus?.running === true);
  protected readonly activeTotal = computed(() => this.data()?.stats?.activeTotal ?? 0);
  protected readonly totalTokens = computed(() =>
    formatTokens(this.data()?.totalEstimatedTokens ?? 0),
  );

  private readonly persistedConcurrency = computed(
    () => this.data()?.maxConcurrentJobs ?? DEFAULT_CONCURRENCY,
  );
  protected readonly displayConcurrency = computed(
    () => this.dragConcurrency() ?? this.persistedConcurrency(),
  );

  protected onDragStart(): void {
    this.dragConcurrency.set(this.persistedConcurrency());
  }

  protected onDragInput(event: Event): void {
    this.dragConcurrency.set(parseInt((event.target as HTMLInputElement).value, 10));
  }

  /** v4 `handleConcurrencyCommit` — persist only a real change on mouse/touch up. */
  protected async onDragCommit(): Promise<void> {
    const value = this.dragConcurrency();
    this.dragConcurrency.set(null);
    if (value !== null && value !== this.persistedConcurrency()) {
      this.error.set(null);
      try {
        await setJobConcurrency(this.core, value);
        await this.query.refetch();
      } catch (err) {
        this.error.set(errText(err, 'Failed to set concurrency'));
      }
    }
  }

  protected async control(action: 'start' | 'stop'): Promise<void> {
    this.controlLoading.set(true);
    this.error.set(null);
    try {
      await controlTasksQueue(this.core, action);
      await this.query.refetch();
    } catch (err) {
      this.error.set(errText(err, `Failed to ${action} queue`));
    } finally {
      this.controlLoading.set(false);
    }
  }

  protected async viewJob(jobId: string): Promise<void> {
    this.jobActionLoading.set(jobId);
    try {
      const job = await fetchJob(this.core, jobId);
      this.selectedJob.set(job);
    } catch {
      /* v4 logs only for job actions */
    } finally {
      this.jobActionLoading.set(null);
    }
  }

  protected async pauseJob(jobId: string): Promise<void> {
    await this.jobAction(jobId, () => controlJob(this.core, jobId, 'pause'));
  }

  protected async resumeJob(jobId: string): Promise<void> {
    await this.jobAction(jobId, () => controlJob(this.core, jobId, 'resume'));
  }

  protected async removeJob(jobId: string): Promise<void> {
    await this.jobAction(jobId, async () => {
      await deleteJob(this.core, jobId);
      // v4 `:183-186` — close the detail dialog if it showed this job.
      if (this.selectedJob()?.id === jobId) this.selectedJob.set(null);
    });
  }

  private async jobAction(jobId: string, run: () => Promise<void>): Promise<void> {
    this.jobActionLoading.set(jobId);
    try {
      await run();
      await this.query.refetch();
    } catch {
      /* v4 logs only for job actions (no error banner) */
    } finally {
      this.jobActionLoading.set(null);
    }
  }
}
