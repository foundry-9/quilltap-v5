import { inject, Injectable } from '@angular/core';

import { CoreClient } from '../core/core-client';
import { CoreDispatchError } from '../core/core-contract';
import type {
  AccessibleStoreDto,
  ActiveDocumentRecordDto,
  ChatDetail,
  DocumentScope,
  DocumentViewTarget,
  LibraryTargetDto,
  OpenDocumentResultDto,
  OpenDocumentResultWithViewer,
  ProjectLibraryTargetDto,
  RecentDocumentDto,
  WriteDocumentResultDto,
} from '../core/core-contract';

/** The document a chat has open, plus its loaded content + mtime (v4 `ActiveDocument`). */
export interface ActiveDocument {
  id: string;
  filePath: string;
  scope: DocumentScope;
  mountPoint?: string | null;
  displayTitle: string;
  content: string;
  mtime?: number;
  /**
   * Set for a store file Document Mode shows read-only instead of editing (an
   * image, a PDF, a `.docx`, any other binary) — v4 `ActiveDocument.viewer?`
   * (`5efe53ea7`; C14). The pane renders the viewer pane, never the editor.
   */
  viewer?: DocumentViewTarget;
}

/** The stores + optional project-library shortcut a chat can reach. */
export interface AccessibleStoresResult {
  stores: AccessibleStoreDto[];
  projectLibrary: ProjectLibraryTargetDto | null;
  /** Quilltap General, the shelf every chat shares (C14; a missing key reads `null`). */
  generalLibrary: LibraryTargetDto | null;
}

/** A document's bytes as of a read. */
export interface ReadDocumentResult {
  content: string;
  mtime?: number;
  /** Set for a file shown read-only (C14, P4.D380). */
  viewer?: DocumentViewTarget;
}

/** The outcome of a write — a discriminated result so 409 handling is explicit. */
export type WriteDocumentOutcome =
  | { kind: 'saved'; mtime?: number; librarianMessage: OpenDocumentResultDto['librarianMessage'] }
  /** The file changed out-of-band (v4's HTTP 409) — reload, do not retry. */
  | { kind: 'conflict' }
  | { kind: 'error'; message: string };

/**
 * The Document Mode dispatch client (v4 `documentModeApi.ts` + `toActiveDocument`).
 *
 * Every server body is read DEFENSIVELY off the dispatch envelope's `data`
 * (their response `type` strings belong to lane B and are not pinned by the
 * Shared contract — the P4.6t / settings precedent), so this reads only the
 * fields the vertical needs and tolerates the rest. The one place the envelope
 * discriminator matters is {@link writeDocument}, which must tell a write
 * conflict (v4's 409 → the `conflict` error kind after P4.6p) apart from any
 * other failure.
 *
 * Not `providedIn: 'root'` — provided at the Salon conversation alongside the
 * document store, one instance per screen.
 */
@Injectable()
export class DocumentApi {
  private readonly core = inject(CoreClient);

  /**
   * Coerce a `chat_documents` row + loaded content into an {@link ActiveDocument}.
   * A `viewer` target (C14) is spread FIRST, as v4 `toActiveDocument` does
   * (`documentModeApi.ts`, `5efe53ea7`).
   */
  toActiveDocument(
    record: ActiveDocumentRecordDto,
    content = '',
    mtime?: number,
    viewer?: DocumentViewTarget,
  ): ActiveDocument {
    return {
      ...(viewer ? { viewer } : {}),
      id: record.id,
      filePath: record.filePath,
      scope: record.scope,
      mountPoint: record.mountPoint,
      displayTitle: record.displayTitle || record.filePath,
      content,
      mtime,
    };
  }

  async fetchActiveDocument(chatId: string): Promise<ActiveDocumentRecordDto | null> {
    const data = await this.core.dispatchData({ type: 'chatActiveDocument', chatId });
    return (data['document'] as ActiveDocumentRecordDto | null) ?? null;
  }

  async fetchOpenDocuments(chatId: string): Promise<ActiveDocumentRecordDto[]> {
    const data = await this.core.dispatchData({ type: 'chatOpenDocuments', chatId });
    return (data['documents'] as ActiveDocumentRecordDto[] | undefined) ?? [];
  }

  async fetchRecentDocuments(chatId: string): Promise<RecentDocumentDto[]> {
    const data = await this.core.dispatchData({ type: 'chatRecentDocuments', chatId });
    return (data['documents'] as RecentDocumentDto[] | undefined) ?? [];
  }

  async fetchAccessibleStores(chatId: string, all: boolean): Promise<AccessibleStoresResult> {
    const data = await this.core.dispatchData({ type: 'chatAccessibleStores', chatId, all });
    return {
      stores: (data['stores'] as AccessibleStoreDto[] | undefined) ?? [],
      projectLibrary: (data['projectLibrary'] as ProjectLibraryTargetDto | null) ?? null,
      generalLibrary: (data['generalLibrary'] as LibraryTargetDto | null | undefined) ?? null,
    };
  }

  async openDocument(
    chatId: string,
    params: {
      filePath?: string;
      title?: string;
      scope?: DocumentScope;
      mountPoint?: string;
      mode: 'split' | 'focus';
      targetFolder?: string;
    },
  ): Promise<OpenDocumentResultWithViewer> {
    const data = await this.core.dispatchData({
      type: 'chatDocumentOpen',
      chatId,
      filePath: params.filePath,
      title: params.title,
      scope: params.scope ?? 'project',
      mountPoint: params.mountPoint,
      mode: params.mode,
      targetFolder: params.targetFolder,
    });
    return data as unknown as OpenDocumentResultWithViewer;
  }

  async closeDocument(chatId: string, chatDocumentId?: string): Promise<void> {
    // v4 fires-and-forgets the close (swallowing failures at the call site).
    await this.core.dispatch({ type: 'chatDocumentClose', chatId, chatDocumentId });
  }

  async readDocument(
    chatId: string,
    params: { filePath: string; scope: DocumentScope; mountPoint?: string | null },
  ): Promise<ReadDocumentResult> {
    const data = await this.core.dispatchData({
      type: 'chatDocumentRead',
      chatId,
      filePath: params.filePath,
      scope: params.scope,
      mountPoint: params.mountPoint,
    });
    const viewer = data['viewer'] as DocumentViewTarget | undefined;
    return {
      content: (data['content'] as string | undefined) ?? '',
      mtime: data['mtime'] as number | undefined,
      ...(viewer ? { viewer } : {}),
    };
  }

  async resolveDocument(
    chatId: string,
    params: { filePath: string; scope?: DocumentScope; mountPoint?: string | null },
  ): Promise<{ exists: boolean; kind: 'document' | 'image' | 'other' }> {
    const data = await this.core.dispatchData({
      type: 'chatDocumentResolve',
      chatId,
      filePath: params.filePath,
      scope: params.scope,
      mountPoint: params.mountPoint,
    });
    return {
      exists: Boolean(data['exists']),
      kind: (data['kind'] as 'document' | 'image' | 'other' | undefined) ?? 'other',
    };
  }

  async writeDocument(
    chatId: string,
    params: {
      filePath: string;
      scope: DocumentScope;
      mountPoint?: string | null;
      content: string;
      mtime?: number;
      diffContent?: string;
    },
  ): Promise<WriteDocumentOutcome> {
    const resp = await this.core.dispatch({
      type: 'chatDocumentWrite',
      chatId,
      filePath: params.filePath,
      scope: params.scope,
      mountPoint: params.mountPoint,
      content: params.content,
      mtime: params.mtime,
      diffContent: params.diffContent,
    });

    if (resp.type === 'error') {
      return isConflictError(resp.data.kind, resp.data.message)
        ? { kind: 'conflict' }
        : { kind: 'error', message: resp.data.message };
    }

    const data = resp.data as unknown as WriteDocumentResultDto;
    return { kind: 'saved', mtime: data.mtime, librarianMessage: data.librarianMessage ?? null };
  }

  async renameDocument(
    chatId: string,
    newTitle: string,
    chatDocumentId?: string,
  ): Promise<OpenDocumentResultDto> {
    const data = await this.core.dispatchData({
      type: 'chatDocumentRename',
      chatId,
      newTitle,
      chatDocumentId,
    });
    return data as unknown as OpenDocumentResultDto;
  }

  async deleteDocument(
    chatId: string,
    chatDocumentId?: string,
  ): Promise<{ librarianMessage: OpenDocumentResultDto['librarianMessage'] }> {
    const data = await this.core.dispatchData({
      type: 'chatDocumentDelete',
      chatId,
      chatDocumentId,
    });
    return { librarianMessage: (data['librarianMessage'] as OpenDocumentResultDto['librarianMessage']) ?? null };
  }

  /** Read the chat's persisted `documentMode` (v4 `fetchChatDocumentState`). */
  async fetchChatDocumentState(chatId: string): Promise<Pick<ChatDetail, 'documentMode'>> {
    const resp = await this.core.dispatchExpect({ type: 'chatGet', chatId }, 'chat');
    return { documentMode: resp.data.chat.documentMode };
  }

  /** Persist `documentMode` / `dividerPosition` onto the chat (v4 `persistChatDocumentState`). */
  async persistChatDocumentState(
    chatId: string,
    updates: { documentMode?: 'normal' | 'split' | 'focus'; dividerPosition?: number },
  ): Promise<void> {
    const resp = await this.core.dispatch({ type: 'chatUpdate', chatId, chat: updates });
    if (resp.type === 'error') {
      throw new CoreDispatchError(resp.data);
    }
  }

  /** List a mount point's files + folders for the picker (lane A `mountFilesList`). */
}

/**
 * Detect v4's write-conflict (HTTP 409) inside the dispatch error envelope. Lane
 * B maps `conflict()` to the `conflict` ErrorKind (P4.6p `+Conflict`); we also
 * tolerate a `code`-style `CONFLICT` and v4's conflict message as a defensive
 * fallback (reconciled at unification — the P4.6t precedent).
 */
function isConflictError(kind: string, message: string): boolean {
  if (kind === 'conflict') return true;
  const haystack = `${kind} ${message}`.toLowerCase();
  return haystack.includes('conflict') || haystack.includes('changed elsewhere');
}
