import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import { CoreDispatchError, type CoreRequest } from '../core/core-contract';
import {
  createWardrobeFolder,
  createWardrobeFoldersStore,
  deleteWardrobeFolder,
  listWardrobeFolders,
  moveWardrobeItemsToFolder,
  renameWardrobeFolder,
} from './wardrobe-folders.api';
import { GENERAL_CONTAINER, type WardrobeContainer } from './wardrobe-container';

/**
 * P4.D285 — v4 `4a38222e6` `lib/hooks/use-wardrobe-folders.ts`: the five
 * folder calls' bodies (C2's scoped verbs, R-D — `scope` + `containerId`,
 * `containerId` absent for General; C3 §0's body keys) and v4's result reads
 * (`data.folder`, `Number(data.movedItems ?? 0)`, `data.items ?? []`,
 * `data.folders ?? []`), plus the signal store standing in for the TanStack
 * query (R-J).
 */
afterEach(() => vi.restoreAllMocks());

function fakeCore(answer: Record<string, unknown> | (() => Promise<Record<string, unknown>>) = {}): {
  core: CoreClient;
  seen: Array<Record<string, unknown>>;
} {
  const seen: Array<Record<string, unknown>> = [];
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      seen.push(req as unknown as Record<string, unknown>);
      return typeof answer === 'function' ? answer() : answer;
    }),
  } as unknown as CoreClient;
  return { core, seen };
}

const CHARACTER: WardrobeContainer = { scope: 'character', id: 'c1' };
const PROJECT: WardrobeContainer = { scope: 'project', id: 'p1' };
const GROUP: WardrobeContainer = { scope: 'group', id: 'g1' };

describe('the request bodies (C2 + C3 §0)', () => {
  it('folders: one scoped verb per tier, containerId absent for General', async () => {
    const { core, seen } = fakeCore({ folders: [] });
    for (const c of [CHARACTER, PROJECT, GROUP, GENERAL_CONTAINER]) {
      await listWardrobeFolders(core, c);
    }
    expect(seen).toEqual([
      { type: 'wardrobeFoldersList', scope: 'character', containerId: 'c1' },
      { type: 'wardrobeFoldersList', scope: 'project', containerId: 'p1' },
      { type: 'wardrobeFoldersList', scope: 'group', containerId: 'g1' },
      { type: 'wardrobeFoldersList', scope: 'general' },
    ]);
  });

  it('create / rename / delete / move carry v4’s body keys raw', async () => {
    const { core, seen } = fakeCore({});
    await createWardrobeFolder(core, CHARACTER, 'Formal/Evening');
    await renameWardrobeFolder(core, PROJECT, 'Formal', 'Dress');
    await deleteWardrobeFolder(core, GROUP, 'Formal');
    await moveWardrobeItemsToFolder(core, GENERAL_CONTAINER, ['a', 'b'], '');
    expect(seen).toEqual([
      { type: 'wardrobeFolderCreate', scope: 'character', containerId: 'c1', path: 'Formal/Evening' },
      { type: 'wardrobeFolderRename', scope: 'project', containerId: 'p1', path: 'Formal', name: 'Dress' },
      { type: 'wardrobeFolderDelete', scope: 'group', containerId: 'g1', path: 'Formal' },
      { type: 'wardrobeItemsMove', scope: 'general', itemIds: ['a', 'b'], folder: '' },
    ]);
  });
});

describe('the result reads (v4 `use-wardrobe-folders.ts:77-140`)', () => {
  it('folders ?? [], folder, items ?? []', async () => {
    const folder = { path: 'Formal', name: 'Formal', itemCount: 1 };
    expect(await listWardrobeFolders(fakeCore({ folders: [folder] }).core, CHARACTER)).toEqual([
      folder,
    ]);
    expect(await listWardrobeFolders(fakeCore({}).core, CHARACTER)).toEqual([]);
    expect(await createWardrobeFolder(fakeCore({ folder }).core, CHARACTER, 'Formal')).toEqual(
      folder,
    );
    expect(await renameWardrobeFolder(fakeCore({ folder }).core, CHARACTER, 'F', 'Formal')).toEqual(
      folder,
    );
    expect(await moveWardrobeItemsToFolder(fakeCore({}).core, CHARACTER, ['a'], 'X')).toEqual([]);
  });

  it('delete reads both counts through Number(… ?? 0)', async () => {
    expect(
      await deleteWardrobeFolder(fakeCore({ movedItems: 3, movedFolders: 1 }).core, CHARACTER, 'F'),
    ).toEqual({ movedItems: 3, movedFolders: 1 });
    expect(await deleteWardrobeFolder(fakeCore({}).core, CHARACTER, 'F')).toEqual({
      movedItems: 0,
      movedFolders: 0,
    });
  });

  it('a refusal surfaces as the dispatch error, its message the server’s sentence', async () => {
    const core = {
      dispatchData: vi.fn(async () => {
        throw new CoreDispatchError({
          kind: 'bad-request',
          message: '"images" is reserved for item pictures and cannot be a wardrobe folder',
        });
      }),
    } as unknown as CoreClient;
    await expect(createWardrobeFolder(core, CHARACTER, 'images')).rejects.toThrow(
      '"images" is reserved for item pictures and cannot be a wardrobe folder',
    );
  });
});

describe('createWardrobeFoldersStore (v4 `useWardrobeFolders`, as signals — R-J)', () => {
  it('starts empty, loads the tree, and a null container clears it without a read', async () => {
    const folder = { path: 'Formal', name: 'Formal', itemCount: 2 };
    const { core, seen } = fakeCore({ folders: [folder] });
    const store = createWardrobeFoldersStore(core);
    expect(store.folders()).toEqual([]);
    await store.load(CHARACTER);
    expect(store.folders()).toEqual([folder]);
    expect(store.loading()).toBe(false);
    await store.load(null);
    expect(store.folders()).toEqual([]);
    expect(seen).toHaveLength(1);
  });

  it('refresh re-reads the container last asked about', async () => {
    const { core, seen } = fakeCore({ folders: [] });
    const store = createWardrobeFoldersStore(core);
    await store.load(PROJECT);
    await store.refresh();
    expect(seen.map((r) => r['containerId'])).toEqual(['p1', 'p1']);
  });

  it('a failed read warns with v4’s line and leaves the tree empty', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const store = createWardrobeFoldersStore(
      fakeCore(async () => {
        throw new Error('boom');
      }).core,
    );
    await store.load(CHARACTER);
    expect(store.folders()).toEqual([]);
    expect(warn).toHaveBeenCalledWith('[useWardrobeFolders] Failed to load wardrobe folders', {
      tierKey: 'character:c1',
      error: expect.any(Error),
    });
  });

  it('a late answer for a container no longer asked about is dropped', async () => {
    let release!: (v: Record<string, unknown>) => void;
    const slow = new Promise<Record<string, unknown>>((r) => (release = r));
    let calls = 0;
    const store = createWardrobeFoldersStore(
      fakeCore(() => (++calls === 1 ? slow : Promise.resolve({ folders: [] }))).core,
    );
    const first = store.load(CHARACTER);
    await store.load(PROJECT);
    release({ folders: [{ path: 'Stale', name: 'Stale', itemCount: 0 }] });
    await first;
    expect(store.folders()).toEqual([]);
  });
});
