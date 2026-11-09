/**
 * The document-store browser's reads and writes over `CoreClient` — the
 * transport half of v4 `components/document-browser/useDocumentStoreSource.ts`
 * (`fetchDocumentStore`, `fetchDocumentListing`, the op table), `useStoreCatalogue`
 * (`DocumentDestinationDialog.tsx:29-46`), `useDocumentFileLinks`,
 * `DocumentViewer`'s two reads, `ProjectStoreBrowser`'s project read, and v4
 * `components/files/mountBlobUrl.ts`'s `buildMountFileRawUrl` (P4.D387 item 6).
 *
 * Every write is an existing or C13 dispatch verb (P4.D381's names); the
 * upload rides the existing multipart REST PUT (`uploadMountFile`). Each
 * failure throws an `Error` carrying the server's sentence (`CoreDispatchError`
 * is built FROM the error envelope), so a toast reads
 * `coreErrorMessage(error, fallback)` — v5's twin of v4's `apiErrorMessage`
 * (`core-client.ts:36-52`). The two GET actions (`?action=links`,
 * `?action=render-html`) ride dispatch too: P4.D381 landed `mountFileLinks` /
 * `mountFileRenderHtml` beside the REST legs (ruling L-2's switch, recorded).
 *
 * @module document-browser/document-store.api
 */

import type { CoreClient } from '../core/core-client';
import type {
  DocMountPointDto,
  MountFileLinksResult,
  MountFileReadResult,
  MountFilesBrowserListing,
  MountPointCapabilities,
  ProjectMountPointsResult,
} from '../core/core-contract';
import { buildMountFileItemUrl } from '../screens/scriptorium/scriptorium.api';
import type { BrowserStore } from './document-actions';
import { deriveMountCapabilities } from './mount-capabilities';

/** A store's record as `mountPointGet` answers it (with its capabilities). */
export type DocumentStoreRecord = DocMountPointDto &
  BrowserStore & { capabilities?: MountPointCapabilities };

/** A store in the destination catalogue (v4 `StoreSummary`). */
export interface StoreSummary extends BrowserStore {
  name: string;
  storeType: string;
}

/** v4 `fetchDocumentStore` — one store's record. */
export async function fetchDocumentStore(
  core: CoreClient,
  id: string,
): Promise<DocumentStoreRecord> {
  const data = await core.dispatchData({ type: 'mountPointGet', mountPointId: id });
  return data['mountPoint'] as DocumentStoreRecord;
}

/** v4 `fetchDocumentListing` — one store's browser listing. */
export async function fetchDocumentListing(
  core: CoreClient,
  id: string,
): Promise<MountFilesBrowserListing> {
  const data = await core.dispatchData({ type: 'mountFilesList', mountPointId: id });
  return {
    files: (data['files'] as MountFilesBrowserListing['files'] | undefined) ?? [],
    folders: (data['folders'] as string[] | undefined) ?? [],
  };
}

/**
 * v4 `useStoreCatalogue`'s `select`: every store, its capabilities derived
 * (`deriveMountCapabilities`), sorted by name, case-insensitively.
 */
export async function fetchStoreCatalogue(core: CoreClient): Promise<StoreSummary[]> {
  const data = await core.dispatchData({ type: 'mountPointList' });
  const rows = (data['mountPoints'] as DocMountPointDto[] | undefined) ?? [];
  return rows
    .map((mp) => ({
      id: mp.id,
      name: mp.name,
      mountType: mp.mountType as BrowserStore['mountType'],
      storeType: mp.storeType,
      enabled: mp.enabled,
      capabilities: deriveMountCapabilities(mp),
    }))
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
}

/** v4 `useDocumentFileLinks`' read — where one file appears. */
export async function fetchDocumentFileLinks(
  core: CoreClient,
  mountPointId: string,
  path: string,
): Promise<MountFileLinksResult> {
  const data = await core.dispatchData({ type: 'mountFileLinks', mountPointId, path });
  return {
    linkGroupId: (data['linkGroupId'] as string | null | undefined) ?? null,
    locations: (data['locations'] as MountFileLinksResult['locations'] | undefined) ?? [],
  };
}

/** `DocumentViewer`'s text read (`?encoding=utf-8`): a text file stored as a blob reads as base64 by default. */
export async function fetchDocumentText(
  core: CoreClient,
  mountPointId: string,
  path: string,
): Promise<string> {
  const data = await core.dispatchData({
    type: 'mountFileRead',
    mountPointId,
    path,
    encoding: 'utf-8',
  });
  return (data as unknown as MountFileReadResult).content;
}

/** `DocumentViewer`'s `.docx` read (`?action=render-html`; v5 refuses `.docx` with a 422, R-E). */
export async function fetchDocumentRenderedHtml(
  core: CoreClient,
  mountPointId: string,
  path: string,
): Promise<string> {
  const data = await core.dispatchData({ type: 'mountFileRenderHtml', mountPointId, path });
  return data['html'] as string;
}

/** `ProjectStoreBrowser`'s read — a project's linked stores and its official one (C14). */
export async function fetchProjectStores(
  core: CoreClient,
  projectId: string,
): Promise<ProjectMountPointsResult> {
  const data = await core.dispatchData({ type: 'projectMountPointList', projectId });
  return {
    mountPoints: (data['mountPoints'] as DocMountPointDto[] | undefined) ?? [],
    officialMountPointId: (data['officialMountPointId'] as string | null | undefined) ?? null,
  };
}

/** v4 `buildMountFileRawUrl` (`mountBlobUrl.ts:29-32`) — the item route's raw bytes, served inline. */
export function buildMountFileRawUrl(mountPointId: string, relativePath: string): string {
  return `${buildMountFileItemUrl(mountPointId, relativePath)}?raw=1`;
}

/** Where a file op sends something: a store and a full path in it (v4 `FileDestination`). */
export interface FileDestination {
  mountPointId: string;
  path: string;
}

// The write verbs (v4 `useDocumentStoreSource.ts:130-205`), one per op.

export async function createDocumentFolder(
  core: CoreClient,
  id: string,
  path: string,
): Promise<void> {
  await core.dispatchData({ type: 'mountFolderCreate', mountPointId: id, path });
}

/** Rename and move share `move-folder`: a move adds `destMountPointId`. */
export async function moveDocumentFolder(
  core: CoreClient,
  id: string,
  fromPath: string,
  toPath: string,
  destMountPointId?: string,
): Promise<void> {
  await core.dispatchData({
    type: 'mountFolderMove',
    mountPointId: id,
    fromPath,
    toPath,
    ...(destMountPointId === undefined ? {} : { destMountPointId }),
  });
}

export async function copyDocumentFolder(
  core: CoreClient,
  id: string,
  fromPath: string,
  dest: FileDestination,
): Promise<void> {
  await core.dispatchData({
    type: 'mountFolderCopy',
    mountPointId: id,
    fromPath,
    toPath: dest.path,
    destMountPointId: dest.mountPointId,
  });
}

export async function deleteDocumentFolder(
  core: CoreClient,
  id: string,
  path: string,
): Promise<{ deletedFiles?: number }> {
  const data = await core.dispatchData({
    type: 'mountFolderDelete',
    mountPointId: id,
    path,
    recursive: true,
  });
  return data as { deletedFiles?: number };
}

export async function transferDocumentFile(
  core: CoreClient,
  type: 'mountFileMove' | 'mountFileCopy' | 'mountFileLink',
  id: string,
  sourcePath: string,
  dest: FileDestination,
): Promise<void> {
  await core.dispatchData({
    type,
    mountPointId: id,
    sourcePath,
    destMountPointId: dest.mountPointId,
    destPath: dest.path,
  });
}

export async function unlinkDocumentFile(
  core: CoreClient,
  id: string,
  path: string,
): Promise<void> {
  await core.dispatchData({ type: 'mountFileUnlink', mountPointId: id, path });
}

export async function renameDocumentFile(
  core: CoreClient,
  id: string,
  path: string,
  newPath: string,
): Promise<void> {
  await core.dispatchData({ type: 'mountFileUpdate', mountPointId: id, path, rename: newPath });
}

export async function describeDocumentFile(
  core: CoreClient,
  id: string,
  path: string,
  description: string,
): Promise<void> {
  await core.dispatchData({ type: 'mountFileUpdate', mountPointId: id, path, description });
}

export async function deleteDocumentFile(
  core: CoreClient,
  id: string,
  path: string,
): Promise<void> {
  await core.dispatchData({ type: 'mountFileDelete', mountPointId: id, path });
}
