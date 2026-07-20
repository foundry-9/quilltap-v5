import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { WritableSignal } from '@angular/core';

import { resetWorkspaceTabsFlagCache } from '../workspace/workspace-flag';
import type { DocumentStandaloneTabPayload, TabKind } from '../workspace/workspace-contract';
import { WorkspaceService } from '../workspace/workspace.service';
import type { DocumentSelection } from './document-picker';
import { DocumentsRailEntry } from './documents-rail-entry';

interface RailHost {
  showPicker: WritableSignal<boolean>;
  openPicker(): void;
  handleSelect(sel: DocumentSelection): void;
}

function make(url = '/workspace') {
  const openTab = vi.fn((_kind: TabKind, _payload?: unknown, _opts?: unknown) => 'tab-x');
  const navigateByUrl = vi.fn();
  const workspace = { openTab };
  const router = { url, navigateByUrl };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [DocumentsRailEntry],
    providers: [
      { provide: WorkspaceService, useValue: workspace },
      { provide: Router, useValue: router },
    ],
  });
  const fixture = TestBed.createComponent(DocumentsRailEntry);
  const c = fixture.componentInstance as unknown as RailHost;
  return { c, openTab, navigateByUrl };
}

/** The last openTab call decomposed for assertions. */
function lastOpen(openTab: ReturnType<typeof vi.fn>): {
  kind: TabKind;
  payload: DocumentStandaloneTabPayload;
  opts: { title?: string };
} {
  const call = openTab.mock.calls.at(-1)!;
  return { kind: call[0], payload: call[1], opts: call[2] };
}

describe('DocumentsRailEntry', () => {
  beforeEach(() => resetWorkspaceTabsFlagCache());

  it('openPicker shows the standalone picker', () => {
    const { c } = make();
    expect(c.showPicker()).toBe(false);
    c.openPicker();
    expect(c.showPicker()).toBe(true);
  });

  it('selecting a store file opens a document-standalone tab keyed by identity', () => {
    const { c, openTab } = make('/workspace');
    c.handleSelect({ filePath: 'notes.md', scope: 'document_store', mountPoint: 'Lib' });
    const { kind, payload, opts } = lastOpen(openTab);
    expect(kind).toBe('document-standalone');
    expect(payload).toEqual({
      docKey: 'document_store:Lib:notes.md',
      scope: 'document_store',
      mountPoint: 'Lib',
      filePath: 'notes.md',
      targetFolder: undefined,
      displayTitle: undefined,
    });
    expect(opts.title).toBe('notes.md');
  });

  it('reopening the same file produces the SAME docKey (the reducer focuses it)', () => {
    const { c, openTab } = make('/workspace');
    const sel: DocumentSelection = { filePath: 'a/b.md', scope: 'document_store', mountPoint: 'Lib' };
    c.handleSelect(sel);
    c.handleSelect(sel);
    const [first, second] = openTab.mock.calls;
    expect((first[1] as DocumentStandaloneTabPayload).docKey).toBe(
      (second[1] as DocumentStandaloneTabPayload).docKey,
    );
    expect((first[1] as DocumentStandaloneTabPayload).docKey).toBe('document_store:Lib:a/b.md');
    // The label is the file's basename, not the full path.
    expect((first[2] as { title?: string }).title).toBe('b.md');
  });

  it('a new blank document mints a fresh docKey each time (title "New Document")', () => {
    const { c, openTab } = make('/workspace');
    c.handleSelect({}); // new blank general document
    c.handleSelect({}); // another blank
    const [first, second] = openTab.mock.calls;
    const k1 = (first[1] as DocumentStandaloneTabPayload).docKey;
    const k2 = (second[1] as DocumentStandaloneTabPayload).docKey;
    expect(k1).not.toBe(k2); // distinct uuids so both blanks coexist
    expect((first[1] as DocumentStandaloneTabPayload).scope).toBe('general');
    expect((first[1] as DocumentStandaloneTabPayload).filePath).toBeUndefined();
    expect((first[2] as { title?: string }).title).toBe('New Document');
  });

  it('maps a stray project scope defensively to general', () => {
    const { c, openTab } = make('/workspace');
    c.handleSelect({ filePath: 'x.md', scope: 'project' as DocumentSelection['scope'] });
    expect((lastOpen(openTab).payload as DocumentStandaloneTabPayload).scope).toBe('general');
  });

  it('off the workspace surface it navigates with an ?open= intent instead', () => {
    const { c, openTab, navigateByUrl } = make('/salon/chat-1');
    c.handleSelect({ filePath: 'notes.md', scope: 'document_store', mountPoint: 'Lib' });
    expect(openTab).not.toHaveBeenCalled();
    expect(navigateByUrl).toHaveBeenCalledWith(
      '/workspace?open=document-standalone&scope=document_store&mountPoint=Lib&filePath=notes.md',
    );
  });
});
