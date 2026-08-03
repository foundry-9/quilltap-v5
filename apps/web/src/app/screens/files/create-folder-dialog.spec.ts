import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';
import { CreateFolderDialog } from './create-folder-dialog';

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

function stubClient(result: Record<string, unknown> | Error): Partial<CoreClient> {
  return {
    dispatchData: (async () => {
      if (result instanceof Error) throw result;
      return result;
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<CreateFolderDialog>> {
  TestBed.configureTestingModule({
    imports: [CreateFolderDialog],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(CreateFolderDialog);
  fixture.componentRef.setInput('currentFolder', '/');
  fixture.detectChanges();
  return fixture;
}

function nameInput(fixture: ComponentFixture<CreateFolderDialog>): HTMLInputElement {
  return fixture.nativeElement.querySelector('#folder-name');
}

function createButton(fixture: ComponentFixture<CreateFolderDialog>): HTMLButtonElement {
  return (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent?.trim() === 'Create Folder',
  )!;
}

/** v4 `CreateFolderModal.tsx:41-84` — all three outcomes are toast only. */
describe('CreateFolderDialog toasts', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('toasts "Folder name cannot be empty" via Enter on a blank field (the button is disabled)', async () => {
    const fixture = await render(stubClient({}));
    nameInput(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'error', message: 'Folder name cannot be empty' }]);
  });

  it('toasts "Folder created" on success and emits success/close', async () => {
    const fixture = await render(stubClient({ folder: { path: '/pics/' } }));
    let success: string | undefined;
    let closed = 0;
    fixture.componentInstance.success.subscribe((p) => (success = p));
    fixture.componentInstance.close.subscribe(() => closed++);

    nameInput(fixture).value = 'pics';
    nameInput(fixture).dispatchEvent(new Event('input'));
    fixture.detectChanges();
    createButton(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(success).toBe('/pics/');
    expect(closed).toBe(1);
    expect(toasts()).toEqual([{ type: 'success', message: 'Folder created' }]);
  });

  it('toasts "Folder already exists" when the server says so', async () => {
    const fixture = await render(stubClient({ folder: { path: '/pics/' }, alreadyExists: true }));
    nameInput(fixture).value = 'pics';
    nameInput(fixture).dispatchEvent(new Event('input'));
    fixture.detectChanges();
    createButton(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'success', message: 'Folder already exists' }]);
  });

  it('toasts the server message on a failed create', async () => {
    const fixture = await render(stubClient(new Error('disk is full')));
    nameInput(fixture).value = 'pics';
    nameInput(fixture).dispatchEvent(new Event('input'));
    fixture.detectChanges();
    createButton(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'error', message: 'disk is full' }]);
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeNull();
  });
});
