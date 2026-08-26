/**
 * The live-stream half of a `CoreClient` stub — spec support only.
 *
 * Since P4.D125 the realtime hub is a root service that subscribes to
 * `CoreClient.events$` and reads its two stream signals the moment anything
 * constructs it. Any spec that provides a partial `CoreClient` and mounts a
 * component touching the hub (directly, or through the toolbar chips, the tasks
 * queue, a memory card…) needs that surface present, or the stub blows up
 * inside the hub's constructor with an unrelated-looking error.
 *
 * Spread this into the stub; leave everything else as it was:
 *
 * ```ts
 * { provide: CoreClient, useValue: { ...coreStreamStub(), dispatchData } }
 * ```
 *
 * The returned `frames` Subject is the seam a spec drives a hint through, and
 * `connection` / `resyncCounter` are the signals it flips to exercise the
 * fallback gating.
 *
 * @module core/core-client.testing
 */

import { signal } from '@angular/core';
import { Subject } from 'rxjs';

import type { ScopedEvent } from './core-contract';
import type { ConnectionState } from './core-transport';

export interface CoreStreamStub {
  frames: Subject<ScopedEvent>;
  events$: ReturnType<Subject<ScopedEvent>['asObservable']>;
  connection: ReturnType<typeof signal<ConnectionState>>;
  resyncCounter: ReturnType<typeof signal<number>>;
}

export function coreStreamStub(): CoreStreamStub {
  const frames = new Subject<ScopedEvent>();
  return {
    frames,
    events$: frames.asObservable(),
    connection: signal<ConnectionState>('idle'),
    resyncCounter: signal(0),
  };
}
