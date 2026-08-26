import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ConnectionState } from '../core/core-transport';
import type { ScopedEvent } from '../core/core-contract';
import { QueueStatusBadges } from './queue-status-badges';
import { notifyQueueChange } from './queue-status.logic';

/**
 * The chips' behavioral specs, rewritten for `f3892158d`'s cadence (P4.D125).
 *
 * The family this replaces covered the pre-rewrite mechanism — an event-driven
 * `setInterval` that woke on `notifyQueueChange()` and stopped itself at zero.
 * Every one of its assertions has a successor here at equal or greater
 * strength: the adaptive cadence (1.5 s busy / 8 s idle, and that it is GATED
 * on the live channel, which the old poller had no notion of), the same-tab
 * kick still causing a re-read, the five badges dimmed at zero with v4's
 * tooltip shape, and a failed read leaving the last good snapshot alone rather
 * than collapsing to `{}` — the one behavior the rewrite deliberately changes,
 * pinned in its new form.
 */

interface JobsBody {
  activeByKind?: Record<string, number>;
  startedByKind?: Record<string, number>;
}

function jobsResponse(body: JobsBody): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

class FakeCore {
  readonly frames = new Subject<ScopedEvent>();
  readonly events$ = this.frames.asObservable();
  readonly connection = signal<ConnectionState>('idle');
  readonly resyncCounter = signal(0);
}

let core: FakeCore;
let fetchMock: ReturnType<typeof vi.fn>;

function render() {
  core = new FakeCore();
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [QueueStatusBadges],
    providers: [
      provideTanStackQuery(
        new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 0 } } }),
      ),
      { provide: CoreClient, useValue: core },
    ],
  });
  const fixture = TestBed.createComponent(QueueStatusBadges);
  fixture.detectChanges();
  return fixture;
}

/** Let the query's fetch chain settle under fake timers. */
async function settle(): Promise<void> {
  await vi.advanceTimersByTimeAsync(0);
  await vi.advanceTimersByTimeAsync(0);
}

function badges(fixture: { nativeElement: HTMLElement }): HTMLElement[] {
  return [...fixture.nativeElement.querySelectorAll('.qt-queue-badge-group > span')] as HTMLElement[];
}

describe('QueueStatusBadges (v4 f3892158d — pushed, with a gated heartbeat)', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    fetchMock = vi.fn(async () => jobsResponse({}));
    vi.stubGlobal('fetch', fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('renders the five chips dimmed at zero, with v4\'s tooltip shape', async () => {
    const fixture = render();
    await settle();
    fixture.detectChanges();
    const rendered = badges(fixture);
    expect(rendered).toHaveLength(5);
    expect(rendered.map((b) => b.textContent?.trim().slice(0, 3))).toEqual([
      'Mem',
      'Emb',
      'Sum',
      'Dgr',
      'Img',
    ]);
    for (const badge of rendered) {
      expect(badge.classList.contains('qt-queue-badge-idle')).toBe(true);
    }
    expect(rendered[0].title).toBe(
      'Memory work (extraction, regeneration, housekeeping): 0 active',
    );
  });

  it('lights a chip from `activeByKind` — never `activeByType`', async () => {
    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: { memory: 3 }, startedByKind: {} }),
    );
    const fixture = render();
    await settle();
    fixture.detectChanges();
    const mem = fixture.nativeElement.querySelector('.qt-queue-badge-memory') as HTMLElement;
    expect(mem.classList.contains('qt-queue-badge-idle')).toBe(false);
    expect(mem.textContent).toContain('3');
  });

  it('asks for the plain snapshot — no `includeByType`', async () => {
    render();
    await settle();
    expect(fetchMock).toHaveBeenCalled();
    const url = String(fetchMock.mock.calls[0][0]);
    expect(url).toContain('/api/v1/system/jobs');
    expect(url).not.toContain('includeByType');
  });

  it('polls the BUSY cadence (1.5 s) while something is in flight', async () => {
    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: { memory: 1 }, startedByKind: {} }),
    );
    render();
    await settle();
    const afterFirst = fetchMock.mock.calls.length;
    await vi.advanceTimersByTimeAsync(1_600);
    expect(fetchMock.mock.calls.length).toBeGreaterThan(afterFirst);
  });

  it('polls the IDLE cadence (8 s) while nothing is — not 1.5 s', async () => {
    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: {}, startedByKind: {} }),
    );
    render();
    await settle();
    const afterFirst = fetchMock.mock.calls.length;
    await vi.advanceTimersByTimeAsync(2_000);
    expect(fetchMock.mock.calls.length).toBe(afterFirst);
    await vi.advanceTimersByTimeAsync(6_500);
    expect(fetchMock.mock.calls.length).toBeGreaterThan(afterFirst);
  });

  it('STOPS polling while the live channel is up, and resumes when it drops', async () => {
    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: { memory: 1 }, startedByKind: {} }),
    );
    const fixture = render();
    await settle();
    core.connection.set('open');
    fixture.detectChanges();
    await settle();
    const whenConnected = fetchMock.mock.calls.length;
    await vi.advanceTimersByTimeAsync(20_000);
    expect(fetchMock.mock.calls.length).toBe(whenConnected);

    core.connection.set('reconnecting');
    fixture.detectChanges();
    await vi.advanceTimersByTimeAsync(1_600);
    expect(fetchMock.mock.calls.length).toBeGreaterThan(whenConnected);
  });

  it('a `jobs` hint re-reads the snapshot without any timer running', async () => {
    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: {}, startedByKind: {} }),
    );
    const fixture = render();
    await settle();
    core.connection.set('open');
    fixture.detectChanges();
    await settle();
    const before = fetchMock.mock.calls.length;
    core.frames.next({ v: 1, topic: 'jobs', at: 1 } as unknown as ScopedEvent);
    await settle();
    expect(fetchMock.mock.calls.length).toBeGreaterThan(before);
  });

  it('notifyQueueChange() still forces an immediate re-read (the same-tab kick)', async () => {
    render();
    await settle();
    const before = fetchMock.mock.calls.length;
    notifyQueueChange();
    await settle();
    expect(fetchMock.mock.calls.length).toBeGreaterThan(before);
  });

  it('pulses a chip whose startedByKind advanced, then clears it after 1.2 s', async () => {
    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: {}, startedByKind: { summary: 1 } }),
    );
    const fixture = render();
    await settle();
    fixture.detectChanges();
    const sum = () => fixture.nativeElement.querySelector('.qt-queue-badge-summary') as HTMLElement;
    // First read is the delta base — no pulse.
    expect(sum().classList.contains('qt-queue-badge-pulse')).toBe(false);

    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: {}, startedByKind: { summary: 2 } }),
    );
    notifyQueueChange();
    await settle();
    fixture.detectChanges();
    expect(sum().classList.contains('qt-queue-badge-pulse')).toBe(true);

    await vi.advanceTimersByTimeAsync(1_300);
    fixture.detectChanges();
    expect(sum().classList.contains('qt-queue-badge-pulse')).toBe(false);
  });

  it('does NOT pulse when the counter falls back (a server restart)', async () => {
    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: {}, startedByKind: { image: 40 } }),
    );
    const fixture = render();
    await settle();
    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: {}, startedByKind: { image: 0 } }),
    );
    notifyQueueChange();
    await settle();
    fixture.detectChanges();
    const img = fixture.nativeElement.querySelector('.qt-queue-badge-story') as HTMLElement;
    expect(img.classList.contains('qt-queue-badge-pulse')).toBe(false);
  });

  it('a chip pulses even though its live count is back to zero', async () => {
    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: {}, startedByKind: { danger: 1 } }),
    );
    const fixture = render();
    await settle();
    // Flush the delta-base read before moving the counter, or the second read
    // becomes the first one the effect ever sees.
    fixture.detectChanges();
    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: {}, startedByKind: { danger: 2 } }),
    );
    notifyQueueChange();
    await settle();
    fixture.detectChanges();
    const dgr = fixture.nativeElement.querySelector('.qt-queue-badge-danger') as HTMLElement;
    expect(dgr.classList.contains('qt-queue-badge-idle')).toBe(true);
    expect(dgr.classList.contains('qt-queue-badge-pulse')).toBe(true);
  });

  it('clears a running pulse timer on destroy (v4\'s unmount cleanup)', async () => {
    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: {}, startedByKind: { memory: 1 } }),
    );
    const fixture = render();
    await settle();
    fixture.detectChanges();
    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: {}, startedByKind: { memory: 2 } }),
    );
    notifyQueueChange();
    await settle();
    fixture.detectChanges();
    const mem = fixture.nativeElement.querySelector('.qt-queue-badge-memory') as HTMLElement;
    expect(mem.classList.contains('qt-queue-badge-pulse')).toBe(true);

    // A timer count is too noisy to read here (the query keeps its own), so
    // watch the handle itself: the pulse timer must be cleared, not left to
    // fire into a destroyed component.
    const cleared = vi.spyOn(globalThis, 'clearTimeout');
    fixture.destroy();
    expect(cleared).toHaveBeenCalled();
    cleared.mockRestore();
  });

  it('a failed read keeps the last good snapshot rather than blanking the chips', async () => {
    fetchMock.mockImplementation(async () =>
      jobsResponse({ activeByKind: { memory: 4 }, startedByKind: {} }),
    );
    const fixture = render();
    await settle();
    fixture.detectChanges();
    fetchMock.mockImplementation(async () => new Response('nope', { status: 500 }));
    notifyQueueChange();
    await settle();
    fixture.detectChanges();
    const mem = fixture.nativeElement.querySelector('.qt-queue-badge-memory') as HTMLElement;
    expect(mem.textContent).toContain('4');
    expect(mem.classList.contains('qt-queue-badge-idle')).toBe(false);
  });

  it('renders zeros against an OLD server that sends neither kind map', async () => {
    fetchMock.mockImplementation(async () => jobsResponse({}));
    const fixture = render();
    await settle();
    fixture.detectChanges();
    for (const badge of badges(fixture)) {
      expect(badge.classList.contains('qt-queue-badge-idle')).toBe(true);
      expect(badge.textContent).toContain('0');
    }
  });
});
