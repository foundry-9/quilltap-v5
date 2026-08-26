/**
 * The realtime hub — v5's twin of v4 `lib/realtime/client.ts` +
 * `hooks/useRealtime.ts` + `components/providers/realtime-provider.tsx`, folded
 * into one root service because v5 already owns the connection those three
 * shared.
 *
 * The contract is deliberately one-way and content-free: the server says
 * *something under this topic changed*, and we mark the matching query keys
 * stale. TanStack then refetches whatever is actually on screen through the
 * ordinary dispatch API, which stays the single source of truth for what the
 * data is. Invalidating a key nothing is watching is a no-op, which is why the
 * server can broadcast every hint to every tab without a subscription protocol
 * (v4 decision 3).
 *
 * ## Where the connection comes from (§Shared contract §B.1)
 *
 * v4 opens a second WebSocket. v5 does not: hints ride the EXISTING event
 * channel {@link CoreClient} already owns — SSE `GET /api/events` in HTTP mode,
 * `quilltap://event` in the Tauri shell. That is the locked transport-agnostic
 * boundary ("streaming only ever on the Event channel") meeting v4's own
 * decision 2 ("one socket per tab"): a second connection would break both.
 *
 * Per-leg disposition of v4's socket machinery:
 *
 * | v4 leg | v5 |
 * |---|---|
 * | 1 s → 30 s jittered backoff | **NO-PORT** — `EventSource` reconnects on its own; the Tauri pump is in-process |
 * | 30 s ping/pong keepalive | **NO-PORT** — a WS-protocol liveness leg SSE and IPC do not need |
 * | hidden-tab retry suppression | **NO-PORT** — the browser owns `EventSource` retry scheduling |
 * | `connected` status | **PORTED** — {@link connected}, read by every fallback gate |
 * | Zod-parsed frames, malformed dropped | **PORTED** — `realtimeHintFromFrame` (§B.5 discrimination, then v4's `safeParse` shape) |
 * | unknown-topic tolerance | **PORTED** — {@link queryKeysForTopic} returns `[]` |
 * | catch-up sweep on every (re)connect | **PORTED** — and additionally on an SSE reopen-after-error or a `quilltap://resync`, which is exactly the missed-frame signal §B.4 asks clients to tolerate |
 *
 * The hub is a ROOT service that wires itself on construction; `app.ts` injects
 * it once at bootstrap, before it opens the stream.
 *
 * @module core/realtime.service
 */

import { DestroyRef, Injectable, effect, inject, signal, untracked } from '@angular/core';
import { QueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from './core-client';
import { realtimeHintFromFrame, type RealtimeHint } from './realtime.types';
import { ALL_REALTIME_PREFIXES, queryKeysForTopic } from './realtime-topic-map';

/** What a consumer wants to hear (v4 `RealtimeSubscriber`). */
export interface RealtimeSubscriber {
  /** A hint arrived. */
  onEvent?: (hint: RealtimeHint) => void;
  /** The stream (re)opened, or signalled a gap — catch up on anything missed. */
  onOpen?: () => void;
}

@Injectable({ providedIn: 'root' })
export class RealtimeService {
  private readonly core = inject(CoreClient);

  private readonly subscribers = new Set<RealtimeSubscriber>();

  /**
   * Whether the live channel is currently up.
   *
   * v4 reads the socket's own status; v5 reads {@link CoreClient.connection},
   * which both transports drive. In the Tauri shell the pump is in-process, so
   * this is true whenever the listener is attached (§B.6).
   */
  readonly connected = signal(false);

  private readonly queryClient = inject(QueryClient);
  private readonly destroyRef = inject(DestroyRef);

  /**
   * The hub wires itself on construction — v4's twin is `RealtimeProvider`
   * mounting inside `QueryProvider`. Nothing constructs a root service on its
   * own, so the app root injects this once at bootstrap (`app.ts`); every other
   * consumer then shares the one instance.
   */
  constructor() {
    const frames = this.core.events$.subscribe((frame) => this.acceptFrame(frame));
    this.destroyRef.onDestroy(() => frames.unsubscribe());

    // The catch-up. Reading BOTH signals in one effect is what keeps a single
    // HTTP reconnect — which flips the connection to 'open' AND bumps the
    // resync counter in the same turn — to one sweep rather than two; Angular
    // coalesces the two dependency changes into one effect run. A Tauri
    // `quilltap://resync` arrives as a bump alone, and sweeps on its own.
    effect(() => {
      const open = this.core.connection() === 'open';
      this.core.resyncCounter();
      untracked(() => {
        this.connected.set(open);
        if (!open) return;
        for (const subscriber of [...this.subscribers]) subscriber.onOpen?.();
        for (const queryKey of ALL_REALTIME_PREFIXES) {
          void this.queryClient.invalidateQueries({ queryKey });
        }
      });
    });
  }

  /**
   * One frame off the shared stream. Not a hint → not ours (a chat-stream or
   * creation-progress frame on the same channel); a hint under a topic this
   * build has never heard of → shrug, per v4's unknown-topic rule.
   */
  private acceptFrame(frame: unknown): void {
    const hint = realtimeHintFromFrame(frame);
    if (!hint) return;
    this.deliver(hint);
    const prefixes = queryKeysForTopic(hint.topic, hint.id);
    for (const queryKey of prefixes) {
      void this.queryClient.invalidateQueries({ queryKey });
    }
  }

  /**
   * Subscribe to hints (v4 `subscribeRealtime`).
   *
   * The escape hatch for readouts whose live path isn't a TanStack
   * invalidation — a card that drives its own fetch, a watch that only wants to
   * re-read while it is armed. Those keep their interval as the offline
   * fallback and use this for the live path.
   *
   * @returns An unsubscribe function.
   */
  subscribe(subscriber: RealtimeSubscriber): () => void {
    this.subscribers.add(subscriber);
    return () => {
      this.subscribers.delete(subscriber);
    };
  }

  /**
   * Run `onChange` whenever the server announces a change under `topic` — v4
   * `useRealtimeTopic`.
   *
   * **Must be called in an injection context**; the subscription is released by
   * the caller's `DestroyRef`.
   *
   * @param topic The topic to listen for, e.g. `'jobs'`.
   * @param onChange Called on a matching hint, and again on every (re)connect —
   *   a reconnecting client has no idea what it missed.
   * @param id Optional entity id. When given, only hints for that row fire —
   *   plus collection-wide hints for the topic, which carry no id and therefore
   *   say nothing about which rows they cover.
   */
  onTopic(topic: string, onChange: () => void, id?: () => string | null | undefined): void {
    const destroyRef = inject(DestroyRef);
    const release = this.subscribe({
      onEvent: (hint) => {
        if (hint.topic !== topic) return;
        const scope = id?.();
        if (scope && hint.id && hint.id !== scope) return;
        onChange();
      },
      onOpen: () => onChange(),
    });
    destroyRef.onDestroy(release);
  }

  /**
   * A `refetchInterval` value that polls only while the channel is down — v4
   * `useRealtimeRefetchInterval`.
   *
   * Every migrated site keeps its original cadence wired but gated this way, so
   * a dropped connection degrades to the behavior that shipped before realtime
   * existed rather than to a frozen screen.
   *
   * @param pollMs The pre-realtime cadence, or `false` to disable polling
   *   outright (a watch that has already seen what it was waiting for). `false`
   *   passes straight through.
   */
  refetchInterval(pollMs: number | false): number | false {
    if (pollMs === false) return false;
    return this.connected() ? false : pollMs;
  }

  /**
   * A gated fallback poll for a readout that is NOT a TanStack query.
   *
   * v4 writes this out as a `useEffect` at four sites — the two memory cards,
   * the Salon's avatar watch, the character conversations tab — always in the
   * same shape: `if (connected || !armed) return; const i = setInterval(…);
   * return () => clearInterval(i)`. One helper, so the four v5 twins cannot
   * drift from each other.
   *
   * **Must be called in an injection context**; the timer is released by the
   * caller's `DestroyRef`.
   *
   * @param pollMs The pre-realtime cadence.
   * @param tick What to run on each fallback tick.
   * @param armed Optional extra gate — a watch that isn't running polls
   *   nothing, connected or not.
   */
  fallbackPoll(pollMs: number, tick: () => void, armed?: () => boolean): void {
    const destroyRef = inject(DestroyRef);
    let handle: ReturnType<typeof setInterval> | null = null;
    const stop = () => {
      if (handle !== null) {
        clearInterval(handle);
        handle = null;
      }
    };
    effect((onCleanup) => {
      const shouldPoll = !this.connected() && (armed?.() ?? true);
      untracked(() => {
        stop();
        if (shouldPoll) {
          handle = setInterval(tick, pollMs);
        }
      });
      onCleanup(stop);
    });
    destroyRef.onDestroy(stop);
  }

  private deliver(hint: RealtimeHint): void {
    for (const subscriber of [...this.subscribers]) subscriber.onEvent?.(hint);
  }
}
