/**
 * One document store, as a folder browser sees it — the SPA twin of v4
 * `components/document-browser/useDocumentStoreSource.ts` (`c34986421`, pin
 * `624d0d25c`; P4.D387 item 6): its record (with its capabilities), its
 * listing (files and folders), and every write the browser makes, each a thin
 * call to an existing (or C13) mount-point verb.
 *
 * v4's why, carried: reads are TanStack queries under
 * `mountPointKeys.store(id)`, which the `mountPoints` realtime topic
 * invalidates — so a change made anywhere (another tab, a character's `doc_*`
 * tool, the CLI) reaches the browser without a poll. A write also invalidates
 * the stores it touched at once, so the browser does not wait on the socket
 * for its own change.
 *
 * @module document-browser/document-store.source
 */

import { computed, inject } from '@angular/core';
import { QueryClient, injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { DocMountBrowserEntry } from '../core/core-contract';
import { folderName, joinFolder, parentFolder } from '../item-browser/item-folders';
import { uploadMountFile } from '../screens/scriptorium/scriptorium.api';
import {
  copyDocumentFolder,
  createDocumentFolder,
  deleteDocumentFile,
  deleteDocumentFolder,
  describeDocumentFile,
  fetchDocumentListing,
  fetchDocumentStore,
  moveDocumentFolder,
  renameDocumentFile,
  transferDocumentFile,
  unlinkDocumentFile,
  type FileDestination,
} from './document-store.api';
import { mountPointKeys } from './document-store.keys';

/** A listed file, with the folder the browser files it under. */
export type DocumentEntry = DocMountBrowserEntry & {
  /** The folder holding it (`''` at the top). */
  folder: string;
};

/** The store's folders with their direct file counts, for the view math. */
export function folderTreeOf(
  folders: readonly string[],
  entries: readonly DocumentEntry[],
): Array<{ path: string; itemCount: number }> {
  const counts = new Map<string, number>();
  for (const e of entries)
    counts.set(e.folder.toLowerCase(), (counts.get(e.folder.toLowerCase()) ?? 0) + 1);
  return folders.map((path) => ({ path, itemCount: counts.get(path.toLowerCase()) ?? 0 }));
}

/**
 * v4 `useDocumentStoreSource(storeId)`. Call in an injection context;
 * `storeId` is read reactively (a signal or input getter).
 */
export function createDocumentStoreSource(storeId: () => string | null) {
  const core = inject(CoreClient);
  const queryClient = inject(QueryClient);
  const id = (): string => storeId() ?? '';

  const storeQuery = injectQuery(() => ({
    queryKey: mountPointKeys.detail(id()),
    queryFn: () => fetchDocumentStore(core, id()),
    enabled: Boolean(storeId()),
  }));

  const listingQuery = injectQuery(() => ({
    queryKey: mountPointKeys.files(id()),
    queryFn: () => fetchDocumentListing(core, id()),
    enabled: Boolean(storeId()),
  }));

  const entries = computed<DocumentEntry[]>(() =>
    (listingQuery.data()?.files ?? []).map((f) => ({ ...f, folder: parentFolder(f.relativePath) })),
  );
  const folders = computed<string[]>(() => listingQuery.data()?.folders ?? []);
  const tree = computed(() => folderTreeOf(folders(), entries()));

  /** After a write: this store, any other store it touched, and the store list's counts. */
  const settle = async (...others: Array<string | undefined>): Promise<void> => {
    const ids = new Set([id(), ...others.filter((o): o is string => Boolean(o))]);
    await Promise.all([
      ...Array.from(ids).map((s) =>
        queryClient.invalidateQueries({ queryKey: mountPointKeys.store(s) }),
      ),
      queryClient.invalidateQueries({ queryKey: mountPointKeys.lists }),
    ]);
  };

  const withSettle =
    <A extends unknown[], R>(
      fn: (...args: A) => Promise<R>,
      touched?: (...args: A) => string | undefined,
    ) =>
    async (...args: A): Promise<R> => {
      try {
        return await fn(...args);
      } finally {
        await settle(touched?.(...args));
      }
    };

  const ops = {
    createFolder: withSettle(async (path: string) => {
      await createDocumentFolder(core, id(), path);
      return { name: folderName(path), path };
    }),
    renameFolder: withSettle(async (path: string, name: string) => {
      const toPath = joinFolder(parentFolder(path), name);
      await moveDocumentFolder(core, id(), path, toPath);
      return { name: folderName(toPath), path: toPath };
    }),
    deleteFolder: withSettle((path: string) => deleteDocumentFolder(core, id(), path)),
    moveFolder: withSettle(
      (fromPath: string, dest: FileDestination) =>
        moveDocumentFolder(core, id(), fromPath, dest.path, dest.mountPointId),
      (_from, dest) => dest.mountPointId,
    ),
    copyFolder: withSettle(
      (fromPath: string, dest: FileDestination) => copyDocumentFolder(core, id(), fromPath, dest),
      (_from, dest) => dest.mountPointId,
    ),
    moveFile: withSettle(
      (sourcePath: string, dest: FileDestination) =>
        transferDocumentFile(core, 'mountFileMove', id(), sourcePath, dest),
      (_from, dest) => dest.mountPointId,
    ),
    copyFile: withSettle(
      (sourcePath: string, dest: FileDestination) =>
        transferDocumentFile(core, 'mountFileCopy', id(), sourcePath, dest),
      (_from, dest) => dest.mountPointId,
    ),
    linkFile: withSettle(
      (sourcePath: string, dest: FileDestination) =>
        transferDocumentFile(core, 'mountFileLink', id(), sourcePath, dest),
      (_from, dest) => dest.mountPointId,
    ),
    unlinkFile: withSettle((path: string) => unlinkDocumentFile(core, id(), path)),
    renameFile: withSettle((path: string, newPath: string) =>
      renameDocumentFile(core, id(), path, newPath),
    ),
    describeFile: withSettle((path: string, description: string) =>
      describeDocumentFile(core, id(), path, description),
    ),
    deleteFile: withSettle((path: string) => deleteDocumentFile(core, id(), path)),
    uploadFile: withSettle((path: string, file: File) => uploadMountFile(id(), path, file)),
  };

  return {
    store: computed(() => storeQuery.data() ?? null),
    storeLoading: computed(() => storeQuery.isLoading()),
    storeError: computed(() => storeQuery.error()),
    entries,
    folders,
    tree,
    listingLoading: computed(() => listingQuery.isLoading()),
    refetch: () => Promise.all([storeQuery.refetch(), listingQuery.refetch()]),
    ops,
  };
}

export type DocumentStoreSource = ReturnType<typeof createDocumentStoreSource>;
export type DocumentStoreOps = DocumentStoreSource['ops'];
