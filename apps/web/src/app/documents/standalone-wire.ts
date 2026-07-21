/**
 * The standalone (chat-less) Document Mode dispatch surface (P4.9J4).
 *
 * v4's `StandaloneDocumentView` and its `DocumentPickerModal` (with `chatId ===
 * null`) drive `/api/v1/documents?action=…` — the file-scoped document verbs
 * that carry no chat context, record no `chat_documents` row beyond the
 * cross-chat recents sentinel, and post no Librarian announcement. The Rust
 * engine already serves all seven (`api/types.rs:1565-1595`,
 * `api/documents.rs:1309-1721`, all `DocumentAccessContext::standalone()`).
 *
 * The seven `document*` request interfaces are pinned NAME-FOR-NAME against
 * `crates/quilltap-core/src/api/types.rs:1565-1595` (the `Request` enum
 * `#[serde(tag = "type", rename_all = "camelCase")]`, each standalone variant a
 * `#[serde(flatten)] body: Value`). They were FOLDED into `core-contract.ts`
 * at the workspace-tabs remainder round's unification (§W.3) and are
 * re-exported below; the lane-era `as unknown as CoreRequest` casts are gone.
 *
 * @module documents/standalone-wire
 */

import { inject, Injectable } from '@angular/core';

import { CoreClient } from '../core/core-client';
import type {
  AccessibleStoreDto,
  DocumentDeleteRequest,
  DocumentOpenRequest,
  DocumentReadRequest,
  DocumentRenameRequest,
  DocumentStoresRequest,
  DocumentWriteRequest,
  DocumentsRecentRequest,
  RecentDocumentDto,
  StandaloneScope,
} from '../core/core-contract';
import type { MountFilesResult } from './document-api';

// Request DTOs — FOLDED into `core-contract.ts` at the workspace-tabs
// remainder round's unification (§W.3, name-for-name vs `api/types.rs`);
// re-exported here so consumers and specs keep their import site.
export type {
  StandaloneScope,
  DocumentStoresRequest,
  DocumentsRecentRequest,
  DocumentOpenRequest,
  DocumentReadRequest,
  DocumentWriteRequest,
  DocumentRenameRequest,
  DocumentDeleteRequest,
} from '../core/core-contract';

export type StandaloneDocumentRequest =
  | DocumentStoresRequest
  | DocumentsRecentRequest
  | DocumentOpenRequest
  | DocumentReadRequest
  | DocumentWriteRequest
  | DocumentRenameRequest
  | DocumentDeleteRequest;

// --- Response DTOs (mirror the Rust handlers' JSON name-for-name) ------------

/** The `document` sub-object open/rename return (handler `api/documents.rs`). */
export interface StandaloneDocumentRecord {
  filePath: string;
  scope: StandaloneScope;
  mountPoint?: string | null;
  displayTitle: string;
}

/** `documentOpen` body — `{ document, content, mtime, isNew }`. */
export interface StandaloneOpenResult {
  document: StandaloneDocumentRecord;
  content: string;
  mtime?: number;
  isNew: boolean;
}

/** `documentRead` body — `{ content, mtime }`. */
export interface StandaloneReadResult {
  content: string;
  mtime?: number;
}

/** `documentRename` body — `{ document }`. */
export interface StandaloneRenameResult {
  document: StandaloneDocumentRecord;
}

/**
 * A write's outcome — a discriminated result so the 409 conflict (v4's HTTP 409
 * → the `conflict` ErrorKind) is explicit and never retried as a plain error.
 */
export type StandaloneWriteOutcome =
  | { kind: 'saved'; mtime?: number }
  /** The file changed out-of-band — reload, do not retry. */
  | { kind: 'conflict' }
  | { kind: 'error'; message: string };

/**
 * The standalone Document Mode dispatch client (v4 `postDocumentsAction` +
 * `DocumentPickerModal`'s standalone fetches). Root-provided (stateless
 * dispatch): the rail opener and the standalone view both reach it, mounted at
 * different levels of the tree.
 *
 * Response bodies are read DEFENSIVELY off the dispatch envelope's `data` (their
 * response `type` strings are lane B's, not pinned by the Shared contract — the
 * P4.6t / document-api precedent). The one place the envelope discriminator
 * matters is {@link write}, which must tell a write conflict apart from any
 * other failure.
 */
@Injectable({ providedIn: 'root' })
export class StandaloneDocumentApi {
  private readonly core = inject(CoreClient);

  /** The seven verbs are `CoreRequest` variants (folded at unification). */
  private async data(request: StandaloneDocumentRequest): Promise<Record<string, unknown>> {
    return this.core.dispatchData(request);
  }

  /** v4 standalone `accessible-stores` — always look-everywhere, no projectLibrary. */
  async fetchStores(): Promise<AccessibleStoreDto[]> {
    const data = await this.data({ type: 'documentStores' });
    return (data['stores'] as AccessibleStoreDto[] | undefined) ?? [];
  }

  /** v4 standalone `recent-documents` — cross-chat, project-scope rows filtered out. */
  async fetchRecent(): Promise<RecentDocumentDto[]> {
    const data = await this.data({ type: 'documentsRecent' });
    return (data['documents'] as RecentDocumentDto[] | undefined) ?? [];
  }

  /** v4 standalone `open-document` — opens (or creates) the file. */
  async open(params: {
    filePath?: string;
    title?: string;
    scope: StandaloneScope;
    mountPoint?: string | null;
    targetFolder?: string;
  }): Promise<StandaloneOpenResult> {
    const data = await this.data({
      type: 'documentOpen',
      filePath: params.filePath,
      title: params.title,
      scope: params.scope,
      mountPoint: params.mountPoint ?? undefined,
      targetFolder: params.targetFolder,
    });
    const doc = (data['document'] ?? {}) as StandaloneDocumentRecord;
    return {
      document: doc,
      content: (data['content'] as string | undefined) ?? '',
      mtime: data['mtime'] as number | undefined,
      isNew: Boolean(data['isNew']),
    };
  }

  /** v4 standalone `read-document` — bytes + mtime, never mutates. */
  async read(params: {
    filePath: string;
    scope: StandaloneScope;
    mountPoint?: string | null;
  }): Promise<StandaloneReadResult> {
    const data = await this.data({
      type: 'documentRead',
      filePath: params.filePath,
      scope: params.scope,
      mountPoint: params.mountPoint ?? undefined,
    });
    return {
      content: (data['content'] as string | undefined) ?? '',
      mtime: data['mtime'] as number | undefined,
    };
  }

  /** v4 standalone `write-document` — mtime-guarded; a 409 is the `conflict` kind. */
  async write(params: {
    filePath: string;
    scope: StandaloneScope;
    mountPoint?: string | null;
    content: string;
    mtime?: number;
  }): Promise<StandaloneWriteOutcome> {
    const resp = await this.core.dispatch({
      type: 'documentWrite',
      filePath: params.filePath,
      scope: params.scope,
      mountPoint: params.mountPoint ?? undefined,
      content: params.content,
      mtime: params.mtime,
    });

    if (resp.type === 'error') {
      return isConflictError(resp.data.kind, resp.data.message)
        ? { kind: 'conflict' }
        : { kind: 'error', message: resp.data.message };
    }
    const data = resp.data as { mtime?: number };
    return { kind: 'saved', mtime: data.mtime };
  }

  /** v4 standalone `rename-document` — relocates the file, returns the new record. */
  async rename(params: {
    filePath: string;
    scope: StandaloneScope;
    mountPoint?: string | null;
    newTitle: string;
  }): Promise<StandaloneRenameResult> {
    const data = await this.data({
      type: 'documentRename',
      filePath: params.filePath,
      scope: params.scope,
      mountPoint: params.mountPoint ?? undefined,
      newTitle: params.newTitle,
    });
    return { document: (data['document'] ?? {}) as StandaloneDocumentRecord };
  }

  /** v4 standalone `delete-document` — removes the underlying file. */
  async remove(params: {
    filePath: string;
    scope: StandaloneScope;
    mountPoint?: string | null;
  }): Promise<void> {
    await this.data({
      type: 'documentDelete',
      filePath: params.filePath,
      scope: params.scope,
      mountPoint: params.mountPoint ?? undefined,
    });
  }

  /**
   * List a mount point's files + folders for the standalone picker (lane A
   * `mountFilesList` — not chat-scoped, shared with the chat picker path).
   */
  async listMountFiles(mountPointId: string): Promise<MountFilesResult> {
    const data = await this.core.dispatchData({ type: 'mountFilesList', mountPointId });
    return {
      files: (data['files'] as MountFilesResult['files'] | undefined) ?? [],
      folders: Array.isArray(data['folders']) ? (data['folders'] as string[]) : [],
    };
  }
}

/**
 * Detect v4's write-conflict (HTTP 409) inside the dispatch error envelope
 * (identical to `document-api.ts`'s private helper — lane B maps `conflict()`
 * to the `conflict` ErrorKind; we also tolerate a `code`-style `CONFLICT` and
 * v4's conflict message defensively, reconciled at unification).
 */
function isConflictError(kind: string, message: string): boolean {
  if (kind === 'conflict') return true;
  const haystack = `${kind} ${message}`.toLowerCase();
  return haystack.includes('conflict') || haystack.includes('changed elsewhere');
}
