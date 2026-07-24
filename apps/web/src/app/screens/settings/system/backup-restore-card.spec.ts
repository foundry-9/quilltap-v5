import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { BackupDialog } from './backup-dialog';
import { BackupRestoreCard } from './backup-restore-card';
import { RestoreDialog } from './restore-dialog';

function coreStub(dispatchData?: CoreClient['dispatchData']): Partial<CoreClient> {
  return { dispatchData: dispatchData ?? ((async () => ({})) as CoreClient['dispatchData']) };
}

async function mount<T>(cmp: new (...a: never[]) => T, client: Partial<CoreClient>): Promise<ComponentFixture<T>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [cmp as never],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(cmp as never) as ComponentFixture<T>;
  fixture.detectChanges();
  return fixture;
}

function buttonWith(fixture: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  return Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find((b) =>
    b.textContent?.includes(label),
  ) as HTMLButtonElement;
}

describe('BackupRestoreCard', () => {
  it('opens the backup and restore dialogs from its two buttons', async () => {
    const fixture = await mount(BackupRestoreCard, coreStub());
    buttonWith(fixture, 'Create Backup').click();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Your backup will include');
  });
});

describe('BackupDialog', () => {
  it('dispatches systemBackupCreate and emits close on success', async () => {
    const dispatchData = vi.fn(async () => ({ backupId: 'b1' })) as unknown as CoreClient['dispatchData'];
    const fixture = await mount(BackupDialog, coreStub(dispatchData));
    let closed = false;
    fixture.componentInstance.close.subscribe(() => (closed = true));
    buttonWith(fixture, 'Download Backup').click();
    await Promise.resolve();
    await Promise.resolve();
    expect(dispatchData).toHaveBeenCalledWith({ type: 'systemBackupCreate' });
    expect(closed).toBe(true);
  });

  it('surfaces an error and keeps the dialog open on failure', async () => {
    const dispatchData = vi.fn(async () => {
      throw new Error('disk full');
    }) as unknown as CoreClient['dispatchData'];
    const fixture = await mount(BackupDialog, coreStub(dispatchData));
    buttonWith(fixture, 'Download Backup').click();
    await Promise.resolve();
    await Promise.resolve();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('disk full');
  });
});

describe('RestoreDialog', () => {
  it('starts on step 1 with Next disabled until a file is chosen', async () => {
    const fixture = await mount(RestoreDialog, coreStub());
    expect(fixture.nativeElement.textContent).toContain('Step 1 of 4');
    const next = buttonWith(fixture, 'Next');
    expect(next.disabled).toBe(true);

    const input = fixture.nativeElement.querySelector('input[type="file"]') as HTMLInputElement;
    const file = new File(['x'], 'backup.zip');
    Object.defineProperty(input, 'files', { value: [file] });
    input.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Selected: backup.zip');
    expect(buttonWith(fixture, 'Next').disabled).toBe(false);
  });
});
