import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { ToastService } from '../../../ui/toast.service';
import { RestoreDialog } from './restore-dialog';

/**
 * P4.D64 — the replace-mode "leave the bundles on the shelf" option (v4
 * `RestoreDialog.tsx:323-341` + `useRestoreData.ts:21,:182` at `d553f72a`).
 *
 * Reaching the mode step means getting past the raw octet-stream XHR upload, so
 * `XMLHttpRequest` is stubbed to a 200 with an uploadId — the same shape the real
 * web-edge leg answers with. That is the only fake here; everything after it is
 * the real component.
 */

class FakeXhr {
  static lastBody: unknown = null;
  status = 200;
  responseText = JSON.stringify({ success: true, uploadId: 'up-1' });
  readonly upload: { onprogress: ((e: unknown) => void) | null } = { onprogress: null };
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onabort: (() => void) | null = null;
  open(): void {}
  setRequestHeader(): void {}
  send(body: unknown): void {
    FakeXhr.lastBody = body;
    // Async, so the component's promise plumbing behaves as it does live.
    setTimeout(() => this.onload?.(), 0);
  }
}

function stub(): { calls: Array<Record<string, unknown>>; client: Partial<CoreClient> } {
  const calls: Array<Record<string, unknown>> = [];
  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    calls.push(req);
    if (req['type'] === 'systemRestorePreview') {
      return { preview: { characters: 1, chats: 1, messages: 2, tags: 0, memories: 0 } };
    }
    return { summary: { characters: 1, chats: 1, messages: 2, files: 0 } };
  });
  return {
    calls,
    client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] },
  };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function buttonWith(fixture: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  return Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
    (b) => b.textContent?.trim() === label,
  ) as HTMLButtonElement;
}

/** Mount, pick a file, upload+preview, and land on the mode step. */
async function toModeStep(client: Partial<CoreClient>): Promise<ComponentFixture<RestoreDialog>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [RestoreDialog],
    providers: [{ provide: CoreClient, useValue: client }, ToastService],
  });
  const fixture = TestBed.createComponent(RestoreDialog);
  fixture.detectChanges();

  const input = fixture.nativeElement.querySelector('input[type="file"]') as HTMLInputElement;
  const file = new File(['zipbytes'], 'backup.zip', { type: 'application/zip' });
  Object.defineProperty(input, 'files', { value: [file], configurable: true });
  input.dispatchEvent(new Event('change'));
  fixture.detectChanges();

  buttonWith(fixture, 'Next').click();
  await settle(fixture);
  buttonWith(fixture, 'Next').click();
  await settle(fixture);
  return fixture;
}

describe('RestoreDialog — sparing the archived-character bundles (P4.D64)', () => {
  const original = globalThis.XMLHttpRequest;

  afterEach(() => {
    globalThis.XMLHttpRequest = original;
    vi.restoreAllMocks();
  });

  function installXhr(): void {
    globalThis.XMLHttpRequest = FakeXhr as unknown as typeof XMLHttpRequest;
  }

  it('offers the option in replace mode only, ticked, with v4 copy', async () => {
    installXhr();
    const fixture = await toModeStep(stub().client);
    // Import mode (the default) says nothing about bundles.
    expect(fixture.nativeElement.textContent).not.toContain('archived-character');

    const replace = fixture.nativeElement.querySelector('input[value="replace"]') as HTMLInputElement;
    replace.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Leave any archived-character bundles on the shelf');
    expect(text).toContain(
      "The wipe that precedes the restore will spare each archived character's .qtap bundle. Their character records are replaced with the backup's like everything else, so a spared bundle is a loose one — importable afresh, but not simply woken. Untick to sweep the shelf bare as well.",
    );
    const boxes = Array.from(
      fixture.nativeElement.querySelectorAll('input[type="checkbox"]'),
    ) as HTMLInputElement[];
    // Two checkboxes in replace mode: the undo confirmation and this one, which
    // is ticked by default (the server's effective default too).
    expect(boxes).toHaveLength(2);
    expect(boxes[1].checked).toBe(true);
  });

  it('sends the flag in IMPORT mode as well — v4 sends it either way', async () => {
    installXhr();
    const s = stub();
    const fixture = await toModeStep(s.client);
    buttonWith(fixture, 'Start Restore').click();
    await settle(fixture);
    expect(s.calls).toContainEqual({
      type: 'systemRestoreExecute',
      uploadId: 'up-1',
      mode: 'new-account',
      keepArchivedCharacterBundles: true,
    });
  });

  it('sends false once unticked in replace mode', async () => {
    installXhr();
    const s = stub();
    const fixture = await toModeStep(s.client);
    (fixture.nativeElement.querySelector('input[value="replace"]') as HTMLInputElement).dispatchEvent(
      new Event('change'),
    );
    fixture.detectChanges();
    const boxes = Array.from(
      fixture.nativeElement.querySelectorAll('input[type="checkbox"]'),
    ) as HTMLInputElement[];
    // Confirm the destructive replace, then untick the sparing option.
    boxes[0].checked = true;
    boxes[0].dispatchEvent(new Event('change'));
    boxes[1].checked = false;
    boxes[1].dispatchEvent(new Event('change'));
    fixture.detectChanges();

    buttonWith(fixture, 'Start Restore').click();
    await settle(fixture);
    expect(s.calls).toContainEqual({
      type: 'systemRestoreExecute',
      uploadId: 'up-1',
      mode: 'replace',
      keepArchivedCharacterBundles: false,
    });
  });

  it('re-ticks nothing when the mode flips — only the undo confirmation clears', async () => {
    installXhr();
    const fixture = await toModeStep(stub().client);
    const replace = fixture.nativeElement.querySelector('input[value="replace"]') as HTMLInputElement;
    replace.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    let boxes = Array.from(
      fixture.nativeElement.querySelectorAll('input[type="checkbox"]'),
    ) as HTMLInputElement[];
    boxes[1].checked = false;
    boxes[1].dispatchEvent(new Event('change'));
    fixture.detectChanges();

    // Away and back: v4's `setRestoreMode` clears `confirmReplace` and NOTHING
    // else, so an unticked sparing option must stay unticked.
    (fixture.nativeElement.querySelector('input[value="import"]') as HTMLInputElement).dispatchEvent(
      new Event('change'),
    );
    fixture.detectChanges();
    replace.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    boxes = Array.from(
      fixture.nativeElement.querySelectorAll('input[type="checkbox"]'),
    ) as HTMLInputElement[];
    expect(boxes[0].checked).toBe(false);
    expect(boxes[1].checked).toBe(false);
  });
});
