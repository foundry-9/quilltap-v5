// @vitest-environment jsdom
// @vitest-environment-options {"url": "http://qtap.localhost/"}
/**
 * The P4.7c one-origin arm of the §3 resolver: when the page ITSELF is
 * served off the qtap origin, `apiUrl` is identity — relative paths
 * already resolve through the protocol into the reused router, so
 * prefixing would be a same-origin no-op at best and a platform-shape
 * hazard at worst. jsdom cannot host a custom-scheme (`qtap://`) document,
 * so this file pins the arm through the WINDOWS one-origin page URL
 * (`http://qtap.localhost/` — an ordinary http URL jsdom accepts); the
 * `location.protocol === 'qtap:'` macOS/Linux arm of the same check is
 * exercised by the real webview (the P4.7c spike record + the human walk).
 */

import { afterEach, describe, expect, it } from 'vitest';

import { apiUrl } from './api-url';

function fakeTauri(): void {
  (globalThis as Record<string, unknown>)['isTauri'] = true;
}

afterEach(() => {
  delete (globalThis as Record<string, unknown>)['isTauri'];
});

describe('apiUrl on a qtap-origin page (one-origin)', () => {
  it('is identity under Tauri — the page origin already routes to the core', () => {
    expect(location.host).toBe('qtap.localhost');
    fakeTauri();
    expect(apiUrl('/api/v1/files/f-1')).toBe('/api/v1/files/f-1');
    expect(apiUrl('/api/v1/terminals?chatId=c-1')).toBe('/api/v1/terminals?chatId=c-1');
  });

  it('stays identity without Tauri too (nothing regresses in a browser)', () => {
    expect(apiUrl('/api/v1/files/f-1')).toBe('/api/v1/files/f-1');
  });
});
