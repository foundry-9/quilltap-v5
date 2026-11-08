import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { InventoryDialogService } from '../inventory/inventory-dialog.service';
import { WardrobeDialogService } from '../wardrobe/wardrobe-dialog.service';
import { ItemDialogState } from './item-dialog.service';

/**
 * v4 `components/item-browser/item-dialog-provider.tsx` at the pin
 * `624d0d25c` (NEW at `600c2906f`) — one open / close state per kind; the
 * wardrobe and Inventory services extend it with their `remountKey`s
 * unchanged. Transcribed from v4 `__tests__/unit/components/providers/
 * item-dialog-provider.test.tsx` (4 cases); the two misuse cases ("throws
 * outside its provider…", "name their own provider when misused") are the
 * RECORDED COLLAPSE — a root service is always present, so v4's provider-
 * missing throw has no v5 counterpart (`wardrobe-dialog.service.ts`'s
 * DIVERGENCE note); each is pinned here as "always present".
 */

class Probe extends ItemDialogState<{ characterId?: string }> {}

describe('ItemDialogState (v4 ItemDialogProvider)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('opens with a context, closes, and opens again with none', () => {
    const d = new Probe();
    expect(d.isOpen()).toBe(false);
    expect(d.context()).toBeNull();
    d.open({ characterId: 'c1' });
    expect(d.isOpen()).toBe(true);
    expect(d.context()).toEqual({ characterId: 'c1' });
    d.close();
    expect(d.isOpen()).toBe(false);
    // close keeps the last context (v4's close only flips isOpen).
    expect(d.context()).toEqual({ characterId: 'c1' });
    d.open();
    expect(d.isOpen()).toBe(true);
    expect(d.context()).toBeNull();
  });

  it('(collapsed: "throws outside its provider") a root service is always present', () => {
    expect(TestBed.inject(WardrobeDialogService)).toBeInstanceOf(ItemDialogState);
    expect(TestBed.inject(InventoryDialogService)).toBeInstanceOf(ItemDialogState);
  });
});

describe('the kind-bound services', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('keep the two dialogs apart', () => {
    const wardrobe = TestBed.inject(WardrobeDialogService);
    const inventory = TestBed.inject(InventoryDialogService);
    wardrobe.open({ characterId: 'c1', chatId: 'chat-1' });
    expect(wardrobe.isOpen()).toBe(true);
    expect(inventory.isOpen()).toBe(false);
    inventory.open({ projectId: 'p1' });
    wardrobe.close();
    expect(wardrobe.isOpen()).toBe(false);
    expect(inventory.isOpen()).toBe(true);
    // Their remount keys are unchanged by the shared base.
    expect(wardrobe.remountKey()).toBe('c1|chat-1');
    expect(inventory.remountKey()).toBe('||p1|');
  });

  it('(collapsed: "name their own provider when misused") each kind is its own root singleton', () => {
    expect(TestBed.inject(WardrobeDialogService)).toBe(TestBed.inject(WardrobeDialogService));
    expect(TestBed.inject(WardrobeDialogService)).not.toBe(TestBed.inject(InventoryDialogService) as unknown);
  });
});
