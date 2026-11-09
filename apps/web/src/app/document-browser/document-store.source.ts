/**
 * One document store, as a folder browser sees it — the SPA twin of v4
 * `components/document-browser/useDocumentStoreSource.ts` (`c34986421`, pin
 * `624d0d25c`).
 *
 * @module document-browser/document-store.source
 */

import type { DocMountBrowserEntry } from '../core/core-contract';

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
