/**
 * File-manager gesture → Quilltap wire-call translation (pure, unit-testable).
 *
 * The heart of the adapter, ported forward from v4
 * `components/files/svar/event-route-map.ts` and **re-targeted** to the v5
 * dispatch surface (P4.6v/P4.6y). It turns each user gesture into the wire
 * call(s) that carry it out, with NO widget runtime and NO network. Where v4
 * emitted `{ method, url, body }` REST calls, we emit dispatch JSON verbs for
 * every mutating op (the P4.6v variant table) and keep upload + open/download
 * on REST (contract §3). The pure shape and the two refusal arms are preserved
 * exactly.
 *
 * Backend reality this encodes (unchanged from v4; wire re-targeted):
 *  - rename        → `mountFileUpdate { mountPointId, path, rename }`
 *  - create folder → `mountFolderCreate { mountPointId, path }`
 *  - upload file   → `POST …?action=write-file` (multipart REST)
 *  - delete file   → `mountFileDelete { mountPointId, path }`
 *  - delete folder → `mountFolderDelete { mountPointId, path }`
 *  - move file     → `mountFileMove { mountPointId, sourcePath, destMountPointId, destPath }`
 *  - move folder   → `mountFolderMove { mountPointId, fromPath, toPath }`  (same-mount only)
 *  - copy file     → `mountFileCopy { mountPointId, sourcePath, destMountPointId, destPath }`
 *  - open/download → `GET …/files/<path>[?raw=1]` (REST)
 *
 * Backend gaps surfaced as `unsupported` (v4's exact prompts — no server arm
 * exists): there is no copy-folder verb, and move-folder cannot cross mounts.
 * The wirer turns an `unsupported` call into the steampunk prompt rather than
 * firing a doomed request.
 *
 * The bespoke widget resolves each node's `(mountId, relativePath, type)`
 * itself (path-native ids), so the mappers take resolved nodes directly rather
 * than v4's opaque-id `NodeResolver`; root ops pass `null` and fall back to the
 * pane's `mountId` (v4's `MapContext` fallback, quirk 4).
 *
 * Cases derived from v4 `components/files/svar/event-route-map.ts`.
 *
 * @module files/event-wire-map
 */

import { encodeRelPath, relBasename, relDirname, relJoin } from './node-id';

export type SvarOp =
  | 'rename'
  | 'create-folder'
  | 'upload'
  | 'delete-file'
  | 'delete-folder'
  | 'move-file'
  | 'move-folder'
  | 'copy-file'
  | 'open'
  | 'download';

/** A resolved tree node: its mount + path + kind (v4 `ResolvedNode`). */
export interface ResolvedNode {
  mountId: string;
  relativePath: string;
  type: 'file' | 'folder';
}

/**
 * A dispatch JSON verb (the mutating file-op family, P4.6v). Field names are
 * camelCase to match the Rust `Request` serde tags byte-for-byte. These verbs
 * are NOT in the SPA `CoreRequest` union (lane A owns `core-contract.ts`) — the
 * wiring core sends them through `CoreClient.dispatch` with a single localized
 * cast (the P4.6x defensive-dispatch precedent).
 */
export type MountDispatchRequest =
  | { type: 'mountFileUpdate'; mountPointId: string; path: string; rename: string }
  | { type: 'mountFolderCreate'; mountPointId: string; path: string }
  | { type: 'mountFileDelete'; mountPointId: string; path: string }
  | { type: 'mountFolderDelete'; mountPointId: string; path: string }
  | { type: 'mountFolderMove'; mountPointId: string; fromPath: string; toPath: string }
  | {
      type: 'mountFileMove';
      mountPointId: string;
      sourcePath: string;
      destMountPointId: string;
      destPath: string;
    }
  | {
      type: 'mountFileCopy';
      mountPointId: string;
      sourcePath: string;
      destMountPointId: string;
      destPath: string;
    };

/** One wire call: a dispatch verb, an upload leg, a REST GET, or a refusal. */
export interface WireCall {
  op: SvarOp;
  /** Dispatch JSON verb (the mutating ops), or undefined for REST-only ops. */
  request?: MountDispatchRequest;
  /** Upload leg: multipart `?action=write-file` REST (the wirer attaches the File). */
  upload?: { mountPointId: string; path: string };
  /** Open/download: the raw item-route GET url. */
  getUrl?: string;
  /** Set when the backend can't express the gesture; carries a user-facing reason. */
  unsupported?: string;
}

// ── URL builders (open/download REST — contract §3) ─────────────────────────
const mountBase = (mountId: string) => `/api/v1/mount-points/${mountId}`;
const itemUrl = (mountId: string, rel: string) =>
  `${mountBase(mountId)}/files/${encodeRelPath(rel)}`;

// ── Per-gesture mappers ─────────────────────────────────────────────────────

/** Rename `node` to the new basename `newName` (same directory). */
export function mapRename(node: ResolvedNode, newName: string): WireCall[] {
  // The gesture gives the new basename; the verb wants the new mount-relative path.
  const rename = relJoin(relDirname(node.relativePath), newName);
  return [
    {
      op: 'rename',
      request: {
        type: 'mountFileUpdate',
        mountPointId: node.mountId,
        path: node.relativePath,
        rename,
      },
    },
  ];
}

/**
 * Create a file or folder named `name` inside `parent` (null = the mount root,
 * resolving to `mountId` — quirk 4). Files ride the multipart upload leg.
 */
export function mapCreate(
  parent: ResolvedNode | null,
  name: string,
  kind: 'file' | 'folder',
  mountId: string,
): WireCall[] {
  const targetMount = parent?.mountId ?? mountId;
  const parentPath = parent?.relativePath ?? '';
  const path = relJoin(parentPath, name);
  if (kind === 'folder') {
    return [
      { op: 'create-folder', request: { type: 'mountFolderCreate', mountPointId: targetMount, path } },
    ];
  }
  return [{ op: 'upload', upload: { mountPointId: targetMount, path } }];
}

/** Delete each of `nodes` (files → `mountFileDelete`, folders → `mountFolderDelete`). */
export function mapDelete(nodes: ResolvedNode[]): WireCall[] {
  return nodes.map((node): WireCall => {
    if (node.type === 'folder') {
      return {
        op: 'delete-folder',
        request: { type: 'mountFolderDelete', mountPointId: node.mountId, path: node.relativePath },
      };
    }
    return {
      op: 'delete-file',
      request: { type: 'mountFileDelete', mountPointId: node.mountId, path: node.relativePath },
    };
  });
}

/** Move each of `sources` into `dest`. Cross-mount folder move is refused. */
export function mapMove(sources: ResolvedNode[], dest: ResolvedNode): WireCall[] {
  return sources.map((src): WireCall => {
    const destPath = relJoin(dest.relativePath, relBasename(src.relativePath));
    if (src.type === 'folder') {
      const call: WireCall = {
        op: 'move-folder',
        request: {
          type: 'mountFolderMove',
          mountPointId: src.mountId,
          fromPath: src.relativePath,
          toPath: destPath,
        },
      };
      if (src.mountId !== dest.mountId) {
        call.unsupported =
          'Whole folders can’t be ferried between repositories — copy the files across instead.';
      }
      return call;
    }
    return {
      op: 'move-file',
      request: {
        type: 'mountFileMove',
        mountPointId: src.mountId,
        sourcePath: src.relativePath,
        destMountPointId: dest.mountId,
        destPath,
      },
    };
  });
}

/** Copy each of `sources` into `dest`. Whole-folder copy is refused (no verb). */
export function mapCopy(sources: ResolvedNode[], dest: ResolvedNode): WireCall[] {
  return sources.map((src): WireCall => {
    const destPath = relJoin(dest.relativePath, relBasename(src.relativePath));
    const call: WireCall = {
      op: 'copy-file',
      request: {
        type: 'mountFileCopy',
        mountPointId: src.mountId,
        sourcePath: src.relativePath,
        destMountPointId: dest.mountId,
        destPath,
      },
    };
    if (src.type === 'folder') {
      // No copy-folder verb exists; surface it rather than fire a doomed request.
      call.unsupported =
        'Folders can’t be copied wholesale just yet — select the files within instead.';
    }
    return call;
  });
}

/** Open `node` (view) — file only. */
export function mapOpen(node: ResolvedNode): WireCall[] {
  if (node.type === 'folder') return [];
  return [{ op: 'open', getUrl: itemUrl(node.mountId, node.relativePath) }];
}

/** Download `node` (raw bytes) — file only. */
export function mapDownload(node: ResolvedNode): WireCall[] {
  if (node.type === 'folder') return [];
  return [{ op: 'download', getUrl: `${itemUrl(node.mountId, node.relativePath)}?raw=1` }];
}
