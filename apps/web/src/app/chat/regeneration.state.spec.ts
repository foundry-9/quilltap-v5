import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub, type CoreStreamStub } from '../core/core-client.testing';
import type { ScopedEvent } from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import { RegenerationController } from './regeneration.state';

/**
 * The regeneration controller — v5's twin of v4
 * `app/salon/[id]/hooks/useRegeneration.ts` (`f564b0de3`).
 *
 * v4 ships no unit test for the hook (its coverage is the route's and the
 * service's, plus the two `selectSwipeVariant` cases in `useChatData.test.ts`).
 * These pins are therefore written against the hook's own SOURCE, arm by arm,
 * at `f45a517a9`:
 *
 *  - one at a time, through a synchronous in-flight guard (`:110-111`);
 *  - `stage` opens `preparing` and flips to `streaming` on the first content
 *    flush (`:97-103`) — the whole point of the plate;
 *  - `content` deltas APPEND, `reasoning` frames REPLACE (`:150-160`);
 *  - `done` shows the persisted line rather than the accumulated one
 *    (`:161-175`), then refetch → selectSwipeVariant → queue notify IN THAT
 *    ORDER (`:179-182`);
 *  - an `{ error }` frame throws after the stream, and the toast is raised
 *    (`:177,:183-185`);
 *  - `finally` clears everything on every path (`:186-192`).
 *
 * The rAF coalescing is real, so every content assertion waits a frame.
 */

function frame(progressId: string, body: Record<string, unknown>): ScopedEvent {
  return { type: 'swipeProgress', progressId, frame: body } as unknown as ScopedEvent;
}

/** One animation frame plus a microtask drain. */
async function tick(): Promise<void> {
  await new Promise((r) => requestAnimationFrame(() => r(null)));
  await new Promise((r) => setTimeout(r, 0));
}

interface Harness {
  controller: RegenerationController;
  stream: CoreStreamStub;
  calls: Record<string, unknown>[];
  /** Resolve or reject the in-flight dispatch. */
  settle: (err?: Error) => void;
  refetched: number;
  selected: string[];
}

function harness(): Harness {
  const stream = coreStreamStub();
  const calls: Record<string, unknown>[] = [];
  let resolve!: (v: Record<string, unknown>) => void;
  let reject!: (e: Error) => void;
  const dispatchData = vi.fn((req: Record<string, unknown>) => {
    calls.push(req);
    return new Promise<Record<string, unknown>>((res, rej) => {
      resolve = res;
      reject = rej;
    });
  });

  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    providers: [
      RegenerationController,
      {
        provide: CoreClient,
        useValue: { ...stream, dispatchData } as unknown as CoreClient,
      },
    ],
  });

  const h: Harness = {
    controller: TestBed.inject(RegenerationController),
    stream,
    calls,
    settle: (err?: Error) => (err ? reject(err) : resolve({ message: null })),
    refetched: 0,
    selected: [],
  };
  return h;
}

describe('RegenerationController (v4 useRegeneration @ f45a517a9)', () => {
  beforeEach(() => {
    vi.spyOn(globalThis, 'requestAnimationFrame').mockImplementation(((cb: FrameRequestCallback) =>
      setTimeout(() => cb(0), 0) as unknown as number) as typeof requestAnimationFrame);
    vi.spyOn(globalThis, 'cancelAnimationFrame').mockImplementation(((id: number) =>
      clearTimeout(id)) as typeof cancelAnimationFrame);
  });
  afterEach(() => {
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  it('dispatches messageSwipe with the stream flag, and opens on the plate', async () => {
    const h = harness();
    void h.controller.regenerate('m-1', async () => {});
    await tick();

    expect(h.calls).toEqual([{ type: 'messageSwipe', messageId: 'm-1', stream: true }]);
    expect(h.controller.isRegenerating()).toBe(true);
    expect(h.controller.regeneration()).toEqual({
      messageId: 'm-1',
      stage: 'preparing',
      content: '',
      reasoning: '',
    });
    expect(h.controller.regenerationStatus()).toEqual({
      stage: 'regenerating',
      message: 'Regenerating...',
    });
    h.settle();
    await tick();
  });

  it('runs ONE at a time — a second press in the same tick loses', async () => {
    const h = harness();
    void h.controller.regenerate('m-1', async () => {});
    void h.controller.regenerate('m-2', async () => {});
    await tick();

    expect(h.calls).toHaveLength(1);
    expect(h.controller.regeneration()!.messageId).toBe('m-1');
    h.settle();
    await tick();
  });

  it('withdraws the plate on the FIRST token, and APPENDS deltas', async () => {
    const h = harness();
    void h.controller.regenerate('m-1', async () => {});
    await tick();
    expect(h.controller.regeneration()!.stage).toBe('preparing');

    h.stream.frames.next(frame('m-1', { content: 'Once ' }));
    await tick();
    expect(h.controller.regeneration()!.stage).toBe('streaming');
    expect(h.controller.regeneration()!.content).toBe('Once ');

    h.stream.frames.next(frame('m-1', { content: 'upon a time.' }));
    await tick();
    expect(h.controller.regeneration()!.content).toBe('Once upon a time.');
    h.settle();
    await tick();
  });

  it('REPLACES reasoning rather than concatenating it (last wins)', async () => {
    const h = harness();
    void h.controller.regenerate('m-1', async () => {});
    await tick();

    h.stream.frames.next(frame('m-1', { reasoning: 'I think' }));
    h.stream.frames.next(frame('m-1', { reasoning: 'I think, therefore' }));
    await tick();
    expect(h.controller.regeneration()!.reasoning).toBe('I think, therefore');
    h.settle();
    await tick();
  });

  it('carries each status frame to the strip', async () => {
    const h = harness();
    void h.controller.regenerate('m-1', async () => {});
    await tick();

    h.stream.frames.next(
      frame('m-1', {
        status: { stage: 'saving', message: 'Regenerating — filing the new line...' },
      }),
    );
    await tick();
    expect(h.controller.regenerationStatus()).toEqual({
      stage: 'saving',
      message: 'Regenerating — filing the new line...',
    });
    h.settle();
    await tick();
  });

  it('ignores frames for another message, and frames of another kind', async () => {
    const h = harness();
    void h.controller.regenerate('m-1', async () => {});
    await tick();

    h.stream.frames.next(frame('m-2', { content: 'not mine' }));
    h.stream.frames.next({
      type: 'generatorProgress',
      progressId: 'm-1',
      event: {},
    } as unknown as ScopedEvent);
    await tick();
    expect(h.controller.regeneration()).toEqual({
      messageId: 'm-1',
      stage: 'preparing',
      content: '',
      reasoning: '',
    });
    h.settle();
    await tick();
  });

  it('on done: refetches, selects the NEW variant, and clears — in v4’s order', async () => {
    const h = harness();
    const order: string[] = [];
    const run = h.controller.regenerate(
      'm-1',
      async () => {
        order.push('refetch');
      },
      (id) => order.push(`select:${id}`),
    );
    await tick();

    h.stream.frames.next(frame('m-1', { content: 'draft' }));
    await tick();
    h.stream.frames.next(
      frame('m-1', { done: true, message: { id: 'swipe-2', content: 'the persisted line' } }),
    );
    await tick();
    // The persisted line replaces the accumulated one before the hand-off, so
    // the swap to the reconciled row is not a visible twitch.
    expect(h.controller.regeneration()!.content).toBe('the persisted line');

    h.settle();
    await run;
    expect(order).toEqual(['refetch', 'select:swipe-2']);
    expect(h.controller.regeneration()).toBeNull();
    expect(h.controller.regenerationStatus()).toBeNull();
    expect(h.controller.isRegenerating()).toBe(false);
  });

  it('does not select a variant when done carried no message id', async () => {
    const h = harness();
    const selected: string[] = [];
    const run = h.controller.regenerate('m-1', async () => {}, (id) => selected.push(id));
    await tick();
    h.stream.frames.next(frame('m-1', { done: true }));
    await tick();
    h.settle();
    await run;
    expect(selected).toEqual([]);
  });

  it('toasts an error FRAME with v4’s two-part sentence, and changes nothing', async () => {
    const h = harness();
    let refetched = 0;
    const run = h.controller.regenerate('m-1', async () => {
      refetched += 1;
    });
    await tick();

    h.stream.frames.next(
      frame('m-1', {
        error: 'Failed to generate alternative response',
        errorType: 'regenerate_failed',
        details: 'the model said no',
      }),
    );
    await tick();
    h.settle();
    await run;

    expect(TestBed.inject(ToastService).toasts().map((t) => t.message)).toContain(
      'Failed to generate alternative response: the model said no',
    );
    // v4 throws BEFORE its refetch, so a failed stream reconciles nothing.
    expect(refetched).toBe(0);
    expect(h.controller.regeneration()).toBeNull();
  });

  it('toasts a REFUSAL — the dispatch that rejects before any frame', async () => {
    const h = harness();
    const run = h.controller.regenerate('m-1', async () => {});
    await tick();
    h.settle(new Error('Only assistant messages can be swiped'));
    await run;

    expect(TestBed.inject(ToastService).toasts().map((t) => t.message)).toContain(
      'Only assistant messages can be swiped',
    );
    expect(h.controller.regeneration()).toBeNull();
    expect(h.controller.isRegenerating()).toBe(false);
  });

  it('releases the floor after a failure, so the next press runs', async () => {
    const h = harness();
    const first = h.controller.regenerate('m-1', async () => {});
    await tick();
    h.settle(new Error('nope'));
    await first;

    void h.controller.regenerate('m-2', async () => {});
    await tick();
    expect(h.calls).toHaveLength(2);
    expect(h.controller.regeneration()!.messageId).toBe('m-2');
    h.settle();
    await tick();
  });
});
