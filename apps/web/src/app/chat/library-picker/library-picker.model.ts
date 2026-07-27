/**
 * The pure model behind the Library file picker (v4
 * `components/chat/LibraryFilePickerModal.tsx` + the pieces of
 * `components/files/FileBrowser.tsx` its browse step depends on).
 *
 * Everything here is a pure function over already-fetched rows, so the scope
 * filter, the persona-album fallback chain, the mount-vs-legacy discriminator
 * and the document-store row translation are pinned by unit tests rather than by
 * reading the component.
 *
 * `pickPrimaryProjectStore` / `isProjectOwnStoreName` are transcribed from v4's
 * **client-safe** `lib/mount-index/project-store-naming.ts` — v4 keeps that split
 * deliberately (anything needing `getRepositories()` must not live beside it, or
 * the FileBrowser bundle picks up `child_process`), and the same split holds
 * here: this module imports nothing but types.
 */

import type { FileEntry } from '../../core/core-contract';
import type { DocumentStoreFile } from '../../screens/scriptorium/scriptorium.api';

/** A mount point as the picker's three scope reads project it (v4 `MountPointSummary`). */
export interface PickerStoreSummary {
  id: string;
  name: string;
  mountType: 'filesystem' | 'obsidian' | 'database';
  storeType: 'documents' | 'character';
  enabled: boolean;
}

/** One album option (v4 `PhotoAlbumOption`) — the picker reads only the persona bits. */
export interface PickerAlbumOption {
  mountPointId: string;
  name: string;
  kind: 'character' | 'project' | 'document-store' | 'general';
  characterId?: string;
  participantId?: string;
  isUserCharacter?: boolean;
  isDefault?: boolean;
}

/** One gallery photo as both photo reads project it (v4 `GalleryEntry`). */
export interface PickerGalleryEntry {
  linkId: string;
  mountPointId: string;
  relativePath: string;
  fileName: string;
  blobUrl: string;
  mimeType: string;
  caption: string | null;
  keptAt: string;
  generationPromptExcerpt?: string;
}

/** The picker's own view of a browsable file (v4 `components/files/types.ts` `FileInfo`). */
export interface PickerFile extends FileEntry {
  /** Present only for a document-store row — the mount-file discriminator's other half. */
  mountPointId?: string;
  relativePath?: string;
}

// ---------------------------------------------------------------------------
// Step 1 — the scope list
// ---------------------------------------------------------------------------

/**
 * The pickable document stores (v4 `:133-135`). Database-backed stores that
 * aren't private character vaults — those are managed via the character
 * optimizer / Aurora tab and are conceptually off-limits for the human composer.
 */
export function pickableDocStores(stores: readonly PickerStoreSummary[]): PickerStoreSummary[] {
  return stores.filter(
    (mp) => mp.enabled && mp.mountType === 'database' && mp.storeType !== 'character',
  );
}

/**
 * The chat's user-persona album (v4 `:156-159`): the DEFAULT user-character
 * album if there is one, else the first user-character album, else null. Only
 * its `name` (the gallery's title) and `characterId` (which gallery endpoint to
 * read) are used — albums are never pickable scopes in their own right.
 */
export function resolveUserPersonaAlbum(
  albums: readonly PickerAlbumOption[],
): PickerAlbumOption | null {
  return (
    albums.find((a) => a.kind === 'character' && a.isUserCharacter && a.isDefault) ??
    albums.find((a) => a.kind === 'character' && a.isUserCharacter) ??
    null
  );
}

/** v4's `PROJECT_OWN_STORE_NAME_PREFIX` (`project-store-naming.ts:21`). */
export const PROJECT_OWN_STORE_NAME_PREFIX = 'Project Files: ';

/** v4 `isProjectOwnStoreName` — the Stage 1 migration's naming convention. */
export function isProjectOwnStoreName(name: string | null | undefined): boolean {
  return typeof name === 'string' && name.startsWith(PROJECT_OWN_STORE_NAME_PREFIX);
}

/**
 * v4 `pickPrimaryProjectStore` — select a project's "own" document store from
 * its linked stores. Prefers a database-backed documents store named by the
 * migration's "Project Files: …" convention; falls back to the first eligible
 * store, which preserves behavior for projects linked only by hand and for
 * projects whose auto-created store was renamed. Filesystem / obsidian mounts
 * and character stores never participate.
 */
export function pickPrimaryProjectStore<
  T extends { name: string; mountType: string; storeType?: string },
>(stores: readonly T[]): T | null {
  const eligible = stores.filter(
    (s) => s.mountType === 'database' && (s.storeType ?? 'documents') === 'documents',
  );
  if (eligible.length === 0) return null;
  return eligible.find((s) => isProjectOwnStoreName(s.name)) ?? eligible[0];
}

// ---------------------------------------------------------------------------
// Step 2 — the mount-mode row translation (v4 FileBrowser `:74-163`)
// ---------------------------------------------------------------------------

/** v4 `deriveMimeTypeFromName` — the blob-row extension table, case for case. */
export function deriveMimeTypeFromName(fileName: string): string {
  const ext = fileName.toLowerCase().split('.').pop() || '';
  switch (ext) {
    case 'webp':
      return 'image/webp';
    case 'png':
      return 'image/png';
    case 'jpg':
    case 'jpeg':
      return 'image/jpeg';
    case 'gif':
      return 'image/gif';
    case 'svg':
      return 'image/svg+xml';
    case 'heic':
      return 'image/heic';
    case 'heif':
      return 'image/heif';
    case 'avif':
      return 'image/avif';
    case 'tiff':
    case 'tif':
      return 'image/tiff';
    case 'mp4':
      return 'video/mp4';
    case 'mov':
      return 'video/quicktime';
    case 'webm':
      return 'video/webm';
    case 'mp3':
      return 'audio/mpeg';
    case 'wav':
      return 'audio/wav';
    case 'ogg':
      return 'audio/ogg';
    case 'txt':
      return 'text/plain';
    case 'md':
    case 'markdown':
      return 'text/markdown';
    case 'html':
    case 'htm':
      return 'text/html';
    case 'json':
      return 'application/json';
    case 'jsonl':
    case 'ndjson':
      return 'application/jsonl';
    case 'csv':
      return 'text/csv';
    case 'zip':
      return 'application/zip';
    default:
      return 'application/octet-stream';
  }
}

/** v4 `mimeTypeForDocumentStoreFile` — the `fileType` enum, blobs by extension. */
export function mimeTypeForDocumentStoreFile(row: DocumentStoreFile): string {
  switch (row.fileType) {
    case 'pdf':
      return 'application/pdf';
    case 'docx':
      return 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
    case 'markdown':
      return 'text/markdown';
    case 'txt':
      return 'text/plain';
    case 'json':
      return 'application/json';
    case 'jsonl':
      return 'application/jsonl';
    case 'blob':
      return deriveMimeTypeFromName(row.fileName);
    default:
      return deriveMimeTypeFromName(row.fileName);
  }
}

/**
 * v4 `folderPathFromRelativePath` — translate a mount-point relativePath
 * ("images/foo.webp") into the legacy folderPath convention the browse panel's
 * folder model works in ("/images/").
 */
export function folderPathFromRelativePath(relativePath: string): string {
  const lastSlash = relativePath.lastIndexOf('/');
  if (lastSlash < 0) return '/';
  return `/${relativePath.slice(0, lastSlash)}/`;
}

/** v4 `documentStoreFileToFileInfo` — a store row as the browse panel sees it. */
export function documentStoreFileToPickerFile(row: DocumentStoreFile): PickerFile {
  return {
    id: row.id,
    originalFilename: row.fileName,
    filename: row.fileName,
    mimeType: mimeTypeForDocumentStoreFile(row),
    size: row.fileSizeBytes,
    // Category isn't tracked in doc_mount_files. Use a harmless default (v4).
    category: row.fileType === 'blob' ? 'binary' : 'document',
    description: null,
    projectId: null,
    folderPath: folderPathFromRelativePath(row.relativePath),
    filepath: '',
    fileStatus: 'ok',
    createdAt: row.createdAt,
    updatedAt: row.lastModified || row.updatedAt,
    mountPointId: row.mountPointId,
    relativePath: row.relativePath,
  };
}

// ---------------------------------------------------------------------------
// The pick discriminator (v4 `handleFileClick:237`)
// ---------------------------------------------------------------------------

/**
 * Whether a picked row is a Scriptorium document-store file (v4 `:237`).
 * Store files don't live in the legacy `files` table, so the link endpoint
 * can't take their id — they go through the attach-mount-file announcement
 * instead. BOTH fields must be present, exactly as v4 tests it.
 */
export function isMountFile(file: PickerFile): boolean {
  return !!file.mountPointId && !!file.relativePath;
}

/** v4's `originalFilename || filename || 'file'` display fallback (`:238`). */
export function pickedFileName(file: PickerFile): string {
  return file.originalFilename || file.filename || 'file';
}
