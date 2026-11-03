/**
 * `isInContainer` — v4 `lib/inventory/container.ts:73-81` at `236c04e35`
 * (`0276d10d9`): an object lives in the tier being browsed. Its three arms,
 * from v4's own reading.
 */
import { describe, expect, it } from 'vitest';

import type { InventoryOrigin } from '../core/core-contract';
import { isInContainer } from './inventory-container';

const at = (origin: InventoryOrigin | null | undefined) => ({ origin });
const VIV: InventoryOrigin = { scope: 'character', id: 'c-viv', name: 'Vivienne' };
const BERT: InventoryOrigin = { scope: 'character', id: 'c-bert', name: 'Bertie' };
const QUEST: InventoryOrigin = { scope: 'project', id: 'p-1', name: 'The Quest' };
const GENERAL: InventoryOrigin = { scope: 'general', id: null, name: 'Quilltap General' };

describe('isInContainer (v4 container.ts:73-81)', () => {
  it('an origin-less object is in any container', () => {
    expect(isInContainer(at(undefined), { scope: 'character', id: 'c-viv' })).toBe(true);
    expect(isInContainer(at(null), { scope: 'general', id: null })).toBe(true);
  });

  it('a shared container holds only objects of its own scope', () => {
    expect(isInContainer(at(QUEST), { scope: 'project', id: 'p-1' })).toBe(true);
    expect(isInContainer(at(GENERAL), { scope: 'general', id: null })).toBe(true);
    expect(isInContainer(at(GENERAL), { scope: 'project', id: 'p-1' })).toBe(false);
    expect(isInContainer(at(VIV), { scope: 'group', id: 'g-1' })).toBe(false);
  });

  it('a character container holds only that character’s own objects', () => {
    expect(isInContainer(at(VIV), { scope: 'character', id: 'c-viv' })).toBe(true);
    expect(isInContainer(at(BERT), { scope: 'character', id: 'c-viv' })).toBe(false);
    expect(isInContainer(at(QUEST), { scope: 'character', id: 'c-viv' })).toBe(false);
  });
});
