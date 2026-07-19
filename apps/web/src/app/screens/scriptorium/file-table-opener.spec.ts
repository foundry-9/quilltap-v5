import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import {
  WORKSPACE_HANDLE,
  type WorkspaceHandle,
} from '../../workspace/workspace-contract';
import { FileTable } from './file-table';

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
