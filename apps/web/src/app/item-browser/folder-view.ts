/**
 * What a folder-of-items browser shows at one place in a folder tree — the
 * pure half of every browser built from `item-browser/` (the wardrobe's, the
 * Inventory's, the Scriptorium's), stated once. A transcription of v4
 * `lib/folder-browser/view.ts` at the pin `624d0d25c` (`b2b783b47` — moved
 * out of `lib/wardrobe/folder-view.ts`, which keeps only `viewTierOf`):
 *
 *   - **folder rows** are the direct children of the folder being browsed,
 *     from the server's tree (empty folders included) unioned with the
 *     folders the listed items imply; a row's count is the live items
 *     directly inside, whatever filters are on;
 *   - **tier rows** (a character's merged view only) are the shared tiers the
 *     character borrows from — one per tier that has items, never merged
 *     across tiers;
 *   - an item belongs to the view when its tier and folder both match.
 *
 * Pinned rule by rule in `folder-view.spec.ts` and row for row against v4's
 * real module in `item-browser.oracle.spec.ts`.
 *
 * Client-safe: pure.
 *
 * @module item-browser/folder-view
 */

import { folderName, folderSegments, isSameFolder, isUnderFolder, joinFolder } from './item-folders';

/** The tier key of the container's own items in a browser's view. */
export const OWN_TIER = 'own';

/** Where a borrowed item came from: the tier that holds it. */
export interface BrowserOrigin {
  scope: 'character' | 'group' | 'project' | 'general';
  /** Container id; null for `general`. */
  id: string | null;
  /** Display name, resolved server-side. */
  name: string;
}

/** What the view math needs to know about an item. */
export interface BrowserViewItem {
  archivedAt?: string | null;
  folder?: string | null;
  origin?: BrowserOrigin | null;
}

/** A folder row: one direct child of the folder being browsed. */
export interface FolderRowView {
  path: string;
  name: string;
  itemCount: number;
}

/** A tier row: a shared tier a character's view borrows from. */
export interface TierRowView {
  key: string;
  label: string;
  /** The scope, for the chip. */
  scope: BrowserOrigin['scope'];
  itemCount: number;
}

/**
 * The label a tier row (and its crumb) carries. A character origin with an
 * empty name falls back to `generalName` — `General` by default, and v4's
 * callers pass none (a measured byte shift from `236c04e35`'s wardrobe
 * module, which said `Quilltap General`).
 */
export function tierLabel(origin: BrowserOrigin | null | undefined, generalName = 'General'): string {
  if (!origin || origin.scope === 'general') return 'General';
  if (origin.scope === 'project') return `Project: ${origin.name}`;
  if (origin.scope === 'group') return `Group: ${origin.name}`;
  return origin.name || generalName;
}

/** The direct child of `current` that `folder` sits in (or is), or null. */
function childOf(folder: string, current: string): string | null {
  if (!folder || isSameFolder(folder, current) || !isUnderFolder(folder, current)) return null;
  const depth = folderSegments(current).length;
  const segments = folderSegments(folder);
  return joinFolder(current, segments[depth]);
}

/**
 * The folder rows under `current`: the server's tree (when the view has one)
 * unioned with the folders `items` imply. Counts are live items directly
 * inside, from the server when it knows the folder, else from `items`.
 * Sorted by name, case-insensitively.
 */
export function childFolderRows(
  current: string,
  items: readonly BrowserViewItem[],
  tree: ReadonlyArray<{ path: string; itemCount: number }> = [],
): FolderRowView[] {
  const rows = new Map<string, FolderRowView>();
  const serverCounts = new Map(tree.map((f) => [f.path.toLowerCase(), f.itemCount]));
  const add = (path: string) => {
    const key = path.toLowerCase();
    if (!rows.has(key)) rows.set(key, { path, name: folderName(path), itemCount: 0 });
  };
  for (const folder of tree) {
    const child = childOf(folder.path, current);
    if (child) add(child);
  }
  for (const item of items) {
    const child = childOf(item.folder ?? '', current);
    if (child) add(child);
  }
  for (const row of rows.values()) {
    const key = row.path.toLowerCase();
    row.itemCount =
      serverCounts.get(key) ??
      items.filter((i) => !i.archivedAt && isSameFolder(i.folder ?? '', row.path)).length;
  }
  return Array.from(rows.values()).sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }),
  );
}

/**
 * The tier rows of a character's view: one per borrowed tier with items, in
 * the order the items first name them (the pool's own order — nearest tier
 * first). `tierOf` maps an item to its view tier.
 */
export function tierRows<T extends BrowserViewItem>(
  items: readonly T[],
  tierOf: (item: T) => string,
): TierRowView[] {
  const rows = new Map<string, TierRowView>();
  for (const item of items) {
    const key = tierOf(item);
    if (key === OWN_TIER) continue;
    const row = rows.get(key);
    const live = item.archivedAt ? 0 : 1;
    if (row) {
      row.itemCount += live;
      continue;
    }
    rows.set(key, {
      key,
      label: tierLabel(item.origin),
      scope: item.origin?.scope ?? 'general',
      itemCount: live,
    });
  }
  return Array.from(rows.values());
}
