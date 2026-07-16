import { afterEach, describe, expect, it } from 'vitest';

import { createCoreTransport } from './core-transport-factory';
import { HttpCoreTransport } from './core-transport';
import { TauriCoreTransport } from './tauri-core-transport';

/**
 * Bootstrap selection (P4.7b): `isTauri()` reads the `isTauri` global the
 * Tauri webview injects — absent in every browser and in this jsdom env.
 */
describe('createCoreTransport', () => {
  afterEach(() => {
    delete (globalThis as Record<string, unknown>)['isTauri'];
  });

  it('selects the HTTP transport in a plain browser', () => {
    expect(createCoreTransport()).toBeInstanceOf(HttpCoreTransport);
  });

  it('selects the Tauri transport inside the Tauri shell', () => {
    (globalThis as Record<string, unknown>)['isTauri'] = true;
    expect(createCoreTransport()).toBeInstanceOf(TauriCoreTransport);
  });
});
