// Cases derived from v4 `components/files/svar/reindex-after-copy.ts`.
import { describe, expect, it } from 'vitest';

import { reindexAfterCopy } from './reindex-after-copy';

describe('reindexAfterCopy', () => {
  it('fires for a byte-copy of an extractable type (fs→db .pdf/.docx)', () => {
    expect(
      reindexAfterCopy({ strategy: 'byte-copy', destMountPointId: 'db1', destPath: 'in/Doc.pdf' }),
    ).toEqual({ mountId: 'db1', path: 'in/Doc.pdf' });
    expect(
      reindexAfterCopy({ strategy: 'byte-copy', destMountPointId: 'db1', destPath: 'Report.DOCX' }),
    ).toEqual({ mountId: 'db1', path: 'Report.DOCX' });
  });

  it('skips non-byte-copy strategies (db-link / fs-link / rename)', () => {
    for (const strategy of ['db-link', 'fs-link', 'rename']) {
      expect(
        reindexAfterCopy({ strategy, destMountPointId: 'db1', destPath: 'in/Doc.pdf' }),
      ).toBeNull();
    }
  });

  it('skips a byte-copy of a non-extractable type', () => {
    expect(
      reindexAfterCopy({ strategy: 'byte-copy', destMountPointId: 'db1', destPath: 'notes.md' }),
    ).toBeNull();
    expect(
      reindexAfterCopy({ strategy: 'byte-copy', destMountPointId: 'db1', destPath: 'pic.png' }),
    ).toBeNull();
  });
});
