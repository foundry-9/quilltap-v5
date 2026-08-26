import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { coreStreamStub, type CoreStreamStub } from '../../../core/core-client.testing';
import type { ScopedEvent } from '../../../core/core-contract';
import { MemoryBackfillCard } from './memory-backfill-card';
import { MemoryRegenerateCard } from './memory-regenerate-card';
import { ToastService } from '../../../ui/toast.service';

/**
 * The three housekeeping cards' migration (P4.D125, v4 `f3892158d`): the `jobs`
 * topic is the live path, the original interval is the fallback, and the "only
 * while something is in flight" scope the old poll had is preserved on BOTH.
 *
 * The old family covered the poll's scope from the card's own side; these cover
 * what the migration ADDS — that the hint drives a re-read, that it does NOT
 * when the card is idle, and that the interval retires while the channel is up.
 */

let stream: CoreStreamStub;
let dispatchData: ReturnType<typeof vi.fn>;

function jobsHint(): ScopedEvent {
  return { v: 1, topic: 'jobs', at: 1 } as unknown as ScopedEvent;
}

function mount<T>(cmp: new (...args: never[]) => T, reply: () => Record<string, unknown>) {
  stream = coreStreamStub();
  dispatchData = vi.fn(async () => reply());
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [cmp as never],
    providers: [
      provideTanStackQuery(
        new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 0 } } }),
      ),
      {
        provide: CoreClient,
        useValue: { ...stream, dispatchData },
      },
      {
        provide: ToastService,
        useValue: { showSuccess: vi.fn(), showError: vi.fn(), showInfo: vi.fn() },
      },
    ],
  });
  const fixture = TestBed.createComponent(cmp as never) as ComponentFixture<T>;
  fixture.detectChanges();
  return fixture;
}

async function settle(): Promise<void> {
  await vi.advanceTimersByTimeAsync(0);
  await vi.advanceTimersByTimeAsync(0);
}

describe('MemoryBackfillCard — pushed, with the 4 s poll in reserve', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('re-reads progress on a `jobs` hint', async () => {
    mount(MemoryBackfillCard, () => ({ remaining: 5, inFlight: 1 }));
    await settle();
    const before = dispatchData.mock.calls.length;
    stream.frames.next(jobsHint());
    await settle();
    expect(dispatchData.mock.calls.length).toBeGreaterThan(before);
  });

  it('polls every 4 s while the channel is down and stops once it is up', async () => {
    const fixture = mount(MemoryBackfillCard, () => ({ remaining: 5, inFlight: 1 }));
    await settle();
    const start = dispatchData.mock.calls.length;
    await vi.advanceTimersByTimeAsync(4_100);
    expect(dispatchData.mock.calls.length).toBeGreaterThan(start);

    stream.connection.set('open');
    fixture.detectChanges();
    await settle();
    const connected = dispatchData.mock.calls.length;
    await vi.advanceTimersByTimeAsync(20_000);
    expect(dispatchData.mock.calls.length).toBe(connected);
  });
});

describe('MemoryRegenerateCard — the hint fires only while a sweep is in flight', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('IGNORES a `jobs` hint while nothing is running (v4 keeps the old scope)', async () => {
    mount(MemoryRegenerateCard, () => ({
      inFlightFanOut: 0,
      inFlightWipes: 0,
      inFlightExtractions: 0,
      inFlight: 0,
    }));
    await settle();
    const before = dispatchData.mock.calls.length;
    stream.frames.next(jobsHint());
    await settle();
    expect(dispatchData.mock.calls.length).toBe(before);
  });

  it('re-reads on a `jobs` hint while a sweep IS draining', async () => {
    const fixture = mount(MemoryRegenerateCard, () => ({
      inFlightFanOut: 1,
      inFlightWipes: 0,
      inFlightExtractions: 2,
      inFlight: 3,
    }));
    await settle();
    fixture.detectChanges();
    const before = dispatchData.mock.calls.length;
    stream.frames.next(jobsHint());
    await settle();
    expect(dispatchData.mock.calls.length).toBeGreaterThan(before);
  });

  it('keeps the interval OFF while idle, on while draining and disconnected', async () => {
    const idle = mount(MemoryRegenerateCard, () => ({
      inFlightFanOut: 0,
      inFlightWipes: 0,
      inFlightExtractions: 0,
      inFlight: 0,
    }));
    await settle();
    idle.detectChanges();
    const idleCalls = dispatchData.mock.calls.length;
    await vi.advanceTimersByTimeAsync(20_000);
    expect(dispatchData.mock.calls.length).toBe(idleCalls);

    const busy = mount(MemoryRegenerateCard, () => ({
      inFlightFanOut: 1,
      inFlightWipes: 0,
      inFlightExtractions: 0,
      inFlight: 1,
    }));
    await settle();
    busy.detectChanges();
    const busyCalls = dispatchData.mock.calls.length;
    await vi.advanceTimersByTimeAsync(5_100);
    expect(dispatchData.mock.calls.length).toBeGreaterThan(busyCalls);
  });
});
