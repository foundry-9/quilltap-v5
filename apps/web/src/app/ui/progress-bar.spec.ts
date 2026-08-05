import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  ACTIVE_FILL_CAP,
  ProgressBar,
  computeSegmentFills,
  formatElapsed,
  type ProgressSegment,
} from './progress-bar';

/**
 * The shared segmented progress bar (v4 `components/ui/ProgressBar.tsx`).
 *
 * The behavior claims worth pinning are the ones a reimplementation quietly
 * breaks: the ACTIVE segment never reaches 100% (v4's whole reason for the 0.9
 * cap — "a slow phase reads as still working, not finished-and-stuck"), the
 * finished state fills everything, and the indeterminate mode paints segment
 * ZERO only.
 */

const SEGMENTS: ProgressSegment[] = [
  { key: 'a', label: 'Alpha', estimatedDurationMs: 1000 },
  { key: 'b', label: 'Beta', estimatedDurationMs: 1000 },
  { key: 'c', label: 'Gamma', estimatedDurationMs: 1000 },
];

describe('computeSegmentFills (v4’s per-tick fill math)', () => {
  it('caps the ACTIVE segment at 90% no matter how long the phase runs', () => {
    // Ten times its estimate: v4 still refuses to draw a full bar.
    const fills = computeSegmentFills(SEGMENTS, 1, false, { b: 0 }, 10_000);
    expect(fills[1]).toBe(ACTIVE_FILL_CAP * 100);
    expect(fills[1]).toBeLessThan(100);
  });

  it('fills the active segment proportionally before the cap bites', () => {
    expect(computeSegmentFills(SEGMENTS, 1, false, { b: 0 }, 250)[1]).toBeCloseTo(25, 10);
    expect(computeSegmentFills(SEGMENTS, 1, false, { b: 0 }, 500)[1]).toBeCloseTo(50, 10);
  });

  it('completes everything before the active one and empties everything after', () => {
    expect(computeSegmentFills(SEGMENTS, 1, false, { b: 0 }, 250)).toEqual([100, 25, 0]);
  });

  it('an unstamped active segment reads 0, not NaN (v4’s `?? now`)', () => {
    expect(computeSegmentFills(SEGMENTS, 0, false, {}, 9_999)[0]).toBe(0);
  });

  it('the finished state fills every segment, including one never entered', () => {
    expect(computeSegmentFills(SEGMENTS, -1, true, {}, 1_000)).toEqual([100, 100, 100]);
  });

  it('before anything starts (no current phase, not finished) all segments are empty', () => {
    expect(computeSegmentFills(SEGMENTS, -1, false, {}, 1_000)).toEqual([0, 0, 0]);
  });
});

describe('formatElapsed (v4 ProgressBar.tsx:51-55)', () => {
  it('drops the minutes component below a minute', () => {
    expect(formatElapsed(0)).toBe('0s');
    expect(formatElapsed(59)).toBe('59s');
  });

  it('renders minutes and seconds past a minute', () => {
    expect(formatElapsed(60)).toBe('1m 0s');
    expect(formatElapsed(125)).toBe('2m 5s');
  });
});

@Component({
  imports: [ProgressBar],
  template: `
    <qt-progress-bar
      [segments]="segments"
      [currentKey]="currentKey()"
      [startedAt]="startedAt()"
      [indeterminate]="indeterminate()"
      [hideLabels]="hideLabels()"
    />
  `,
})
class Host {
  readonly segments = SEGMENTS;
  readonly currentKey = signal<string | null>(null);
  readonly startedAt = signal<number | null>(null);
  readonly indeterminate = signal(false);
  readonly hideLabels = signal(false);
}

describe('ProgressBar (rendering)', () => {
  let fixture: ComponentFixture<Host>;

  beforeEach(() => {
    vi.useFakeTimers();
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [Host] });
    fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function fills(): HTMLElement[] {
    return Array.from(fixture.nativeElement.querySelectorAll('[data-segment]'));
  }

  /** Advance past one 500 ms tick and re-render. */
  function tick(ms = 600): void {
    vi.advanceTimersByTime(ms);
    fixture.detectChanges();
  }

  it('renders one track per segment, each titled with its label', () => {
    const tracks = fixture.nativeElement.querySelectorAll('.qt-progress');
    expect(tracks.length).toBe(3);
    expect(tracks[0].getAttribute('title')).toBe('Alpha');
  });

  it('indeterminate slides segment ZERO only, with no inline width', () => {
    fixture.componentInstance.indeterminate.set(true);
    tick();

    const [first, second, third] = fills();
    expect(first.className).toContain('qt-progress-indeterminate');
    // v4 passes `style={showIndeterminate ? undefined : {width}}` — the class's
    // `width: 40% !important` is what paints it.
    expect(first.style.width).toBe('');
    expect(second.className).not.toContain('qt-progress-indeterminate');
    expect(second.className).toContain('qt-progress-fill-idle');
    expect(third.className).toContain('qt-progress-fill-idle');
  });

  /**
   * A v4 QUIRK, carried deliberately. The Almanack card passes
   * `indeterminate={currentPhase === null}` together with a non-null
   * `startedAt`, and v4's `finished` is exactly `currentKey === null &&
   * startedAt !== null` — so during the window between "generate pressed" and
   * the first phase frame, every segment is treated as DONE: the tail renders
   * full green while segment zero slides over the top of it. It reads as
   * "finished" for a beat before the run visibly starts.
   *
   * v4 has shipped it this way; a port that quietly "fixed" it would diverge on
   * the most-seen frame of the whole feature, so it is pinned instead.
   */
  it('carries v4’s indeterminate-with-startedAt quirk: the tail renders as done', () => {
    fixture.componentInstance.indeterminate.set(true);
    fixture.componentInstance.startedAt.set(Date.now());
    tick();

    const [first, second, third] = fills();
    expect(first.className).toContain('qt-progress-indeterminate');
    expect(second.className).toContain('qt-progress-fill-success');
    expect(second.style.width).toBe('100%');
    expect(third.className).toContain('qt-progress-fill-success');
  });

  it('the ACTIVE segment’s rendered width never reaches 100%', () => {
    const start = Date.now();
    fixture.componentInstance.startedAt.set(start);
    fixture.componentInstance.currentKey.set('b');
    fixture.detectChanges();

    // Far past the segment's 1 s estimate.
    tick(30_000);

    const [alpha, beta, gamma] = fills();
    expect(alpha.style.width).toBe('100%');
    expect(beta.style.width).toBe('90%');
    expect(gamma.style.width).toBe('0%');
  });

  it('finishing (currentKey → null with a startedAt) fills every segment as success', () => {
    fixture.componentInstance.startedAt.set(Date.now());
    fixture.componentInstance.currentKey.set('b');
    tick();

    fixture.componentInstance.currentKey.set(null);
    tick();

    for (const fill of fills()) {
      expect(fill.style.width).toBe('100%');
      expect(fill.className).toContain('qt-progress-fill-success');
    }
  });

  it('the elapsed timer appears only with a startedAt, and counts in v4’s format', () => {
    expect(fixture.nativeElement.querySelector('.tabular-nums')).toBeNull();

    fixture.componentInstance.startedAt.set(Date.now());
    fixture.detectChanges();
    tick(65_000);

    expect(fixture.nativeElement.querySelector('.tabular-nums')?.textContent?.trim()).toBe('1m 5s');
  });

  it('hideLabels drops the whole label row (timer included)', () => {
    fixture.componentInstance.startedAt.set(Date.now());
    fixture.componentInstance.hideLabels.set(true);
    tick();

    expect(fixture.nativeElement.textContent).not.toContain('Alpha');
    expect(fixture.nativeElement.querySelector('.tabular-nums')).toBeNull();
    // The bar itself is untouched.
    expect(fills().length).toBe(3);
  });

  it('label colouring tracks done / active / pending', () => {
    fixture.componentInstance.startedAt.set(Date.now());
    fixture.componentInstance.currentKey.set('b');
    tick();

    const host = fixture.nativeElement as HTMLElement;
    const labels = Array.from(host.querySelectorAll<HTMLElement>('span.text-\\[10px\\]')).filter(
      (el) => !el.className.includes('tabular-nums'),
    );
    expect(labels[0].className).toContain('qt-text-success');
    expect(labels[1].className).toContain('text-primary');
    expect(labels[2].className).toContain('qt-text-secondary');
  });

  it('stops ticking once destroyed (no interval left running)', () => {
    fixture.componentInstance.startedAt.set(Date.now());
    tick();
    fixture.destroy();
    expect(vi.getTimerCount()).toBe(0);
  });
});
