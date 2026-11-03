/**
 * The vantage loader's `pool.owns` over `isInContainer` (v4 `0276d10d9`,
 * `use-inventory-vantage-items.ts` at `236c04e35`): the pre-fix predicate
 * compared SCOPES only, so a character vantage counted another character's
 * own object as its own.
 */
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { CoreClient } from '../core/core-client';
import { OWN, PROJECT, obj } from './__fixtures__/inventory-ui.fixture';
import { InventoryVantageItemsLoader } from './inventory-vantage-items';

function loaderOver(items: unknown[]): InventoryVantageItemsLoader {
  const core = { dispatchData: async () => ({ items }) } as unknown as CoreClient;
  return TestBed.runInInjectionContext(() => new InventoryVantageItemsLoader(core));
}

describe('InventoryVantageItemsLoader — owns (v4 0276d10d9 isInContainer)', () => {
  const mine = obj('mine', { origin: OWN });
  const bertie = obj('bertie', { origin: { scope: 'character', id: 'c-bert', name: 'Bertie' } });
  const quest = obj('quest', { origin: PROJECT });
  const group = obj('group', { origin: { scope: 'group', id: 'g-1', name: 'The Regulars' } });
  const loose = obj('loose', { origin: undefined });

  it('a character vantage owns its own objects and origin-less ones — not another character’s, nor a group’s', async () => {
    const l = loaderOver([mine, bertie, quest, group, loose]);
    await l.load({ kind: 'character', characterId: 'c-viv' });
    const owns = l.pool().owns;
    expect(['mine', 'bertie', 'quest', 'group', 'loose'].map(owns)).toEqual([true, false, false, false, true]);
  });

  it('a shared-tier vantage owns only its scope’s objects', async () => {
    const l = loaderOver([quest, group, loose]);
    await l.load({ kind: 'container', container: { scope: 'project', id: 'p-1' } });
    const owns = l.pool().owns;
    expect(['quest', 'group', 'loose'].map(owns)).toEqual([true, false, true]);
  });
});
