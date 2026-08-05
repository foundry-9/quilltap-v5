import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  effect,
  inject,
  input,
  signal,
} from '@angular/core';

/**
 * The app's one segmented task-progress bar — a port of v4
 * `components/ui/ProgressBar.tsx` (HEAD `f7f1a956`).
 *
 * A bar of named segments, one per phase of whatever long operation is running.
 * Completed segments are full, the active one animates toward ~90% over its
 * estimated duration (never to 100%, so a slow phase reads as "still working"
 * rather than "finished and stuck"), and the rest sit empty.
 *
 * v4 lifted this out of its character optimizer when The Almanack needed the
 * same thing, which is why the segments are a prop rather than a module
 * constant. v5 has no optimizer surface, so the bar is born with the Almanack as
 * its only PHASED consumer; the two static value meters (Proving Bench outcome
 * shares, search-result importance) use the `qt-progress` CSS family directly,
 * exactly as v4's do.
 *
 * Styling comes entirely from the `qt-progress` class family — no raw colour
 * utilities — so a theme can restyle every bar in the app from two variables.
 *
 * ## The one shape difference
 *
 * v4 takes a `className` prop and merges it into its root element. Angular
 * applies a parent-supplied `class` to the host element natively, so there is
 * nothing for such an input to do: the root layout (`flex flex-col gap-2`) is a
 * host class here and consumers add their own beside it.
 */

/** v4 `ProgressSegment`. */
export interface ProgressSegment {
  /** Stable key; also what {@link ProgressBar.currentKey} is matched against. */
  key: string;
  /** Short label rendered under the bar. */
  label: string;
  /**
   * Roughly how long this phase takes. Only drives the fill animation — a
   * segment that outlives its estimate holds at 90% rather than overflowing.
   */
  estimatedDurationMs: number;
}

/** v4's tick cadence — the interval that recomputes fills and the elapsed timer. */
export const PROGRESS_TICK_MS = 500;

/**
 * How far the ACTIVE segment may fill. v4's central behavior claim: the running
 * phase never reaches 100%, so a phase that outlives its estimate reads as
 * still working rather than finished-and-stuck.
 */
export const ACTIVE_FILL_CAP = 0.9;

/**
 * The per-segment fill percentages for one tick (v4's `setFillPercents` body,
 * extracted so the never-100% cap and the finished state are pinnable without
 * a component).
 *
 * @param currentIndex index of the running segment, or -1 when none is
 * @param isDone       v4's "no current phase, but we did start" finished state
 * @param startTimes   when each segment first became current; a missing entry
 *                     falls back to `now` (v4's `?? now`), which yields 0
 * @param now          the tick's clock reading
 */
export function computeSegmentFills(
  segments: readonly ProgressSegment[],
  currentIndex: number,
  isDone: boolean,
  startTimes: Readonly<Record<string, number>>,
  now: number,
): number[] {
  return segments.map((segment, index) => {
    if (isDone || index < currentIndex) return 100;
    if (index !== currentIndex) return 0;
    const startTime = startTimes[segment.key] ?? now;
    return Math.min((now - startTime) / segment.estimatedDurationMs, ACTIVE_FILL_CAP) * 100;
  });
}

/** v4's `formatElapsed` (`ProgressBar.tsx:51-55`). */
export function formatElapsed(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return mins > 0 ? `${mins}m ${secs}s` : `${secs}s`;
}

/** One segment's rendering state for a tick. */
interface SegmentView {
  key: string;
  label: string;
  fillClass: string;
  indeterminate: boolean;
  /** `null` on the indeterminate segment — v4 omits the inline width there. */
  width: number | null;
  labelClass: string;
}

@Component({
  selector: 'qt-progress-bar',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'flex flex-col gap-2' },
  template: `
    <div class="flex gap-1">
      @for (view of views(); track view.key) {
        <div class="qt-progress flex-1" [title]="view.label">
          <div
            [class]="view.fillClass"
            [style.width.%]="view.width"
            [attr.data-segment]="view.key"
          ></div>
        </div>
      }
    </div>

    @if (!hideLabels()) {
      <div class="flex justify-between">
        <div class="flex gap-2">
          @for (view of views(); track view.key) {
            <span [class]="view.labelClass">{{ view.label }}</span>
          }
        </div>
        @if (startedAt() !== null) {
          <span class="text-[10px] qt-text-secondary tabular-nums">{{ elapsedLabel() }}</span>
        }
      </div>
    }
  `,
})
export class ProgressBar {
  readonly segments = input.required<readonly ProgressSegment[]>();
  /** The phase currently running, or `null` when finished (or not yet started). */
  readonly currentKey = input.required<string | null>();
  /** When the operation began; drives the elapsed timer. `null` hides it. */
  readonly startedAt = input.required<number | null>();
  /**
   * True between "started" and the first phase event, when there is nothing
   * honest to show a width for. Renders the sliding indeterminate animation.
   */
  readonly indeterminate = input(false);
  /** Hide the per-segment labels — for bars in tight rows. */
  readonly hideLabels = input(false);

  /**
   * When each segment first became current, so its fill animates from when the
   * phase actually started rather than from when the bar mounted (v4's
   * `segmentStartTimeRef`). Deliberately NOT a signal: v4 holds it in a ref and
   * the 500 ms tick is what re-reads it.
   */
  private readonly segmentStartTimes: Record<string, number> = {};

  /** The tick's clock reading. v4 recomputes everything from `Date.now()`. */
  private readonly now = signal(0);
  /**
   * v4 seeds `fillPercents` to `{}` and `elapsed` to `0`, so the first paint is
   * all-empty and the first tick fills it in. Carried: `ticked` gates the fills
   * on the first interval firing.
   */
  private readonly ticked = signal(false);
  private readonly elapsedSeconds = signal(0);

  constructor() {
    // Stamp each segment the first time it becomes current (v4's `useEffect` on
    // `currentKey`). The tick's `?? now` fallback covers the sliver before this
    // runs, exactly as it does in v4.
    effect(() => {
      const key = this.currentKey();
      if (key && !this.segmentStartTimes[key]) {
        this.segmentStartTimes[key] = Date.now();
      }
    });

    const handle = setInterval(() => {
      const now = Date.now();
      this.now.set(now);
      this.ticked.set(true);
      const startedAt = this.startedAt();
      if (startedAt) {
        this.elapsedSeconds.set(Math.floor((now - startedAt) / 1000));
      }
    }, PROGRESS_TICK_MS);
    inject(DestroyRef).onDestroy(() => clearInterval(handle));
  }

  private readonly currentIndex = computed(() => {
    const key = this.currentKey();
    return key ? this.segments().findIndex((s) => s.key === key) : -1;
  });

  /**
   * v4's `finished`: "no current phase, but we did start" — as opposed to "no
   * current phase because nothing has begun".
   */
  private readonly finished = computed(() => this.currentKey() === null && this.startedAt() !== null);

  protected readonly elapsedLabel = computed(() => formatElapsed(this.elapsedSeconds()));

  protected readonly views = computed<SegmentView[]>(() => {
    const segments = this.segments();
    const currentIndex = this.currentIndex();
    const finished = this.finished();
    const indeterminate = this.indeterminate();

    const fills = this.ticked()
      ? computeSegmentFills(
          segments,
          currentIndex,
          finished,
          this.segmentStartTimes,
          this.now(),
        )
      : segments.map(() => 0);

    return segments.map((segment, index) => {
      const isActive = index === currentIndex;
      const isDone = finished || index < currentIndex;
      const showIndeterminate = indeterminate && index === 0;

      const fillState = isDone
        ? 'qt-progress-fill-success'
        : isActive || showIndeterminate
          ? ''
          : 'qt-progress-fill-idle';

      return {
        key: segment.key,
        label: segment.label,
        fillClass:
          `qt-progress-fill ${fillState} ${showIndeterminate ? 'qt-progress-indeterminate' : ''}`.trim(),
        indeterminate: showIndeterminate,
        width: showIndeterminate ? null : (fills[index] ?? 0),
        labelClass: `text-[10px] ${
          isActive ? 'text-primary font-medium' : isDone ? 'qt-text-success' : 'qt-text-secondary'
        }`,
      };
    });
  });
}
