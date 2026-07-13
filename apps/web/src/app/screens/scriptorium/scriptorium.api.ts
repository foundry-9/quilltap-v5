/**
 * The Scriptorium data layer (v4 `app/scriptorium/hooks/*` + `types.ts`): the
 * view-model types plus thin `CoreClient` dispatch helpers for the store list,
 * the routed detail, and the FileTable's per-file ops. Reads go through
 * {@link CoreClient.dispatchData} (raw `data`) — the P4.6v/P4.6y Shared contract
 * pins the response bodies via its differentials.
 */

import type { CoreClient } from '../../core/core-client';
import type {
  BrowseDirectoryResult,
  DocMountPointDto,
  MountPointCreateBag,
  MountPointUpdateBag,
} from '../../core/core-contract';

/** A document store (v4 `DocumentStore`) — the enriched mount-point row. */
export type DocumentStore = DocMountPointDto;

/** The create bag (v4 `CreateDocumentStoreData`; `basePath` '' for database). */
export type CreateDocumentStoreData = MountPointCreateBag;
/** The PATCH bag (v4 `UpdateDocumentStoreData`). */
export type UpdateDocumentStoreData = MountPointUpdateBag;

/** A row in a store's file index (v4 `DocumentStoreFile`). */
export interface DocumentStoreFile {
  id: string;
  mountPointId: string;
  relativePath: string;
  fileName: string;
  fileType: 'pdf' | 'docx' | 'markdown' | 'txt' | 'json' | 'jsonl' | 'blob';
  sha256: string;
  fileSizeBytes: number;
  lastModified: string;
  conversionStatus: 'pending' | 'converted' | 'failed' | 'skipped';
  conversionError: string | null;
  plainTextLength: number | null;
  chunkCount: number;
  createdAt: string;
  updatedAt: string;
}

/** A blob-backed file's detail (v4 `DocumentStoreBlob`). */
export interface DocumentStoreBlob {
  id: string;
  mountPointId: string;
  relativePath: string;
  originalFileName: string;
  originalMimeType: string;
  storedMimeType: string;
  sizeBytes: number;
  sha256: string;
  description: string;
  descriptionUpdatedAt: string | null;
  extractedText: string | null;
  extractedTextSha256: string | null;
  extractionStatus: 'none' | 'pending' | 'converted' | 'failed' | 'skipped';
  extractionError: string | null;
  createdAt: string;
  updatedAt: string;
}

/** The scan result (v4 `ScanResult`). */
export interface ScanResult {
  filesScanned: number;
  filesNew: number;
  filesModified: number;
  filesDeleted: number;
  chunksCreated: number;
  errors: Array<{ file: string; error: string }>;
}

/** The create result (`{mountPoint, warning?}`). */
export interface CreateStoreResult {
  mountPoint: DocumentStore;
  warning?: string;
}

/** The scan result envelope (`{scanResult, embeddingJobsEnqueued}`). */
export interface ScanStoreResult {
  scanResult: ScanResult;
  embeddingJobsEnqueued: number;
}

// --- Store CRUD + actions ---------------------------------------------------

export async function fetchStores(core: CoreClient): Promise<DocumentStore[]> {
  const data = await core.dispatchData({ type: 'mountPointList' });
  return (data['mountPoints'] as DocumentStore[]) ?? [];
}

export async function fetchStore(core: CoreClient, id: string): Promise<DocumentStore> {
  const data = await core.dispatchData({ type: 'mountPointGet', mountPointId: id });
  return data['mountPoint'] as DocumentStore;
}

export async function createStore(
  core: CoreClient,
  data: CreateDocumentStoreData,
): Promise<CreateStoreResult> {
  const body = await core.dispatchData({ type: 'mountPointCreate', mountPoint: data });
  return {
    mountPoint: body['mountPoint'] as DocumentStore,
    warning: body['warning'] as string | undefined,
  };
}

export async function updateStore(
  core: CoreClient,
  id: string,
  data: UpdateDocumentStoreData,
): Promise<DocumentStore> {
  const body = await core.dispatchData({
    type: 'mountPointUpdate',
    mountPointId: id,
    mountPoint: data,
  });
  return body['mountPoint'] as DocumentStore;
}

export async function deleteStore(core: CoreClient, id: string): Promise<void> {
  await core.dispatchData({ type: 'mountPointDelete', mountPointId: id });
}

export async function scanStore(core: CoreClient, id: string): Promise<ScanStoreResult> {
  const body = await core.dispatchData({ type: 'mountScan', mountPointId: id });
  return {
    scanResult: body['scanResult'] as ScanResult,
    embeddingJobsEnqueued: (body['embeddingJobsEnqueued'] as number) ?? 0,
  };
}

/** REFUSAL-ARMED live (P4.6y): the server answers the typed refusal, surfaced loudly. */
export async function convertStore(core: CoreClient, id: string): Promise<void> {
  await core.dispatchData({ type: 'mountConvert', mountPointId: id });
}

/** REFUSAL-ARMED live (P4.6y). */
export async function deconvertStore(
  core: CoreClient,
  id: string,
  targetPath: string,
): Promise<void> {
  await core.dispatchData({ type: 'mountDeconvert', mountPointId: id, targetPath });
}

export async function fetchStoreFiles(
  core: CoreClient,
  id: string,
): Promise<DocumentStoreFile[]> {
  const data = await core.dispatchData({ type: 'mountFilesList', mountPointId: id });
  return (data['files'] as DocumentStoreFile[]) ?? [];
}

// --- The DirectoryPicker ----------------------------------------------------

export async function browseDirectory(
  core: CoreClient,
  path?: string,
): Promise<BrowseDirectoryResult> {
  const data = await core.dispatchData(
    path ? { type: 'systemBrowseDirectory', path } : { type: 'systemBrowseDirectory' },
  );
  return data as unknown as BrowseDirectoryResult;
}

// --- FileTable per-file ops -------------------------------------------------

export async function listMountBlobs(
  core: CoreClient,
  mountPointId: string,
): Promise<DocumentStoreBlob[]> {
  const data = await core.dispatchData({ type: 'mountBlobsList', mountPointId });
  return (data['blobs'] as DocumentStoreBlob[]) ?? [];
}

export async function deleteMountFile(
  core: CoreClient,
  mountPointId: string,
  path: string,
): Promise<void> {
  await core.dispatchData({ type: 'mountFileDelete', mountPointId, path });
}

export async function describeMountFile(
  core: CoreClient,
  mountPointId: string,
  path: string,
  description: string,
): Promise<void> {
  await core.dispatchData({ type: 'mountFileUpdate', mountPointId, path, description });
}

/**
 * Upload a file to a database-backed store (v4 FileTable: multipart PUT to the
 * item route, the byte-preserving `storeMountFile` ingest — the P4.6y
 * `mount_file_put` web-edge leg). Not a `/api/dispatch` verb.
 */
export async function uploadMountFile(
  mountPointId: string,
  relativePath: string,
  file: File,
): Promise<void> {
  const form = new FormData();
  form.set('file', file);
  const res = await fetch(buildMountFileItemUrl(mountPointId, relativePath), {
    method: 'PUT',
    body: form,
  });
  if (!res.ok) {
    let message = `Upload failed (${res.status})`;
    try {
      const body = (await res.json()) as { error?: string };
      if (body?.error) {
        message = body.error;
      }
    } catch {
      /* keep the status message */
    }
    throw new Error(message);
  }
}

// --- URL helpers (v4 `components/files/mountBlobUrl.ts`, the pieces we need) --

/** Encode each path segment once (both catch-all routes decode once per segment). */
export function encodeMountBlobPath(relativePath: string): string {
  return relativePath.split('/').map(encodeURIComponent).join('/');
}

/** The canonical item route (`/files/...`) — write/delete/upload. */
export function buildMountFileItemUrl(mountPointId: string, relativePath: string): string {
  return `/api/v1/mount-points/${encodeURIComponent(mountPointId)}/files/${encodeMountBlobPath(relativePath)}`;
}

/** The persisted byte-serving route (`/blobs/...`) — image previews + downloads. */
export function buildMountBlobUrl(mountPointId: string, relativePath: string): string {
  return `/api/v1/mount-points/${encodeURIComponent(mountPointId)}/blobs/${encodeMountBlobPath(relativePath)}`;
}
