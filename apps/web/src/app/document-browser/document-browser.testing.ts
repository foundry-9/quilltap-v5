/**
 * Test-only fixtures for the document-store browser specs — v4
 * `__tests__/unit/components/document-browser/document-store-browser.test.tsx`'s
 * `store()` / `file()` / `FILES` / `CATALOGUE`, served through a fake
 * `CoreClient.dispatchData` instead of a mocked `fetch` (v5 reads over
 * dispatch).
 */

import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { DocMountBrowserEntry } from '../core/core-contract';

export const T = '2026-10-31T00:00:00.000Z';
export const CAPS = {
  canWrite: true,
  canDelete: true,
  canCreateFolder: true,
  canMoveIn: true,
  canMoveOut: true,
  canConvert: true,
};

export function store(id: string, name: string, capabilities: Record<string, boolean> = CAPS) {
  return {
    id,
    userId: 'u',
    name,
    basePath: '',
    mountType: 'database',
    storeType: 'documents',
    enabled: true,
    capabilities,
    includePatterns: [],
    excludePatterns: [],
    lastScannedAt: null,
    scanStatus: 'idle',
    lastScanError: null,
    conversionStatus: 'idle',
    conversionError: null,
    fileCount: 3,
    chunkCount: 0,
    totalSizeBytes: 300,
    embeddedChunkCount: 0,
    createdAt: T,
    updatedAt: T,
  };
}

export function file(
  relativePath: string,
  fields: Partial<DocMountBrowserEntry> = {},
): DocMountBrowserEntry {
  return {
    id: `link-${relativePath}`,
    fileId: `file-${relativePath}`,
    mountPointId: 'store-a',
    relativePath,
    fileName: relativePath.split('/').pop() ?? relativePath,
    folderId: null,
    originalFileName: null,
    originalMimeType: null,
    description: '',
    descriptionUpdatedAt: null,
    conversionStatus: 'converted',
    conversionError: null,
    plainTextLength: null,
    extractedTextSha256: null,
    extractionStatus: 'none',
    extractionError: null,
    chunkCount: 0,
    allowEmbed: true,
    allowCharacterRead: true,
    allowCharacterWrite: true,
    linkGroupId: null,
    linkCount: 1,
    lastModified: T,
    createdAt: T,
    updatedAt: T,
    sha256: 'x',
    fileSizeBytes: 100,
    fileType: 'markdown',
    source: 'database',
    ...fields,
  };
}

export const FILES = [
  file('notes.md'),
  file('Drafts/chapter-one.md', { linkGroupId: 'g1', linkCount: 2 }),
  file('Drafts/old/outline.md'),
];

export const CATALOGUE = {
  mountPoints: [store('store-a', 'Archive'), store('store-b', 'Second Shelf')],
};

export type Req = Record<string, unknown> & { type: string };
export type Route = (req: Req) => unknown;

/** v4's default `serve`: store-a with three files, store-b with an Inbox, the two-store catalogue. */
export const DEFAULT_ROUTE: Route = (req) => {
  if (req.type === 'mountPointGet' && req['mountPointId'] === 'store-a')
    return { mountPoint: store('store-a', 'Archive') };
  if (req.type === 'mountFilesList' && req['mountPointId'] === 'store-a')
    return { files: FILES, folders: ['Drafts', 'Drafts/old', 'Empty'] };
  if (req.type === 'mountFilesList' && req['mountPointId'] === 'store-b')
    return { files: [], folders: ['Inbox'] };
  if (req.type === 'mountPointList') return CATALOGUE;
  return { success: true };
};

/** Configure TestBed with a fake CoreClient over `route` and a retry-free QueryClient. */
export function provideDocumentStoreTesting(route: Route = DEFAULT_ROUTE, extra: unknown[] = []) {
  const seen: Req[] = [];
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  TestBed.configureTestingModule({
    providers: [
      provideTanStackQuery(queryClient),
      {
        provide: CoreClient,
        useValue: {
          dispatchData: vi.fn(async (req: Req) => {
            seen.push(req);
            const out = route(req);
            if (out instanceof Error) throw out;
            return out;
          }),
        },
      },
      ...(extra as never[]),
    ],
  });
  return { seen, queryClient };
}

/** Let queries resolve and signals settle. */
export async function settle(fixture?: {
  detectChanges(): void;
  whenStable(): Promise<unknown>;
}): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture?.detectChanges();
  }
  await fixture?.whenStable();
  fixture?.detectChanges();
}
