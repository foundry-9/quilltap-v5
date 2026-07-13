/**
 * Map a Quilltap mount listing → flat tree nodes (pure).
 *
 * A ported-forward copy of v4 `components/files/svar/listing-to-tree.ts`,
 * re-targeted to the v5 `mountFilesList` dispatch envelope
 * (`{ files, folders }` — the same `{files, folders}` shape v4's
 * `GET …/files` returned; the server serializes the full
 * `DocMountFileLinkWithContent` rows, so `relativePath` / `fileSizeBytes` /
 * `lastModified` are the live field names — see `services/mount_index/list.rs`).
 *
 * Like v4 it emits a FLAT node list (the widget assembles the nested tree from
 * it — `build-tree.ts`): it
 *  - keys every node by its mount-relative path (`node-id.ts`),
 *  - synthesizes any missing ancestor folders so a nested file is never
 *    orphaned even if the listing didn't enumerate every intermediate dir,
 *  - carries size + mtime so the listing columns have something to show,
 *  - sorts folders shallowest-first (parents before children).
 *
 * Cases derived from v4 `components/files/svar/listing-to-tree.ts` (no v4 unit
 * test existed).
 *
 * @module files/listing-to-tree
 */

import { relDirname } from './node-id';
import type { FileNode } from './types';

/** One `doc_mount_files` row as `mountFilesList` serializes it (permissive). */
export interface MountFileRow {
  relativePath: string;
  fileSizeBytes?: number | null;
  lastModified?: string | number | null;
}

/** The `mountFilesList` envelope: the full row set + the merged folder paths. */
export interface MountListing {
  files: MountFileRow[];
  folders: string[];
}

export function listingToTree(listing: MountListing): FileNode[] {
  const folderPaths = new Set<string>();
  const addWithAncestors = (rel: string) => {
    for (const p of withAncestors(rel)) folderPaths.add(p);
  };

  for (const folder of listing.folders) {
    if (folder) addWithAncestors(folder);
  }
  for (const f of listing.files) {
    const dir = relDirname(f.relativePath);
    if (dir) addWithAncestors(dir);
  }

  const nodes: FileNode[] = [];
  // Shallower folders first so a tree builder sees parents before children.
  for (const folder of [...folderPaths].sort((a, b) => depth(a) - depth(b) || a.localeCompare(b))) {
    nodes.push({ path: folder, type: 'folder' });
  }
  for (const f of listing.files) {
    nodes.push({
      path: f.relativePath,
      type: 'file',
      size: f.fileSizeBytes ?? undefined,
      date: toDate(f.lastModified),
    });
  }
  return nodes;
}

/** A path plus every ancestor directory: 'a/b/c' → ['a', 'a/b', 'a/b/c']. */
function withAncestors(rel: string): string[] {
  const parts = rel.split('/').filter(Boolean);
  const out: string[] = [];
  for (let i = 1; i <= parts.length; i++) out.push(parts.slice(0, i).join('/'));
  return out;
}

function depth(rel: string): number {
  return rel.split('/').filter(Boolean).length;
}

function toDate(ts: string | number | null | undefined): Date | undefined {
  if (ts == null) return undefined;
  const d = new Date(ts);
  return Number.isNaN(d.getTime()) ? undefined : d;
}
