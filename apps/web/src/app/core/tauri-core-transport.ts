/**
 * The Tauri IPC implementation of {@link CoreTransport} (P4.7b — the D14 seam
 * made real, against the P4.7 Shared contract §1/§2 lane A implements):
 *
 *  - §1 `invoke('dispatch', { request })` — the command ALWAYS resolves with
 *    the `Response` enum serialized verbatim (typed error envelopes included;
 *    IPC carries no HTTP status, the envelope alone is authoritative).
 *  - §1 `invoke('health')` → `{ status, body }` where `status` is exactly the
 *    HTTP status `health.rs` would set — interpreted by the SAME shared
 *    `interpretHealth` the HTTP transport uses.
 *  - §2 `listen('quilltap://event')` THEN `invoke('events_attach')` — the
 *    listener must be live before attach, or the Green-Room `active_snapshot`
 *    backlog replay is dropped. `quilltap://resync` (broadcast lag, drop-
 *    oldest) bumps the resync counter — the IPC analogue of the SSE reopen.
 *
 * Payloads arrive pre-parsed as JSON values (no SSE framing), so frames feed
 * the sink's `acceptFrame` path directly — never re-stringified through
 * `parseEventData`, whose skip rules exist for SSE chunking artifacts.
 */

import type { CoreRequest, CoreResponse, ScopedEvent } from './core-contract';
import {
  type CoreTransport,
  type EventStreamSink,
  type HealthStatus,
  errorMessage,
  interpretHealth,
  isCoreResponse,
  syntheticError,
} from './core-transport';
import { loadTauriIpc } from './tauri-api';

/** The §1 `health` command reply: the status health.rs would set + the body. */
interface HealthCommandReply {
  status: number;
  body: unknown;
}

export class TauriCoreTransport implements CoreTransport {
  /** True while a stream session is up (or coming up) — the idempotence gate. */
  private active = false;
  /** Bumped by `disconnect()` so an in-flight attach knows it lost the race. */
  private generation = 0;
  private unlisteners: Array<() => void> = [];

  async dispatch(request: CoreRequest): Promise<CoreResponse> {
    let body: unknown;
    try {
      const ipc = await loadTauriIpc();
      body = await ipc.invoke('dispatch', { request });
    } catch (err) {
      // §1: typed errors RESOLVE — a rejection means the IPC bridge itself
      // failed, the analogue of the HTTP network failure (same envelope).
      return syntheticError('Connection lost. The server may still be starting.', err);
    }
    if (isCoreResponse(body)) {
      return body;
    }
    return syntheticError('The server returned an unexpected response (IPC).');
  }

  async fetchHealth(): Promise<HealthStatus> {
    let reply: HealthCommandReply;
    try {
      const ipc = await loadTauriIpc();
      reply = await ipc.invoke<HealthCommandReply>('health');
    } catch (err) {
      // The shell couldn't answer (engine still booting) — retry, like an
      // unreachable HTTP server.
      return { kind: 'unreachable', message: errorMessage(err) };
    }
    const body = (
      typeof reply?.body === 'object' && reply.body !== null ? reply.body : {}
    ) as Record<string, unknown>;
    // A malformed status falls through to interpretHealth's loud default arm.
    return interpretHealth(Number(reply?.status), body);
  }

  /** Open the §2 stream (idempotent): 'connecting' until attach resolves. */
  connect(sink: EventStreamSink): void {
    if (this.active) {
      return;
    }
    this.active = true;
    sink.setConnection('connecting');
    void this.attach(sink, this.generation);
  }

  private async attach(sink: EventStreamSink, gen: number): Promise<void> {
    const stale = () => gen !== this.generation;
    try {
      const ipc = await loadTauriIpc();

      // §2 ordering rule: subscribe FIRST, then attach — events_attach replays
      // the Green-Room active_snapshot backlog as quilltap://event frames, and
      // a late listener would drop them.
      const unlistenEvent = await ipc.listen('quilltap://event', (evt) => {
        if (stale()) return;
        const frame = frameFromPayload(evt.payload);
        if (frame) {
          sink.acceptFrame(frame);
        }
      });
      this.unlisteners.push(unlistenEvent);
      if (stale()) {
        this.releaseListeners();
        return;
      }

      const unlistenResync = await ipc.listen('quilltap://resync', () => {
        if (stale()) return;
        sink.bumpResync();
      });
      this.unlisteners.push(unlistenResync);
      if (stale()) {
        this.releaseListeners();
        return;
      }

      await ipc.invoke('events_attach');
      if (stale()) {
        return;
      }
      sink.setConnection('open');
    } catch {
      // Attach failed (should not happen in-process). Surface the gap in the
      // shared stream vocabulary; the startup gate / shell indicator treat it
      // exactly like an HTTP stream drop.
      if (!stale()) {
        sink.setConnection('reconnecting');
      }
    }
  }

  /** Unlisten and invalidate any in-flight attach (the caller sets 'closed'). */
  disconnect(): void {
    this.generation += 1;
    this.active = false;
    this.releaseListeners();
  }

  private releaseListeners(): void {
    const unlisteners = this.unlisteners;
    this.unlisteners = [];
    for (const unlisten of unlisteners) {
      try {
        unlisten();
      } catch {
        // already torn down
      }
    }
  }
}

/**
 * Accept one pre-parsed Tauri event payload as a scope-tagged frame. The §2
 * payload is the `Event` JSON exactly as the SSE `data:` payload — but already
 * decoded, so the only guard needed is "is this an object at all".
 */
function frameFromPayload(payload: unknown): ScopedEvent | null {
  return typeof payload === 'object' && payload !== null ? (payload as ScopedEvent) : null;
}
