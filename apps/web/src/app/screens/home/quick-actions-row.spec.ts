import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

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
