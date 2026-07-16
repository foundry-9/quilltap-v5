/**
 * Bootstrap transport selection (P4.7b): decided ONCE, at `CoreClient`
 * construction — i.e. before the first injection resolves — so nothing above
 * the seam ever learns which transport it is on. `isTauri()` is the library's
 * own detection (a bare global check, safe to import statically); the heavy
 * IPC modules stay behind `tauri-api.ts`'s lazy gateway.
 */

import { isTauri } from '@tauri-apps/api/core';

import { type CoreTransport, HttpCoreTransport } from './core-transport';
import { TauriCoreTransport } from './tauri-core-transport';

/** The HTTP transport in a browser; the IPC transport inside the Tauri shell. */
export function createCoreTransport(): CoreTransport {
  return isTauri() ? new TauriCoreTransport() : new HttpCoreTransport();
}
