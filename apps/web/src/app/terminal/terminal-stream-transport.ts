/**
 * The terminal stream-transport seam (P4.7b §4): the PIPE under
 * `TerminalSessionService`, extracted so the WS lifecycle and the Tauri
 * channel ride the same service logic (ping cadence, reconnect/backoff,
 * state transitions, the fan-out — all stay in the service). The wire
 * vocabulary is the frozen union in `terminal-protocol.ts` on both pipes.
 */

import type { WsClientMessage, WsServerMessage } from './terminal-protocol';

/** The service's window onto one stream's lifecycle. */
export interface TerminalStreamCallbacks {
  /** The pipe is live (WS `onopen` / `terminal_attach` resolved). */
  onOpen(): void;
  /** One decoded server frame (the frozen `WsServerMessage` union). */
  onMessage(message: WsServerMessage): void;
  /** A transport-level error (WS `onerror` / a failed attach). */
  onError(): void;
  /**
   * The pipe closed. `transient` marks closes eligible for the service's
   * backoff reconnect path — WS codes 1006/1011, or a Tauri attach
   * failure/channel drop (the §4 rule: both route into the SAME path).
   */
  onClose(info: { transient: boolean }): void;
}

/** One open (or opening) pipe. */
export interface TerminalStreamConnection {
  /** Live right now? (WS `readyState === OPEN` / attached and not closed.) */
  isOpen(): boolean;
  /** Send one client frame; silently dropped unless open (the WS idiom). */
  send(message: WsClientMessage): void;
  /** Tear the pipe down (no further callbacks are expected). */
  close(): void;
}

export interface TerminalStreamTransport {
  /**
   * Open a stream for one PTY session. Returns `null` where the environment
   * has no pipe at all (SSR / jsdom — the pre-split `typeof WebSocket` guard),
   * in which case the session simply stays in its initial state.
   */
  open(sessionId: string, callbacks: TerminalStreamCallbacks): TerminalStreamConnection | null;
}

/**
 * The browser pipe: ONE WebSocket on `/api/v1/terminals/{id}/stream`,
 * extracted byte-for-byte from the pre-P4.7b service (URL scheme choice,
 * JSON parse with the error-log-and-skip idiom, the 1006/1011 transient
 * classification, the readyState send guard).
 */
export class WsTerminalStreamTransport implements TerminalStreamTransport {
  open(sessionId: string, callbacks: TerminalStreamCallbacks): TerminalStreamConnection | null {
    if (typeof WebSocket === 'undefined' || typeof window === 'undefined') {
      return null;
    }
    const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
    const url = `${protocol}://${window.location.host}/api/v1/terminals/${sessionId}/stream`;
    const ws = new WebSocket(url);

    ws.onopen = () => {
      callbacks.onOpen();
    };
    ws.onmessage = (event: MessageEvent<string>) => {
      let message: WsServerMessage;
      try {
        message = JSON.parse(event.data) as WsServerMessage;
      } catch (err) {
        console.error('Failed to parse WS message:', err);
        return;
      }
      callbacks.onMessage(message);
    };
    ws.onerror = () => {
      callbacks.onError();
    };
    ws.onclose = (event: CloseEvent) => {
      callbacks.onClose({ transient: event.code === 1006 || event.code === 1011 });
    };

    return {
      isOpen: () => ws.readyState === WebSocket.OPEN,
      send: (message) => {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify(message));
        }
      },
      close: () => {
        try {
          ws.close();
        } catch {
          // already closing
        }
      },
    };
  }
}
