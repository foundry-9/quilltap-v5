/**
 * Bootstrap selection for the terminal stream pipe (P4.7b §4) — the same
 * once-at-construction rule as `createCoreTransport`: the service picks its
 * pipe before the first `acquire`, and nothing above the seam knows which.
 */

import { isTauri } from '@tauri-apps/api/core';

import { TauriTerminalStreamTransport } from './tauri-terminal-stream-transport';
import {
  type TerminalStreamTransport,
  WsTerminalStreamTransport,
} from './terminal-stream-transport';

/** The WS pipe in a browser; the Channel-backed IPC pipe in the Tauri shell. */
export function createTerminalStreamTransport(): TerminalStreamTransport {
  return isTauri() ? new TauriTerminalStreamTransport() : new WsTerminalStreamTransport();
}
