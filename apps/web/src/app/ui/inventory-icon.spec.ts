import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { beforeAll, describe, expect, it } from 'vitest';

import { DEFAULT_TAB_META } from '../workspace/core/tab-meta';
import { Icon, type IconName } from './icon';

/**
 * v4 `__tests__/unit/components/icons/inventory-icon.test.ts` (`e27fb6b34`),
 * its four cases transcribed (P4.D330 Tier 2 item 25):
 *
 *  1. `inventory` is a registered icon name — here a type-level `satisfies`
 *     plus the rendered `data-icon` hook the `_icons.css` mask rules key on.
 *  2. the default stylesheet carries the inventory mask rule
 *     (`src/styles/qt-components/_icons.css`).
 *  3. the Madman's Box manifest overrides it (`public/themes/madmans-box/theme.json`).
 *
 * Cases 2 and 3 read files through `node:fs`, loaded at RUN time (vitest runs
 * in Node) through a non-literal specifier the browser-platform bundler leaves
 * alone — the `pascal/raw-gate-reads.census.spec.ts` mechanism.
 *  4. the Inventory workspace tab wears it rather than Pascal's `dice`.
 */

/** The slice of `node:fs` this spec reads. */
interface NodeFs {
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

@Component({
  imports: [Icon],
  template: `<qt-icon name="inventory" class="w-7 h-7" />`,
})
class Host {}

describe('inventory icon', () => {
  it('is a registered icon name, rendered through its data-icon hook', () => {
    const name = 'inventory' satisfies IconName;
    TestBed.configureTestingModule({ imports: [Host] });
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    const span = (fixture.nativeElement as HTMLElement).querySelector('qt-icon span')!;
    expect(span.getAttribute('data-icon')).toBe(name);
    TestBed.resetTestingModule();
  });

  it('has a mask rule in the default stylesheet', () => {
    const css = fs.readFileSync(webPath('src/styles/qt-components/_icons.css'), 'utf8');
    expect(css).toMatch(/\[data-icon="inventory"\][^\n]*url\("\/images\/icons\/inventory\.svg"\)/);
  });

  it('is overridden by the Madman’s Box bundle manifest', () => {
    const manifest = JSON.parse(fs.readFileSync(webPath('public/themes/madmans-box/theme.json'), 'utf8')) as {
      icons?: Record<string, string>;
    };
    expect(manifest.icons?.['inventory']).toBe('icons/inventory.svg');
  });

  it('is the Inventory workspace tab icon', () => {
    expect(DEFAULT_TAB_META.inventory.icon).toBe('inventory');
    expect(DEFAULT_TAB_META.inventory.title).toBe('The Inventory');
  });
});
