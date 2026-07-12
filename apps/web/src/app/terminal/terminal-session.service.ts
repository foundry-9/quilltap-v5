import { Injectable, signal, type Signal, type WritableSignal } from '@angular/core';

import type { PtySessionMeta, WsClientMessage, WsServerMessage } from './terminal-protocol';

/** The lifecycle of one terminal WebSocket (v4 `TerminalState`). */
export type TerminalState = 'connecting' | 'live' | 'exited' | 'error';

/** The PTY exit info (v4 `ExitInfo`). */
export interface ExitInfo {
  code: number | null;
  signal?: string;
}

/** The reactive read surface + input methods one consumer holds for a session. */
export interface TerminalSessionHandle {
  readonly state: Signal<TerminalState>;
  readonly meta: Signal<PtySessionMeta | null>;
  readonly exitInfo: Signal<ExitInfo | null>;
  /** Send keystrokes to the PTY (v4 `send`). */
  send(data: string): void;
  /** Report a new viewport size (v4 `resize`). */
  resize(cols: number, rows: number): void;
  /**
   * Subscribe to PTY output. On subscribe the buffered output (server ring
   * buffer + any chunks that arrived before this subscriber attached) replays
   * immediately so a fresh xterm lands on the first prompt. Returns an
   * unsubscribe (v4 `onData`).
   */
  onData(cb: (chunk: string) => void): () => void;
  /** Release this consumer's hold; the WS closes once the last handle releases. */
  release(): void;
}

const MAX_REPLAY_BYTES = 512 * 1024;
const PING_INTERVAL_MS = 30_000;
const MAX_RECONNECTS = 3;
const RECONNECT_BACKOFF_MS = 2_000;

/**
 * The client-side output fan-out (v4 `dispatchOutput` + `onData`), factored out
 * so the replay-buffer + subscriber semantics are unit-testable without a live
 * WebSocket. Output (including the server ring buffer sent on subscribe and the
 * shell's first prompt) can arrive before xterm finishes its async open; without
 * buffering those chunks would dispatch to zero subscribers and be dropped.
 */
export class OutputFanout {
  private buffer = '';
  private readonly subscribers = new Set<(chunk: string) => void>();

  /** Append a chunk (capped at the tail) and notify live subscribers. */
  dispatch(chunk: string): void {
    const next = this.buffer + chunk;
    this.buffer = next.length > MAX_REPLAY_BYTES ? next.slice(-MAX_REPLAY_BYTES) : next;
    this.subscribers.forEach((cb) => cb(chunk));
  }

  /** Add a subscriber, replaying the buffered output to it. Returns unsubscribe. */
  subscribe(cb: (chunk: string) => void): () => void {
    this.subscribers.add(cb);
    if (this.buffer.length > 0) {
      try {
        cb(this.buffer);
      } catch (err) {
        console.error('Terminal replay subscriber threw:', err);
      }
    }
    return () => {
      this.subscribers.delete(cb);
    };
  }
}

/**
 * The reactive slots a server frame writes into — the structural subset of
 * {@link Session} that {@link TerminalSessionService.handleServerMessage} needs.
 * Exported so the frame-mapping spec can drive frames without a live socket.
 */
export interface SessionSink {
  readonly fanout: OutputFanout;
  readonly state: WritableSignal<TerminalState>;
  readonly meta: WritableSignal<PtySessionMeta | null>;
  readonly exitInfo: WritableSignal<ExitInfo | null>;
}

/** One live session shared by all handles bound to a given session id. */
interface Session extends SessionSink {
  readonly id: string;
  ws: WebSocket | null;
  refCount: number;
  reconnectCount: number;
  pingInterval: ReturnType<typeof setInterval> | null;
  reconnectTimeout: ReturnType<typeof setTimeout> | null;
  disposed: boolean;
}

/**
 * The terminal session service (v4 `useTerminalSession`, as an injectable): ONE
 * WebSocket per session id, ref-counted across the pane, the inline embed, and
 * the pop-out page. Connects to `/api/v1/terminals/{id}/stream`, sends
 * `input`/`resize`/`ping` (a 30s keepalive), fans out PTY `output`, tracks
 * `meta`/`exit`, re-emits `chat-update` as a DOM event so the Salon page can
 * refetch, and reconnects on a transient close (1006/1011) with 2s backoff, max
 * 3 retries.
 */
@Injectable({ providedIn: 'root' })
export class TerminalSessionService {
  private readonly sessions = new Map<string, Session>();

  /** Acquire a handle for a session id, opening the WS on the first acquire. */
  acquire(sessionId: string): TerminalSessionHandle {
    let session = this.sessions.get(sessionId);
    if (!session) {
      session = {
        id: sessionId,
        ws: null,
        refCount: 0,
        reconnectCount: 0,
        pingInterval: null,
        reconnectTimeout: null,
        disposed: false,
        fanout: new OutputFanout(),
        state: signal<TerminalState>('connecting'),
        meta: signal<PtySessionMeta | null>(null),
        exitInfo: signal<ExitInfo | null>(null),
      };
      this.sessions.set(sessionId, session);
      this.connect(session);
    }
    session.refCount += 1;
    const active = session;

    let released = false;
    return {
      state: active.state.asReadonly(),
      meta: active.meta.asReadonly(),
      exitInfo: active.exitInfo.asReadonly(),
      send: (data) => this.send(active, data),
      resize: (cols, rows) => this.resize(active, cols, rows),
      onData: (cb) => active.fanout.subscribe(cb),
      release: () => {
        if (released) return;
        released = true;
        this.release(active);
      },
    };
  }

  // ---------------------------------------------------------------------------
  // WebSocket lifecycle
  // ---------------------------------------------------------------------------

  private connect(session: Session): void {
    if (typeof WebSocket === 'undefined' || typeof window === 'undefined') {
      return;
    }
    if (session.ws && session.ws.readyState === WebSocket.OPEN) {
      return;
    }
    session.state.set('connecting');
    const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
    const url = `${protocol}://${window.location.host}/api/v1/terminals/${session.id}/stream`;
    const ws = new WebSocket(url);
    session.ws = ws;

    ws.onopen = () => {
      session.state.set('live');
      session.reconnectCount = 0;
      session.pingInterval = setInterval(() => {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ type: 'ping' } satisfies WsClientMessage));
        }
      }, PING_INTERVAL_MS);
    };

    ws.onmessage = (event: MessageEvent<string>) => {
      let message: WsServerMessage;
      try {
        message = JSON.parse(event.data) as WsServerMessage;
      } catch (err) {
        console.error('Failed to parse WS message:', err);
        return;
      }
      this.handleServerMessage(session, message);
    };

    ws.onerror = () => {
      session.state.set('error');
    };

    ws.onclose = (event: CloseEvent) => {
      if (session.pingInterval) {
        clearInterval(session.pingInterval);
        session.pingInterval = null;
      }
      if (session.disposed) {
        return;
      }
      if ((event.code === 1006 || event.code === 1011) && session.reconnectCount < MAX_RECONNECTS) {
        session.reconnectCount += 1;
        const backoff = RECONNECT_BACKOFF_MS * session.reconnectCount;
        session.reconnectTimeout = setTimeout(() => this.connect(session), backoff);
      } else {
        session.state.set('error');
      }
    };
  }

  /**
   * Route one server frame to the session's reactive state (v4 `ws.onmessage`).
   * Exposed for the unit spec — it drives frames without a live socket.
   */
  handleServerMessage(session: SessionSink, message: WsServerMessage): void {
    switch (message.type) {
      case 'output':
        session.fanout.dispatch(message.data);
        break;
      case 'meta':
        session.meta.set(message.meta);
        break;
      case 'exit':
        session.state.set('exited');
        session.exitInfo.set({ code: message.code, signal: message.signal ?? undefined });
        break;
      case 'chat-update':
        // A new chat message (e.g. an Ariel periodic terminal-output summary)
        // landed. Re-emit as a DOM event so the Salon page can refetch its list.
        if (typeof window !== 'undefined') {
          window.dispatchEvent(
            new CustomEvent('quilltap:chat-update', {
              detail: { chatId: message.chatId, reason: message.reason },
            }),
          );
        }
        break;
      case 'pong':
        // Keepalive acknowledgement — no state change (v4 ignores it).
        break;
    }
  }

  private send(session: Session, data: string): void {
    if (session.ws?.readyState === WebSocket.OPEN) {
      session.ws.send(JSON.stringify({ type: 'input', data } satisfies WsClientMessage));
    }
  }

  private resize(session: Session, cols: number, rows: number): void {
    if (session.ws?.readyState === WebSocket.OPEN) {
      session.ws.send(JSON.stringify({ type: 'resize', cols, rows } satisfies WsClientMessage));
    }
  }

  private release(session: Session): void {
    session.refCount -= 1;
    if (session.refCount > 0) {
      return;
    }
    session.disposed = true;
    if (session.pingInterval) {
      clearInterval(session.pingInterval);
      session.pingInterval = null;
    }
    if (session.reconnectTimeout) {
      clearTimeout(session.reconnectTimeout);
      session.reconnectTimeout = null;
    }
    try {
      session.ws?.close();
    } catch {
      // already closing
    }
    session.ws = null;
    this.sessions.delete(session.id);
  }
}
