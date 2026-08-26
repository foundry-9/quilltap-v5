import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub, type CoreStreamStub } from '../core/core-client.testing';
import type { AutonomousRoomSummary, ScopedEvent } from '../core/core-contract';
import { AutonomousRoomBadges } from './autonomous-room-badges';

/**
 * The badges' migration onto the live channel (P4.D125, v4 `f3892158d`): the
 * bespoke 5 s loop becomes a `autonomousRooms`-driven read with the same
 * cadence held in reserve, and the bespoke 1 s `setNowMs` tick becomes the
 * shared clock, enabled only while a running, time-budgeted room needs one.
 */

function room(over: Partial<AutonomousRoomSummary> = {}): AutonomousRoomSummary {
  return {
    id: 'chat-1',
    title: 'A Room',
    runState: 'running',
    budgetMaxWallClockMs: null,
    ...over,
  } as unknown as AutonomousRoomSummary;
}

let stream: CoreStreamStub;
let list: ReturnType<typeof vi.fn>;

function render(rooms: AutonomousRoomSummary[] = []) {
  stream = coreStreamStub();
  list = vi.fn(async () => rooms);
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [AutonomousRoomBadges],
    providers: [
      provideRouter([]),
      provideTanStackQuery(new QueryClient()),
      {
        provide: CoreClient,
        useValue: { ...stream, listAutonomousRooms: list },
      },
    ],
  });
  const fixture = TestBed.createComponent(AutonomousRoomBadges);
  fixture.detectChanges();
  return fixture;
}

describe('AutonomousRoomBadges (pushed, with the 5 s poll in reserve)', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('re-reads on an `autonomousRooms` hint', async () => {
    render();
    await vi.advanceTimersByTimeAsync(0);
    const before = list.mock.calls.length;
    stream.frames.next({ v: 1, topic: 'autonomousRooms', at: 1 } as unknown as ScopedEvent);
    await vi.advanceTimersByTimeAsync(0);
    expect(list.mock.calls.length).toBeGreaterThan(before);
  });

  it('does NOT re-read on some other topic', async () => {
    render();
    await vi.advanceTimersByTimeAsync(0);
    const before = list.mock.calls.length;
    stream.frames.next({ v: 1, topic: 'jobs', at: 1 } as unknown as ScopedEvent);
    await vi.advanceTimersByTimeAsync(0);
    expect(list.mock.calls.length).toBe(before);
  });

  it('polls every 5 s while the channel is down — and stops once it is up', async () => {
    const fixture = render();
    await vi.advanceTimersByTimeAsync(0);
    const start = list.mock.calls.length;
    await vi.advanceTimersByTimeAsync(5_100);
    const polled = list.mock.calls.length;
    expect(polled).toBeGreaterThan(start);

    stream.connection.set('open');
    fixture.detectChanges();
    await vi.advanceTimersByTimeAsync(0);
    const afterConnect = list.mock.calls.length;
    await vi.advanceTimersByTimeAsync(20_000);
    expect(list.mock.calls.length).toBe(afterConnect);
  });

  it('runs NO second-hand timer unless a running, time-budgeted room is on screen', async () => {
    // A running room with no wall-clock budget: nothing to tick for.
    const fixture = render([room({ budgetMaxWallClockMs: null })]);
    await vi.advanceTimersByTimeAsync(0);
    fixture.detectChanges();
    stream.connection.set('open');
    fixture.detectChanges();
    await vi.advanceTimersByTimeAsync(0);
    // With the channel up there is no fallback poll either, so any pending
    // timer would have to be the clock.
    expect(vi.getTimerCount()).toBe(0);
  });

  it('arms the shared second hand for a running, time-budgeted room', async () => {
    const fixture = render([room({ budgetMaxWallClockMs: 60_000 })]);
    await vi.advanceTimersByTimeAsync(0);
    fixture.detectChanges();
    stream.connection.set('open');
    fixture.detectChanges();
    await vi.advanceTimersByTimeAsync(0);
    expect(vi.getTimerCount()).toBe(1);
  });
});
