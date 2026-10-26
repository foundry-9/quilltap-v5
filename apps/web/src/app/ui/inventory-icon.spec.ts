import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { DEFAULT_TAB_META } from '../workspace/core/tab-meta';
import { Icon, type IconName } from './icon';

/**
 * v4 `__tests__/unit/components/icons/inventory-icon.test.ts` (`e27fb6b34`),
 * its four cases transcribed (P4.D330 Tier 2 item 25):
 *
 *  1. `inventory` is a registered icon name — here a type-level `satisfies`
 *     plus the rendered `data-icon` hook the `_icons.css` mask rules key on.
 *  2. the default stylesheet carries the inventory mask rule — the row is
 *     P4.D336's file (`_icons.css`, a HANDOFF); and a vitest spec here cannot
 *     read a stylesheet or any file (no `?raw`, no `node:fs` — the standing
 *     `spa-spec-cannot-read-source` limit). SKIPPED with that reason; the
 *     unifier checks the row by `grep` at §S.5 (10). The SVG's bytes are
 *     likewise checked by `cmp` against the pin, in the lane record.
 *  3. the Madman's Box manifest overrides it — P4.D339's file; same limit.
 *  4. the Inventory workspace tab wears it rather than Pascal's `dice`.
 */

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

  it.skip('has a mask rule in the default stylesheet (HANDOFF: P4.D336 owns _icons.css; specs cannot read files)', () => {
    // [data-icon="inventory"] { --_qt-icon-mask: url("/images/icons/inventory.svg"); --_qt-icon-bg: none; }
  });

  it.skip('is overridden by the Madman’s Box bundle manifest (P4.D339 owns theme.json; specs cannot read files)', () => {
    // "inventory": "icons/inventory.svg"
  });

  it('is the Inventory workspace tab icon', () => {
    expect(DEFAULT_TAB_META.inventory.icon).toBe('inventory');
    expect(DEFAULT_TAB_META.inventory.title).toBe('The Inventory');
  });
});
