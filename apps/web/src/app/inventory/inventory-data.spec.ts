import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import type { CoreRequest } from '../core/core-contract';
import { BOW, LAMP, RING, SWORD, obj, PROJECT } from './__fixtures__/inventory-ui.fixture';
import { createChatInventoryData } from './chat-inventory-data';
import type { InventoryContainer } from './inventory-container';
import { InventoryDialogService } from './inventory-dialog.service';
import { createInventoryFoldersStore } from './inventory-folders-data';
import { createInventoryListData } from './inventory-list-data';

/**
 * P4.D330 R-1 / R-2 / R-3 — the Inventory's signal stores over C7's verbs
 * (v4 `useInventoryList` / `useInventoryListData` / `useInventoryFolders` /
 * `useChatInventory`) and the dialog service (v4 `InventoryDialogProvider`).
 */

type Answer = (req: Record<string, unknown>) => Promise<Record<string, unknown>>;

function fakeCore(answer: Answer): { core: CoreClient; seen: Array<Record<string, unknown>> } {
  const seen: Array<Record<string, unknown>> = [];
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      const r = req as unknown as Record<string, unknown>;
      seen.push(r);
      return answer(r);
    }),
  } as unknown as CoreClient;
  return { core, seen };
}

async function settle(): Promise<void> {
  TestBed.tick();
  for (let i = 0; i < 6; i++) await Promise.resolve();
  await new Promise((r) => setTimeout(r, 0));
}

const VIV: InventoryContainer = { scope: 'character', id: 'c-viv' };
const QUEST: InventoryContainer = { scope: 'project', id: 'p-1' };

describe('createInventoryListData (v4 useInventoryListData)', () => {
  function mount(core: CoreClient, container: InventoryContainer | null) {
    const c = signal<InventoryContainer | null>(container);
    const archived = signal(false);
    const projectId = signal<string | null>(null);
    const data = TestBed.runInInjectionContext(() =>
      createInventoryListData(core, { container: c, includeArchived: archived, projectId }),
    );
    return { data, c, archived, projectId };
  }

  it('a character reads its whole pool (shared); archived and the project tier are new requests', async () => {
    const { core, seen } = fakeCore(async () => ({ items: [SWORD, BOW] }));
    const { data, archived, projectId } = mount(core, VIV);
    await settle();
    expect(data.listItems()).toEqual([SWORD, BOW]);
    archived.set(true);
    await settle();
    projectId.set('p-1');
    await settle();
    expect(seen).toEqual([
      { type: 'inventoryList', container: VIV, shared: true },
      { type: 'inventoryList', container: VIV, includeArchived: true, shared: true },
      { type: 'inventoryList', container: VIV, includeArchived: true, shared: true, projectId: 'p-1' },
    ]);
  });

  it('canManage: own (or origin-less) rows in a character view; everything in a shared tier', async () => {
    const { core } = fakeCore(async () => ({ items: [] }));
    const { data, c } = mount(core, VIV);
    await settle();
    expect(data.canManage(SWORD)).toBe(true);
    expect(data.canManage(obj('loose', { origin: undefined }))).toBe(true);
    expect(data.canManage(BOW)).toBe(false);
    expect(data.canManage(obj('other', { origin: { scope: 'character', id: 'c-bert', name: 'Bertie' } }))).toBe(false);
    c.set(QUEST);
    expect(data.canManage(SWORD)).toBe(true);
    c.set(null);
    expect(data.canManage(BOW)).toBe(false);
  });

  it('loading only for the first read of a view; a stale answer is dropped', async () => {
    const pending: Array<(v: Record<string, unknown>) => void> = [];
    const { core } = fakeCore(() => new Promise((resolve) => pending.push(resolve)));
    const { data, c } = mount(core, VIV);
    TestBed.tick();
    expect(data.loading()).toBe(true);
    c.set(QUEST);
    TestBed.tick();
    pending[1]({ items: [LAMP] });
    await settle();
    pending[0]({ items: [SWORD] });
    await settle();
    expect(data.listItems()).toEqual([LAMP]);
    expect(data.loading()).toBe(false);
    void data.reload();
    expect(data.loading()).toBe(false);
  });

  it('a failed read keeps the error and an empty list', async () => {
    const { core } = fakeCore(async () => {
      throw new Error('vault unavailable');
    });
    const { data } = mount(core, VIV);
    await settle();
    expect(data.listItems()).toEqual([]);
    expect((data.error() as Error).message).toBe('vault unavailable');
  });
});

describe('createInventoryFoldersStore (v4 useInventoryFolders)', () => {
  it('reads a tier’s folders; a new container starts empty; reload re-reads the last one', async () => {
    const { core, seen } = fakeCore(async (req) => ({
      folders: (req['container'] as InventoryContainer).scope === 'character' ? [{ path: 'Arms', name: 'Arms', itemCount: 1 }] : [],
    }));
    const store = createInventoryFoldersStore(core);
    await store.load(VIV);
    expect(store.folders()).toEqual([{ path: 'Arms', name: 'Arms', itemCount: 1 }]);
    const next = store.load(QUEST);
    expect(store.folders()).toEqual([]);
    expect(store.loading()).toBe(true);
    await next;
    await store.reload();
    expect(seen.map((r) => r['container'])).toEqual([VIV, QUEST, QUEST]);
    await store.load(null);
    expect(store.folders()).toEqual([]);
  });

  it('a failed read leaves the tree empty', async () => {
    const { core } = fakeCore(async () => {
      throw new Error('nope');
    });
    const store = createInventoryFoldersStore(core);
    await store.load(VIV);
    expect(store.folders()).toEqual([]);
    expect(store.loading()).toBe(false);
  });
});

describe('createChatInventoryData (v4 useChatInventory)', () => {
  function mount(core: CoreClient, chatId: string | null) {
    const id = signal<string | null>(chatId);
    const reloadList = vi.fn(async () => undefined);
    const data = TestBed.runInInjectionContext(() => createChatInventoryData(core, { chatId: id, reloadList }));
    return { data, id, reloadList };
  }

  it('reads the chat’s carried objects and project; carriedBy answers none for a stranger', async () => {
    const { core } = fakeCore(async () => ({
      inventory: { 'c-viv': { equipped: [RING], kits: { kit: 'some' } } },
      projectId: 'p-1',
    }));
    const { data } = mount(core, 'chat-1');
    await settle();
    expect(data.projectId()).toBe('p-1');
    expect(data.carriedBy('c-viv')).toEqual({ equipped: [RING], kits: { kit: 'some' } });
    expect(data.carriedBy('c-bert')).toEqual({ equipped: [], kits: {} });
    expect(data.carriedBy(null)).toEqual({ equipped: [], kits: {} });
  });

  it('every gesture reloads the chat read AND the list, even when refused (v4 onSettled, trap 12)', async () => {
    let refuse = false;
    const { core, seen } = fakeCore(async (req) => {
      if (refuse && req['type'] !== 'chatInventoryGet') throw new Error('Not a participant');
      return req['type'] === 'chatInventoryGet' ? { inventory: {}, projectId: null } : { equipped: [], created: [], skipped: [], alreadyEquipped: [] };
    });
    const { data, reloadList } = mount(core, 'chat-1');
    await settle();
    await data.equip('c-viv', ['ring']);
    await data.unequip('c-viv', ['ring']);
    await data.resetToDefaults('c-viv');
    refuse = true;
    await expect(data.equip('c-viv', ['x'])).rejects.toThrow('Not a participant');
    expect(seen.map((r) => r['type'])).toEqual([
      'chatInventoryGet',
      'chatInventoryEquip',
      'chatInventoryGet',
      'chatInventoryUnequip',
      'chatInventoryGet',
      'chatInventoryResetDefaults',
      'chatInventoryGet',
      'chatInventoryEquip',
      'chatInventoryGet',
    ]);
    expect(reloadList).toHaveBeenCalledTimes(4);
  });

  it('no chat reads nothing', async () => {
    const { core, seen } = fakeCore(async () => ({}));
    const { data } = mount(core, null);
    await settle();
    expect(seen).toEqual([]);
    expect(data.inventory()).toEqual({});
  });
});

describe('InventoryDialogService (v4 InventoryDialogProvider)', () => {
  it('open / close, and the four-part remount key', () => {
    const dialog = new InventoryDialogService();
    expect(dialog.isOpen()).toBe(false);
    expect(dialog.remountKey()).toBe('|||');
    dialog.open({ characterId: 'c-viv', chatId: 'chat-1' });
    expect(dialog.isOpen()).toBe(true);
    expect(dialog.remountKey()).toBe('c-viv|chat-1||');
    dialog.open({ projectId: PROJECT.id!, groupId: 'g-1' });
    expect(dialog.remountKey()).toBe('||p-1|g-1');
    dialog.close();
    expect(dialog.isOpen()).toBe(false);
    // Close keeps the last context, as v4's close only flips isOpen.
    expect(dialog.context()).toEqual({ projectId: 'p-1', groupId: 'g-1' });
    dialog.open();
    expect(dialog.context()).toBeNull();
  });
});
