// Cases derived from v4 `components/files/svar/event-route-map.ts`, re-targeted
// to the v5 dispatch verbs (payloads pinned by the Rust `Request` serde in
// `crates/quilltap-core/src/api/types.rs`).
import { describe, expect, it } from 'vitest';

import {
  mapCopy,
  mapCreate,
  mapDelete,
  mapDownload,
  mapMove,
  mapOpen,
  mapRename,
  type ResolvedNode,
} from './event-wire-map';

const file = (relativePath: string, mountId = 'm1'): ResolvedNode => ({
  mountId,
  relativePath,
  type: 'file',
});
const folder = (relativePath: string, mountId = 'm1'): ResolvedNode => ({
  mountId,
  relativePath,
  type: 'folder',
});

describe('mapRename', () => {
  it('renames within the same directory via mountFileUpdate', () => {
    expect(mapRename(file('a/b/old.txt'), 'new.txt')).toEqual([
      {
        op: 'rename',
        request: { type: 'mountFileUpdate', mountPointId: 'm1', path: 'a/b/old.txt', rename: 'a/b/new.txt' },
      },
    ]);
  });
  it('renames a top-level item (no parent dir)', () => {
    expect(mapRename(file('old.txt'), 'new.txt')[0].request).toEqual({
      type: 'mountFileUpdate',
      mountPointId: 'm1',
      path: 'old.txt',
      rename: 'new.txt',
    });
  });
});

describe('mapCreate', () => {
  it('creates a folder inside a parent via mountFolderCreate', () => {
    expect(mapCreate(folder('Drafts'), 'Sub', 'folder', 'm1')).toEqual([
      { op: 'create-folder', request: { type: 'mountFolderCreate', mountPointId: 'm1', path: 'Drafts/Sub' } },
    ]);
  });
  it('creates a file as a multipart upload leg (no dispatch request)', () => {
    const calls = mapCreate(folder('Drafts'), 'note.md', 'file', 'm1');
    expect(calls).toEqual([{ op: 'upload', upload: { mountPointId: 'm1', path: 'Drafts/note.md' } }]);
    expect(calls[0].request).toBeUndefined();
  });
  it('falls back to the pane mount id + root at the root (quirk 4)', () => {
    expect(mapCreate(null, 'Top', 'folder', 'paneMount')).toEqual([
      { op: 'create-folder', request: { type: 'mountFolderCreate', mountPointId: 'paneMount', path: 'Top' } },
    ]);
  });
});

describe('mapDelete', () => {
  it('routes files and folders to their delete verbs', () => {
    expect(mapDelete([file('a.txt'), folder('Dir')])).toEqual([
      { op: 'delete-file', request: { type: 'mountFileDelete', mountPointId: 'm1', path: 'a.txt' } },
      { op: 'delete-folder', request: { type: 'mountFolderDelete', mountPointId: 'm1', path: 'Dir' } },
    ]);
  });
});

describe('mapMove', () => {
  it('moves a file into a dest folder via mountFileMove (destPath = dest/basename)', () => {
    expect(mapMove([file('a/x.txt')], folder('b'))).toEqual([
      {
        op: 'move-file',
        request: {
          type: 'mountFileMove',
          mountPointId: 'm1',
          sourcePath: 'a/x.txt',
          destMountPointId: 'm1',
          destPath: 'b/x.txt',
        },
      },
    ]);
  });
  it('same-mount folder move is allowed (mountFolderMove, no refusal)', () => {
    const [call] = mapMove([folder('src')], folder('dst'));
    expect(call.unsupported).toBeUndefined();
    expect(call.request).toEqual({
      type: 'mountFolderMove',
      mountPointId: 'm1',
      fromPath: 'src',
      toPath: 'dst/src',
    });
  });
  it('REFUSES a cross-mount folder move (backend gap — copy-across prompt)', () => {
    const [call] = mapMove([folder('src', 'm1')], folder('dst', 'm2'));
    expect(call.unsupported).toContain('copy the files across');
    // The request shape is still computed but must never be fired.
    expect(call.request?.type).toBe('mountFolderMove');
  });
});

describe('mapCopy', () => {
  it('copies a file via mountFileCopy', () => {
    expect(mapCopy([file('a/x.txt')], folder('b'))[0].request).toEqual({
      type: 'mountFileCopy',
      mountPointId: 'm1',
      sourcePath: 'a/x.txt',
      destMountPointId: 'm1',
      destPath: 'b/x.txt',
    });
  });
  it('REFUSES a whole-folder copy (no copy-folder verb — select-the-files prompt)', () => {
    const [call] = mapCopy([folder('src')], folder('dst'));
    expect(call.unsupported).toContain('select the files within');
  });
});

describe('mapOpen / mapDownload', () => {
  it('open returns the raw item route for a file, nothing for a folder', () => {
    expect(mapOpen(file('a b/c&d.txt'))).toEqual([
      { op: 'open', getUrl: '/api/v1/mount-points/m1/files/a%20b/c%26d.txt' },
    ]);
    expect(mapOpen(folder('Dir'))).toEqual([]);
  });
  it('download appends ?raw=1 for a file, nothing for a folder', () => {
    expect(mapDownload(file('x.pdf'))).toEqual([
      { op: 'download', getUrl: '/api/v1/mount-points/m1/files/x.pdf?raw=1' },
    ]);
    expect(mapDownload(folder('Dir'))).toEqual([]);
  });
});
