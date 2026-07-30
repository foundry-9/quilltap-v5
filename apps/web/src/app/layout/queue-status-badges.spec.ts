import { provideRouter } from '@angular/router';
import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { QueueStatusBadges } from './queue-status-badges';
import {
  QUEUE_TYPES,
  getQueueCount,
  hasActiveJobs,
  notifyQueueChange,
} from './queue-status.logic';

function jobsResponse(activeByType: Record<string, number>): Response {
  return new Response(JSON.stringify({ activeByType }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

async function flush(): Promise<void> {
  // Let the fetch promise chain settle.
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

describe('queue-status logic (v4 queue-status-badges.tsx)', () => {
  it('carries v4\'s five buckets with the exact job-type keys', () => {
    expect(QUEUE_TYPES.map((q) => q.label)).toEqual(['Mem', 'Emb', 'Sum', 'Dgr', 'Img']);
    expect(QUEUE_TYPES.find((q) => q.key === 'memory')?.jobTypes).toEqual([
      'MEMORY_EXTRACTION',
      'INTER_CHARACTER_MEMORY',
      'MEMORY_REGENERATE_CHAT',
      'MEMORY_REGENERATE_ALL',
    ]);
    expect(QUEUE_TYPES.find((q) => q.key === 'summary')?.jobTypes).toContain(
      'REGENERATE_CONVERSATION_SUMMARIES',
    );
  });

  it('getQueueCount sums only the bucket\'s types; hasActiveJobs needs a positive count', () => {
    const counts = { MEMORY_EXTRACTION: 2, EMBEDDING_GENERATE: 0, TITLE_UPDATE: 1 };
    expect(getQueueCount(counts, ['MEMORY_EXTRACTION', 'INTER_CHARACTER_MEMORY'])).toBe(2);
    expect(getQueueCount(counts, ['EMBEDDING_GENERATE'])).toBe(0);
    expect(hasActiveJobs({ A: 0, B: 0 })).toBe(false);
    expect(hasActiveJobs({ A: 0, B: 3 })).toBe(true);
  });
});

describe('QueueStatusBadges (event-driven polling)', () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.useFakeTimers();
    fetchMock = vi.fn(async () => jobsResponse({}));
    vi.stubGlobal('fetch', fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  function render() {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [QueueStatusBadges],
      providers: [provideRouter([])],
    });
    const fixture = TestBed.createComponent(QueueStatusBadges);
    fixture.detectChanges();
    return fixture;
  }

  it('renders all five badges dimmed at zero, with v4\'s tooltip shape', async () => {
    const fixture = render();
    await flush();
    fixture.detectChanges();
    const badges = fixture.nativeElement.querySelectorAll('.qt-queue-badge-group > span');
    expect(badges).toHaveLength(5);
    for (const badge of badges) {
      expect((badge as HTMLElement).classList.contains('qt-queue-badge-idle')).toBe(true);
    }
    expect((badges[0] as HTMLElement).title).toBe('Memory extraction queue: 0 active');
  });

  it('mount fires one check; zero counts start NO poll (v4 stop-at-zero)', async () => {
    render();
    await flush();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(20_000);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('active counts light the badge and start the 5s poll, which stops once counts drain', async () => {
    fetchMock.mockImplementation(async () => jobsResponse({ MEMORY_EXTRACTION: 3 }));
    const fixture = render();
    await flush();
    fixture.detectChanges();
    const mem = fixture.nativeElement.querySelector('.qt-queue-badge-memory') as HTMLElement;
    expect(mem.classList.contains('qt-queue-badge-idle')).toBe(false);
    expect(mem.textContent).toContain('3');

    // Poll keeps running while active…
    await vi.advanceTimersByTimeAsync(5_000);
    expect(fetchMock).toHaveBeenCalledTimes(2);

    // …then the counts hit zero and the poll stops itself.
    fetchMock.mockImplementation(async () => jobsResponse({ MEMORY_EXTRACTION: 0 }));
    await vi.advanceTimersByTimeAsync(5_000);
    const callsAtZero = fetchMock.mock.calls.length;
    await vi.advanceTimersByTimeAsync(20_000);
    expect(fetchMock.mock.calls.length).toBe(callsAtZero);
  });

  it('notifyQueueChange() wakes a stopped poller', async () => {
    render();
    await flush();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    fetchMock.mockImplementation(async () => jobsResponse({ EMBEDDING_GENERATE: 1 }));
    notifyQueueChange();
    await flush();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(fetchMock.mock.calls.length).toBeGreaterThanOrEqual(3);
  });

  it('a failed fetch collapses to {} (badges stay at zero, no crash)', async () => {
    fetchMock.mockImplementation(async () => new Response('nope', { status: 500 }));
    const fixture = render();
    await flush();
    fixture.detectChanges();
    const badges = fixture.nativeElement.querySelectorAll('.qt-queue-badge-group > span');
    for (const badge of badges) {
      expect((badge as HTMLElement).classList.contains('qt-queue-badge-idle')).toBe(true);
    }
  });
});
