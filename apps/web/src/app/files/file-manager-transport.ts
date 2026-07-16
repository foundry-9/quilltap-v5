/**
 * The concrete file-manager transport over {@link CoreClient} + `fetch`.
 *
 * The one runtime seam that touches the app's dispatch client. JSON verbs ride
 * `CoreClient.dispatch` (`/api/dispatch`); the upload leg is the multipart
 * `?action=write-file` REST route (contract §3, the P4.6y byte-preserving leg).
 * The mutating file-op verbs (`mountFileUpdate`, `mountFolderCreate`, …) are
 * NOT in the SPA `CoreRequest` union — lane A owns `core-contract.ts`, so this
 * lane sends them through `dispatch` with a single, localized cast (the P4.6x
 * defensive-dispatch precedent; the unifier may fold the verbs into the union).
 * The dispatch `code` is read defensively off `resp.data` (skip-serialized when
 * absent — the P4.6y pin).
 *
 * @module files/file-manager-transport
 */

import { apiUrl } from '../core/api-url';
import type { CoreClient } from '../core/core-client';
import type { CoreRequest } from '../core/core-contract';

import type { FileManagerTransport, MountVerbRequest, WireResult } from './file-manager-adapter';
import { listingToTree, type MountFileRow } from './listing-to-tree';
import type { FileNode } from './types';

export class CoreFileManagerTransport implements FileManagerTransport {
  constructor(private readonly core: CoreClient) {}

  async dispatch(request: MountVerbRequest): Promise<WireResult> {
    // The file-op verbs are not in `CoreRequest` (lane A's file); cast at the
    // one seam and read the envelope defensively.
    const resp = await this.core.dispatch(request as unknown as CoreRequest);
    if (resp.type === 'error') {
      const data = resp.data as { kind?: string; code?: string; message?: string };
      return { ok: false, kind: data.kind, code: data.code, message: data.message };
    }
    return { ok: true, data: (resp.data ?? {}) as Record<string, unknown> };
  }

  async writeFile(mountId: string, path: string, file: File): Promise<WireResult> {
    const form = new FormData();
    form.append('file', file);
    form.append('path', path);
    let res: Response;
    try {
      res = await fetch(apiUrl(`/api/v1/mount-points/${mountId}?action=write-file`), {
        method: 'POST',
        body: form,
      });
    } catch (err) {
      return { ok: false, message: String(err) };
    }
    if (!res.ok) {
      // The multipart edge returns v4's `{ error, code }` body (files_routes.rs).
      let code: string | undefined;
      let message: string | undefined;
      try {
        const body = (await res.json()) as { error?: string; code?: string };
        code = body?.code;
        message = body?.error;
      } catch {
        /* non-JSON error body */
      }
      return { ok: false, code, kind: httpStatusToKind(res.status), message };
    }
    let data: Record<string, unknown> = {};
    try {
      data = (await res.json()) as Record<string, unknown>;
    } catch {
      /* empty body */
    }
    return { ok: true, data };
  }
}

/** Map the multipart edge's HTTP status onto the dispatch `ErrorKind` kebab strings. */
function httpStatusToKind(status: number): string {
  switch (status) {
    case 409:
      return 'conflict';
    case 404:
      return 'not-found';
    case 400:
      return 'bad-request';
    default:
      return 'internal';
  }
}

/**
 * Load a mount's listing (`mountFilesList`) and map it to flat tree nodes.
 * Throws on a failed dispatch (the caller shows the read-failed notice).
 */
export async function loadMountTree(core: CoreClient, mountId: string): Promise<FileNode[]> {
  const resp = await core.dispatch({ type: 'mountFilesList', mountPointId: mountId });
  if (resp.type === 'error') {
    throw new Error(resp.data.message);
  }
  const data = (resp.data ?? {}) as { files?: MountFileRow[]; folders?: string[] };
  return listingToTree({
    files: Array.isArray(data.files) ? data.files : [],
    folders: Array.isArray(data.folders) ? data.folders : [],
  });
}
