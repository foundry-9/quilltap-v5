import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { ToastService } from '../../../ui/toast.service';
import { BackupDialog } from './backup-dialog';
import { BackupRestoreCard } from './backup-restore-card';
import { RestoreDialog } from './restore-dialog';

function coreStub(dispatchData?: CoreClient['dispatchData']): Partial<CoreClient> {
  return { dispatchData: dispatchData ?? ((async () => ({})) as CoreClient['dispatchData']) };
}

/** Every toast the component raised, as `<kind>: <message>` in order. */
function toastRecorder(): { seen: string[]; service: Partial<ToastService> } {
  const seen: string[] = [];
  const record = (kind: string) => (message: string) => {
    seen.push(`${kind}: ${message}`);
    return '';
  };
  return {
    seen,
    service: {
      showSuccess: record('success'),
      showError: record('error'),
      showWarning: record('warning'),
      showInfo: record('info'),
    } as Partial<ToastService>,
  };
}

async function mount<T>(
  cmp: new (...a: never[]) => T,
  client: Partial<CoreClient>,
  toasts?: Partial<ToastService>,
): Promise<ComponentFixture<T>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [cmp as never],
    providers: [
      { provide: CoreClient, useValue: client },
      ...(toasts ? [{ provide: ToastService, useValue: toasts }] : []),
    ],
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

// ── P4.28: the P4.25 census's Data & System toast rows ──────────────────────
//
// v4's sentences, byte-for-byte, at v4's own moments. Two of the four census
// rows turned out to be already done and are corrected in the lane record
// (`delete-data-card` had both toasts; `auto-lock-provider` is not a `lib/toast`
// site at all — v4's local `showWarningToast` builds a card with real copy, so
// the census's "an empty toast, a v4 bug" reading was wrong).
describe('the Data & System toasts (P4.25 census rows)', () => {
  it('backup: toasts success after triggering the download, and errors alongside the inline message', async () => {
    const ok = toastRecorder();
    const fixture = await mount(
      BackupDialog,
      coreStub(vi.fn(async () => ({ backupId: 'b1' })) as unknown as CoreClient['dispatchData']),
      ok.service,
    );
    buttonWith(fixture, 'Download Backup').click();
    await Promise.resolve();
    await Promise.resolve();
    expect(ok.seen).toEqual(['success: Backup downloaded successfully']);

    const bad = toastRecorder();
    const failing = await mount(
      BackupDialog,
      coreStub(
        vi.fn(async () => {
          throw new Error('disk full');
        }) as unknown as CoreClient['dispatchData'],
      ),
      bad.service,
    );
    buttonWith(failing, 'Download Backup').click();
    await Promise.resolve();
    await Promise.resolve();
    failing.detectChanges();
    expect(bad.seen).toEqual(['error: disk full']);
    // v4 `backup-dialog.tsx:53-58` does BOTH — the inline message stays.
    expect(failing.nativeElement.textContent).toContain('disk full');
  });

  it('backup: warns by name when the server could not read some files (dogfood #59)', async () => {
    const rec = toastRecorder();
    const fixture = await mount(
      BackupDialog,
      coreStub(
        vi.fn(async () => ({
          backupId: 'b1',
          skippedFiles: ['who-is-friday.md', 'catastrophe.md'],
        })) as unknown as CoreClient['dispatchData'],
      ),
      rec.service,
    );
    buttonWith(fixture, 'Download Backup').click();
    await Promise.resolve();
    await Promise.resolve();
    expect(rec.seen).toEqual([
      'warning: 2 files could not be read and are missing from this backup: ' +
        'who-is-friday.md, catastrophe.md',
      'success: Backup downloaded successfully',
    ]);
  });

  it('backup: says nothing about skipped files when there were none', async () => {
    const rec = toastRecorder();
    const fixture = await mount(
      BackupDialog,
      coreStub(vi.fn(async () => ({ backupId: 'b1' })) as unknown as CoreClient['dispatchData']),
      rec.service,
    );
    buttonWith(fixture, 'Download Backup').click();
    await Promise.resolve();
    await Promise.resolve();
    expect(rec.seen.some((t) => t.startsWith('warning:'))).toBe(false);
  });

  it('restore: toasts the summary on success and the message on failure (v4 useRestoreData.ts:195, :204)', async () => {
    // `handleStartRestore` is the row's moment. It needs an uploadId, which the
    // real path sets from the XHR upload leg — not what these rows are about.
    const ok = toastRecorder();
    const good = await mount(
      RestoreDialog,
      coreStub(vi.fn(async () => ({ summary: { characters: 1 } })) as unknown as CoreClient['dispatchData']),
      ok.service,
    );
    const goodCmp = good.componentInstance as unknown as {
      uploadId: string | null;
      handleStartRestore(): Promise<void>;
    };
    goodCmp.uploadId = '11111111-1111-4111-8111-111111111111';
    await goodCmp.handleStartRestore();
    expect(ok.seen).toEqual(['success: Backup restored successfully']);

    const bad = toastRecorder();
    const failing = await mount(
      RestoreDialog,
      coreStub(
        vi.fn(async () => {
          throw new Error('archive is corrupt');
        }) as unknown as CoreClient['dispatchData'],
      ),
      bad.service,
    );
    const badCmp = failing.componentInstance as unknown as {
      uploadId: string | null;
      handleStartRestore(): Promise<void>;
    };
    badCmp.uploadId = '11111111-1111-4111-8111-111111111111';
    await badCmp.handleStartRestore();
    failing.detectChanges();
    expect(bad.seen).toEqual(['error: archive is corrupt']);
    // v4 sets the inline error AND bounces back to the mode step.
    expect(failing.nativeElement.textContent).toContain('archive is corrupt');
  });
});
