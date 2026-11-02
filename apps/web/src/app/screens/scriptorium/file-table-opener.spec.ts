import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import {
  WORKSPACE_HANDLE,
  type WorkspaceHandle,
} from '../../workspace/workspace-contract';
import { FileTable } from './file-table';
import type { DocumentStoreFile } from './scriptorium.api';

interface OpenIn {
  openInWorkbench(path: string): void;
}

/**
 * The Workbench opener arm (p4.9j2 item 6, v4 `redirectToWorkspaceTab`): hosted
 * ⇒ open (or focus) the `custom-tools` tab with the definition's payload; routed
 * ⇒ the `?mount=&path=` query-param push, unchanged.
 */
describe('FileTable — Workbench opener arm', () => {
  function render(handle?: WorkspaceHandle): ComponentFixture<FileTable> {
    TestBed.configureTestingModule({
      imports: [FileTable],
      providers: [
        provideRouter([]),
        { provide: CoreClient, useValue: {} },
        ...(handle ? [{ provide: WORKSPACE_HANDLE, useValue: handle }] : []),
      ],
    });
    const fixture = TestBed.createComponent(FileTable);
    fixture.componentRef.setInput('files', []);
    fixture.componentRef.setInput('mountPointId', 'm1');
    fixture.componentRef.setInput('mountType', 'filesystem');
    fixture.detectChanges();
    return fixture;
  }

  it('hosted: opens the custom-tools tab with the mount + path payload', () => {
    const opened: unknown[] = [];
    const handle: WorkspaceHandle = {
      openTab: vi.fn((kind: string, payload?: unknown) => {
        opened.push({ kind, payload });
        return 'tab-x';
      }),
      closeTab: vi.fn(),
      refreshTab: vi.fn(),
    };
    const fixture = render(handle);
    (fixture.componentInstance as unknown as OpenIn).openInWorkbench('Tools/unlock.tool.json');
    expect(opened).toEqual([
      { kind: 'custom-tools', payload: { mountPointId: 'm1', path: 'Tools/unlock.tool.json' } },
    ]);
  });

  it('routed: falls back to the ?mount=&path= navigation', () => {
    const fixture = render();
    const router = TestBed.inject(Router);
    const nav = vi.spyOn(router, 'navigate').mockResolvedValue(true);
    (fixture.componentInstance as unknown as OpenIn).openInWorkbench('Tools/unlock.tool.json');
    expect(nav).toHaveBeenCalledWith(['/custom-tools'], {
      queryParams: { mount: 'm1', path: 'Tools/unlock.tool.json' },
    });
  });
});

/** P4.D358 — v4 `FileTable` (`dff76ebef`): the file name is the row's keyboard handle. */
describe('FileTable — the row name is a disclosure button', () => {
  const file: DocumentStoreFile = {
    id: 'f1',
    mountPointId: 'm1',
    relativePath: 'notes/intro.txt',
    fileName: 'intro.txt',
    fileType: 'txt',
    sha256: 'abc',
    fileSizeBytes: 1024,
    lastModified: '2024-01-01T00:00:00Z',
    conversionStatus: 'converted',
    conversionError: null,
    plainTextLength: 100,
    chunkCount: 3,
    createdAt: '2024-01-01T00:00:00Z',
    updatedAt: '2024-01-01T00:00:00Z',
  };

  it('is role=button, tabindex 0, aria-expanded false; Enter expands it', () => {
    TestBed.configureTestingModule({
      imports: [FileTable],
      providers: [provideRouter([]), { provide: CoreClient, useValue: {} }],
    });
    const fixture = TestBed.createComponent(FileTable);
    fixture.componentRef.setInput('files', [file]);
    fixture.componentRef.setInput('mountPointId', 'm1');
    fixture.componentRef.setInput('mountType', 'filesystem');
    fixture.detectChanges();

    const name = fixture.nativeElement.querySelector('tbody div[role="button"]') as HTMLElement;
    expect(name).toBeTruthy();
    expect(name.textContent?.trim()).toBe('intro.txt');
    expect(name.getAttribute('tabindex')).toBe('0');
    expect(name.getAttribute('aria-expanded')).toBe('false');

    name.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    fixture.detectChanges();
    expect(
      (fixture.nativeElement.querySelector('tbody div[role="button"]') as HTMLElement).getAttribute(
        'aria-expanded',
      ),
    ).toBe('true');
  });
});
