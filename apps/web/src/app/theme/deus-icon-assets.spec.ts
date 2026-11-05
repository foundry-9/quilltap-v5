import { beforeAll, describe, expect, it } from 'vitest';

/**
 * The Deus Ex Machina icon's assets (P4.D383, contract C16 — v4 `df8d4d70a`,
 * read at `624d0d25c`): the default glyph, the Madman's Box override, the
 * pack manifest key that points at it, and the `_icons.css` mask rule the
 * `data-icon` hook keys on. The `IconName` union entry itself is P4.D385's
 * (`ui/icon.ts`), so this spec reads only the files, not the component.
 *
 * Files are read through `node:fs` at RUN time — the
 * `ui/inventory-icon.spec.ts` mechanism.
 */

/** The slice of `node:fs` this spec reads. */
interface NodeFs {
  existsSync(path: string): boolean;
  readFileSync(path: string, encoding: 'utf8'): string;
}

let fs: NodeFs;

beforeAll(async () => {
  const specifier = 'node:fs';
  fs = (await import(/* @vite-ignore */ specifier)) as NodeFs;
});

/** A path under `apps/web`, from the runner's working directory. */
function webPath(rel: string): string {
  const cwd = (globalThis as unknown as { process: { cwd(): string } }).process.cwd();
  return `${cwd.replace(/\\/g, '/')}/${rel}`;
}

const SVGS = [
  'public/images/icons/deus-ex-machina.svg',
  'public/themes/madmans-box/icons/deus-ex-machina.svg',
] as const;

describe('deus-ex-machina icon assets', () => {
  it('has a mask rule in the default stylesheet', () => {
    const css = fs.readFileSync(webPath('src/styles/qt-components/_icons.css'), 'utf8');
    expect(css).toMatch(
      /\[data-icon="deus-ex-machina"\][^\n]*url\("\/images\/icons\/deus-ex-machina\.svg"\)/,
    );
  });

  for (const rel of SVGS) {
    it(`ships ${rel} as a 24x24 SVG`, () => {
      expect(fs.existsSync(webPath(rel))).toBe(true);
      expect(fs.readFileSync(webPath(rel), 'utf8')).toMatch(/^<svg [^>]*viewBox="0 0 24 24"/);
    });
  }

  it('is overridden by the Madman’s Box bundle manifest', () => {
    const manifest = JSON.parse(
      fs.readFileSync(webPath('public/themes/madmans-box/theme.json'), 'utf8'),
    ) as { icons?: Record<string, string> };
    expect(manifest.icons?.['deus-ex-machina']).toBe('icons/deus-ex-machina.svg');
  });
});
