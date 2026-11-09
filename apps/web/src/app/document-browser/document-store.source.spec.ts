import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { DEFAULT_ROUTE, provideDocumentStoreTesting, settle } from './document-browser.testing';
import { mountPointKeys } from './document-store.keys';
import { createDocumentStoreSource } from './document-store.source';

/**
 * v4 `useDocumentStoreSource` (`c34986421`, pin `624d0d25c`): the reads under
 * `mountPointKeys.detail` / `files`, the entries filed under their folder, and
 * each op as its verb + body (the request shapes v4's
 * `document-store-browser.test.tsx:118-160` asserts, over v5's dispatch), each
 * settling this store, the touched destination, and the store lists.
 */

function make(route = DEFAULT_ROUTE) {
  const { seen, queryClient } = provideDocumentStoreTesting(route);
  const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
  const source = TestBed.runInInjectionContext(() => createDocumentStoreSource(() => 'store-a'));
  return { source, seen, invalidate };
}

const keysOf = (spy: { mock: { calls: unknown[][] } }): string[] =>
  spy.mock.calls.map((c: unknown[]) => JSON.stringify((c[0] as { queryKey: unknown }).queryKey));

describe('createDocumentStoreSource (v4 useDocumentStoreSource)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('reads the record and the listing, filing each entry under its folder', async () => {
    const { source, seen } = make();
    await settle();
    expect(seen.map((r) => r.type).sort()).toEqual(['mountFilesList', 'mountPointGet']);
    expect(source.store()?.name).toBe('Archive');
    expect(source.entries().map((e) => [e.relativePath, e.folder])).toEqual([
      ['notes.md', ''],
      ['Drafts/chapter-one.md', 'Drafts'],
      ['Drafts/old/outline.md', 'Drafts/old'],
    ]);
    expect(source.tree()).toEqual([
      { path: 'Drafts', itemCount: 1 },
      { path: 'Drafts/old', itemCount: 1 },
      { path: 'Empty', itemCount: 0 },
    ]);
  });

  it('dispatches every op as its verb and body', async () => {
    const { source, seen } = make();
    await settle();
    seen.length = 0;
    const dest = { mountPointId: 'store-b', path: 'Inbox/x' };
    await source.ops.createFolder('New');
    await source.ops.renameFolder('Drafts/old', 'older');
    await source.ops.deleteFolder('Drafts');
    await source.ops.moveFolder('Drafts', dest);
    await source.ops.copyFolder('Drafts', dest);
    await source.ops.moveFile('notes.md', { mountPointId: 'store-b', path: 'Inbox/notes.md' });
    await source.ops.copyFile('notes.md', dest);
    await source.ops.linkFile('notes.md', dest);
    await source.ops.unlinkFile('notes.md');
    await source.ops.renameFile('notes.md', 'journal.md');
    await source.ops.describeFile('a.pdf', 'scan');
    await source.ops.deleteFile('notes.md');
    // Each write's settle refetches the two reads; only the writes are asserted.
    const writes = seen.filter((r) => r.type !== 'mountPointGet' && r.type !== 'mountFilesList');
    expect(writes).toEqual([
      { type: 'mountFolderCreate', mountPointId: 'store-a', path: 'New' },
      {
        type: 'mountFolderMove',
        mountPointId: 'store-a',
        fromPath: 'Drafts/old',
        toPath: 'Drafts/older',
      },
      { type: 'mountFolderDelete', mountPointId: 'store-a', path: 'Drafts', recursive: true },
      {
        type: 'mountFolderMove',
        mountPointId: 'store-a',
        fromPath: 'Drafts',
        toPath: 'Inbox/x',
        destMountPointId: 'store-b',
      },
      {
        type: 'mountFolderCopy',
        mountPointId: 'store-a',
        fromPath: 'Drafts',
        toPath: 'Inbox/x',
        destMountPointId: 'store-b',
      },
      {
        type: 'mountFileMove',
        mountPointId: 'store-a',
        sourcePath: 'notes.md',
        destMountPointId: 'store-b',
        destPath: 'Inbox/notes.md',
      },
      {
        type: 'mountFileCopy',
        mountPointId: 'store-a',
        sourcePath: 'notes.md',
        destMountPointId: 'store-b',
        destPath: 'Inbox/x',
      },
      {
        type: 'mountFileLink',
        mountPointId: 'store-a',
        sourcePath: 'notes.md',
        destMountPointId: 'store-b',
        destPath: 'Inbox/x',
      },
      { type: 'mountFileUnlink', mountPointId: 'store-a', path: 'notes.md' },
      { type: 'mountFileUpdate', mountPointId: 'store-a', path: 'notes.md', rename: 'journal.md' },
      { type: 'mountFileUpdate', mountPointId: 'store-a', path: 'a.pdf', description: 'scan' },
      { type: 'mountFileDelete', mountPointId: 'store-a', path: 'notes.md' },
    ]);
  });

  it('a write settles this store, the touched destination and the store lists — even when it fails', async () => {
    const { source, invalidate } = make((req) =>
      req.type === 'mountFileMove'
        ? new Error('A file of that name is already there')
        : DEFAULT_ROUTE(req),
    );
    await settle();
    invalidate.mockClear();
    await expect(
      source.ops.moveFile('notes.md', { mountPointId: 'store-b', path: 'notes.md' }),
    ).rejects.toThrow('A file of that name is already there');
    expect(keysOf(invalidate).sort()).toEqual(
      [mountPointKeys.store('store-a'), mountPointKeys.store('store-b'), mountPointKeys.lists]
        .map((k) => JSON.stringify(k))
        .sort(),
    );
    invalidate.mockClear();
    await source.ops.unlinkFile('notes.md');
    expect(keysOf(invalidate).sort()).toEqual(
      [mountPointKeys.store('store-a'), mountPointKeys.lists].map((k) => JSON.stringify(k)).sort(),
    );
  });
});
