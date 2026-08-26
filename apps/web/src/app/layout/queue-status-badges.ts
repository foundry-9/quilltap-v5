import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  effect,
  inject,
  signal,
  untracked,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { RealtimeService } from '../core/realtime.service';
import {
  ACTIVITY_CHIPS,
  type ActivityCounts,
  type ActivityKind,
  blippedKinds,
  coerceCounts,
  emptyActivityCounts,
  hasActivity,
} from './activity-kinds';
import { QUEUE_CHANGE_EVENT } from './queue-status.logic';
import { fetchActivitySnapshot, systemJobsKeys } from './system-jobs.api';

/** v4 `ACTIVE_POLL_INTERVAL` — fallback cadence while something is in flight. */
const ACTIVE_POLL_INTERVAL = 1_500;
/** v4 `IDLE_POLL_INTERVAL` — fallback cadence while everything is idle. */
const IDLE_POLL_INTERVAL = 8_000;
/** v4 `PULSE_DURATION` — how long a chip keeps pulsing after a blip. */
const PULSE_DURATION = 1_200;

/**
 * Queue Status Badges (v4 `components/layout/queue-status-badges.tsx`).
 *
 * The compact chip group in the page toolbar — "Mem", "Emb", "Sum", "Dgr",
 * "Img" — reporting how much work of each kind is in flight right now.
 *
 * What a chip counts is defined once, server-side and client-side alike: active
 * `background_jobs` rows mapped to their kind, plus non-job work registered
 * with the activity registry (the inline image tool, the Concierge classifier,
 * live embedding calls). A chip is lit for the entire span of the work it
 * names, from the first token of prompt crafting through to the result landing.
 *
 * How the counts arrive:
 * - **Push, normally.** The server publishes a `jobs` hint from every queue
 *   chokepoint — enqueue, claim, complete, fail, cancel, and both edges of an
 *   activity span — and the realtime hub invalidates this query. The bus
 *   coalesces, so a thousand-job reindex is a stream of hints the chips can
 *   actually keep up with.
 * - **Polling, as the fallback.** While the channel is down the adaptive
 *   heartbeat comes back: a fast tick while something is in flight, a slow one
 *   while everything is idle. A dropped connection costs latency, not
 *   correctness.
 * - `notifyQueueChange()` remains as an instant same-tab kick after a
 *   known-enqueuing action, but nothing depends on it any more.
 *
 * Work that starts and finishes between two reads would otherwise be invisible,
 * so the API also returns a monotonic `startedByKind` counter; a chip that has
 * advanced since the previous read pulses even if its live count is back to
 * zero. That is the missed-event insurance this design wants, push or no push.
 */
@Component({
  selector: 'qt-queue-status-badges',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-queue-badge-group" title="Background activity">
      @for (chip of chips; track chip.kind) {
        <span
          [class]="chipClass(chip.badgeClass, chip.kind)"
          [title]="chip.title + ': ' + counts()[chip.kind] + ' active'"
        >
          <span>{{ chip.label }}</span>
          <span>{{ counts()[chip.kind] }}</span>
        </span>
      }
    </div>
  `,
})
export class QueueStatusBadges {
  private readonly realtime = inject(RealtimeService);
  private readonly queryClient = injectQueryClient();
  private readonly destroyRef = inject(DestroyRef);

  protected readonly chips = ACTIVITY_CHIPS;

  private readonly pulsing = signal<ReadonlySet<ActivityKind>>(new Set());
  private previousStarted: ActivityCounts | null = null;
  private readonly pulseTimers = new Map<ActivityKind, ReturnType<typeof setTimeout>>();

  private readonly query = injectQuery(() => ({
    queryKey: systemJobsKeys.all,
    queryFn: ({ signal }: { signal: AbortSignal }) => fetchActivitySnapshot(signal),
    // Counts are a live readout; a cached one is worse than none.
    staleTime: 0,
    // The old adaptive heartbeat, kept whole but gated. Reading the cadence off
    // the query's own last response is what lets it stay adaptive without a
    // second timer racing the first (v4's `refetchInterval` function form).
    refetchInterval: this.realtime.connected()
      ? (false as const)
      : (query: { state: { data?: { activeByKind?: unknown } } }) =>
          hasActivity(coerceCounts(query.state.data?.activeByKind))
            ? ACTIVE_POLL_INTERVAL
            : IDLE_POLL_INTERVAL,
    // Keep the last good snapshot on a transient error rather than blanking
    // every chip, matching the old fetch-returns-null behaviour.
    retry: false,
  }));

  protected readonly counts = computed<ActivityCounts>(() =>
    this.query.data() ? coerceCounts(this.query.data()?.activeByKind) : emptyActivityCounts(),
  );

  constructor() {
    // A kind whose monotonic completed-span counter advanced did work since the
    // last read — pulse it even though the work has already finished. The
    // counter resets when the server restarts, so a decrease is a fresh
    // baseline rather than a blip, and the first read is a delta base.
    effect(() => {
      const data = this.query.data();
      if (!data) return;
      untracked(() => this.notePulses(coerceCounts(data.startedByKind)));
    });

    // Same-tab zero-latency kick, unchanged in spirit: an action that just
    // enqueued something invalidates instead of driving a bespoke re-poll.
    const onQueueChange = () => {
      void this.queryClient.invalidateQueries({ queryKey: systemJobsKeys.all });
    };
    window.addEventListener(QUEUE_CHANGE_EVENT, onQueueChange);

    this.destroyRef.onDestroy(() => {
      window.removeEventListener(QUEUE_CHANGE_EVENT, onQueueChange);
      // Clear any pulse timers still running when the toolbar goes away.
      for (const timer of this.pulseTimers.values()) clearTimeout(timer);
      this.pulseTimers.clear();
    });
  }

  protected chipClass(badgeClass: string, kind: ActivityKind): string {
    return [
      badgeClass,
      this.counts()[kind] === 0 ? 'qt-queue-badge-idle' : '',
      this.pulsing().has(kind) ? 'qt-queue-badge-pulse' : '',
    ]
      .filter(Boolean)
      .join(' ');
  }

  private notePulses(started: ActivityCounts): void {
    const previous = this.previousStarted;
    this.previousStarted = started;
    const blipped = blippedKinds(previous, started);
    if (blipped.length === 0) return;

    this.pulsing.update((prev) => {
      const next = new Set(prev);
      for (const kind of blipped) next.add(kind);
      return next;
    });

    for (const kind of blipped) {
      const existing = this.pulseTimers.get(kind);
      if (existing) clearTimeout(existing);
      this.pulseTimers.set(
        kind,
        setTimeout(() => {
          this.pulseTimers.delete(kind);
          this.pulsing.update((prev) => {
            if (!prev.has(kind)) return prev;
            const next = new Set(prev);
            next.delete(kind);
            return next;
          });
        }, PULSE_DURATION),
      );
    }
  }
}
