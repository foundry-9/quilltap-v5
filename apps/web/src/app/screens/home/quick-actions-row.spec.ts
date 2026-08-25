import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';
import {
  WORKSPACE_HANDLE,
  type WorkspaceHandle,
} from '../../workspace/workspace-contract';
import { QuickActionsRow } from './quick-actions-row';

/**
 * The Generate Image opener arm (p4.9j2 item 6): hosted ⇒ the action opens a
 * `generate-image` workspace tab; routed ⇒ the `/generate-image` routerLink.
 */
describe('QuickActionsRow — Generate Image opener arm', () => {
  function render(handle?: WorkspaceHandle): ComponentFixture<QuickActionsRow> {
    TestBed.configureTestingModule({
      imports: [QuickActionsRow],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        ...(handle ? [{ provide: WORKSPACE_HANDLE, useValue: handle }] : []),
      ],
    });
    const fixture = TestBed.createComponent(QuickActionsRow);
    fixture.componentRef.setInput('lastChatId', null);
    fixture.detectChanges();
    return fixture;
  }

  it('routed: renders the /generate-image link', () => {
    const fixture = render();
    expect(fixture.nativeElement.querySelector('a[href="/generate-image"]')).not.toBeNull();
  });

  it('hosted: the Generate Image button opens the generate-image tab', () => {
    const opened: string[] = [];
    const handle: WorkspaceHandle = {
      openTab: vi.fn((kind: string) => {
        opened.push(kind);
        return 'tab-x';
      }),
      closeTab: vi.fn(),
      refreshTab: vi.fn(),
    };
    const fixture = render(handle);
    // No /generate-image anchor when hosted — it is a button.
    expect(fixture.nativeElement.querySelector('a[href="/generate-image"]')).toBeNull();
    const button = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.includes('Generate Image'),
    ) as HTMLButtonElement;
    expect(button).toBeTruthy();
    button.click();
    expect(opened).toEqual(['generate-image']);
  });
});

/**
 * v4 `c93ec7ff` gave `QuickActionsRow`'s own create handler the two toasts its
 * raw `fetch` never had, with the SAME sentences Prospero's `useProjects` hook
 * already used. v5 reaches both hosts through ONE shared
 * `qt-project-create-dialog`, so these drive the real dialog inside the real
 * quick-actions row and read the sentences off the toast service.
 */
describe('QuickActionsRow — New Project toasts (v4 c93ec7ff)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  async function openAndSubmit(
    dispatchData: () => Promise<Record<string, unknown>>,
  ): Promise<{ success: string[]; error: string[] }> {
    TestBed.configureTestingModule({
      imports: [QuickActionsRow],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: { dispatchData } as Partial<CoreClient> },
      ],
    });
    const fixture = TestBed.createComponent(QuickActionsRow);
    fixture.componentRef.setInput('lastChatId', null);
    fixture.detectChanges();

    const toasts = TestBed.inject(ToastService);
    const success: string[] = [];
    const error: string[] = [];
    vi.spyOn(toasts, 'showSuccess').mockImplementation((m: string) => (success.push(m), 'id'));
    vi.spyOn(toasts, 'showError').mockImplementation((m: string) => (error.push(m), 'id'));

    const newProject = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.includes('New Project'),
    ) as HTMLButtonElement;
    newProject.click();
    fixture.detectChanges();

    // Drive the hosted dialog's real submit path. (The Create button lives in
    // the modal footer and reaches the form through `form="…"`, which jsdom
    // does not implement; and ngModel's write-back does not settle under the
    // zoneless fixture — so the value is set on the dialog and `submit()` runs
    // for real, toasts, `createProject` and all.)
    const dialog = fixture.debugElement.query(By.css('qt-project-create-dialog'))
      .componentInstance as {
      name: { set: (v: string) => void };
      submit: () => Promise<void>;
    };
    dialog.name.set('Mu');
    await dialog.submit();
    for (let i = 0; i < 8; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }

    return { success, error };
  }

  it('a successful create toasts v4’s success sentence and closes the dialog', async () => {
    const { success, error } = await openAndSubmit(async () => ({ project: { id: 'p1' } }));
    expect(success).toEqual(['Project created successfully!']);
    expect(error).toEqual([]);
  });

  it('a failed create toasts v4’s FIXED sentence, never the server’s own', async () => {
    // Bug 98's schema answers `Validation error`; v4 never shows it here —
    // `useProjects` throws its own sentence without reading the body.
    const { success, error } = await openAndSubmit(async () => {
      throw new Error('Validation error');
    });
    expect(success).toEqual([]);
    expect(error).toEqual(['Failed to create project']);
  });
});
