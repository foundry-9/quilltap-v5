import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import type { CoreRequest, WardrobeItemDto } from '../core/core-contract';
import { GENERAL_CONTAINER, type WardrobeContainer } from './wardrobe-container';
import { createWardrobeItemActions, type WardrobeItemActionsOptions } from './wardrobe-item-actions';

/**
 * P4.D285 — v4 `components/wardrobe/hooks/useWardrobeItemActions.ts` at
 * `4a38222e6` + `97f0d8cb8` (bug 229's picker state). v4 has no unit test of
 * the hook itself (the browser and dialog tests drive it); these rows pin each
 * handler's address, write, toast and refresh, and the overlay truth table the
 * dialog's Escape gate reads (R-O).
 */
afterEach(() => vi.restoreAllMocks());

const ALICE: WardrobeContainer = { scope: 'character', id: 'alice' };
const PROJECT: WardrobeContainer = { scope: 'project', id: 'p1' };

const item = (id: string, extra: Partial<WardrobeItemDto> = {}): WardrobeItemDto => ({
  id,
  title: id,
  types: ['top'],
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
  ...extra,
});

function harness(over: Partial<WardrobeItemActionsOptions> & { fail?: string | Error } = {}) {
  const seen: Array<Record<string, unknown>> = [];
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      seen.push(req as unknown as Record<string, unknown>);
      if (over.fail !== undefined) throw over.fail instanceof Error ? over.fail : new Error(over.fail);
      return {};
    }),
  } as unknown as CoreClient;
  const toasts = { showError: vi.fn(), showSuccess: vi.fn() };
  const reloadLists = vi.fn(async () => undefined);
  const reloadOutfit = vi.fn(async () => undefined);
  const confirm = vi.fn(() => true);
  const actions = createWardrobeItemActions({
    core,
    toasts,
    container: () => ALICE,
    listItems: () => [item('Greatcoat'), item('Greatcoat (copy)')],
    confirm,
    reloadLists,
    reloadOutfit,
    ...over,
  });
  return { actions, seen, toasts, reloadLists, reloadOutfit, confirm };
}

describe('the overlay truth table (v4 `97f0d8cb8` `overlayOpen` — bug 229)', () => {
  it('each overlay, the folder picker included, opens the gate; closing it shuts the gate', () => {
    const { actions } = harness();
    expect(actions.overlayOpen()).toBe(false);
    const steps: Array<[() => void, () => void]> = [
      [() => actions.openEditor(item('a')), () => actions.closeEditor()],
      [() => actions.openCreate('single'), () => actions.closeEditor()],
      [() => actions.move(item('a')), () => actions.closeTransfer()],
      [() => actions.copy(item('a')), () => actions.closeTransfer()],
      [() => actions.openImage(item('a', { characterId: 'alice', imageFileId: 'f1' })), () => actions.closeImage()],
      [() => actions.openFolderPicker([item('a')]), () => actions.closeFolderPicker()],
    ];
    for (const [open, close] of steps) {
      open();
      expect(actions.overlayOpen()).toBe(true);
      close();
      expect(actions.overlayOpen()).toBe(false);
    }
  });

  it('openFolderPicker is a no-op on an empty selection', () => {
    const { actions } = harness();
    actions.openFolderPicker([]);
    expect(actions.folderPicker()).toBeNull();
    actions.openFolderPicker([item('a'), item('b')]);
    expect(actions.folderPicker()?.items.map((i) => i.id)).toEqual(['a', 'b']);
  });

  it('openFolderPicker defaults to a move and carries a copy (v4 `bbbcd2388`)', () => {
    const { actions } = harness();
    actions.openFolderPicker([item('a')]);
    expect(actions.folderPicker()?.mode).toBe('move');
    actions.openFolderPicker([item('a')], 'copy');
    expect(actions.folderPicker()?.mode).toBe('copy');
    actions.openFolderPicker([], 'copy');
    expect(actions.folderPicker()?.mode).toBe('copy');
  });
});

describe('addressOf (v4 `:134-138` — origin first in a character view)', () => {
  it('a character view addresses each item through containerForListedItem', () => {
    const { actions } = harness();
    expect(actions.addressOf(item('own', { characterId: 'alice' }))).toEqual(ALICE);
    expect(
      actions.addressOf(item('cape', { origin: { scope: 'project', id: 'p1', name: 'Blackwood' } })),
    ).toEqual(PROJECT);
    expect(actions.addressOf(item('watch'))).toEqual(GENERAL_CONTAINER);
  });

  it('a shared view addresses every item through the browsed container', () => {
    const { actions } = harness({ container: () => PROJECT });
    expect(actions.addressOf(item('x', { origin: { scope: 'general', id: null, name: 'G' } }))).toEqual(
      PROJECT,
    );
  });
});

describe('the row writes', () => {
  it('toggleDefault PUTs { isDefault: !isDefault } at the home and refreshes lists, folders and outfit', async () => {
    const { actions, seen, reloadLists, reloadOutfit } = harness();
    const tick = actions.refreshTick();
    await actions.toggleDefault(item('a', { characterId: 'alice', isDefault: false }));
    expect(seen).toEqual([
      { type: 'characterWardrobeUpdate', characterId: 'alice', itemId: 'a', item: { isDefault: true } },
    ]);
    expect(reloadLists).toHaveBeenCalledTimes(1);
    expect(reloadOutfit).toHaveBeenCalledTimes(1);
    expect(actions.refreshTick()).toBe(tick + 1);
    expect(actions.updatingDefaultId()).toBeNull();
  });

  it('toggleArchived sends { archived: !archivedAt } through a borrowed item’s own wardrobe', async () => {
    const { actions, seen } = harness();
    await actions.toggleArchived(
      item('cape', { archivedAt: '2026-01-01', origin: { scope: 'project', id: 'p1', name: 'B' } }),
    );
    expect(seen).toEqual([
      { type: 'projectWardrobeUpdate', projectId: 'p1', itemId: 'cape', item: { archived: false } },
    ]);
  });

  it('remove asks first with v4’s sentence; a refusal writes nothing, a yes deletes and toasts', async () => {
    const declined = harness({ confirm: vi.fn(() => false) });
    await declined.actions.remove(item('Greatcoat', { characterId: 'alice' }));
    expect(declined.seen).toEqual([]);

    const { actions, seen, confirm, toasts } = harness();
    await actions.remove(item('Greatcoat', { characterId: 'alice' }));
    expect(confirm).toHaveBeenCalledWith('Delete "Greatcoat"? This cannot be undone.');
    expect(seen).toEqual([{ type: 'characterWardrobeDelete', characterId: 'alice', itemId: 'Greatcoat' }]);
    expect(toasts.showSuccess).toHaveBeenCalledWith('Deleted "Greatcoat"');
  });

  it('a failed write toasts the server’s message, or v4’s fallback when it has none, and does not refresh', async () => {
    const withMessage = harness({ fail: 'Wardrobe item not found' });
    await withMessage.actions.toggleArchived(item('a', { characterId: 'alice' }));
    expect(withMessage.toasts.showError).toHaveBeenCalledWith('Wardrobe item not found');
    expect(withMessage.reloadLists).not.toHaveBeenCalled();

    const bare = harness({ fail: new Error('') });
    await bare.actions.remove(item('a', { characterId: 'alice' }));
    expect(bare.toasts.showError).toHaveBeenCalledWith('Failed to delete item');
    expect(bare.toasts.showSuccess).not.toHaveBeenCalled();
  });

  it('duplicate picks a title not in the list, posts at the home and toasts — and logs nothing (v4 `01f508d4e`)', async () => {
    const debug = vi.spyOn(console, 'debug').mockImplementation(() => undefined);
    const { actions, seen, toasts } = harness();
    await actions.duplicate(item('Greatcoat', { characterId: 'alice', componentItemIds: [] }));
    expect(seen).toHaveLength(1);
    expect(seen[0]['type']).toBe('characterWardrobeCreate');
    expect((seen[0]['item'] as Record<string, unknown>)['title']).toBe('Greatcoat (copy 2)');
    // v4's `01f508d4e` pruned the `[useWardrobeItemActions] Duplicating
    // wardrobe item` debug line; the duplicate is silent now.
    expect(debug).not.toHaveBeenCalled();
    expect(toasts.showSuccess).toHaveBeenCalledWith('Duplicated "Greatcoat"');
  });
});

describe('pictures', () => {
  it('generateImage posts to the home’s images route, toasts the rerouted desk, refreshes', async () => {
    const fetchMock = vi.fn(async () => ({
      ok: true,
      status: 201,
      json: async () => ({ rerouted: true }),
    }));
    vi.stubGlobal('fetch', fetchMock);
    const { actions, toasts, reloadLists } = harness();
    const done = actions.generateImage(item('Coat', { characterId: 'alice' }));
    expect(actions.generatingImageIds().has('Coat')).toBe(true);
    await done;
    expect(String((fetchMock.mock.calls[0] as unknown[])[0])).toContain(
      '/api/v1/wardrobe/Coat/images?scope=character&id=alice&action=generate',
    );
    expect(toasts.showSuccess).toHaveBeenCalledWith(
      'A portrait of "Coat" is hung — drawn at the uncensored desk',
    );
    expect(reloadLists).toHaveBeenCalled();
    expect(actions.generatingImageIds().size).toBe(0);
    vi.unstubAllGlobals();
  });

  it('openImage needs a picture, and opens it addressed through the item’s home', () => {
    const { actions } = harness();
    actions.openImage(item('bare'));
    expect(actions.imageView()).toBeNull();
    const cape = item('cape', { imageFileId: 'f1', origin: { scope: 'group', id: 'g1', name: 'S' } });
    actions.openImage(cape);
    expect(actions.imageView()).toEqual({ item: cape, container: { scope: 'group', id: 'g1' } });
  });
});

describe('the editor request + the two refreshes', () => {
  it('openCreate carries the browsed folder; openEditor the item', () => {
    const { actions } = harness();
    actions.openCreate('single', [], { folder: 'Formal/Evening' });
    expect(actions.editor()).toEqual({
      kind: 'create',
      mode: 'single',
      componentItemIds: [],
      folder: 'Formal/Evening',
    });
    actions.openCreate('bundle', ['a', 'b']);
    expect(actions.editor()).toEqual({
      kind: 'create',
      mode: 'bundle',
      componentItemIds: ['a', 'b'],
      folder: undefined,
    });
  });

  it('refreshLists re-reads the lists and bumps the folder tick but leaves the outfit alone', async () => {
    const { actions, reloadLists, reloadOutfit } = harness();
    await actions.refreshLists();
    expect(reloadLists).toHaveBeenCalledTimes(1);
    expect(reloadOutfit).not.toHaveBeenCalled();
    expect(actions.refreshTick()).toBe(1);
    await actions.refresh();
    expect(reloadOutfit).toHaveBeenCalledTimes(1);
    expect(actions.refreshTick()).toBe(2);
  });
});
