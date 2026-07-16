/**
 * The §3 origin resolver (P4.7b, reconciled for one-origin in P4.7c): the
 * raw REST/byte surface — URL builders and multipart/binary fetches that
 * cannot ride the dispatch envelope — keeps its PATHS on every transport;
 * only the ORIGIN is transport-dependent. In a browser the resolver is
 * identity (same-origin, the dev proxy / axum static serving). Inside the
 * Tauri shell the paths ride the `qtap` custom protocol, which delegates
 * the full HTTP request (method + path + query + headers + body, multipart
 * included) into the reused `quilltap-web` router — and since P4.7c the
 * window ITSELF is served off the qtap origin (dogfood finding #12), so
 * the resolver is identity there too: relative paths already resolve
 * through the protocol. The prefix arm remains for the documented `devUrl`
 * dev loop, where the page origin is the Angular dev server and qtap
 * fetches stay cross-origin.
 *
 * Excluded surfaces (per §3, never called via protocol): `GET /api/events`
 * (§2 replaces it), `POST /api/dispatch` + `GET /health` (§1), and the
 * terminal WS upgrade (§4).
 */

import { isTauri } from '@tauri-apps/api/core';

/**
 * Resolve an absolute API path (`/api/v1/...`) for the active transport:
 * identity in the browser and on a qtap-origin page, `qtap`-origin-prefixed
 * inside a Tauri webview whose page lives elsewhere (the dev loop).
 */
export function apiUrl(path: string): string {
  return isTauri() && !onQtapOrigin() ? `${qtapOrigin()}${path}` : path;
}

/**
 * True when the page itself is already served off the qtap origin (the
 * P4.7c one-origin arrangement) — relative paths then resolve through the
 * protocol without help, on either platform shape.
 */
function onQtapOrigin(): boolean {
  return (
    typeof location !== 'undefined' &&
    (location.protocol === 'qtap:' || location.host === 'qtap.localhost')
  );
}

/**
 * The `qtap` scheme's origin, per Tauri 2 platform behavior: custom protocols
 * are `scheme://localhost` on macOS/Linux but `http://scheme.localhost` on
 * Windows (WebView2 cannot register bare custom schemes).
 */
function qtapOrigin(): string {
  const windows = typeof navigator !== 'undefined' && navigator.userAgent.includes('Windows');
  return windows ? 'http://qtap.localhost' : 'qtap://localhost';
}
