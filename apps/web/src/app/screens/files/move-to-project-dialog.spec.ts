import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';
import { MoveToProjectDialog } from './move-to-project-dialog';

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

function stubClient(promoteFails?: string): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      if (req.type === 'projectList') {
        return { projects: [{ id: 'p1', name: 'Airship Saga' }] };
      }
      if (req.type === 'filePromote' && promoteFails) {
        throw new Error(promoteFails);
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<MoveToProjectDialog>> {
  TestBed.configureTestingModule({
    imports: [MoveToProjectDialog],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(MoveToProjectDialog);
  fixture.componentRef.setInput('fileId', 'f1');
  fixture.componentRef.setInput('fileName', 'notes.txt');
  fixture.detectChanges();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function select(fixture: ComponentFixture<MoveToProjectDialog>): HTMLSelectElement {
  return fixture.nativeElement.querySelector('#move-to-project');
}

function moveButton(fixture: ComponentFixture<MoveToProjectDialog>): HTMLButtonElement {
  return (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent?.trim() === 'Move to Project',
  )!;
}

/** v4 `MoveToProjectModal.tsx:73-113` — toast only, no inline surface. */
describe('MoveToProjectDialog toasts', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('toasts "<file> moved to <project>" on success', async () => {
    const fixture = await render(stubClient());
    select(fixture).value = 'p1';
    select(fixture).dispatchEvent(new Event('change'));
    fixture.detectChanges();
    moveButton(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'success', message: '"notes.txt" moved to Airship Saga' }]);
  });

  it('toasts the server message on a failed move, with no inline banner', async () => {
    const fixture = await render(stubClient('the archive is locked'));
    select(fixture).value = 'p1';
    select(fixture).dispatchEvent(new Event('change'));
    fixture.detectChanges();
    moveButton(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'error', message: 'the archive is locked' }]);
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeNull();
  });
});
