/**
 * The file-manager wiring core — the testable adapter between a widget gesture
 * and the Quilltap wire.
 *
 * A ported-forward copy of v4 `components/files/svar/createSvarAdapter.ts`,
 * adapted from SVAR's `api.on(action)` interception to a plain method surface
 * the bespoke `qt-file-manager` calls directly (the D18 spike sent us to
 * build-our-own). It keeps v4's choreography exactly:
 *  - **serialized mutation chain** (`enqueue`) — a flurry of gestures can't
 *    interleave;
 *  - **capability gating** before any request (a read-only mount refuses,
 *    reverting the optimistic UI change);
 *  - **reload-to-reconcile** — every mutating op is followed by `onReloadNeeded`
 *    (re-read server truth: new ids reconcile on success, revert on failure);
 *  - **error translation** to a steampunk verdict (reading the dispatch `code`
 *    then `kind` defensively);
 *  - the **post-copy reindex** for cross-storage byte-copies (`onIndexing`).
 *
 * Framework-free (no Angular / no widget import): unit-testable with a fake
 * transport. The concrete transport over `CoreClient` lives in
 * `file-manager-transport.ts`.
 *
 * Cases derived from v4 `components/files/svar/createSvarAdapter.ts`.
 *
 * @module files/file-manager-adapter
 */

import {
  mapCopy,
  mapCreate,
  mapDelete,
  mapDownload,
  mapMove,
  mapOpen,
  mapRename,
  type MountDispatchRequest,
  type ResolvedNode,
  type WireCall,
} from './event-wire-map';
import { reindexAfterCopy } from './reindex-after-copy';
import { translateMountOpError } from './error-translation';
import type { MountCapabilities } from './types';

export interface AdapterCallbacks {
  /** Surface a user-facing failure (steampunk voice already applied). */
  onError?: (message: string, info: { suggestCopy: boolean; conflict: boolean }) => void;
  /** A cross-storage copy is being re-indexed in the background. */
  onIndexing?: (target: { mountId: string; path: string }) => void;
  /** Re-read the listing and refresh the tree (reconcile / revert). */
  onReloadNeeded?: () => void;
  /** Debug logging hook (wired to the app logger by the host). */
  log?: (message: string, data?: Record<string, unknown>) => void;
}

export interface AdapterConfig {
  mountId: string;
  capabilities: MountCapabilities;
}

/** A dispatch verb (mutations + the scoped reindex/embed) — `{type, …}`. */
export type MountVerbRequest = { type: string } & Record<string, unknown>;

/** The defensive dispatch/REST outcome (the P4.6y `code`-may-be-absent pin). */
export type WireResult =
  | { ok: true; data: Record<string, unknown> }
  | { ok: false; kind?: string; code?: string; message?: string };

/**
 * The transport seam. `dispatch` carries the JSON verbs (`/api/dispatch`);
 * `writeFile` is the multipart `?action=write-file` REST upload leg (contract
 * §3). Both resolve to a {@link WireResult} — errors never throw.
 */
export interface FileManagerTransport {
  dispatch(request: MountVerbRequest): Promise<WireResult>;
  writeFile(mountId: string, path: string, file: File): Promise<WireResult>;
}

/** The mutating gestures, for the capability gate. */
type MutatingGesture = 'rename' | 'create-folder' | 'upload' | 'delete' | 'move' | 'copy';

export class FileManagerAdapter {
  /** Serialize mutations so a flurry of gestures can't interleave. */
  private chain: Promise<void> = Promise.resolve();

  constructor(
    private readonly transport: FileManagerTransport,
    private readonly config: AdapterConfig,
    private readonly cb: AdapterCallbacks = {},
  ) {}

  // ── Mutating gestures (each capability-gated + serialized + reconciled) ────

  rename(node: ResolvedNode, newName: string): Promise<void> {
    return this.runGesture('rename', () => mapRename(node, newName));
  }

  createFolder(parent: ResolvedNode | null, name: string): Promise<void> {
    return this.runGesture('create-folder', () =>
      mapCreate(parent, name, 'folder', this.config.mountId),
    );
  }

  uploadFile(parent: ResolvedNode | null, file: File): Promise<void> {
    return this.runGesture(
      'upload',
      () => mapCreate(parent, file.name, 'file', this.config.mountId),
      file,
    );
  }

  delete(nodes: ResolvedNode[]): Promise<void> {
    return this.runGesture('delete', () => mapDelete(nodes));
  }

  move(sources: ResolvedNode[], dest: ResolvedNode): Promise<void> {
    return this.runGesture('move', () => mapMove(sources, dest));
  }

  copy(sources: ResolvedNode[], dest: ResolvedNode): Promise<void> {
    return this.runGesture('copy', () => mapCopy(sources, dest));
  }

  // ── Read gestures (no network; the widget navigates to the URL) ───────────

  /** The raw item-route URL to open (view) a file, or null for a folder. */
  openUrl(node: ResolvedNode): string | null {
    return mapOpen(node)[0]?.getUrl ?? null;
  }

  /** The `?raw=1` item-route URL to download a file, or null for a folder. */
  downloadUrl(node: ResolvedNode): string | null {
    return mapDownload(node)[0]?.getUrl ?? null;
  }

  // ── Internals ─────────────────────────────────────────────────────────────

  private runGesture(
    gesture: MutatingGesture,
    buildCalls: () => WireCall[],
    file?: File,
  ): Promise<void> {
    if (!allowed(gesture, this.config.capabilities)) {
      this.cb.onError?.('This repository won’t accept changes just now.', {
        suggestCopy: false,
        conflict: false,
      });
      this.cb.onReloadNeeded?.(); // revert the optimistic change
      return Promise.resolve();
    }
    return this.enqueue(() => this.runCalls(buildCalls(), file));
  }

  private enqueue(task: () => Promise<void>): Promise<void> {
    // .then(task, task): run next regardless of the previous op's outcome.
    this.chain = this.chain.then(task, task);
    return this.chain;
  }

  private async runCalls(calls: WireCall[], file?: File): Promise<void> {
    if (calls.length === 0) return;
    for (const call of calls) {
      if (call.unsupported) {
        this.cb.onError?.(call.unsupported, { suggestCopy: false, conflict: false });
        continue;
      }
      const result = await this.send(call, file);
      if (result.ok) {
        this.cb.log?.('[file-manager] op ok', { op: call.op });
        if (call.op === 'copy-file' || call.op === 'move-file') {
          const fileOp = asFileOpResult(result.data);
          if (fileOp) {
            const target = reindexAfterCopy(fileOp);
            if (target) {
              this.cb.onIndexing?.(target);
              void this.fireReindex(target);
            }
          }
        }
      } else {
        const verdict = translateMountOpError({ kind: result.kind, code: result.code });
        this.cb.onError?.(verdict.message, {
          suggestCopy: verdict.suggestCopy,
          conflict: verdict.conflict,
        });
        this.cb.log?.('[file-manager] op failed', {
          op: call.op,
          kind: result.kind,
          code: result.code,
        });
      }
    }
    // Re-read server truth: reconciles new ids on success, reverts on failure.
    this.cb.onReloadNeeded?.();
  }

  private send(call: WireCall, file?: File): Promise<WireResult> {
    if (call.request) return this.transport.dispatch(call.request as MountDispatchRequest);
    if (call.upload) {
      if (!file) {
        return Promise.resolve({ ok: false, message: 'No file to upload' });
      }
      return this.transport.writeFile(call.upload.mountPointId, call.upload.path, file);
    }
    // open/download carry only a getUrl — not executed here (the widget navigates).
    return Promise.resolve({ ok: true, data: {} });
  }

  /** Fire-and-forget scoped reindex + embed; a failure must NOT undo the copy. */
  private async fireReindex(target: { mountId: string; path: string }): Promise<void> {
    try {
      await this.transport.dispatch({
        type: 'mountReindex',
        mountPointId: target.mountId,
        path: target.path,
      });
      await this.transport.dispatch({
        type: 'mountEmbed',
        mountPointId: target.mountId,
        path: target.path,
      });
      this.cb.log?.('[file-manager] reindex+embed enqueued', target);
    } catch (err) {
      this.cb.log?.('[file-manager] reindex enqueue failed (non-fatal)', { error: String(err) });
    }
  }
}

/** v4 `allowed(action, capabilities)`, split per the bespoke gesture set. */
function allowed(gesture: MutatingGesture, c: MountCapabilities): boolean {
  switch (gesture) {
    case 'rename':
    case 'upload':
      return c.canWrite;
    case 'create-folder':
      return c.canCreateFolder;
    case 'delete':
      return c.canDelete;
    case 'move':
    case 'copy':
      return c.canMoveOut;
  }
}

/** v4 `isFileOpResult` — the move/copy result carries the strategy + dest pair. */
function asFileOpResult(
  x: unknown,
): { strategy: string; destMountPointId: string; destPath: string } | null {
  if (
    typeof x === 'object' &&
    x !== null &&
    typeof (x as { strategy?: unknown }).strategy === 'string' &&
    typeof (x as { destPath?: unknown }).destPath === 'string' &&
    typeof (x as { destMountPointId?: unknown }).destMountPointId === 'string'
  ) {
    return x as { strategy: string; destMountPointId: string; destPath: string };
  }
  return null;
}
