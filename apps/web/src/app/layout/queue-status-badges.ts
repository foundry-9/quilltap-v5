import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  OnInit,
  inject,
  signal,
} from '@angular/core';
import { Router, NavigationEnd } from '@angular/router';

import { apiUrl } from '../core/api-url';
import {
  QUEUE_CHANGE_EVENT,
  QUEUE_TYPES,
  getQueueCount,
  hasActiveJobs,
  notifyQueueChange,
} from './queue-status.logic';

/** v4 `POLL_INTERVAL` (:76). */
const POLL_INTERVAL = 5000;

/**
 * The queue-status badges (v4 `components/layout/queue-status-badges.tsx`) —
 * five compact badges (Mem/Emb/Sum/Dgr/Img) over `/api/v1/system/jobs`'s
 * `activeByType`. Polling is EVENT-driven, not always-on: it starts on the
 * `quilltap:queue-change` window event (fired by `notifyQueueChange()` from
 * any code that enqueues jobs, on route change, and on mount) and stops on its
 * own once every count is zero. Badges are always rendered, dimmed
 * `qt-queue-badge-idle` at 0 (the CSS has been waiting since the m6 survey).
 *
 * Route changes reproduce v4's usePathname effect: stop the current poll, then
 * re-fire the event so a fresh check decides whether to resume.
 */
@Component({
  selector: 'qt-queue-status-badges',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-queue-badge-group" title="Background job queues">
      @for (queue of queueTypes; track queue.key) {
        <span
          [class]="queue.badgeClass + (count(queue.jobTypes) === 0 ? ' qt-queue-badge-idle' : '')"
          [title]="queue.title + ': ' + count(queue.jobTypes) + ' active'"
        >
          <span>{{ queue.label }}</span>
          <span>{{ count(queue.jobTypes) }}</span>
        </span>
      }
    </div>
  `,
})
export class QueueStatusBadges implements OnInit {
  private readonly router = inject(Router);
  private readonly destroyRef = inject(DestroyRef);

  protected readonly queueTypes = QUEUE_TYPES;
  private readonly activeByType = signal<Record<string, number>>({});

  private intervalHandle: ReturnType<typeof setInterval> | null = null;
  private destroyed = false;

  protected count(jobTypes: readonly string[]): number {
    return getQueueCount(this.activeByType(), jobTypes);
  }

  ngOnInit(): void {
    const handleQueueChange = () => void this.checkAndPoll();
    window.addEventListener(QUEUE_CHANGE_EVENT, handleQueueChange);

    // v4's usePathname effect (:145-151): a navigation stops the current poll
    // and re-fires the event, so a fresh check decides whether to resume.
    const sub = this.router.events.subscribe((event) => {
      if (event instanceof NavigationEnd) {
        this.stopPolling();
        notifyQueueChange();
      }
    });

    this.destroyRef.onDestroy(() => {
      this.destroyed = true;
      this.stopPolling();
      window.removeEventListener(QUEUE_CHANGE_EVENT, handleQueueChange);
      sub.unsubscribe();
    });

    // v4 :163 — the initial check on mount rides the same event.
    notifyQueueChange();
  }

  /** v4 `fetchStatus` (:105-124) — no-store, failures collapse to `{}`. */
  private async fetchStatus(): Promise<Record<string, number>> {
    try {
      const res = await fetch(apiUrl('/api/v1/system/jobs'), {
        cache: 'no-store',
        headers: { 'Cache-Control': 'no-cache, no-store, must-revalidate' },
      });
      if (!res.ok) return {};
      const data = (await res.json()) as { activeByType?: Record<string, number> };
      const counts = data.activeByType || {};
      if (!this.destroyed) {
        this.activeByType.set(counts);
      }
      return counts;
    } catch {
      return {};
    }
  }

  /** v4 `startPolling` (:126-136) — stops itself when every count hits zero. */
  private startPolling(): void {
    if (this.intervalHandle) return;
    this.intervalHandle = setInterval(() => {
      void this.fetchStatus().then((counts) => {
        if (!hasActiveJobs(counts)) this.stopPolling();
      });
    }, POLL_INTERVAL);
  }

  private stopPolling(): void {
    if (this.intervalHandle) {
      clearInterval(this.intervalHandle);
      this.intervalHandle = null;
    }
  }

  /** v4 `checkAndPoll` (:138-143). */
  private async checkAndPoll(): Promise<void> {
    const counts = await this.fetchStatus();
    if (hasActiveJobs(counts)) {
      this.startPolling();
    }
  }
}
