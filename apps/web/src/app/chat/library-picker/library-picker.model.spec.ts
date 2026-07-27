import type { DocumentStoreFile } from '../../screens/scriptorium/scriptorium.api';
import {
  deriveMimeTypeFromName,
  documentStoreFileToPickerFile,
  folderPathFromRelativePath,
  isMountFile,
  isProjectOwnStoreName,
  mimeTypeForDocumentStoreFile,
  pickPrimaryProjectStore,
  pickableDocStores,
  pickedFileName,
  resolveUserPersonaAlbum,
  type PickerAlbumOption,
  type PickerFile,
  type PickerStoreSummary,
} from './library-picker.model';

function store(over: Partial<PickerStoreSummary> & { id: string }): PickerStoreSummary {
  return {
    name: over.id,
    mountType: 'database',
    storeType: 'documents',
    enabled: true,
    ...over,
  };
}

function album(over: Partial<PickerAlbumOption> & { name: string }): PickerAlbumOption {
  return { mountPointId: `mp-${over.name}`, kind: 'character', ...over };
}

function storeRow(over: Partial<DocumentStoreFile> & { relativePath: string }): DocumentStoreFile {
  return {
    id: 'f1',
    mountPointId: 'mp1',
    fileName: over.relativePath.split('/').pop() ?? 'f',
    fileType: 'markdown',
    sha256: 'abc',
    fileSizeBytes: 12,
    lastModified: '2026-02-02T00:00:00.000Z',
    conversionStatus: 'converted',
    conversionError: null,
    plainTextLength: 3,
    chunkCount: 1,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-03T00:00:00.000Z',
    ...over,
  };
}

describe('library picker — the scope list (v4 LibraryFilePickerModal)', () => {
  it('offers only enabled, database-backed, non-character stores (v4 :133-135)', () => {
    const result = pickableDocStores([
      store({ id: 'ok' }),
      store({ id: 'disabled', enabled: false }),
      store({ id: 'fs', mountType: 'filesystem' }),
      store({ id: 'obsidian', mountType: 'obsidian' }),
      store({ id: 'vault', storeType: 'character' }),
    ]);
    expect(result.map((s) => s.id)).toEqual(['ok']);
  });

  it('keeps a character vault out of the composer’s reach even when enabled', () => {
    expect(pickableDocStores([store({ id: 'vault', storeType: 'character' })])).toEqual([]);
  });
});

describe('library picker — the persona-album fallback chain (v4 :156-159)', () => {
  it('prefers the DEFAULT user-character album', () => {
    const chosen = resolveUserPersonaAlbum([
      album({ name: 'Someone else', isUserCharacter: false, isDefault: true }),
      album({ name: 'Persona alt', isUserCharacter: true }),
      album({ name: 'Persona default', isUserCharacter: true, isDefault: true }),
    ]);
    expect(chosen?.name).toBe('Persona default');
  });

  it('falls back to the first user-character album when none is default', () => {
    const chosen = resolveUserPersonaAlbum([
      album({ name: 'Not mine', isUserCharacter: false }),
      album({ name: 'Persona alt', isUserCharacter: true }),
      album({ name: 'Persona later', isUserCharacter: true }),
    ]);
    expect(chosen?.name).toBe('Persona alt');
  });

  it('ignores non-character albums entirely, and yields null when there is no persona', () => {
    expect(
      resolveUserPersonaAlbum([
        album({ name: 'A project', kind: 'project', isUserCharacter: true, isDefault: true }),
        album({ name: 'A store', kind: 'document-store', isUserCharacter: true }),
      ]),
    ).toBeNull();
    expect(resolveUserPersonaAlbum([])).toBeNull();
  });
});

describe('library picker — pickPrimaryProjectStore (v4 project-store-naming.ts)', () => {
  it('prefers the migration-named "Project Files: …" store', () => {
    const chosen = pickPrimaryProjectStore([
      store({ id: 'hand-linked', name: 'Research' }),
      store({ id: 'own', name: 'Project Files: Novel' }),
    ]);
    expect(chosen?.id).toBe('own');
  });

  it('falls back to the first eligible store when nothing matches the name', () => {
    const chosen = pickPrimaryProjectStore([
      store({ id: 'fs', name: 'On disk', mountType: 'filesystem' }),
      store({ id: 'first', name: 'Research' }),
      store({ id: 'second', name: 'Notes' }),
    ]);
    expect(chosen?.id).toBe('first');
  });

  it('treats a missing storeType as documents, and ignores character vaults', () => {
    expect(
      pickPrimaryProjectStore([{ name: 'Untyped', mountType: 'database' }])?.name,
    ).toBe('Untyped');
    expect(
      pickPrimaryProjectStore([{ name: 'V', mountType: 'database', storeType: 'character' }]),
    ).toBeNull();
  });

  it('yields null when there is nothing eligible', () => {
    expect(pickPrimaryProjectStore([])).toBeNull();
  });

  it('recognises the name prefix exactly', () => {
    expect(isProjectOwnStoreName('Project Files: Novel')).toBe(true);
    expect(isProjectOwnStoreName('project files: Novel')).toBe(false);
    expect(isProjectOwnStoreName(null)).toBe(false);
    expect(isProjectOwnStoreName(undefined)).toBe(false);
  });
});

describe('library picker — the document-store row translation (v4 FileBrowser)', () => {
  it('maps every fileType to v4’s mime, deriving blobs from the extension', () => {
    expect(mimeTypeForDocumentStoreFile(storeRow({ relativePath: 'a.pdf', fileType: 'pdf' }))).toBe(
      'application/pdf',
    );
    expect(
      mimeTypeForDocumentStoreFile(storeRow({ relativePath: 'a.docx', fileType: 'docx' })),
    ).toBe('application/vnd.openxmlformats-officedocument.wordprocessingml.document');
    expect(
      mimeTypeForDocumentStoreFile(storeRow({ relativePath: 'a.md', fileType: 'markdown' })),
    ).toBe('text/markdown');
    expect(mimeTypeForDocumentStoreFile(storeRow({ relativePath: 'a.txt', fileType: 'txt' }))).toBe(
      'text/plain',
    );
    expect(
      mimeTypeForDocumentStoreFile(storeRow({ relativePath: 'a.json', fileType: 'json' })),
    ).toBe('application/json');
    expect(
      mimeTypeForDocumentStoreFile(storeRow({ relativePath: 'a.jsonl', fileType: 'jsonl' })),
    ).toBe('application/jsonl');
    expect(
      mimeTypeForDocumentStoreFile(
        storeRow({ relativePath: 'images/p.webp', fileName: 'p.webp', fileType: 'blob' }),
      ),
    ).toBe('image/webp');
  });

  it('derives mime from the extension, case-insensitively, with an octet-stream default', () => {
    expect(deriveMimeTypeFromName('SHOT.PNG')).toBe('image/png');
    expect(deriveMimeTypeFromName('scan.TIF')).toBe('image/tiff');
    expect(deriveMimeTypeFromName('notes.markdown')).toBe('text/markdown');
    expect(deriveMimeTypeFromName('log.ndjson')).toBe('application/jsonl');
    expect(deriveMimeTypeFromName('mystery')).toBe('application/octet-stream');
    expect(deriveMimeTypeFromName('archive.tar.gz')).toBe('application/octet-stream');
  });

  it('translates a relativePath into the legacy folderPath convention', () => {
    expect(folderPathFromRelativePath('images/foo.webp')).toBe('/images/');
    expect(folderPathFromRelativePath('a/b/c.md')).toBe('/a/b/');
    expect(folderPathFromRelativePath('root.md')).toBe('/');
  });

  it('projects a store row into a browsable file carrying BOTH mount fields', () => {
    const file = documentStoreFileToPickerFile(
      storeRow({
        id: 'row-1',
        mountPointId: 'store-9',
        relativePath: 'notes/plan.md',
        fileName: 'plan.md',
        fileSizeBytes: 42,
      }),
    );
    expect(file.id).toBe('row-1');
    expect(file.originalFilename).toBe('plan.md');
    expect(file.filename).toBe('plan.md');
    expect(file.mimeType).toBe('text/markdown');
    expect(file.size).toBe(42);
    expect(file.category).toBe('document');
    expect(file.folderPath).toBe('/notes/');
    expect(file.mountPointId).toBe('store-9');
    expect(file.relativePath).toBe('notes/plan.md');
    // v4 uses `lastModified || updatedAt` for the display date.
    expect(file.updatedAt).toBe('2026-02-02T00:00:00.000Z');
    expect(isMountFile(file)).toBe(true);
  });

  it('categorises a blob row as binary and falls back to updatedAt', () => {
    const file = documentStoreFileToPickerFile(
      storeRow({ relativePath: 'p.webp', fileName: 'p.webp', fileType: 'blob', lastModified: '' }),
    );
    expect(file.category).toBe('binary');
    expect(file.updatedAt).toBe('2026-01-03T00:00:00.000Z');
  });
});

describe('library picker — the pick discriminator (v4 :237-238)', () => {
  const legacy: PickerFile = {
    id: 'f1',
    originalFilename: 'note.txt',
    filename: 'note.txt',
    mimeType: 'text/plain',
    size: 1,
    category: 'general',
    description: null,
    projectId: null,
    folderPath: '/',
    filepath: '/api/v1/files/f1',
    fileStatus: 'ok',
    createdAt: 'x',
    updatedAt: 'x',
  };

  it('needs BOTH mount fields to count as a store file', () => {
    expect(isMountFile(legacy)).toBe(false);
    expect(isMountFile({ ...legacy, mountPointId: 'mp' })).toBe(false);
    expect(isMountFile({ ...legacy, relativePath: 'a.md' })).toBe(false);
    expect(isMountFile({ ...legacy, mountPointId: 'mp', relativePath: 'a.md' })).toBe(true);
  });

  it('treats an EMPTY relativePath as legacy (v4’s truthiness test, not presence)', () => {
    expect(isMountFile({ ...legacy, mountPointId: 'mp', relativePath: '' })).toBe(false);
  });

  it('names a picked file originalFilename → filename → "file"', () => {
    expect(pickedFileName(legacy)).toBe('note.txt');
    expect(pickedFileName({ ...legacy, originalFilename: '' })).toBe('note.txt');
    expect(pickedFileName({ ...legacy, originalFilename: '', filename: '' })).toBe('file');
  });
});
