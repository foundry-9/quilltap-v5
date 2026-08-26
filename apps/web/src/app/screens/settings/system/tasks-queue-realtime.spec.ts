import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { coreStreamStub, type CoreStreamStub } from '../../../core/core-client.testing';
import type { ScopedEvent } from '../../../core/core-contract';
import { TasksQueueCard } from './tasks-queue-card';

/**
 * The tasks queue's migration (P4.D125, v4 `f3892158d`): the `jobs` topic keeps
 * the ledger current, and the user-facing toggle is relabeled from
 * "Auto-refresh (5s)" to "Fallback polling (5s)" because that is what it now
 * governs. Same switch, honest name — v4's exact string.
 */

let stream: CoreStreamStub;
let dispatchData: ReturnType<typeof vi.fn>;

const EMPTY_QUEUE = {
  stats: { pending: 0, processing: 0, failed: 0, completed: 0, dead: 0, paused: 0, activeTotal: 0 },
  jobs: [],
  totalEstimatedTokens: 0,
  processorStatus: { running: true },
  maxConcurrentJobs: 4,
};

function mount() {
  stream = coreStreamStub();
  dispatchData = vi.fn(async () => EMPTY_QUEUE as unknown as Record<string, unknown>);
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [TasksQueueCard],
    providers: [
      provideTanStackQuery(
        new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 0 } } }),
      ),
      { provide: CoreClient, useValue: { ...stream, dispatchData } },
    ],
  });
  const fixture = TestBed.createComponent(TasksQueueCard);
  fixture.detectChanges();
  return fixture;
}

async function settle(): Promise<void> {
  await vi.advanceTimersByTimeAsync(0);
  await vi.advanceTimersByTimeAsync(0);
}

function toggle(fixture: { nativeElement: HTMLElement }): HTMLInputElement {
  const label = [...fixture.nativeElement.querySelectorAll('label')].find((l) =>
    l.textContent?.includes('polling'),
  ) as HTMLLabelElement;
  return label.querySelector('input') as HTMLInputElement;
}

describe('TasksQueueCard — pushed, with the toggle governing the fallback', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('labels the switch "Fallback polling (5s)" (v4\'s exact string)', async () => {
    const fixture = mount();
    await settle();
    fixture.detectChanges();
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Fallback polling (5s)');
    expect(text).not.toContain('Auto-refresh');
  });

  it('carries v4\'s tooltip on the switch\'s label', async () => {
    const fixture = mount();
    await settle();
    fixture.detectChanges();
    const label = [...fixture.nativeElement.querySelectorAll('label')].find((l) =>
      l.textContent?.includes('Fallback polling'),
    ) as HTMLLabelElement;
    expect(label.title).toBe(
      'Should the ledger be re-read every five seconds whenever the live wire is down?',
    );
  });

  it('re-reads on a `jobs` hint with the toggle OFF — the push path needs no switch', async () => {
    const fixture = mount();
    await settle();
    fixture.detectChanges();
    expect(toggle(fixture).checked).toBe(false);
    const before = dispatchData.mock.calls.length;
    stream.frames.next({ v: 1, topic: 'jobs', at: 1 } as unknown as ScopedEvent);
    await settle();
    expect(dispatchData.mock.calls.length).toBeGreaterThan(before);
  });

  it('with the toggle ON, polls every 5 s while down — and NOT while connected', async () => {
    const fixture = mount();
    await settle();
    fixture.detectChanges();
    toggle(fixture).click();
    fixture.detectChanges();
    await settle();

    const start = dispatchData.mock.calls.length;
    await vi.advanceTimersByTimeAsync(5_100);
    expect(dispatchData.mock.calls.length).toBeGreaterThan(start);

    stream.connection.set('open');
    fixture.detectChanges();
    await settle();
    const connected = dispatchData.mock.calls.length;
    await vi.advanceTimersByTimeAsync(20_000);
    expect(dispatchData.mock.calls.length).toBe(connected);
  });

  it('with the toggle OFF there is no interval at all, connected or not', async () => {
    const fixture = mount();
    await settle();
    fixture.detectChanges();
    const start = dispatchData.mock.calls.length;
    await vi.advanceTimersByTimeAsync(20_000);
    expect(dispatchData.mock.calls.length).toBe(start);
  });
});
