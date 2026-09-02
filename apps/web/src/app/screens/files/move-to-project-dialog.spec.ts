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

/** Every promote body the dialog dispatched, in order. */
const promoted: Record<string, unknown>[] = [];

function stubClient(promoteFails?: string): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      if (req.type === 'projectList') {
        return { projects: [{ id: 'p1', name: 'Airship Saga' }] };
      }
      // The folder picker's two reads (v4 `FolderPicker.tsx:69-83`).
      if (req.type === 'filesList') {
        return { files: [{ folderPath: '/Ledgers/Quarterly/' }] };
      }
      if (req.type === 'filesFoldersList') {
        return {
          folders: [
            { id: 'f1', path: '/Ledgers/', name: 'Ledgers' },
            { id: 'f2', path: '/Ledgers/Quarterly/', name: 'Quarterly' },
          ],
        };
      }
      if (req.type === 'filePromote') {
        promoted.push(req);
        if (promoteFails) throw new Error(promoteFails);
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

/** The folder picker's dropdown, once a project destination is chosen. */
function folderSelect(fixture: ComponentFixture<MoveToProjectDialog>): HTMLSelectElement | null {
  return fixture.nativeElement.querySelector('#move-folder');
}

async function chooseProject(fixture: ComponentFixture<MoveToProjectDialog>): Promise<void> {
  select(fixture).value = 'p1';
  select(fixture).dispatchEvent(new Event('change'));
  fixture.detectChanges();
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function moveButton(fixture: ComponentFixture<MoveToProjectDialog>): HTMLButtonElement {
  return (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent?.trim() === 'Move to Project',
  )!;
}

/** v4 `MoveToProjectModal.tsx:73-113` — toast only, no inline surface. */
describe('MoveToProjectDialog toasts', () => {
  afterEach(() => {
    promoted.length = 0;
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

/**
 * The folder field is the real `qt-folder-picker` (v4 `MoveToProjectModal.tsx`
 * renders `<FolderPicker>` once a project destination is chosen), and its value
 * is what the promote body carries.
 */
describe('MoveToProjectDialog folder picker', () => {
  afterEach(() => {
    promoted.length = 0;
    vi.restoreAllMocks();
  });

  it("offers the destination project's real folders, not Root alone", async () => {
    const fixture = await render(stubClient());
    expect(folderSelect(fixture)).toBeNull(); // no destination chosen yet
    await chooseProject(fixture);

    const labels = Array.from(folderSelect(fixture)!.querySelectorAll('option')).map((o) =>
      (o.textContent ?? '').trim().replace(/ \(\d+ files\)$/, ''),
    );
    // `.trim()` strips U+00A0 too, so the indent is asserted on raw textContent.
    expect(labels).toEqual(['/ (Root)', '\u2514 Ledgers', '\u2514 Quarterly']);
    const nested = Array.from(folderSelect(fixture)!.querySelectorAll('option')).find((o) =>
      (o.textContent ?? '').includes('Quarterly'),
    );
    expect(nested?.textContent).toContain('\u00a0\u00a0\u2514 Quarterly');
  });

  it('promotes to the folder the picker selected', async () => {
    const fixture = await render(stubClient());
    await chooseProject(fixture);

    const picker = folderSelect(fixture)!;
    picker.value = '/Ledgers/Quarterly/';
    picker.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    moveButton(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(promoted).toEqual([
      {
        type: 'filePromote',
        fileId: 'f1',
        targetProjectId: 'p1',
        folderPath: '/Ledgers/Quarterly/',
      },
    ]);
  });

  it('resets the folder to Root when the destination changes', async () => {
    const fixture = await render(stubClient());
    await chooseProject(fixture);
    const picker = folderSelect(fixture)!;
    picker.value = '/Ledgers/';
    picker.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    // Re-picking the destination resets the folder (v4 `:167-170`).
    await chooseProject(fixture);
    moveButton(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(promoted).toEqual([
      { type: 'filePromote', fileId: 'f1', targetProjectId: 'p1', folderPath: '/' },
    ]);
  });
});
