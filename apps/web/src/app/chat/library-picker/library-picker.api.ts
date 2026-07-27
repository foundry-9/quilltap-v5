/**
 * The Library file picker's data layer (v4 `LibraryFilePickerModal.tsx`'s four
 * scope reads, its two browse reads and its two write legs).
 *
 * The two writes are **raw REST**, exactly as v4 issues them: the picker POSTs
 * `…/files?action=link` and `…/files?action=attach-mount-file` rather than
 * dispatching a verb. `ChatAttachMountFile` is mirrored into `core-contract.ts`
 * name-for-name so the §1 diff stays clean, but the modal deliberately does not
 * consume it — the REST leg is the surface v4 exercises, so it is the surface
 * this port exercises.
 *
 * Every URL goes through `apiUrl()` (the P4.7b origin resolver); none is inlined.
 */

import { apiUrl } from '../../core/api-url';
import type { CoreClient } from '../../core/core-client';
import type { AlbumOption, FolderEntry, ProjectSummary } from '../../core/core-contract';
import type { UploadedChatFile } from '../chat-files.api';
import {
  documentStoreFileToPickerFile,
  type PickerFile,
  type PickerGalleryEntry,
  type PickerStoreSummary,
} from './library-picker.model';
import type { DocumentStoreFile } from '../../screens/scriptorium/scriptorium.api';

/** Query keys for the picker's reads (kept off the other verticals' namespaces). */
export const libraryPickerKeys = {
  scopes: (chatId: string) => ['library-picker', 'scopes', chatId] as const,
  legacyFiles: (projectId: string | null) => ['library-picker', 'legacy', projectId] as const,
  mountFiles: (mountPointId: string) => ['library-picker', 'mount', mountPointId] as const,
  projectStore: (projectId: string) => ['library-picker', 'project-store', projectId] as const,
  gallery: (characterId: string | null) => ['library-picker', 'gallery', characterId] as const,
};

/** The step-1 payload: everything the scope list renders (v4's four queries). */
export interface PickerScopes {
  projects: ProjectSummary[];
  docStores: PickerStoreSummary[];
  groupStores: PickerStoreSummary[];
  albums: AlbumOption[];
}

/** v4 `GET /api/v1/projects` — the Projects section. */
export async function fetchPickerProjects(core: CoreClient): Promise<ProjectSummary[]> {
  const data = await core.dispatchData({ type: 'projectList' });
  return (data['projects'] as ProjectSummary[]) ?? [];
}

/** v4 `GET /api/v1/mount-points` — the Document Stores section (unfiltered here). */
export async function fetchPickerMountPoints(core: CoreClient): Promise<PickerStoreSummary[]> {
  const data = await core.dispatchData({ type: 'mountPointList' });
  return (data['mountPoints'] as PickerStoreSummary[]) ?? [];
}

/** v4 `…/chats/{id}?action=group-stores` — the Group Files section. */
export async function fetchPickerGroupStores(
  core: CoreClient,
  chatId: string,
): Promise<PickerStoreSummary[]> {
  const data = await core.dispatchData({ type: 'chatGroupStores', chatId });
  return (data['stores'] as PickerStoreSummary[]) ?? [];
}

/**
 * v4 `…/chats/{id}?action=photo-albums` — fetched ONLY to resolve the gallery's
 * display name and target character. Albums are never pickable scopes.
 */
export async function fetchPickerAlbums(core: CoreClient, chatId: string): Promise<AlbumOption[]> {
  const data = await core.dispatchData({ type: 'chatPhotoAlbums', chatId });
  return (data['albums'] as AlbumOption[]) ?? [];
}

/**
 * v4 `GET /api/v1/projects/{id}/mount-points` — the FileBrowser's auto-resolve
 * of a project's linked stores (`FileBrowser.tsx:242`). The caller narrows the
 * list with `pickPrimaryProjectStore`; a failure falls back to legacy mode, since
 * one missing link shouldn't take down the browser (v4's own comment).
 */
export async function fetchProjectStores(
  core: CoreClient,
  projectId: string,
): Promise<PickerStoreSummary[]> {
  const data = await core.dispatchData({ type: 'projectMountPointList', projectId });
  return (data['mountPoints'] as PickerStoreSummary[]) ?? [];
}

/** Legacy-mode browse (v4 `/files?filter=general` or project `?action=list-files`). */
export async function fetchLegacyFiles(
  core: CoreClient,
  projectId: string | null,
): Promise<{ files: PickerFile[]; folders: FolderEntry[] }> {
  const [filesData, foldersData] = await Promise.all([
    core.dispatchData(
      projectId ? { type: 'filesList', projectId } : { type: 'filesList', filter: 'general' },
    ),
    core.dispatchData(
      projectId ? { type: 'filesFoldersList', projectId } : { type: 'filesFoldersList' },
    ),
  ]);
  return {
    files: (filesData['files'] as PickerFile[]) ?? [],
    folders: (foldersData['folders'] as FolderEntry[]) ?? [],
  };
}

/** Mount-mode browse (v4 `GET /api/v1/mount-points/{id}/files`, rows translated). */
export async function fetchMountFiles(
  core: CoreClient,
  mountPointId: string,
): Promise<PickerFile[]> {
  const data = await core.dispatchData({ type: 'mountFilesList', mountPointId });
  const rows = (data['files'] as DocumentStoreFile[]) ?? [];
  return rows.map(documentStoreFileToPickerFile);
}

/** The gallery grid's read: the persona's vault photos, else the global gallery. */
export async function fetchPickerGallery(
  core: CoreClient,
  characterId: string | null,
): Promise<PickerGalleryEntry[]> {
  const data = characterId
    ? await core.dispatchData({ type: 'characterPhotoList', characterId, limit: 200 })
    : await core.dispatchData({ type: 'photoGalleryList', limit: 200 });
  return (data['entries'] as PickerGalleryEntry[]) ?? [];
}

// ---------------------------------------------------------------------------
// The two write legs (raw REST — v4 `fetch`es both)
// ---------------------------------------------------------------------------

/**
 * Pull v4's error message off a failed picker write. v4 reads `errorData.error`
 * and falls back to `HTTP {status}: {statusText}` when the body isn't JSON
 * (`:183-191`, `:257-265`); the fallback message differs per leg, so it is the
 * caller's.
 */
async function pickerWriteError(res: Response, fallback: string): Promise<Error> {
  let errorMessage = fallback;
  try {
    const errorData = (await res.json()) as { error?: string };
    errorMessage = errorData.error || errorMessage;
  } catch {
    errorMessage = `HTTP ${res.status}: ${res.statusText}`;
  }
  return new Error(errorMessage);
}

/**
 * Link a legacy library file to the chat (v4 `:248-255`). The response `file` is
 * what the parent pushes into the composer's pending-attachment tray.
 */
export async function linkLibraryFile(
  chatId: string,
  fileId: string,
): Promise<UploadedChatFile> {
  const res = await fetch(apiUrl(`/api/v1/chats/${encodeURIComponent(chatId)}/files?action=link`), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ fileId }),
  });
  if (!res.ok) {
    throw await pickerWriteError(res, 'Failed to link file');
  }
  const data = (await res.json()) as { file: UploadedChatFile };
  const linkedFile = data.file;
  return {
    id: linkedFile.id,
    filename: linkedFile.filename,
    filepath: linkedFile.filepath,
    mimeType: linkedFile.mimeType,
    url: linkedFile.url,
  };
}

/**
 * Pin a document-store file to the chat via a Librarian attachment announcement
 * (v4 `attachMountFile`, `:173-200`). There is no composer-tray hand-off — the
 * announcement is already a transcript message.
 */
export async function attachMountFile(
  chatId: string,
  mountPointId: string,
  relativePath: string,
): Promise<void> {
  const res = await fetch(
    apiUrl(`/api/v1/chats/${encodeURIComponent(chatId)}/files?action=attach-mount-file`),
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ mountPointId, relativePath }),
    },
  );
  if (!res.ok) {
    throw await pickerWriteError(res, 'Failed to attach document');
  }
}
