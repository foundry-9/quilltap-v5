import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import {
  clearInventoryOverrides,
  createInventoryFolder,
  createInventoryItem,
  deleteInventoryFolder,
  deleteInventoryItem,
  deleteInventoryItemImage,
  detachInventoryItem,
  duplicateInventoryItem,
  equipChatInventory,
  fileInventoryItems,
  generateInventoryItemImage,
  getChatInventory,
  instantiateInventoryItem,
  listInventory,
  listInventoryArchetypes,
  listInventoryDependents,
  listInventoryFolders,
  listInventoryImages,
  listInventorySaveTargets,
  renameInventoryFolder,
  resetChatInventoryDefaults,
  saveInventoryImageToStore,
  setCurrentInventoryItemImage,
  transferInventoryItem,
  unequipChatInventory,
  updateInventoryItem,
  uploadInventoryItemImage,
} from './inventory.api';
import type { InventoryContainer } from './inventory-container';

/**
 * Every C7 request this lane builds, pinned whole by `toStrictEqual` (P4.D330
 * R-1): an optional key is ABSENT unless v4's URL / body would carry it.
 */

const VIV: InventoryContainer = { scope: 'character', id: 'c-viv' };
const QUEST: InventoryContainer = { scope: 'project', id: 'p-1' };
const GENERAL: InventoryContainer = { scope: 'general', id: null };

function recorder(answer: Record<string, unknown> = {}): { core: CoreClient; seen: unknown[] } {
  const seen: unknown[] = [];
  const core = {
    dispatchData: async (req: unknown) => {
      seen.push(req);
      return answer;
    },
  } as unknown as CoreClient;
  return { core, seen };
}

describe('inventory.api — the reads', () => {
  it('listInventory: shared only for a character; includeArchived and projectId only when set', async () => {
    const { core, seen } = recorder({ items: [{ id: 'a' }] });
    expect(await listInventory(core, VIV, { includeArchived: false })).toEqual([{ id: 'a' }]);
    await listInventory(core, VIV, { includeArchived: true, shared: true, projectId: 'p-1' });
    await listInventory(core, QUEST, { includeArchived: false, shared: true, projectId: null });
    await listInventory(core, GENERAL, { includeArchived: true });
    expect(seen).toStrictEqual([
      { type: 'inventoryList', container: VIV },
      { type: 'inventoryList', container: VIV, includeArchived: true, shared: true, projectId: 'p-1' },
      { type: 'inventoryList', container: QUEST },
      { type: 'inventoryList', container: GENERAL, includeArchived: true },
    ]);
  });

  it('an absent items / folders / dependents / albums key reads as none', async () => {
    const { core } = recorder({});
    expect(await listInventory(core, VIV, { includeArchived: false })).toEqual([]);
    expect(await listInventoryFolders(core, VIV)).toEqual([]);
    expect(await listInventoryArchetypes(core, VIV)).toEqual([]);
    expect(await listInventoryDependents(core, VIV, 'x')).toEqual([]);
    expect(await listInventorySaveTargets(core, VIV, 'x')).toEqual([]);
    expect(await listInventoryImages(core, VIV, 'x')).toEqual({ current: null, images: [] });
    expect(await getChatInventory(core, 'chat-1')).toEqual({ inventory: {}, projectId: null });
  });

  it('folders, archetypes, dependents, images, save targets, the chat read', async () => {
    const { core, seen } = recorder({});
    await listInventoryFolders(core, GENERAL);
    await listInventoryArchetypes(core, VIV, 'p-1');
    await listInventoryArchetypes(core, VIV, null);
    await listInventoryDependents(core, QUEST, 'bow');
    await listInventoryImages(core, VIV, 'sword');
    await listInventorySaveTargets(core, GENERAL, 'lamp');
    await getChatInventory(core, 'chat-1');
    expect(seen).toStrictEqual([
      { type: 'inventoryFolders', container: GENERAL },
      { type: 'inventoryArchetypes', container: VIV, projectId: 'p-1' },
      { type: 'inventoryArchetypes', container: VIV },
      { type: 'inventoryDependents', container: QUEST, itemId: 'bow' },
      { type: 'inventoryItemImagesList', container: VIV, itemId: 'sword' },
      { type: 'inventoryItemImageSaveTargets', container: GENERAL, itemId: 'lamp' },
      { type: 'chatInventoryGet', chatId: 'chat-1' },
    ]);
  });
});

describe('inventory.api — the writes', () => {
  it('create / update carry projectId only when set; clear-override, detach, delete (force only when set)', async () => {
    const { core, seen } = recorder({ item: { id: 'n' } });
    expect(await createInventoryItem(core, VIV, { name: 'sword' })).toEqual({ id: 'n' });
    await createInventoryItem(core, VIV, { kind: 'kit', name: 'kit', itemIds: ['a'] }, 'p-1');
    await updateInventoryItem(core, VIV, 'sword', { archived: true });
    await updateInventoryItem(core, VIV, 'sword', { folder: '' }, 'p-1');
    await clearInventoryOverrides(core, VIV, 'inst', ['title']);
    await detachInventoryItem(core, VIV, 'inst');
    await deleteInventoryItem(core, QUEST, 'bow');
    await deleteInventoryItem(core, QUEST, 'bow', true);
    expect(seen).toStrictEqual([
      { type: 'inventoryCreate', container: VIV, body: { name: 'sword' } },
      { type: 'inventoryCreate', container: VIV, projectId: 'p-1', body: { kind: 'kit', name: 'kit', itemIds: ['a'] } },
      { type: 'inventoryUpdate', container: VIV, itemId: 'sword', body: { archived: true } },
      { type: 'inventoryUpdate', container: VIV, itemId: 'sword', projectId: 'p-1', body: { folder: '' } },
      { type: 'inventoryClearOverride', container: VIV, itemId: 'inst', fields: ['title'] },
      { type: 'inventoryDetach', container: VIV, itemId: 'inst' },
      { type: 'inventoryDelete', container: QUEST, itemId: 'bow' },
      { type: 'inventoryDelete', container: QUEST, itemId: 'bow', force: true },
    ]);
  });

  it('instantiate carries its body as given; duplicate; the three folder verbs; file items; transfer', async () => {
    const { core, seen } = recorder({ item: { id: 'n' }, movedItems: 2 });
    await instantiateInventoryItem(core, VIV, { archetypeId: 'bow', folder: 'Arms', projectId: null, chatId: 'chat-1' });
    await duplicateInventoryItem(core, VIV, 'sword');
    await createInventoryFolder(core, VIV, 'Arms');
    await renameInventoryFolder(core, VIV, 'Arms', 'Weapons');
    expect(await deleteInventoryFolder(core, VIV, 'Weapons')).toEqual({ movedItems: 2, movedFolders: 0 });
    await fileInventoryItems(core, 'move', VIV, ['a', 'b'], 'Arms');
    await fileInventoryItems(core, 'copy', VIV, ['a'], '');
    await transferInventoryItem(core, { action: 'copy', itemId: 'a', from: VIV, to: GENERAL });
    expect(seen).toStrictEqual([
      { type: 'inventoryInstantiate', container: VIV, archetypeId: 'bow', folder: 'Arms', projectId: null, chatId: 'chat-1' },
      { type: 'inventoryDuplicate', container: VIV, itemId: 'sword' },
      { type: 'inventoryFolderCreate', container: VIV, path: 'Arms' },
      { type: 'inventoryFolderRename', container: VIV, path: 'Arms', name: 'Weapons' },
      { type: 'inventoryFolderDelete', container: VIV, path: 'Weapons' },
      { type: 'inventoryItemsMove', container: VIV, itemIds: ['a', 'b'], folder: 'Arms' },
      { type: 'inventoryItemsCopy', container: VIV, itemIds: ['a'], folder: '' },
      { type: 'inventoryTransfer', action: 'copy', itemId: 'a', from: VIV, to: GENERAL },
    ]);
  });

  it('the picture verbs; the save omits a blank caption', async () => {
    const { core, seen } = recorder({});
    await generateInventoryItemImage(core, VIV, 'sword');
    await setCurrentInventoryItemImage(core, VIV, 'sword', 'f1');
    await deleteInventoryItemImage(core, VIV, 'sword', 'f1');
    await saveInventoryImageToStore(core, VIV, 'sword', { fileId: 'f1', mountPointId: 'mp' });
    await saveInventoryImageToStore(core, VIV, 'sword', { fileId: 'f1', mountPointId: 'mp', caption: '' });
    expect(seen).toStrictEqual([
      { type: 'inventoryItemImageGenerate', container: VIV, itemId: 'sword' },
      { type: 'inventoryItemImageSetCurrent', container: VIV, itemId: 'sword', fileId: 'f1' },
      { type: 'inventoryItemImageDelete', container: VIV, itemId: 'sword', fileId: 'f1' },
      { type: 'inventoryItemImageSaveToStore', container: VIV, itemId: 'sword', fileId: 'f1', mountPointId: 'mp' },
      { type: 'inventoryItemImageSaveToStore', container: VIV, itemId: 'sword', fileId: 'f1', mountPointId: 'mp', caption: '' },
    ]);
  });

  it('the three chat gestures', async () => {
    const { core, seen } = recorder({ equipped: [] });
    await equipChatInventory(core, 'chat-1', 'c-viv', ['a']);
    await unequipChatInventory(core, 'chat-1', 'c-viv', ['a', 'b']);
    await resetChatInventoryDefaults(core, 'chat-1', 'c-viv');
    expect(seen).toStrictEqual([
      { type: 'chatInventoryEquip', chatId: 'chat-1', characterId: 'c-viv', ids: ['a'] },
      { type: 'chatInventoryUnequip', chatId: 'chat-1', characterId: 'c-viv', ids: ['a', 'b'] },
      { type: 'chatInventoryResetDefaults', chatId: 'chat-1', characterId: 'c-viv' },
    ]);
  });
});

describe('inventory.api — the REST upload (v4 inventory-item-editor.tsx:473-479)', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('POSTs the file alone to the images route with the container in the query', async () => {
    const fetchMock = vi.fn(async () => new Response('{}', { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);
    const file = new File(['x'], 'p.png', { type: 'image/png' });
    await uploadInventoryItemImage(VIV, 'sword', file);
    await uploadInventoryItemImage(GENERAL, 'lamp', file);
    const calls = fetchMock.mock.calls as unknown as Array<[string, RequestInit]>;
    expect(calls[0][0]).toBe('/api/v1/inventory/sword/images?scope=character&id=c-viv&action=upload');
    expect(calls[1][0]).toBe('/api/v1/inventory/lamp/images?scope=general&action=upload');
    expect(calls[0][1].method).toBe('POST');
    expect([...(calls[0][1].body as FormData).keys()]).toEqual(['file']);
  });

  it('a refusal throws the server’s error, else "Upload failed"', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ error: 'Too large' }), { status: 413 })));
    const file = new File(['x'], 'p.png');
    await expect(uploadInventoryItemImage(VIV, 'sword', file)).rejects.toThrow('Too large');
    vi.stubGlobal('fetch', vi.fn(async () => new Response('not json', { status: 500 })));
    await expect(uploadInventoryItemImage(VIV, 'sword', file)).rejects.toThrow('Upload failed');
  });
});
