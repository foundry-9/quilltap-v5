/**
 * The transport boundary UNDER {@link CoreClient} (D14, made real in P4.7b):
 * the three raw server touchpoints — the dispatch POST, the health GET, and
 * the single global event stream — behind one interface, with the HTTP
 * implementation here (the frozen browser path, extracted byte-for-byte from
 * `core-client.ts`) and the Tauri IPC implementation in
 * `tauri-core-transport.ts`. Everything above the seam (the typed helpers,
 * TanStack Query, the stream reducer) never learns which transport it is on.
 */

import type { CoreRequest, CoreResponse, PepperState, ScopedEvent } from './core-contract';

/**
 * The interpreted `GET /health` result (the startup gate branches on this).
 *
 * `version` (P4.9c, §3) is the ONLY addition: the serving build's version,
 * which the health body now carries on its healthy and locked arms. It is
 * optional because a server older than that change simply omits it — no gate
 * behavior depends on it, and the About screen is its only reader.
 */
export type HealthStatus =
  | { kind: 'healthy'; version?: string }
  | { kind: 'locked'; pepperState: PepperState; version?: string }
  | { kind: 'lock-conflict'; lockConflict: unknown }
  | { kind: 'unhealthy'; message: string }
  /** The server never answered (still booting / network hiccup) — retry. */
  | { kind: 'unreachable'; message: string };

/** The one live-stream connection state, exposed as a signal for the shell. */
export type ConnectionState = 'idle' | 'connecting' | 'open' | 'reconnecting' | 'closed';

/**
 * The narrow window a transport gets onto {@link CoreClient}'s reactive
 * surface: connection-state transitions, resync bumps, and the one
 * "frame accepted" path both transports feed AFTER their native decode
 * (SSE `data:` text for HTTP, a pre-parsed JSON value under Tauri).
 */
export interface EventStreamSink {
  /** Read the current connection state (the HTTP reopen-after-error check). */
  connectionState(): ConnectionState;
  setConnection(state: ConnectionState): void;
  /** "You may have missed frames — refetch authoritative state." */
  bumpResync(): void;
  /** One decoded scope-tagged frame off the live stream. */
  acceptFrame(frame: ScopedEvent): void;
}

/**
 * The internal transport pair's contract. Implementations own the raw IO and
 * its failure vocabulary; the interpreted shapes ({@link CoreResponse},
 * {@link HealthStatus}) are identical on both sides.
 */
export interface CoreTransport {
  /** One request → ALWAYS a typed envelope (failures become synthetic errors). */
  dispatch(request: CoreRequest): Promise<CoreResponse>;
  /** The `GET /health` readiness vocabulary (startup gate). */
  fetchHealth(): Promise<HealthStatus>;
  /** Open the live stream into `sink` (idempotent while open). */
  connect(sink: EventStreamSink): void;
  /** Tear the live stream down (the caller owns the 'closed' state). */
  disconnect(): void;
}

// ---------------------------------------------------------------------------
// The HTTP transport (the frozen browser path)
// ---------------------------------------------------------------------------

/**
 * The browser/HTTP implementation: `POST /api/dispatch`, `GET /health`, and
 * ONE `EventSource` on `GET /api/events` — extracted verbatim from the
 * pre-P4.7b `CoreClient` (same synthetic-error messages, same health
 * vocabulary, same connection-state transitions, same `parseEventData` skips).
 */
export class HttpCoreTransport implements CoreTransport {
  /** Same-origin by default (the dev proxy forwards `/api` + `/health`). */
  private readonly baseUrl = '';

  private eventSource: EventSource | null = null;

  /**
   * Send one request to `POST /api/dispatch`. ALWAYS resolves to a typed
   * `CoreResponse` — a non-2xx body is still the typed envelope (the Locked 503
   * carries the error envelope plus v4's `setupUrl` body, which we ignore here);
   * a network failure becomes a synthetic `internal` error so callers get a
   * uniform shape.
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
   * Read `GET /health` and interpret v4's status vocabulary (health.rs) via the
   * shared {@link interpretHealth}; an unreachable server (still booting) →
   * retry.
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
    return interpretHealth(res.status, body);
  }

  /** Open the one `EventSource` (idempotent). No-op where EventSource is absent. */
  connect(sink: EventStreamSink): void {
    if (this.eventSource || typeof EventSource === 'undefined') {
      return;
    }
    sink.setConnection('connecting');
    const es = new EventSource(`${this.baseUrl}/api/events`);
    this.eventSource = es;

    es.onopen = () => {
      // Reopening after an error is a resync signal (drop-oldest may have
      // discarded frames while we were away).
      if (sink.connectionState() === 'reconnecting') {
        sink.bumpResync();
      }
      sink.setConnection('open');
    };
    es.onmessage = (ev: MessageEvent<string>) => {
      const frame = parseEventData(ev.data);
      if (frame) {
        sink.acceptFrame(frame);
      }
    };
    es.onerror = () => {
      // EventSource retries on its own; surface the gap as "reconnecting".
      sink.setConnection('reconnecting');
    };
  }

  /** Close the stream (the shell calls this on lock / teardown). */
  disconnect(): void {
    this.eventSource?.close();
    this.eventSource = null;
  }
}

// ---------------------------------------------------------------------------
// Shared vocabulary (one interpreter, both transports — never forked)
// ---------------------------------------------------------------------------

/**
 * Interpret a `GET /health` status + body pair (health.rs's vocabulary): 200
 * healthy, 423 locked (carries `dbKeyState`), 409 lock-conflict, 503
 * unhealthy. Shared verbatim by the HTTP fetch and the Tauri `health` command
 * (whose reply carries the same status number) so the branches cannot drift.
 *
 * P4.9c: the healthy and locked arms additionally carry the serving build's
 * `version` when the server sends one. Reading it here (rather than fetching it
 * separately) is what lets both transports agree for free — the Tauri `health`
 * command already replies with the same body.
 */
export function interpretHealth(status: number, body: Record<string, unknown>): HealthStatus {
  const version = typeof body['version'] === 'string' ? body['version'] : undefined;
  switch (status) {
    case 200:
      return version ? { kind: 'healthy', version } : { kind: 'healthy' };
    case 423:
      return {
        kind: 'locked',
        pepperState: (body['dbKeyState'] as PepperState) ?? 'needs-passphrase',
        ...(version ? { version } : {}),
      };
    case 409:
      return { kind: 'lock-conflict', lockConflict: body['lockConflict'] ?? null };
    case 503:
      return {
        kind: 'unhealthy',
        message: (body['error'] as string) ?? 'The server is not ready.',
      };
    default:
      return { kind: 'unhealthy', message: `Unexpected health status ${status}.` };
  }
}

/**
 * Parse one SSE `data:` payload into a scope-tagged frame, or `null` to skip
 * (v4 `parseSSEData`: trim, drop empty / `[DONE]` / `{}`, JSON.parse, swallow
 * chunking-artifact parse errors). HTTP-side only: the Tauri event payload
 * arrives pre-parsed, and its skip rules exist for SSE chunking artifacts that
 * IPC cannot produce — do not round-trip Tauri payloads through this.
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

export function isCoreResponse(body: unknown): body is CoreResponse {
  return (
    typeof body === 'object' &&
    body !== null &&
    typeof (body as { type?: unknown }).type === 'string' &&
    'data' in (body as object)
  );
}

export function syntheticError(message: string, cause?: unknown): CoreResponse {
  return {
    type: 'error',
    data: { kind: 'internal', message: cause ? `${message} (${errorMessage(cause)})` : message },
  };
}

export function errorMessage(err: unknown): string {
  if (err instanceof Error) {
    return err.message || err.name || 'Unknown error';
  }
  return typeof err === 'string' ? err : 'Unknown error';
}
