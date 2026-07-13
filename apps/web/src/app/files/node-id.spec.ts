// Cases derived from v4 `components/files/svar/node-id.ts` (the source had no
// unit test; this pins its exact behavior for the ported-forward copy).
import { describe, expect, it } from 'vitest';

import {
  encodeRelPath,
  extOf,
  nodeIdToRelPath,
  relBasename,
  relDirname,
  relJoin,
  relPathToNodeId,
} from './node-id';

describe('relPathToNodeId / nodeIdToRelPath (SVAR-style absolute-path codec)', () => {
  it('maps root to "/" and back to ""', () => {
    expect(relPathToNodeId('')).toBe('/');
    expect(nodeIdToRelPath('/')).toBe('');
  });

  it('prefixes a single slash and strips a leading one', () => {
    expect(relPathToNodeId('Music/Info.txt')).toBe('/Music/Info.txt');
    expect(relPathToNodeId('/already/slashed')).toBe('/already/slashed');
    expect(nodeIdToRelPath('/Music/Info.txt')).toBe('Music/Info.txt');
    expect(nodeIdToRelPath('bare')).toBe('bare');
  });
});

describe('relDirname', () => {
  it('returns "" for a top-level entry', () => {
    expect(relDirname('Info.txt')).toBe('');
    expect(relDirname('/Info.txt')).toBe('');
  });
  it('returns the parent dir for a nested path', () => {
    expect(relDirname('a/b/c.txt')).toBe('a/b');
    expect(relDirname('a/b/')).toBe('a');
  });
});

describe('relBasename', () => {
  it('returns the final segment, tolerating leading/trailing slashes', () => {
    expect(relBasename('a/b/c.txt')).toBe('c.txt');
    expect(relBasename('/solo')).toBe('solo');
    expect(relBasename('dir/')).toBe('dir');
  });
});

describe('relJoin', () => {
  it('joins a dir + segment; a blank dir yields the bare name', () => {
    expect(relJoin('', 'top.txt')).toBe('top.txt');
    expect(relJoin('a/b', 'c.txt')).toBe('a/b/c.txt');
    expect(relJoin('a/b/', 'c.txt')).toBe('a/b/c.txt');
    expect(relJoin('/a', 'c.txt')).toBe('a/c.txt');
  });
});

describe('extOf', () => {
  it('lower-cases the extension without the dot', () => {
    expect(extOf('Report.PDF')).toBe('pdf');
    expect(extOf('a/b/photo.JPEG')).toBe('jpeg');
  });
  it('returns "" when there is no extension or the dot leads', () => {
    expect(extOf('README')).toBe('');
    expect(extOf('.gitignore')).toBe(''); // leading dot (dot index 0) → no ext
    expect(extOf('archive.')).toBe('');
  });
});

describe('encodeRelPath', () => {
  it('percent-encodes each segment but keeps the separators', () => {
    expect(encodeRelPath('a b/c&d.txt')).toBe('a%20b/c%26d.txt');
    expect(encodeRelPath('/leading/space here.md')).toBe('leading/space%20here.md');
  });
});
