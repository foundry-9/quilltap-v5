/**
 * The FileBrowser data model + pure helpers (v4 `components/files/types.ts` +
 * `FileThumbnail`'s `getFileIcon` + `FilePreview/types.ts`'s `getPreviewType`,
 * plus the browser's subfolder-derivation useMemo). Kept pure and unit-tested so
 * the sort orders, the icon/label/preview routing, and the two-source subfolder
 * derivation are pinned exactly to v4.
 *
 * The wire row is {@link FileEntry} (v4 `serializeFileEntry`); this module works
 * against it directly (legacy general-files mode only — the mount-mode
 * `qt-file-manager` lives in `app/files/**` and is untouched this round).
 */

import type { FileEntry } from '../../core/core-contract';

/** A derived folder in the current directory (v4 `FolderInfo`). */
export interface FolderInfo {
  path: string;
  name: string;
  fileCount: number;
  /** Present when the row came from a DB folder (else derived from a file path). */
  id?: string;
  isDbFolder?: boolean;
}

/** A DB folder row as `filesFoldersList` returns it (v4 `DbFolder`). */
export interface DbFolder {
  id: string;
  path: string;
  name: string;
  parentFolderId: string | null;
  projectId: string | null;
}

export type SortField = 'name' | 'type' | 'size' | 'date';
export type SortDirection = 'asc' | 'desc';

export interface SortState {
  field: SortField;
  direction: SortDirection;
}

/** The type-specific preview renderer to use (v4 `PreviewType`). */
export type PreviewType = 'image' | 'pdf' | 'text' | 'unsupported';

/** Format a date for display (v4 `formatFileDate` — `toLocaleDateString`). */
export function formatFileDate(dateStr: string): string {
  return new Date(dateStr).toLocaleDateString();
}

/** The short file-type label from a MIME type (v4 `getFileTypeLabel`). */
export function getFileTypeLabel(mimeType: string): string {
  if (mimeType.startsWith('image/')) return 'Image';
  if (mimeType.startsWith('video/')) return 'Video';
  if (mimeType.startsWith('audio/')) return 'Audio';
  if (mimeType === 'application/pdf') return 'PDF';
  if (mimeType.includes('spreadsheet') || mimeType.includes('excel')) return 'Spreadsheet';
  if (mimeType.includes('document') || mimeType.includes('word')) return 'Document';
  if (mimeType.includes('json')) return 'JSON';
  if (mimeType.includes('javascript')) return 'JavaScript';
  if (mimeType.includes('typescript')) return 'TypeScript';
  if (mimeType.startsWith('text/')) return 'Text';
  return 'File';
}

/** The fallback emoji icon for a MIME type (v4 `FileThumbnail.getFileIcon`). */
export function getFileIcon(mimeType: string): string {
  if (mimeType.startsWith('image/')) return '\u{1F5BC}️';
  if (mimeType.startsWith('video/')) return '\u{1F3AC}';
  if (mimeType.startsWith('audio/')) return '\u{1F3B5}';
  if (mimeType === 'application/pdf') return '\u{1F4C4}';
  if (mimeType.includes('spreadsheet') || mimeType.includes('excel')) return '\u{1F4CA}';
  if (mimeType.includes('document') || mimeType.includes('word')) return '\u{1F4DD}';
  if (
    mimeType.includes('json') ||
    mimeType.includes('javascript') ||
    mimeType.includes('typescript')
  ) {
    return '\u{1F4DC}';
  }
  if (mimeType.startsWith('text/')) return '\u{1F4C3}';
  return '\u{1F4C1}';
}

/** Whether a MIME type has a cached server thumbnail (v4 `supportsThumbnail`). */
export function supportsThumbnail(mimeType: string): boolean {
  return ['image/jpeg', 'image/jpg', 'image/png', 'image/webp', 'image/gif'].includes(
    mimeType.toLowerCase(),
  );
}

/**
 * The preview renderer for a file (v4 `getPreviewType`): image, pdf, text (any
 * plain-text/code/json/xml/js/ts), else unsupported.
 */
export function getPreviewType(mimeType: string, isPlainText?: boolean): PreviewType {
  if (mimeType.startsWith('image/')) return 'image';
  if (mimeType === 'application/pdf') return 'pdf';
  if (
    isPlainText ||
    mimeType.startsWith('text/') ||
    mimeType === 'application/json' ||
    mimeType === 'application/javascript' ||
    mimeType === 'application/typescript' ||
    mimeType === 'application/xml'
  ) {
    return 'text';
  }
  return 'unsupported';
}

/** The display name for a file (v4's `originalFilename || filename` idiom). */
export function fileDisplayName(file: FileEntry): string {
  return file.originalFilename || file.filename || '';
}

/**
 * Sort files by field + direction (v4 `sortFiles`). Name compares on
 * `originalFilename || filename`; type on `mimeType`; size numerically; date on
 * `createdAt` epoch. Descending negates the comparison. Returns a new array.
 */
export function sortFiles(files: FileEntry[], sort: SortState): FileEntry[] {
  const sorted = [...files];
  sorted.sort((a, b) => {
    let comparison = 0;
    switch (sort.field) {
      case 'name':
        comparison = (a.originalFilename || a.filename || '').localeCompare(
          b.originalFilename || b.filename || '',
        );
        break;
      case 'type':
        comparison = a.mimeType.localeCompare(b.mimeType);
        break;
      case 'size':
        comparison = a.size - b.size;
        break;
      case 'date':
        comparison = new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime();
        break;
    }
    return sort.direction === 'asc' ? comparison : -comparison;
  });
  return sorted;
}

/** The files in the current folder, sorted (v4 `filteredFiles` useMemo). */
export function filesInFolder(
  files: FileEntry[],
  currentFolder: string,
  sort: SortState,
): FileEntry[] {
  const filtered = files.filter((file) => (file.folderPath || '/') === currentFolder);
  return sortFiles(filtered, sort);
}

/**
 * Derive the subfolders of `currentFolder` from BOTH the DB folder rows AND the
 * file-path prefixes, union'd into one list sorted by `name.localeCompare` (v4
 * `subfolders` useMemo, lines 379-436). A DB folder one level below current wins
 * (its `fileCount` counts files at or under its path); a path-prefix-only folder
 * accumulates a `+1` per file beneath it.
 */
export function deriveSubfolders(
  files: FileEntry[],
  dbFolders: DbFolder[],
  currentFolder: string,
): FolderInfo[] {
  const folderMap = new Map<string, FolderInfo>();

  for (const dbFolder of dbFolders) {
    const folderPath = dbFolder.path;
    if (folderPath === currentFolder) continue;

    const pathParts = folderPath.split('/').filter(Boolean);
    const currentParts = currentFolder.split('/').filter(Boolean);

    if (pathParts.length === currentParts.length + 1) {
      const parentPath = currentParts.length === 0 ? '/' : '/' + currentParts.join('/') + '/';
      if (parentPath === currentFolder) {
        const fileCount = files.filter((f) => {
          const fp = f.folderPath || '/';
          return fp === folderPath || fp.startsWith(folderPath);
        }).length;

        folderMap.set(folderPath, {
          path: folderPath,
          name: dbFolder.name,
          fileCount,
          id: dbFolder.id,
          isDbFolder: true,
        });
      }
    }
  }

  for (const file of files) {
    const fileFolderPath = file.folderPath || '/';
    if (fileFolderPath.startsWith(currentFolder) && fileFolderPath !== currentFolder) {
      const remainder = fileFolderPath.slice(currentFolder.length);
      const nextSlash = remainder.indexOf('/');
      if (nextSlash > 0) {
        const subfolder = currentFolder + remainder.slice(0, nextSlash + 1);
        const existing = folderMap.get(subfolder);

        if (existing) {
          if (!existing.isDbFolder) {
            existing.fileCount++;
          }
        } else {
          const name = subfolder.split('/').filter(Boolean).pop() || subfolder;
          folderMap.set(subfolder, {
            path: subfolder,
            name,
            fileCount: 1,
            isDbFolder: false,
          });
        }
      }
    }
  }

  return Array.from(folderMap.values()).sort((a, b) => a.name.localeCompare(b.name));
}

/** The parent folder of `currentFolder`, or itself at root (v4 `handleGoUp`). */
export function parentFolder(currentFolder: string): string {
  if (currentFolder === '/') return '/';
  const parts = currentFolder.split('/').filter(Boolean);
  parts.pop();
  return parts.length === 0 ? '/' : '/' + parts.join('/') + '/';
}

/** The breadcrumb segments for `currentFolder` (v4's inline `.map`). */
export function breadcrumbSegments(currentFolder: string): { label: string; path: string }[] {
  if (currentFolder === '/') return [];
  const parts = currentFolder.split('/').filter(Boolean);
  return parts.map((part, index) => ({
    label: part,
    path: '/' + parts.slice(0, index + 1).join('/') + '/',
  }));
}
