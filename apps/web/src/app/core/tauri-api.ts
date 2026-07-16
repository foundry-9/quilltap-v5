/**
 * The one lazy gateway to `@tauri-apps/api` (P4.7b). Every Tauri-side
 * transport loads `invoke`/`listen` through here so the library lands in a
 * lazily imported chunk — a plain-browser build must never pull Tauri IPC
 * code into the initial bundle, let alone execute it. (The sole static
 * exception is `isTauri()` in `core-transport-factory.ts`: bootstrap selection
 * runs before any lazy chunk can load, and that helper is a bare global
 * check.)
 *
 * In specs the REAL modules load here too — `mockIPC(cb, { shouldMockEvents:
 * true })` from `@tauri-apps/api/mocks` intercepts below this seam, so the
 * round trips exercise the genuine `invoke`/`listen`/`Channel` plumbing.
 */

/** The narrow slice of the Tauri API the transports consume. */
export interface TauriIpc {
  invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  /** Subscribe to a Tauri event; resolves to the unlisten function. */
  listen(event: string, handler: (evt: { payload: unknown }) => void): Promise<() => void>;
  /**
   * Build a `Channel` delivering `onmessage(msg)` per frame. The returned
   * value is opaque here — it only ever travels as an `invoke` argument
   * (the §4 `terminal_attach` `onMessage` key).
   */
  channel<T>(onmessage: (message: T) => void): unknown;
}

let loaded: Promise<TauriIpc> | null = null;

/** Load (once) and adapt the Tauri API modules. */
export function loadTauriIpc(): Promise<TauriIpc> {
  loaded ??= Promise.all([import('@tauri-apps/api/core'), import('@tauri-apps/api/event')]).then(
    ([core, event]) => ({
      invoke: (cmd, args) => core.invoke(cmd, args),
      listen: (name, handler) => event.listen(name, handler),
      channel: (onmessage) => new core.Channel(onmessage),
    }),
  );
  return loaded;
}
