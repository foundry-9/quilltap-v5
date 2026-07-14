import { describe, expect, it } from 'vitest';

import type { FileEntry } from '../../core/core-contract';
import {
  breadcrumbSegments,
  deriveSubfolders,
  filesInFolder,
  getFileTypeLabel,
  getPreviewType,
  parentFolder,
  sortFiles,
  type DbFolder,
  type SortState,
} from './file-model';

function file(partial: Partial<FileEntry> & { id: string }): FileEntry {
  return {
    originalFilename: partial.id,
    filename: partial.id,
    mimeType: 'text/plain',
    size: 0,
    category: 'general',
    description: null,
    projectId: null,
    folderPath: '/',
    filepath: `/api/v1/files/${partial.id}`,
    fileStatus: 'ok',
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...partial,
  };
}

describe('sortFiles', () => {
  it('sorts by name ascending on originalFilename (localeCompare)', () => {
    const files = [
      file({ id: 'c', originalFilename: 'cherry.txt' }),
      file({ id: 'a', originalFilename: 'apple.txt' }),
      file({ id: 'b', originalFilename: 'banana.txt' }),
    ];
    const sorted = sortFiles(files, { field: 'name', direction: 'asc' });
    expect(sorted.map((f) => f.id)).toEqual(['a', 'b', 'c']);
  });

  it('descending negates the comparison', () => {
    const files = [
      file({ id: 'a', originalFilename: 'apple.txt' }),
      file({ id: 'b', originalFilename: 'banana.txt' }),
    ];
    expect(sortFiles(files, { field: 'name', direction: 'desc' }).map((f) => f.id)).toEqual([
      'b',
      'a',
    ]);
  });

  it('sorts by size numerically and date by createdAt epoch', () => {
    const files = [
      file({ id: 'big', size: 900 }),
      file({ id: 'small', size: 10 }),
    ];
    expect(sortFiles(files, { field: 'size', direction: 'asc' }).map((f) => f.id)).toEqual([
      'small',
      'big',
    ]);

    const dated = [
      file({ id: 'newer', createdAt: '2026-02-01T00:00:00.000Z' }),
      file({ id: 'older', createdAt: '2026-01-01T00:00:00.000Z' }),
    ];
    expect(sortFiles(dated, { field: 'date', direction: 'asc' }).map((f) => f.id)).toEqual([
      'older',
      'newer',
    ]);
  });

  it('does not mutate the input array', () => {
    const files = [file({ id: 'b' }), file({ id: 'a' })];
    const before = files.map((f) => f.id);
    sortFiles(files, { field: 'name', direction: 'asc' });
    expect(files.map((f) => f.id)).toEqual(before);
  });
});

describe('filesInFolder', () => {
  const sort: SortState = { field: 'name', direction: 'asc' };

  it('keeps only files whose folderPath equals the current folder, then sorts', () => {
    const files = [
      file({ id: 'root-b', originalFilename: 'b', folderPath: '/' }),
      file({ id: 'root-a', originalFilename: 'a', folderPath: '/' }),
      file({ id: 'nested', folderPath: '/docs/' }),
    ];
    expect(filesInFolder(files, '/', sort).map((f) => f.id)).toEqual(['root-a', 'root-b']);
  });

  it('treats a null folderPath as root', () => {
    const files = [file({ id: 'orphan', folderPath: null })];
    expect(filesInFolder(files, '/', sort).map((f) => f.id)).toEqual(['orphan']);
  });
});

describe('deriveSubfolders', () => {
  it('derives one-level DB folders and counts files at/under their path', () => {
    const dbFolders: DbFolder[] = [
      { id: 'f1', path: '/docs/', name: 'docs', parentFolderId: null, projectId: null },
    ];
    const files = [
      file({ id: 'a', folderPath: '/docs/' }),
      file({ id: 'b', folderPath: '/docs/sub/' }),
    ];
    const result = deriveSubfolders(files, dbFolders, '/');
    expect(result).toHaveLength(1);
    expect(result[0]).toMatchObject({ path: '/docs/', name: 'docs', isDbFolder: true, id: 'f1' });
    // Counts both the direct file and the one under the nested prefix.
    expect(result[0].fileCount).toBe(2);
  });

  it('derives path-prefix folders from files with no DB row, accumulating counts', () => {
    const files = [
      file({ id: 'a', folderPath: '/images/' }),
      file({ id: 'b', folderPath: '/images/' }),
    ];
    const result = deriveSubfolders(files, [], '/');
    expect(result).toHaveLength(1);
    expect(result[0]).toMatchObject({ path: '/images/', name: 'images', isDbFolder: false });
    expect(result[0].fileCount).toBe(2);
  });

  it('unions both sources and sorts by name.localeCompare', () => {
    const dbFolders: DbFolder[] = [
      { id: 'z', path: '/zed/', name: 'zed', parentFolderId: null, projectId: null },
    ];
    const files = [file({ id: 'a', folderPath: '/alpha/' })];
    const result = deriveSubfolders(files, dbFolders, '/');
    expect(result.map((f) => f.name)).toEqual(['alpha', 'zed']);
  });
});

describe('getPreviewType', () => {
  it('routes image / pdf / text / unsupported', () => {
    expect(getPreviewType('image/png')).toBe('image');
    expect(getPreviewType('application/pdf')).toBe('pdf');
    expect(getPreviewType('text/markdown')).toBe('text');
    expect(getPreviewType('application/json')).toBe('text');
    expect(getPreviewType('application/xml')).toBe('text');
    expect(getPreviewType('application/octet-stream')).toBe('unsupported');
  });

  it('honors the isPlainText override', () => {
    expect(getPreviewType('application/octet-stream', true)).toBe('text');
  });
});

describe('getFileTypeLabel', () => {
  it('maps common MIME types to short labels', () => {
    expect(getFileTypeLabel('image/png')).toBe('Image');
    expect(getFileTypeLabel('application/pdf')).toBe('PDF');
    expect(getFileTypeLabel('application/json')).toBe('JSON');
    expect(getFileTypeLabel('text/plain')).toBe('Text');
    expect(getFileTypeLabel('application/octet-stream')).toBe('File');
  });
});

describe('breadcrumbSegments + parentFolder', () => {
  it('splits a nested path into cumulative segments', () => {
    expect(breadcrumbSegments('/a/b/')).toEqual([
      { label: 'a', path: '/a/' },
      { label: 'b', path: '/a/b/' },
    ]);
    expect(breadcrumbSegments('/')).toEqual([]);
  });

  it('walks up one level, stopping at root', () => {
    expect(parentFolder('/a/b/')).toBe('/a/');
    expect(parentFolder('/a/')).toBe('/');
    expect(parentFolder('/')).toBe('/');
  });
});
