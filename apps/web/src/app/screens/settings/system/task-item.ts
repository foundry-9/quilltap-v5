import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { Icon } from '../../../ui/icon';
import { formatRelativeDate, formatTokens, type JobDetail } from './tasks-queue.api';

/**
 * One background-job row (v4 `components/tools/tasks-queue/TaskItem.tsx`): a
 * status glyph + type name + priority badge, a meta line (character / attempts
 * / scheduled), the token estimate, and the per-job action buttons. Pause/Resume
 * is status-gated (PAUSED → Resume; PENDING/FAILED → Pause; PROCESSING shows
 * neither); Delete is hidden while PROCESSING (v4 `:124-163`).
 */
@Component({
  selector: 'qt-task-item',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="qt-card p-3 hover:qt-bg-muted/50 transition-colors">
      <div class="flex items-start justify-between gap-3">
        <div class="flex items-start gap-2 min-w-0 flex-1">
          <span [class]="statusColor()">
            @switch (job().status) {
              @case ('PROCESSING') {
                <div class="animate-spin rounded-full h-4 w-4 border-b-2 border-current"></div>
              }
              @case ('PENDING') {
                <qt-icon name="clock" class="w-4 h-4" />
              }
              @case ('FAILED') {
                <qt-icon name="alert-triangle" class="w-4 h-4" />
              }
              @case ('PAUSED') {
                <qt-icon name="pause" class="w-4 h-4" />
              }
            }
          </span>
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="qt-text-primary">{{ job().typeName }}</span>
              @if (job().priority > 0) {
                <span class="text-xs px-1.5 py-0.5 qt-bg-primary/10 text-primary rounded"
                  >P{{ job().priority }}</span
                >
              }
            </div>
            <div class="qt-text-xs mt-0.5">
              @if (job().characterName) {
                <span class="mr-2">Character: {{ job().characterName }}</span>
              }
              @if (job().attempts > 0) {
                <span class="mr-2">Attempt {{ job().attempts }}/{{ job().maxAttempts }}</span>
              }
              <span>{{ scheduled() }}</span>
            </div>
            @if (job().lastError) {
              <div class="text-xs qt-text-destructive mt-1 truncate">Error: {{ job().lastError }}</div>
            }
          </div>
        </div>

        <div class="flex items-center gap-2">
          <div class="text-right qt-text-xs whitespace-nowrap">~{{ tokens() }} tokens</div>
          <div class="flex items-center gap-1">
            @if (job().status === 'PAUSED') {
              <button
                type="button"
                class="p-1 rounded hover:qt-bg-success/10 qt-text-success"
                title="Resume"
                [disabled]="busy()"
                (click)="resume.emit(job().id)"
              >
                <qt-icon name="play" class="w-4 h-4" />
              </button>
            } @else if (job().status === 'PENDING' || job().status === 'FAILED') {
              <button
                type="button"
                class="p-1 rounded hover:qt-bg-warning/10 qt-text-warning"
                title="Pause"
                [disabled]="busy()"
                (click)="pause.emit(job().id)"
              >
                <qt-icon name="pause" class="w-4 h-4" />
              </button>
            }
            <button
              type="button"
              class="p-1 rounded hover:qt-bg-info/10 qt-text-info"
              title="View Details"
              [disabled]="busy()"
              (click)="view.emit(job().id)"
            >
              <qt-icon name="eye" class="w-4 h-4" />
            </button>
            @if (job().status !== 'PROCESSING') {
              <button
                type="button"
                class="p-1 rounded hover:qt-bg-destructive/10 qt-text-destructive"
                title="Delete"
                [disabled]="busy()"
                (click)="delete.emit(job().id)"
              >
                <qt-icon name="trash" class="w-4 h-4" />
              </button>
            }
            @if (busy()) {
              <div class="animate-spin rounded-full h-4 w-4 border-b-2 border-current qt-text-secondary"></div>
            }
          </div>
        </div>
      </div>
    </div>
  `,
})
export class TaskItem {
  readonly job = input.required<JobDetail>();
  /** True while THIS job has an action in flight (v4 `jobActionLoading === id`). */
  readonly busy = input(false);

  readonly view = output<string>();
  readonly pause = output<string>();
  readonly resume = output<string>();
  readonly delete = output<string>();

  /** v4 `getStatusColor` (`:36-49`). */
  protected readonly statusColor = computed(() => {
    switch (this.job().status) {
      case 'PROCESSING':
        return 'qt-text-info';
      case 'PENDING':
        return 'qt-text-warning';
      case 'FAILED':
        return 'qt-text-destructive';
      case 'PAUSED':
        return 'qt-text-warning';
      default:
        return 'qt-text-secondary';
    }
  });

  protected readonly tokens = computed(() => formatTokens(this.job().estimatedTokens));
  protected readonly scheduled = computed(() => formatRelativeDate(this.job().scheduledAt));
}
