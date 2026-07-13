// Cases derived from v4 `components/files/svar/listing-to-tree.ts` (no v4 unit
// test existed), re-targeted to the v5 `mountFilesList` envelope.
import { describe, expect, it } from 'vitest';

import { listingToTree, type MountListing } from './listing-to-tree';

describe('listingToTree', () => {
  it('emits folders shallowest-first, then files; carries size + date', () => {
    const listing: MountListing = {
      folders: ['Drafts'],
      files: [
        { relativePath: 'Report.md', fileSizeBytes: 12, lastModified: '2026-01-02T00:00:00.000Z' },
        { relativePath: 'Drafts/idea.md', fileSizeBytes: 3, lastModified: 1_700_000_000_000 },
      ],
    };
    const nodes = listingToTree(listing);
    // Folders come first.
    expect(nodes.filter((n) => n.type === 'folder').map((n) => n.path)).toEqual(['Drafts']);
    const report = nodes.find((n) => n.path === 'Report.md');
    expect(report).toMatchObject({ type: 'file', size: 12 });
    expect(report?.date instanceof Date).toBe(true);
  });

  it('synthesizes missing ancestor folders so a nested file is never orphaned', () => {
    const listing: MountListing = {
      folders: [],
      files: [{ relativePath: 'a/b/c.txt', fileSizeBytes: 1, lastModified: null }],
    };
    const folders = listingToTree(listing)
      .filter((n) => n.type === 'folder')
      .map((n) => n.path);
    // a, a/b synthesized, in shallow→deep order.
    expect(folders).toEqual(['a', 'a/b']);
  });

  it('sorts synthesized folders shallow→deep, ties by name', () => {
    const listing: MountListing = {
      folders: ['z/inner', 'a'],
      files: [{ relativePath: 'a/deep/file.txt', fileSizeBytes: 0, lastModified: null }],
    };
    const folders = listingToTree(listing)
      .filter((n) => n.type === 'folder')
      .map((n) => n.path);
    // depth 1 (a, z) before depth 2 (a/deep, z/inner); ties by localeCompare.
    expect(folders).toEqual(['a', 'z', 'a/deep', 'z/inner']);
  });

  it('leaves date undefined for a null or unparseable mtime', () => {
    const listing: MountListing = {
      folders: [],
      files: [
        { relativePath: 'x.txt', fileSizeBytes: 1, lastModified: null },
        { relativePath: 'y.txt', fileSizeBytes: 1, lastModified: 'not-a-date' },
      ],
    };
    for (const n of listingToTree(listing)) {
      if (n.type === 'file') expect(n.date).toBeUndefined();
    }
  });

  it('ignores blank folder entries', () => {
    const listing: MountListing = { folders: ['', 'Keep'], files: [] };
    expect(listingToTree(listing).map((n) => n.path)).toEqual(['Keep']);
  });
});
