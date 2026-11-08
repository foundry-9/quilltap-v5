import { afterEach, describe, expect, it } from 'vitest';

/**
 * Deus Ex Machina's shortcut, the exact-modifier measurement on BOTH platforms
 * (P4.D385 item 5). v4's `KeyboardPlugin.tsx:116` reads `isMac ? metaKey &&
 * !ctrlKey : ctrlKey && !metaKey`; the editor binds `Shift-Mod-Enter`, whose
 * exact-modifier match must agree.
 *
 * prosemirror-keymap decides `Mod` = Cmd ONCE, at module load, from
 * `navigator.platform`, and the bundled copy the editor uses is shared across
 * spec files (measured: a `vi.hoisted` platform stub held alone and failed in
 * the full run; `vi.resetModules` + a dynamic import did not re-evaluate it).
 * So each platform loads a FRESH CommonJS copy of the package's own
 * `dist/index.cjs` under a stubbed platform, through `node:module`'s
 * `createRequire` with its cache entry dropped — loaded at RUN time through a
 * non-literal specifier the bundler leaves alone (the
 * `inventory-icon.spec.ts` mechanism).
 */

interface NodeModule {
  createRequire(path: string): NodeRequire;
}
interface NodeRequire {
  (id: string): unknown;
  resolve(id: string): string;
  cache: Record<string, unknown>;
}
type KeydownHandler = (bindings: Record<string, () => boolean>) => (view: unknown, event: KeyboardEvent) => boolean;

async function freshKeymap(platform: string): Promise<KeydownHandler> {
  Object.defineProperty(navigator, 'platform', { value: platform, configurable: true });
  const specifier = 'node:module';
  const { createRequire } = (await import(/* @vite-ignore */ specifier)) as NodeModule;
  const cwd = (globalThis as unknown as { process: { cwd(): string } }).process.cwd();
  const req = createRequire(`${cwd}/package.json`);
  const path = req.resolve('prosemirror-keymap');
  delete req.cache[path];
  return (req(path) as { keydownHandler: KeydownHandler }).keydownHandler;
}

describe('Shift-Mod-Enter is the primary modifier only, on each platform', () => {
  afterEach(() => {
    delete (navigator as unknown as Record<string, unknown>)['platform'];
  });

  it.each([
    ['MacIntel', { metaKey: true }, { ctrlKey: true }],
    ['Win32', { ctrlKey: true }, { metaKey: true }],
    ['Linux x86_64', { ctrlKey: true }, { metaKey: true }],
  ] as const)('on %s', async (platform, primary, other) => {
    const keydownHandler = await freshKeymap(platform);
    let fired = 0;
    const handler = keydownHandler({ 'Shift-Mod-Enter': () => ((fired += 1), true) });
    const press = (mods: KeyboardEventInit) =>
      handler({}, new KeyboardEvent('keydown', { key: 'Enter', shiftKey: true, ...mods }));
    expect(press(primary)).toBe(true);
    expect(press(other)).toBe(false);
    expect(press({ ...primary, ...other })).toBe(false);
    expect(press({ ...primary, altKey: true })).toBe(false);
    expect(press({})).toBe(false);
    expect(fired).toBe(1);
  });
});
