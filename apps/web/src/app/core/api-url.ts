/**
 * The §3 origin resolver (P4.7b): the raw REST/byte surface — URL builders and
 * multipart/binary fetches that cannot ride the dispatch envelope — keeps its
 * PATHS on every transport; only the ORIGIN is transport-dependent. In a
 * browser the resolver is identity (same-origin, the dev proxy / axum static
 * serving). Inside the Tauri shell the same paths ride the `qtap` custom
 * protocol, which delegates the full HTTP request (method + path + query +
 * headers + body, multipart included) into the reused `quilltap-web` router.
 *
 * Excluded surfaces (per §3, never called via protocol): `GET /api/events`
 * (§2 replaces it), `POST /api/dispatch` + `GET /health` (§1), the terminal
 * WS upgrade (§4), and static assets (the webview serves `frontendDist`).
 */

import { isTauri } from '@tauri-apps/api/core';

/**
 * Resolve an absolute API path (`/api/v1/...`) for the active transport:
 * identity in the browser, `qtap`-origin-prefixed inside the Tauri shell.
 */
export function apiUrl(path: string): string {
  return isTauri() ? `${qtapOrigin()}${path}` : path;
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
