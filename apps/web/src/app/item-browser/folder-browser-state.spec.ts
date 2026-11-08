import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ToastService } from '../ui/toast.service';
import {
  createFolderBrowserState,
  createItemFolderHandlers,
  pluralize,
  type FolderBrowserState,
  type ItemFolderOps,
  type ItemFolderWording,
} from './folder-browser-state';
import { OWN_TIER, type BrowserOrigin } from './folder-view';

/**
 * v4 `components/item-browser/useFolderBrowserState.ts` at the pin `624d0d25c`
 * (`b2b783b47`) — the state every folder-of-items browser keeps, and
 * `useItemFolderHandlers`' New folder / Rename / Delete.
 *
 * v4 ships no jest file for the hook itself (`wardrobe-browser.folders.test.tsx`
 * drives it through the DOM; transcribed with the wardrobe browser in commit
 * 2). These rows pin the KEY-SCOPED invariant directly: "Each piece of view
 * state remembers the container (or view) it was set in and reads as its
 * starting value anywhere else … without an effect to reset them."
 */

interface Item {
  id: string;
  folder?: string | null;
  archivedAt?: string | null;
  origin?: BrowserOrigin | null;
}

const PROJECT: BrowserOrigin = { scope: 'project', id: 'p1', name: 'Blackwood' };
const GROUP: BrowserOrigin = { scope: 'group', id: 'g1', name: 'Sisters' };

const tierOf = (i: Item): string => (i.origin ? `${i.origin.scope}:${i.origin.id ?? ''}` : OWN_TIER);

function setup(items: Item[] = []) {
  const containerKey = signal('character:c1');
  const pool = signal<Item[]>(items);
  const filtered = signal<Item[]>(items);
  const tree = signal<ReadonlyArray<{ path: string; itemCount: number }>>([]);
  const characterView = signal(true);
  const showShared = signal(true);
  const searching = signal(false);
  const state = createFolderBrowserState<Item>({
    containerKey,
    items: pool,
    filtered,
    tree,
    tierOf: () => tierOf,
    characterView,
    showShared,
    searching,
  });
  return { state, containerKey, pool, filtered, tree, characterView, showShared, searching };
}

const A: Item = { id: 'a', folder: 'Formal' };
const B: Item = { id: 'b', folder: 'Formal/Evening' };
const C: Item = { id: 'c', folder: '' };
const P: Item = { id: 'p', folder: 'Hats', origin: PROJECT };

describe('createFolderBrowserState — the key-scoped view (v4 :101-118)', () => {
  it('opens at the own root with nothing selected and select mode off', () => {
    const { state } = setup([A, C]);
    expect(state.currentTier()).toBe(OWN_TIER);
    expect(state.currentFolder()).toBe('');
    expect(state.inOwnTier()).toBe(true);
    expect(state.selectMode()).toBe(false);
    expect(state.selectedIds().size).toBe(0);
  });

  it('a new container opens at ITS root with nothing selected; the old one is remembered by key', () => {
    const t = setup([A, B, C]);
    t.state.navigate(OWN_TIER, 'Formal');
    t.state.setSelectMode(true);
    t.state.toggleSelected(A);
    expect(t.state.selectedIds().has('a')).toBe(true);
    t.containerKey.set('character:c2');
    expect(t.state.currentFolder()).toBe('');
    expect(t.state.selectMode()).toBe(false);
    expect(t.state.selectedIds().size).toBe(0);
    // Nothing was SET in c2, so c1's view still reads as c1's own (v4's
    // `view.key === containerKey`).
    t.containerKey.set('character:c1');
    expect(t.state.currentFolder()).toBe('Formal');
    expect(t.state.selectMode()).toBe(true);
    expect(t.state.selectedIds().has('a')).toBe(true);
  });

  it('leaving a folder, or toggling search, drops the selection', () => {
    const t = setup([A, B, C]);
    t.state.navigate(OWN_TIER, 'Formal');
    t.state.setSelectMode(true);
    t.state.toggleSelected(A);
    t.state.navigate(OWN_TIER, '');
    expect(t.state.selectedIds().size).toBe(0);
    t.state.navigate(OWN_TIER, 'Formal');
    t.state.toggleSelected(A);
    t.searching.set(true);
    expect(t.state.selectedIds().size).toBe(0);
    expect(t.state.selectedItems()).toEqual([]);
  });

  it('toggleSelected adds then removes; selectedItems keeps the POOL’s order; clearSelection empties', () => {
    const t = setup([A, B, C]);
    t.state.toggleSelected(C);
    t.state.toggleSelected(A);
    expect(t.state.selectedItems().map((i) => i.id)).toEqual(['a', 'c']);
    t.state.toggleSelected(C);
    expect([...t.state.selectedIds()]).toEqual(['a']);
    t.state.clearSelection();
    expect(t.state.selectedIds().size).toBe(0);
  });

  it('setSelectMode empties the selection either way', () => {
    const t = setup([A]);
    t.state.setSelectMode(true);
    t.state.toggleSelected(A);
    t.state.setSelectMode(true);
    expect(t.state.selectedIds().size).toBe(0);
    t.state.toggleSelected(A);
    t.state.setSelectMode(false);
    expect(t.state.selectMode()).toBe(false);
    expect(t.state.selectedIds().size).toBe(0);
  });

  it('Show shared off inside a borrowed tier reads as the own root; on again, the tier returns', () => {
    const t = setup([A, P]);
    t.state.navigate('project:p1', '');
    expect(t.state.inOwnTier()).toBe(false);
    expect(t.state.managed()).toBe(false);
    t.showShared.set(false);
    expect(t.state.currentTier()).toBe(OWN_TIER);
    expect(t.state.currentFolder()).toBe('');
    t.showShared.set(true);
    expect(t.state.currentTier()).toBe('project:p1');
  });

  it('the new-folder draft is per view: it opens with setName and is gone in another folder', () => {
    const t = setup([A, B]);
    expect(t.state.newFolder.open()).toBe(false);
    t.state.newFolder.setName('');
    expect(t.state.newFolder.open()).toBe(true);
    t.state.newFolder.setName('Hats');
    expect(t.state.newFolder.name()).toBe('Hats');
    t.state.navigate(OWN_TIER, 'Formal');
    expect(t.state.newFolder.open()).toBe(false);
    expect(t.state.newFolder.name()).toBe('');
    t.state.navigate(OWN_TIER, '');
    expect(t.state.newFolder.open()).toBe(true);
    t.state.newFolder.close();
    expect(t.state.newFolder.open()).toBe(false);
  });

  it('managed = in the own tier with a container chosen', () => {
    const t = setup([A]);
    expect(t.state.managed()).toBe(true);
    t.containerKey.set('');
    expect(t.state.managed()).toBe(false);
  });
});

describe('createFolderBrowserState — what the view shows (v4 :153-189)', () => {
  it('folder rows: the direct children, the tree only when managed, none while searching', () => {
    const t = setup([A, B, C]);
    t.tree.set([{ path: 'Rainy', itemCount: 0 }]);
    expect(t.state.folderRows().map((r) => r.path)).toEqual(['Formal', 'Rainy']);
    t.state.navigate(OWN_TIER, 'Formal');
    expect(t.state.folderRows().map((r) => r.path)).toEqual(['Formal/Evening']);
    t.searching.set(true);
    expect(t.state.folderRows()).toEqual([]);
  });

  it('shared tier rows only in a character view, own tier, at the root, not searching, Show shared on', () => {
    const t = setup([A, P, { id: 'g', origin: GROUP }]);
    expect(t.state.sharedTierRows().map((r) => r.key)).toEqual(['project:p1', 'group:g1']);
    t.showShared.set(false);
    expect(t.state.sharedTierRows()).toEqual([]);
    t.showShared.set(true);
    t.state.navigate(OWN_TIER, 'Formal');
    expect(t.state.sharedTierRows()).toEqual([]);
    t.state.navigate(OWN_TIER, '');
    t.searching.set(true);
    expect(t.state.sharedTierRows()).toEqual([]);
    t.searching.set(false);
    t.characterView.set(false);
    expect(t.state.sharedTierRows()).toEqual([]);
  });

  it('a borrowed tier with no item left labels itself Shared', () => {
    const t = setup([A, P]);
    t.state.navigate('project:p1', '');
    expect(t.state.currentTierLabel()).toBe('Project: Blackwood');
    t.pool.set([A]);
    expect(t.state.currentTierLabel()).toBe('Shared');
    t.state.navigate(OWN_TIER, '');
    expect(t.state.currentTierLabel()).toBeNull();
  });

  it('visible: the filtered items in the tier AND folder; everything filtered while searching', () => {
    const t = setup([A, B, C, P]);
    expect(t.state.visible().map((i) => i.id)).toEqual(['c']);
    t.state.navigate(OWN_TIER, 'formal');
    expect(t.state.visible().map((i) => i.id)).toEqual(['a']);
    t.searching.set(true);
    expect(t.state.visible().map((i) => i.id)).toEqual(['a', 'b', 'c', 'p']);
    t.searching.set(false);
    t.state.navigate('project:p1', 'Hats');
    expect(t.state.visible().map((i) => i.id)).toEqual(['p']);
    expect(t.state.tierItems().map((i) => i.id)).toEqual(['p']);
  });

  it('folderIsEmpty: nothing here at all; a filter that hides everything is NOT empty (it is no-match)', () => {
    const t = setup([A, C]);
    t.characterView.set(false);
    t.state.navigate(OWN_TIER, 'Formal');
    expect(t.state.folderIsEmpty()).toBe(false);
    t.filtered.set([]);
    expect(t.state.folderIsEmpty()).toBe(false);
    expect(t.state.visible()).toEqual([]);
    t.state.navigate(OWN_TIER, 'Nowhere');
    expect(t.state.folderIsEmpty()).toBe(true);
    t.searching.set(true);
    expect(t.state.folderIsEmpty()).toBe(false);
  });
});

describe('pluralize (v4 :226)', () => {
  it('counts its word', () => {
    expect(pluralize(0, 'item')).toBe('0 items');
    expect(pluralize(1, 'item')).toBe('1 item');
    expect(pluralize(2, 'object')).toBe('2 objects');
  });
});

describe('createItemFolderHandlers (v4 :228-297)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const WORDING: ItemFolderWording = {
    noun: 'item',
    rootPhrase: 'the top of the wardrobe',
    created: (name) => `Folder "${name}" made`,
    movedUp: (count) => `${count} moved up`,
  };

  function handlers(
    state: FolderBrowserState<Item>,
    ops: ItemFolderOps | null,
    opts: { confirm?: boolean; wording?: ItemFolderWording } = {},
  ) {
    const toasts = { showSuccess: vi.fn(), showError: vi.fn() };
    TestBed.configureTestingModule({ providers: [{ provide: ToastService, useValue: toasts }] });
    const confirm = vi.fn(async (_m: string) => opts.confirm ?? true);
    const h = TestBed.runInInjectionContext(() =>
      createItemFolderHandlers<Item>({
        state,
        ops: () => ops,
        wording: opts.wording ?? WORDING,
        confirm,
        describeError: (error, fallback) => (error instanceof Error && error.message ? error.message : fallback),
      }),
    );
    return { h, toasts, confirm };
  }

  function ops(): { [K in keyof ItemFolderOps]: ReturnType<typeof vi.fn> } & ItemFolderOps {
    return {
      create: vi.fn(async (path: string) => ({ name: path.split('/').pop()! })),
      rename: vi.fn(async (_p: string, name: string) => ({ name })),
      remove: vi.fn(async () => ({ movedItems: 0 })),
    } as never;
  }

  it('submitNewFolder joins the current folder, toasts `created`, closes the draft; blank or no ops is a no-op', async () => {
    const t = setup([A]);
    t.state.navigate(OWN_TIER, 'Formal');
    t.state.newFolder.setName('  Gala ');
    const o = ops();
    const { h, toasts } = handlers(t.state, o);
    await h.submitNewFolder();
    expect(o.create).toHaveBeenCalledWith('Formal/Gala');
    expect(toasts.showSuccess).toHaveBeenCalledWith('Folder "Gala" made');
    expect(t.state.newFolder.open()).toBe(false);
    t.state.newFolder.setName('   ');
    await h.submitNewFolder();
    expect(o.create).toHaveBeenCalledTimes(1);
  });

  it('a refused create toasts the refusal, else `The folder would not be made`', async () => {
    const t = setup([A]);
    t.state.newFolder.setName('X');
    const o = ops();
    o.create.mockRejectedValueOnce(new Error('Already there'));
    o.create.mockRejectedValueOnce(new Error(''));
    const { h, toasts } = handlers(t.state, o);
    await h.submitNewFolder();
    await h.submitNewFolder();
    expect(toasts.showError.mock.calls).toEqual([['Already there'], ['The folder would not be made']]);
    expect(t.state.newFolder.open()).toBe(true);
  });

  it('renameFolder toasts `Renamed to "…"` and answers true; a refusal answers false', async () => {
    const t = setup([A]);
    const o = ops();
    const { h, toasts } = handlers(t.state, o);
    expect(await h.renameFolder('Formal', 'Dress')).toBe(true);
    expect(toasts.showSuccess).toHaveBeenCalledWith('Renamed to "Dress"');
    o.rename.mockRejectedValueOnce(new Error(''));
    expect(await h.renameFolder('Formal', 'Dress')).toBe(false);
    expect(toasts.showError).toHaveBeenCalledWith('The folder would not be renamed');
  });

  it('with no ops every handler is a no-op (rename answers false)', async () => {
    const t = setup([A]);
    t.state.newFolder.setName('X');
    const { h, toasts, confirm } = handlers(t.state, null);
    await h.submitNewFolder();
    expect(await h.renameFolder('Formal', 'Y')).toBe(false);
    await h.deleteFolder('Formal');
    expect(confirm).not.toHaveBeenCalled();
    expect(toasts.showSuccess).not.toHaveBeenCalled();
  });

  it('deleteFolder: the default confirmations, the toasts, the refusal fallback', async () => {
    const t = setup([A, B, C]);
    const o = ops();
    const { h, toasts, confirm } = handlers(t.state, o);
    await h.deleteFolder('Formal');
    expect(confirm).toHaveBeenLastCalledWith(
      'Send its 2 items up to the top of the wardrobe? Only the folder itself goes.',
    );
    expect(toasts.showSuccess).toHaveBeenLastCalledWith('Folder removed');
    await h.deleteFolder('Formal/Evening');
    expect(confirm).toHaveBeenLastCalledWith('Send its 1 item up to "Formal"? Only the folder itself goes.');
    await h.deleteFolder('Empty');
    expect(confirm).toHaveBeenLastCalledWith('Remove the folder "Empty"?');
    o.remove.mockResolvedValueOnce({ movedItems: 3 });
    await h.deleteFolder('Formal');
    expect(toasts.showSuccess).toHaveBeenLastCalledWith('3 items moved up');
    o.remove.mockRejectedValueOnce(new Error(''));
    await h.deleteFolder('Formal');
    expect(toasts.showError).toHaveBeenLastCalledWith('The folder would not be removed');
  });

  it('a declined confirmation writes nothing', async () => {
    const t = setup([A]);
    const o = ops();
    const { h } = handlers(t.state, o, { confirm: false });
    await h.deleteFolder('Formal');
    expect(o.remove).not.toHaveBeenCalled();
  });

  it('a caller’s deleteMessage replaces the default, with the name, count and parent label', async () => {
    const t = setup([A, B]);
    const deleteMessage = vi.fn(({ name, count, parentLabel }) => `${name}|${count}|${parentLabel}`);
    const { h, confirm } = handlers(t.state, ops(), { wording: { ...WORDING, deleteMessage } });
    await h.deleteFolder('Formal/Evening');
    expect(confirm).toHaveBeenCalledWith('Evening|1|"Formal"');
  });

  it('deleting the folder that holds the current one navigates up to its parent, in the same tier', async () => {
    const t = setup([A, B]);
    t.state.navigate(OWN_TIER, 'Formal/Evening');
    const { h } = handlers(t.state, ops());
    await h.deleteFolder('Formal');
    expect(t.state.currentFolder()).toBe('');
    t.state.navigate(OWN_TIER, 'Formal/Evening');
    await h.deleteFolder('Hats');
    expect(t.state.currentFolder()).toBe('Formal/Evening');
  });
});
