import { describe, expect, it } from 'vitest';

import corpusText from '../../testing/fixtures/document-browser.oracle.ndjson';
import { getPreviewType } from '../screens/files/file-model';
import {
  canLinkBetween,
  capabilitiesOf,
  documentFileActions,
  documentFolderActions,
  documentPreviewKind,
  hasDescription,
  isCustomToolDefinition,
  type BrowserFile,
  type BrowserStore,
} from './document-actions';
import { folderTreeOf, type DocumentEntry } from './document-store.source';
import { deriveMountCapabilities, type MountCapabilityInput } from './mount-capabilities';
import { mimeTypeForName, previewKindOf, type PreviewSubject } from './preview-kind';

/**
 * The tier-1 SPA differential for the document-store browser's pure half
 * (P4.D387 item 3): v4's REAL `lib/folder-browser/preview-kind.ts`,
 * `lib/folder-browser/document-actions.ts`, `lib/mount-index/capabilities.ts`,
 * `components/files/FilePreview/types.ts` (`getPreviewType`) and
 * `components/document-browser/useDocumentStoreSource.ts` (`folderTreeOf`), at
 * the pin `624d0d25c`. Recorded by `apps/web/oracle/document-browser.recorder.ts`
 * (its header has the pinned-worktree invocation and the expected count);
 * replayed here row for row through v5's `document-browser/` twins and
 * `screens/files/file-model.ts`'s `getPreviewType`.
 */

interface Row {
  fn: string;
  id: string;
  args: unknown[];
  out: unknown;
}

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

function run(row: Row): unknown {
  const a = row.args;
  switch (row.fn) {
    case 'previewKindOf':
      return previewKindOf(a[0] as PreviewSubject);
    case 'mimeTypeForName':
      return mimeTypeForName(a[0] as string);
    case 'getPreviewType':
      return getPreviewType(a[0] as string, a[1] as boolean | undefined);
    case 'documentPreviewKind':
      return documentPreviewKind(a[0] as BrowserFile);
    case 'documentFileActions':
      return documentFileActions(
        a[0] as BrowserStore,
        a[1] as BrowserFile,
        a[2] as { linkTargets: boolean },
      );
    case 'documentFolderActions':
      return documentFolderActions(a[0] as BrowserStore);
    case 'canLinkBetween':
    case 'canLinkGates':
      return canLinkBetween(a[0] as BrowserStore, a[1] as BrowserStore);
    case 'isCustomToolDefinition':
      return isCustomToolDefinition(a[0] as string);
    case 'hasDescription':
      return hasDescription(a[0] as BrowserFile);
    case 'capabilitiesOf':
      return capabilitiesOf(a[0] as BrowserStore | null);
    case 'deriveMountCapabilities':
      return deriveMountCapabilities(a[0] as MountCapabilityInput);
    case 'folderTreeOf':
      return folderTreeOf(
        a[0] as string[],
        (a[1] as string[]).map((folder) => ({ folder }) as DocumentEntry),
      );
    default:
      throw new Error(`unknown fn ${row.fn}`);
  }
}

describe('document-browser oracle (v4 624d0d25c)', () => {
  it('carries the recorded corpus whole', () => {
    expect(ROWS.length).toBe(422);
    expect(ROWS.filter((r) => r.fn === 'documentFileActions').length).toBe(256);
  });

  it.each(ROWS.map((r) => [`${r.fn} — ${r.id}`, r] as const))('%s', (_label, row) => {
    expect(JSON.stringify(run(row))).toBe(JSON.stringify(row.out));
  });
});
