/**
 * The Tauri pipe for the terminal stream (P4.7b §4, against the Shared
 * contract lane A implements):
 *
 *  - `terminal_attach`, args `sessionId` + `onMessage: Channel` — the channel
 *    carries `WsServerMessage` JSON verbatim (the frozen union); what is sent
 *    on open mirrors the frozen WS route exactly (server-side).
 *  - `terminal_send`, args `sessionId` + `message: <WsClientMessage>`.
 *  - `terminal_detach`, args `sessionId`.
 *
 * An attach failure routes into the SAME reconnect path the WS close handler
 * uses (`onError` + `onClose({transient: true})` — a WS that fails to connect
 * fires error-then-1006 the same way), so the service's backoff/retry logic
 * is shared, never forked. The terminal REST verbs ride §3 unchanged.
 */

import { loadTauriIpc, type TauriIpc } from '../core/tauri-api';

import type { WsServerMessage } from './terminal-protocol';
import type {
  TerminalStreamCallbacks,
  TerminalStreamConnection,
  TerminalStreamTransport,
} from './terminal-stream-transport';

export class TauriTerminalStreamTransport implements TerminalStreamTransport {
  open(sessionId: string, callbacks: TerminalStreamCallbacks): TerminalStreamConnection | null {
    let closed = false;
    let attached = false;
    let ipcRef: TauriIpc | null = null;

    void (async () => {
      const ipc = await loadTauriIpc();
      ipcRef = ipc;
      const onMessage = ipc.channel<WsServerMessage>((message) => {
        if (!closed) {
          callbacks.onMessage(message);
        }
      });
      await ipc.invoke('terminal_attach', { sessionId, onMessage });
      if (closed) {
        // close() raced the attach — detach the forwarder we just created.
        void ipc.invoke('terminal_detach', { sessionId }).catch(() => undefined);
        return;
      }
      attached = true;
      callbacks.onOpen();
    })().catch(() => {
      if (!closed) {
        // The WS failure shape: error, then a transient close → the service's
        // shared backoff reconnect path (§4's attach-failure rule).
        callbacks.onError();
        callbacks.onClose({ transient: true });
      }
    });

    return {
      isOpen: () => attached && !closed,
      send: (message) => {
        if (attached && !closed) {
          void ipcRef?.invoke('terminal_send', { sessionId, message }).catch(() => undefined);
        }
      },
      close: () => {
        if (closed) {
          return;
        }
        closed = true;
        if (attached) {
          attached = false;
          void ipcRef?.invoke('terminal_detach', { sessionId }).catch(() => undefined);
        }
      },
    };
  }
}
