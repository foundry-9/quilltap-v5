import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { Icon } from '../../../ui/icon';
import { Modal } from '../../../ui/modal';
import { formatRelativeDate, type FullJobDetail } from './tasks-queue.api';

/**
 * The job-detail modal (v4 `components/tools/tasks-queue/TaskDetails.tsx`):
 * status / priority / attempts / scheduled, the last error, and the raw job
 * payload as pretty JSON. Delete is hidden while PROCESSING (v4 `:115`). Ported
 * over `qt-modal` (the v5 house dialog shell) rather than v4's bespoke overlay;
 * v4's title + `job.type` subtitle move to the modal title + a body line.
 */
@Component({
  selector: 'qt-task-details-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Modal],
  template: `
    <qt-modal title="Job Details" maxWidth="2xl" (close)="close.emit()">
      <p class="qt-text-small qt-text-secondary mb-4 -mt-1">{{ job().type }}</p>

      <div class="grid grid-cols-2 gap-4 mb-4">
        <div>
          <span class="qt-text-xs qt-text-secondary">Status</span>
          <div [class]="'font-medium ' + statusColor()">{{ job().status }}</div>
        </div>
        <div>
          <span class="qt-text-xs qt-text-secondary">Priority</span>
          <div class="font-medium">{{ job().priority }}</div>
        </div>
        <div>
          <span class="qt-text-xs qt-text-secondary">Attempts</span>
          <div class="font-medium">{{ job().attempts }} / {{ job().maxAttempts }}</div>
        </div>
        <div>
          <span class="qt-text-xs qt-text-secondary">Scheduled</span>
          <div class="font-medium text-sm">{{ scheduled() }}</div>
        </div>
      </div>

      @if (job().lastError) {
        <div class="mb-4">
          <span class="qt-text-xs qt-text-secondary">Last Error</span>
          <div class="text-sm qt-text-destructive qt-bg-destructive/10 p-2 rounded mt-1">
            {{ job().lastError }}
          </div>
        </div>
      }

      <div>
        <span class="qt-text-xs qt-text-secondary">Job Parameters</span>
        <pre
          class="mt-1 p-3 qt-bg-muted rounded text-xs overflow-x-auto whitespace-pre-wrap break-words"
          >{{ payloadJson() }}</pre
        >
      </div>

      <div qt-modal-footer class="flex items-center justify-between w-full">
        <button type="button" class="qt-button qt-button-secondary" (click)="close.emit()">
          Close
        </button>
        @if (job().status !== 'PROCESSING') {
          <button
            type="button"
            class="qt-button qt-button-destructive"
            [disabled]="busy()"
            (click)="delete.emit(job().id)"
          >
            @if (busy()) {
              <div class="animate-spin rounded-full h-4 w-4 border-b-2 border-current inline-block"></div>
            } @else {
              <qt-icon name="trash" class="w-4 h-4" />
            }
            Delete Job
          </button>
        }
      </div>
    </qt-modal>
  `,
})
export class TaskDetailsModal {
  readonly job = input.required<FullJobDetail>();
  readonly busy = input(false);

  readonly close = output<void>();
  readonly delete = output<string>();

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

  protected readonly scheduled = computed(() => formatRelativeDate(this.job().scheduledAt));
  protected readonly payloadJson = computed(() => JSON.stringify(this.job().payload, null, 2));
}
