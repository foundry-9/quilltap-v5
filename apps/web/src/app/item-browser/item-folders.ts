/**
 * The rules-free folder path helpers every folder-of-items browser shares
 * (the wardrobe's, the Inventory's, the Scriptorium's) — v4
 * `lib/mount-index/item-folders.ts` at the pin `624d0d25c`, the client-safe
 * half: bodies byte-identical to v4's (measured), and to what v5 carried in
 * `wardrobe/folders.ts` since `4a38222e6`. One copy: `wardrobe/folders.ts`
 * re-exports them, keeping only the wardrobe's own rules (the `WARDROBE_*`
 * constants, the normalisers, the errors).
 *
 * A folder is a path relative to the browser's root folder: `''` for the
 * root, `Formal/Evening` for a nested one. Folder names are case-insensitive.
 *
 * Client-safe: pure.
 *
 * @module item-browser/item-folders
 */

/** A folder as the browser and the folder routes see it (v4 `ItemFolder`). */
export interface ItemFolder {
  /** Path relative to the root folder, e.g. `Formal/Evening`. Never `''`. */
  path: string;
  /** The last segment. */
  name: string;
  /** Non-archived items directly inside (not in subfolders). */
  itemCount: number;
}

/** Split a folder path into its segments (`''` → `[]`). */
export function folderSegments(folder: string): string[] {
  return folder.length === 0 ? [] : folder.split('/');
}

/** The folder above `folder`; the root's parent is the root. */
export function parentFolder(folder: string): string {
  const slash = folder.lastIndexOf('/');
  return slash < 0 ? '' : folder.slice(0, slash);
}

/** The last segment of `folder`; `''` for the root. */
export function folderName(folder: string): string {
  const slash = folder.lastIndexOf('/');
  return slash < 0 ? folder : folder.slice(slash + 1);
}

/** Join path pieces, skipping empty ones: `joinFolder('', 'x.md')` → `x.md`. */
export function joinFolder(...parts: string[]): string {
  return parts.filter((part) => part.length > 0).join('/');
}

/** Folder names are case-insensitive: `Formal` and `formal` are one folder. */
export function isSameFolder(a: string | null | undefined, b: string | null | undefined): boolean {
  return (a ?? '').toLowerCase() === (b ?? '').toLowerCase();
}

/** True when `folder` is `ancestor` or nested anywhere beneath it. */
export function isUnderFolder(folder: string, ancestor: string): boolean {
  if (ancestor.length === 0) return true;
  const f = folder.toLowerCase();
  const a = ancestor.toLowerCase();
  return f === a || f.startsWith(`${a}/`);
}

/**
 * Re-root `folder` from under `fromAncestor` to under `toAncestor`
 * (`Formal/Evening`, `Formal` → `Dress` gives `Dress/Evening`). A folder not
 * under `fromAncestor` is returned unchanged.
 */
export function rebaseFolder(folder: string, fromAncestor: string, toAncestor: string): string {
  if (!isUnderFolder(folder, fromAncestor)) return folder;
  const rest = folder.slice(fromAncestor.length).replace(/^\//, '');
  return joinFolder(toAncestor, rest);
}

/** The display form of a folder: `Formal › Evening`. */
export function folderCrumb(folder: string): string {
  return folderSegments(folder).join(' › ');
}
