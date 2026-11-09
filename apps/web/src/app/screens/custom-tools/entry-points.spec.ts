import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import {
  documentFileActions,
  type BrowserFile,
  type BrowserStore,
} from '../../document-browser/document-actions';
import { file as storeFile } from '../../document-browser/document-browser.testing';
import { DocumentRow, type DocumentRowAction } from '../../document-browser/document-row';
import type { DocumentEntry } from '../../document-browser/document-store.source';

/**
 * The Workbench entry points that lanes P4.6z / P4.6an / P4.6ba left as named
 * omissions, landed by P4.6bb unit 8.
 *
 * The left-rail item and the Settings-card button are asserted in `shell` /
 * `custom-tools-settings.spec.ts`; the composer-popup trio in
 * `custom-tools-popup.spec.ts`. This file covers the Scriptorium row action and
 * — importantly — the path predicate that decides which rows get it at all.
 *
 * RE-AIMED by P4.D387 (the D18 retirement, ruling R-D): v4 `c34986421` moved
 * the action from the retired `FileTable` / `FileDetailRow` onto the
 * document-store browser — `documentFileActions` grants `workbench` when
 * `isCustomToolDefinition` (`/^tools\/[^/]+\.tool\.json$/i`,
 * `lib/folder-browser/document-actions.ts:86-88`) holds, and `DocumentRow`
 * emits it for the browser to open. Each case below is the same question,
 * asked of v4's new home.
 */

const STORE: BrowserStore = {
  id: 'm1',
  mountType: 'database',
  capabilities: {
    canWrite: true,
    canDelete: true,
    canCreateFolder: true,
    canMoveIn: true,
    canMoveOut: true,
  },
};

const offered = (relativePath: string): boolean => {
  const f: BrowserFile = {
    relativePath,
    fileName: relativePath.split('/').pop()!,
    fileType: 'json',
    source: 'database',
  };
  return documentFileActions(STORE, f, { linkTargets: false }).includes('workbench');
};

describe('Scriptorium row action (v4 document-actions isCustomToolDefinition, c34986421)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('offers the Workbench on a ROOT-level Tools/*.tool.json', () => {
    expect(offered('Tools/unlock.tool.json')).toBe(true);
  });

  it('is case-insensitive on the folder and suffix, as v4 is', () => {
    expect(offered('TOOLS/Unlock.TOOL.JSON')).toBe(true);
  });

  it('does NOT offer it on a nested Tools path — the folder must be at the root', () => {
    expect(offered('Notes/Tools/unlock.tool.json')).toBe(false);
  });

  it('does NOT offer it on a deeper file inside Tools/', () => {
    expect(offered('Tools/sub/unlock.tool.json')).toBe(false);
  });

  it('does NOT offer it on an ordinary file', () => {
    expect(offered('Tools/readme.md')).toBe(false);
  });

  it("emits the entry from the row's Open in Pascal's Workbench so the browser can open it", () => {
    TestBed.configureTestingModule({ imports: [DocumentRow] });
    const fixture = TestBed.createComponent(DocumentRow);
    const entry: DocumentEntry = {
      ...storeFile('Tools/unlock.tool.json', { fileType: 'json' }),
      folder: 'Tools',
    };
    fixture.componentRef.setInput('entry', entry);
    fixture.componentRef.setInput('actions', ['open', 'workbench', 'download']);
    fixture.componentRef.setInput('rename', async () => true);
    document.body.appendChild(fixture.nativeElement);
    fixture.detectChanges();
    let emitted: DocumentRowAction | undefined;
    fixture.componentInstance.acted.subscribe((a) => (emitted = a));
    (
      fixture.nativeElement.querySelector(
        '[aria-label="Actions for unlock.tool.json"]',
      ) as HTMLButtonElement
    ).click();
    fixture.detectChanges();
    const item = [...document.querySelectorAll('[role="menuitem"]')].find((b) =>
      b.textContent?.includes("Open in Pascal's Workbench"),
    ) as HTMLButtonElement;
    item.click();
    expect(emitted?.action).toBe('workbench');
    expect(emitted?.entry.relativePath).toBe('Tools/unlock.tool.json');
    document.body.innerHTML = '';
  });
});
