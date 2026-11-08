import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { beforeAll, describe, expect, it } from 'vitest';

import { Icon, type IconName } from './icon';

/**
 * v4 `__tests__/unit/components/icons/deus-ex-machina-icon.test.ts`
 * (`db6190b44` / `df8d4d70a`), its three cases transcribed (P4.D385 item 4):
 *
 *  1. `deus-ex-machina` is a registered icon name — a type-level `satisfies`
 *     plus the rendered `data-icon` hook the `_icons.css` mask rules key on.
 *  2. the default stylesheet carries its mask rule, and the asset exists.
 *  3. the Madman's Box manifest overrides it with its own glyph, and that file
 *     exists.
 *
 * The SVGs and the mask row are P4.D383's (`CSS`); this lane adds the
 * `IconName` entry. Files are read through `node:fs` at RUN time, the
 * `inventory-icon.spec.ts` mechanism.
 */

/** The slice of `node:fs` this spec reads. */
interface NodeFs {
  readFileSync(path: string, encoding: 'utf8'): string;
  existsSync(path: string): boolean;
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

@Component({
  imports: [Icon],
  template: `<qt-icon name="deus-ex-machina" class="w-4 h-4" />`,
})
class Host {}

describe('deus-ex-machina icon', () => {
  it('is a registered icon name, rendered through its data-icon hook', () => {
    const name = 'deus-ex-machina' satisfies IconName;
    TestBed.configureTestingModule({ imports: [Host] });
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    const span = (fixture.nativeElement as HTMLElement).querySelector('qt-icon span')!;
    expect(span.getAttribute('data-icon')).toBe(name);
    TestBed.resetTestingModule();
  });

  it('has a mask rule in the default stylesheet, and the default asset exists', () => {
    const css = fs.readFileSync(webPath('src/styles/qt-components/_icons.css'), 'utf8');
    expect(css).toMatch(/\[data-icon="deus-ex-machina"\][^\n]*url\("\/images\/icons\/deus-ex-machina\.svg"\)/);
    expect(fs.existsSync(webPath('public/images/icons/deus-ex-machina.svg'))).toBe(true);
  });

  it('is overridden by the Madman’s Box bundle manifest', () => {
    const manifest = JSON.parse(fs.readFileSync(webPath('public/themes/madmans-box/theme.json'), 'utf8')) as {
      icons?: Record<string, string>;
    };
    expect(manifest.icons?.['deus-ex-machina']).toBe('icons/deus-ex-machina.svg');
    expect(fs.existsSync(webPath('public/themes/madmans-box/icons/deus-ex-machina.svg'))).toBe(true);
  });
});
