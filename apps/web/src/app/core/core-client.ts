import { Injectable, signal } from '@angular/core';
import { Subject, type Observable } from 'rxjs';

import {
  type CoreRequest,
  type CoreResponse,
  type PepperState,
  type ScopedEvent,
  CoreDispatchError,
  expectResponse,
  type ResponseType,
  type AutonomousRoomSummary,
  type AutonomousRoomStatusDto,
  type AutonomousRoomSettingsPatch,
  type AutonomousRoomUpdateResult,
} from './core-contract';

/** The interpreted `GET /health` result (the startup gate branches on this). */
export type HealthStatus =
  | { kind: 'healthy' }
  | { kind: 'locked'; pepperState: PepperState }
  | { kind: 'lock-conflict'; lockConflict: unknown }
  | { kind: 'unhealthy'; message: string }
  /** The server never answered (still booting / network hiccup) — retry. */
  | { kind: 'unreachable'; message: string };

/** The one live-stream connection state, exposed as a signal for the shell. */
export type ConnectionState = 'idle' | 'connecting' | 'open' | 'reconnecting' | 'closed';

/**
 * The ONE transport seam (D14): the whole server surface behind a single
 * injectable. Components never touch `fetch` / `EventSource` directly — the
 * Tauri shell later swaps this one class for an IPC-backed implementation.
 *
 *  - {@link dispatch} → `POST /api/dispatch` (the typed action route).
 *  - {@link events$} → ONE `EventSource` on `GET /api/events` per app instance.
 *  - {@link fetchHealth} → the `GET /health` readiness vocabulary (startup gate).
 */
@Injectable({ providedIn: 'root' })
export class CoreClient {
  /** Same-origin by default (the dev proxy forwards `/api` + `/health`). */
  private readonly baseUrl = '';

  private eventSource: EventSource | null = null;
  private readonly frames = new Subject<ScopedEvent>();

  /** Parsed scope-tagged frames off the single global stream. */
  readonly events$: Observable<ScopedEvent> = this.frames.asObservable();

  /** The live-stream connection state (signal — drives the shell indicator). */
  readonly connection = signal<ConnectionState>('idle');

  /**
   * A monotonically increasing resync counter. EventSource auto-reconnects on a
   * drop; each time the stream REOPENS after an error, this bumps — consumers
   * treat the change as a "you may have missed frames, refetch authoritative
   * state" signal (D3 best-effort: no server-side replay buffer this round).
   */
  readonly resyncCounter = signal(0);

  // -------------------------------------------------------------------------
  // Dispatch
  // -------------------------------------------------------------------------

  /**
   * Send one request to `POST /api/dispatch`. ALWAYS resolves to a typed
   * `CoreResponse` — a non-2xx body is still the typed envelope (the Locked 503
   * carries the error envelope plus v4's `setupUrl` body, which we ignore here);
   * a network failure becomes a synthetic `internal` error so callers get a
   * uniform shape (use {@link dispatchExpect} to throw instead).
   */
  async dispatch(request: CoreRequest): Promise<CoreResponse> {
    let res: Response;
    try {
      res = await fetch(`${this.baseUrl}/api/dispatch`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(request),
      });
    } catch (err) {
      return syntheticError('Connection lost. The server may still be starting.', err);
    }

    let body: unknown;
    try {
      body = await res.json();
    } catch {
      return syntheticError(`The server returned an unreadable response (HTTP ${res.status}).`);
    }

    if (isCoreResponse(body)) {
      return body;
    }
    return syntheticError(`The server returned an unexpected response (HTTP ${res.status}).`);
  }

  /**
   * Dispatch and narrow to the expected response variant, throwing a
   * {@link CoreDispatchError} on a `{ type: "error" }` envelope (or a plain
   * `Error` on a variant mismatch).
   */
  async dispatchExpect<T extends ResponseType>(
    request: CoreRequest,
    expect: T,
  ): Promise<Extract<CoreResponse, { type: T }>> {
    const resp = await this.dispatch(request);
    return expectResponse(resp, expect);
  }

  /**
   * Dispatch and return the raw `data` body of any non-error response, throwing a
   * {@link CoreDispatchError} on `{ type: "error" }`. Use for ops whose response
   * `type` string is not pinned by the Shared contract (the provider-test
   * actions) — only their `data` shape is load-bearing.
   */
  async dispatchData(request: CoreRequest): Promise<Record<string, unknown>> {
    const resp = await this.dispatch(request);
    if (resp.type === 'error') {
      throw new CoreDispatchError(resp.data);
    }
    return (resp.data ?? {}) as Record<string, unknown>;
  }

  // -------------------------------------------------------------------------
  // Autonomous rooms (P4.6ad) — the run-control surface behind one opaque
  // `autonomousRoom` response; each verb casts the `data` body it expects.
  // -------------------------------------------------------------------------

  /** v4 `GET /system/autonomous-rooms` — the user's rooms, running-first. */
  async listAutonomousRooms(): Promise<AutonomousRoomSummary[]> {
    const data = await this.dispatchData({ type: 'systemAutonomousRooms' });
    return (data['rooms'] as AutonomousRoomSummary[] | undefined) ?? [];
  }

  /** The per-room status snapshot (the editor modal's initial load). */
  async autonomousRoomStatus(chatId: string): Promise<AutonomousRoomStatusDto> {
    const data = await this.dispatchData({ type: 'chatAutonomousRoomStatus', chatId });
    return data as unknown as AutonomousRoomStatusDto;
  }

  /** Manually start a run → `{runId, jobId}`. */
  async autonomousRoomStart(chatId: string): Promise<{ runId: string; jobId: string }> {
    const data = await this.dispatchData({ type: 'chatAutonomousRoomStart', chatId });
    return data as unknown as { runId: string; jobId: string };
  }

  /** Pause the current run. */
  async autonomousRoomPause(chatId: string): Promise<void> {
    await this.dispatchData({ type: 'chatAutonomousRoomPause', chatId });
  }

  /** Stop the current run (bumps `currentRunId`). */
  async autonomousRoomStop(chatId: string): Promise<void> {
    await this.dispatchData({ type: 'chatAutonomousRoomStop', chatId });
  }

  /** Resume (or fresh-start) a paused/idle room → `{runId, jobId}`. */
  async autonomousRoomResume(chatId: string): Promise<{ runId: string; jobId: string }> {
    const data = await this.dispatchData({ type: 'chatAutonomousRoomResume', chatId });
    return data as unknown as { runId: string; jobId: string };
  }

  /** Apply the Edit-Enclave modal patch → `{updated, clampedDestructive}`. */
  async autonomousRoomUpdateSettings(
    chatId: string,
    settings: AutonomousRoomSettingsPatch,
  ): Promise<AutonomousRoomUpdateResult> {
    const data = await this.dispatchData({
      type: 'chatAutonomousRoomUpdateSettings',
      chatId,
      settings,
    });
    return {
      updated: data['updated'] === true,
      clampedDestructive: data['clampedDestructive'] === true,
    };
  }

  // -------------------------------------------------------------------------
  // Health (the startup readiness gate)
  // -------------------------------------------------------------------------

  /**
   * Read `GET /health` and interpret v4's status vocabulary (health.rs): 200
   * healthy, 423 locked (carries `dbKeyState`), 409 lock-conflict, 503
   * unhealthy, and an unreachable server (still booting) → retry.
   */
  async fetchHealth(): Promise<HealthStatus> {
    let res: Response;
    try {
      res = await fetch(`${this.baseUrl}/health`, { headers: { 'cache-control': 'no-cache' } });
    } catch (err) {
      return { kind: 'unreachable', message: errorMessage(err) };
    }
    let body: Record<string, unknown> = {};
    try {
      body = (await res.json()) as Record<string, unknown>;
    } catch {
      // A body-less health response still tells us via the status code.
    }
    switch (res.status) {
      case 200:
        return { kind: 'healthy' };
      case 423:
        return {
          kind: 'locked',
          pepperState: (body['dbKeyState'] as PepperState) ?? 'needs-passphrase',
        };
      case 409:
        return { kind: 'lock-conflict', lockConflict: body['lockConflict'] ?? null };
      case 503:
        return {
          kind: 'unhealthy',
          message: (body['error'] as string) ?? 'The server is not ready.',
        };
      default:
        return { kind: 'unhealthy', message: `Unexpected health status ${res.status}.` };
    }
  }

  // -------------------------------------------------------------------------
  // The single global event stream
  // -------------------------------------------------------------------------

  /** Open the one `EventSource` (idempotent). No-op where EventSource is absent. */
  connect(): void {
    if (this.eventSource || typeof EventSource === 'undefined') {
      return;
    }
    this.connection.set('connecting');
    const es = new EventSource(`${this.baseUrl}/api/events`);
    this.eventSource = es;

    es.onopen = () => {
      // Reopening after an error is a resync signal (drop-oldest may have
      // discarded frames while we were away).
      if (this.connection() === 'reconnecting') {
        this.resyncCounter.update((n) => n + 1);
      }
      this.connection.set('open');
    };
    es.onmessage = (ev: MessageEvent<string>) => {
      const frame = parseEventData(ev.data);
      if (frame) {
        this.frames.next(frame);
      }
    };
    es.onerror = () => {
      // EventSource retries on its own; surface the gap as "reconnecting".
      this.connection.set('reconnecting');
    };
  }

  /** Close the stream (the shell calls this on lock / teardown). */
  disconnect(): void {
    this.eventSource?.close();
    this.eventSource = null;
    this.connection.set('closed');
  }
}

/**
 * Parse one SSE `data:` payload into a scope-tagged frame, or `null` to skip
 * (v4 `parseSSEData`: trim, drop empty / `[DONE]` / `{}`, JSON.parse, swallow
 * chunking-artifact parse errors).
 */
export function parseEventData(raw: string): ScopedEvent | null {
  const trimmed = raw.trim();
  if (!trimmed || trimmed === '[DONE]' || trimmed === '{}') {
    return null;
  }
  try {
    return JSON.parse(trimmed) as ScopedEvent;
  } catch {
    return null;
  }
}

function isCoreResponse(body: unknown): body is CoreResponse {
  return (
    typeof body === 'object' &&
    body !== null &&
    typeof (body as { type?: unknown }).type === 'string' &&
    'data' in (body as object)
  );
}

function syntheticError(message: string, cause?: unknown): CoreResponse {
  return {
    type: 'error',
    data: { kind: 'internal', message: cause ? `${message} (${errorMessage(cause)})` : message },
  };
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) {
    return err.message || err.name || 'Unknown error';
  }
  return typeof err === 'string' ? err : 'Unknown error';
}
