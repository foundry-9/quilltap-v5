import { Injector, runInInjectionContext, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from './core-client';
import type { ConnectionState } from './core-transport';
import type { ScopedEvent } from './core-contract';
import { RealtimeService } from './realtime.service';
import { ALL_REALTIME_PREFIXES } from './realtime-topic-map';
import { systemJobsKeys } from '../layout/system-jobs.api';
import { chatKeys } from '../chat/chat-keys';

/**
 * Parity specs for the hub against v4's `lib/realtime/client.ts` +
 * `hooks/useRealtime.ts` + `components/providers/realtime-provider.tsx` at
 * `f3892158d`, and against §Shared contract §B.
 *
 * The mechanism under test is v5's, not v4's: hints ride the EXISTING event
 * stream, so the fake `CoreClient` below is the whole seam — a frame Subject
 * plus the two signals both transports drive.
 */

class FakeCore {
  readonly frames = new Subject<ScopedEvent>();
  readonly events$ = this.frames.asObservable();
  readonly connection = signal<ConnectionState>('idle');
  readonly resyncCounter = signal(0);
}

function setup() {
  const core = new FakeCore();
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: core }],
  });
  const queryClient = TestBed.inject(QueryClient);
  const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
  const service = TestBed.inject(RealtimeService);
  TestBed.tick();
  invalidate.mockClear();
  return { core, service, invalidate };
}

type InvalidateSpy = { mock: { calls: unknown[][] } };

/** The keys invalidated since the last clear, as plain arrays. */
function keysFrom(spy: InvalidateSpy): unknown[][] {
  return spy.mock.calls.map((call) => [
    ...((call[0] as { queryKey: readonly unknown[] }).queryKey as unknown[]),
  ]);
}

function hint(topic: string, id?: string): ScopedEvent {
  return (id === undefined
    ? { v: 1, topic, at: 1 }
    : { v: 1, topic, id, at: 1 }) as unknown as ScopedEvent;
}

describe('RealtimeService — the hint → invalidation path', () => {
  it('maps a topic onto its query keys (v4 RealtimeProvider.onEvent)', () => {
    const { core, invalidate } = setup();
    core.frames.next(hint('jobs'));
    expect(keysFrom(invalidate)).toContainEqual([...systemJobsKeys.all]);
  });

  it('narrows a row-scoped hint to that row', () => {
    const { core, invalidate } = setup();
    core.frames.next(hint('chats', 'chat-7'));
    expect(keysFrom(invalidate)).toEqual([[...chatKeys.detail('chat-7')]]);
  });

  it('ignores an unknown topic without touching the cache', () => {
    const { core, invalidate } = setup();
    core.frames.next(hint('a-topic-from-a-newer-server'));
    expect(invalidate).not.toHaveBeenCalled();
  });

  it('ignores the OTHER frames on the shared stream (§B.5)', () => {
    const { core, invalidate } = setup();
    core.frames.next({ chatId: 'c-1', type: 'token', content: 'hi' } as unknown as ScopedEvent);
    core.frames.next({ progressId: 'p-1', kind: 'status' } as unknown as ScopedEvent);
    expect(invalidate).not.toHaveBeenCalled();
  });

  it('is idempotent under duplicate hints (§B.4 — the coalescer may repeat)', () => {
    const { core, invalidate } = setup();
    core.frames.next(hint('jobs'));
    core.frames.next(hint('jobs'));
    expect(keysFrom(invalidate).filter((k) => k[0] === 'systemJobs')).toHaveLength(2);
  });
});

describe('RealtimeService — connection status and the catch-up sweep', () => {
  it('reports connected only while the stream is open', () => {
    const { core, service } = setup();
    expect(service.connected()).toBe(false);
    core.connection.set('open');
    TestBed.tick();
    expect(service.connected()).toBe(true);
    core.connection.set('reconnecting');
    TestBed.tick();
    expect(service.connected()).toBe(false);
  });

  it('invalidates EVERY mapped prefix on connect (v4 RealtimeProvider.onOpen)', () => {
    const { core, invalidate } = setup();
    core.connection.set('open');
    TestBed.tick();
    const keys = keysFrom(invalidate);
    for (const prefix of ALL_REALTIME_PREFIXES) {
      expect(keys).toContainEqual([...prefix]);
    }
  });

  it('sweeps again on a resync bump — the Tauri `quilltap://resync` path', () => {
    const { core, invalidate } = setup();
    core.connection.set('open');
    TestBed.tick();
    invalidate.mockClear();
    core.resyncCounter.update((n) => n + 1);
    TestBed.tick();
    expect(keysFrom(invalidate)).toContainEqual([...systemJobsKeys.all]);
  });

  it('an HTTP reconnect — open + resync in one turn — sweeps ONCE, not twice', () => {
    const { core, invalidate } = setup();
    core.connection.set('open');
    TestBed.tick();
    invalidate.mockClear();
    // The drop, then the EventSource reopening: `bumpResync()` and
    // `setConnection('open')` land in the same turn (core-transport.ts onopen).
    core.connection.set('reconnecting');
    TestBed.tick();
    invalidate.mockClear();
    core.resyncCounter.update((n) => n + 1);
    core.connection.set('open');
    TestBed.tick();
    const jobsSweeps = keysFrom(invalidate).filter((k) => k[0] === 'systemJobs');
    expect(jobsSweeps).toHaveLength(1);
  });

  it('does NOT sweep while the stream is down', () => {
    const { core, invalidate } = setup();
    core.connection.set('reconnecting');
    core.resyncCounter.update((n) => n + 1);
    TestBed.tick();
    expect(invalidate).not.toHaveBeenCalled();
  });
});

describe('RealtimeService.refetchInterval (v4 useRealtimeRefetchInterval)', () => {
  it('returns the cadence while down and false while connected', () => {
    const { core, service } = setup();
    expect(service.refetchInterval(5_000)).toBe(5_000);
    core.connection.set('open');
    TestBed.tick();
    expect(service.refetchInterval(5_000)).toBe(false);
  });

  it('passes `false` straight through, connected or not', () => {
    const { core, service } = setup();
    expect(service.refetchInterval(false)).toBe(false);
    core.connection.set('open');
    TestBed.tick();
    expect(service.refetchInterval(false)).toBe(false);
  });
});

describe('RealtimeService.onTopic (v4 useRealtimeTopic)', () => {
  function armed(topic: string, id?: string) {
    const ctx = setup();
    const calls: number[] = [];
    const child = Injector.create({ providers: [], parent: TestBed.inject(Injector) });
    runInInjectionContext(child, () =>
      ctx.service.onTopic(topic, () => calls.push(1), id ? () => id : undefined),
    );
    return { ...ctx, calls, child };
  }

  it('fires on a matching topic and stays silent on others', () => {
    const { core, calls } = armed('jobs');
    core.frames.next(hint('jobs'));
    expect(calls).toHaveLength(1);
    core.frames.next(hint('chats'));
    expect(calls).toHaveLength(1);
  });

  it('with an id: fires for that row, and for collection-wide hints, never for another row', () => {
    const { core, calls } = armed('chats', 'chat-7');
    core.frames.next(hint('chats', 'chat-7'));
    expect(calls).toHaveLength(1);
    // Collection-wide: no id, so it says nothing about which rows it covers.
    core.frames.next(hint('chats'));
    expect(calls).toHaveLength(2);
    core.frames.next(hint('chats', 'chat-9'));
    expect(calls).toHaveLength(2);
  });

  it('fires on (re)connect too — a reconnecting client has no idea what it missed', () => {
    const { core, calls } = armed('jobs');
    core.connection.set('open');
    TestBed.tick();
    expect(calls).toHaveLength(1);
  });

  it('releases the subscription when the caller\'s context is destroyed', () => {
    const { core, calls, child } = armed('jobs');
    core.frames.next(hint('jobs'));
    expect(calls).toHaveLength(1);
    (child as unknown as { destroy(): void }).destroy();
    core.frames.next(hint('jobs'));
    expect(calls).toHaveLength(1);
  });
});
