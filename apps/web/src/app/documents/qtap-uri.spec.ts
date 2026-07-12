import { describe, expect, it } from 'vitest';

import { documentPaneUri, formatQtapUri } from './qtap-uri';

describe('qtap-uri (v4 formatQtapUri subset)', () => {
  it('emits reserved authorities for project / general', () => {
    expect(formatQtapUri({ scope: 'project', path: 'notes/todo.md' })).toBe(
      'qtap://project/notes/todo.md',
    );
    expect(formatQtapUri({ scope: 'general', path: 'a.md' })).toBe('qtap://general/a.md');
  });

  it('emits the self token for the self vault', () => {
    expect(formatQtapUri({ scope: 'document_store', mountPoint: 'self', path: 'x.md' })).toBe(
      'qtap://self/x.md',
    );
    // Case-insensitive.
    expect(formatQtapUri({ scope: 'document_store', mountPoint: 'SELF', path: 'x.md' })).toBe(
      'qtap://self/x.md',
    );
  });

  it('percent-encodes the mount name and each path segment', () => {
    expect(
      formatQtapUri({ scope: 'document_store', mountPoint: "Ada's Vault", path: 'a b/c?.md' }),
    ).toBe("qtap://Ada's%20Vault/a%20b/c%3F.md");
  });

  it('empty path yields the bare authority slash', () => {
    expect(formatQtapUri({ scope: 'project', path: '' })).toBe('qtap://project/');
  });

  describe('documentPaneUri (v4 DocumentPane documentUri)', () => {
    it('project scope', () => {
      expect(documentPaneUri({ scope: 'project', filePath: 'p.md' })).toBe('qtap://project/p.md');
    });

    it('document_store falls back to self when the mount is blank', () => {
      expect(documentPaneUri({ scope: 'document_store', mountPoint: '  ', filePath: 'd.md' })).toBe(
        'qtap://self/d.md',
      );
    });

    it('document_store keeps a real mount name', () => {
      expect(
        documentPaneUri({ scope: 'document_store', mountPoint: 'Library', filePath: 'd.md' }),
      ).toBe('qtap://Library/d.md');
    });
  });
});
