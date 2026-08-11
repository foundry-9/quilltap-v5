import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { DeleteDataCard } from './delete-data-card';

const SUMMARY = {
  characters: 3,
  chats: 2,
  tags: 1,
  files: 4,
  memories: 10,
  apiKeys: 1,
  backups: 0,
  projects: 2,
  profiles: { connection: 1, image: 0, embedding: 0 },
  templates: { prompt: 0, roleplay: 0 },
};

interface Stub {
  calls: { type: string; [k: string]: unknown }[];
  client: Partial<CoreClient>;
}

function stub(over: Record<string, unknown> = {}): Stub {
  const calls: { type: string; [k: string]: unknown }[] = [];
  const dispatchData = vi.fn(async (req: { type: string }) => {
    calls.push(req);
    return { summary: { ...SUMMARY, ...over } };
  });
  return { calls, client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] } };
}

async function mount(s: Stub): Promise<ComponentFixture<DeleteDataCard>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [DeleteDataCard],
    providers: [{ provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(DeleteDataCard);
  fixture.detectChanges();
  return fixture;
}

function buttonWith(fixture: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  return Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
    (b) => b.textContent?.trim() === label,
  ) as HTMLButtonElement;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('DeleteDataCard', () => {
  it('previews on open and computes a real (non-NaN) total', async () => {
    const s = stub();
    const fixture = await mount(s);
    buttonWith(fixture, 'Delete All Data').click();
    await settle(fixture);
    expect(s.calls).toContainEqual({ type: 'systemDeleteDataPreview' });
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('Projects'); // the row v4 dropped
    // 3+2+1+4+10+1+0+2 + 1+0+0 = 24 (templates 0)
    expect(text).toContain('24');
    expect(text).not.toContain('NaN');
  });

  it('gates the delete behind the typed DELETE and sends the wire token', async () => {
    const s = stub();
    const fixture = await mount(s);
    buttonWith(fixture, 'Delete All Data').click();
    await settle(fixture);
    buttonWith(fixture, 'Continue').click();
    fixture.detectChanges();

    const del = buttonWith(fixture, 'Delete Everything');
    expect(del.disabled).toBe(true);

    const input = fixture.nativeElement.querySelector('input[type="text"]') as HTMLInputElement;
    input.value = 'delete'; // lower-case — the component upper-cases it
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(buttonWith(fixture, 'Delete Everything').disabled).toBe(false);

    buttonWith(fixture, 'Delete Everything').click();
    await settle(fixture);
    // P4.D64: v4's client ALWAYS sends the spare-bundles flag, defaulted true.
    expect(s.calls).toContainEqual({
      type: 'systemDeleteData',
      confirm: 'DELETE_ALL_MY_DATA',
      keepArchivedCharacterBundles: true,
    });
    expect(fixture.nativeElement.textContent).toContain('Successfully deleted 24 items');
  });
});

// ===========================================================================
// P4.D64 — sparing the archived-character bundles (v4 `delete-data-card.tsx`)
// ===========================================================================

describe('DeleteDataCard — the archived-bundle option (P4.D64)', () => {
  async function openPreview(over: Record<string, unknown>): Promise<{
    fixture: ComponentFixture<DeleteDataCard>;
    s: Stub;
  }> {
    const s = stub(over);
    const fixture = await mount(s);
    buttonWith(fixture, 'Delete All Data').click();
    await settle(fixture);
    return { fixture, s };
  }

  it('says nothing about bundles when the shelf is empty', async () => {
    const { fixture } = await openPreview({});
    const text = fixture.nativeElement.textContent as string;
    expect(text).not.toContain('Archived Character Bundles');
    expect(text).not.toContain('on the shelf');
  });

  it('reports the bundles, marks them kept, and offers the untick with v4 copy', async () => {
    const { fixture } = await openPreview({ archiveBundles: 2, archiveBundlesKept: true });
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Archived Character Bundles');
    expect(text).toContain('(kept)');
    expect(text).toContain('Leave the archived-character bundles on the shelf (2 files)');
    expect(text).toContain(
      'Their character records perish with everything else, so what remains is a loose bundle apiece — each may be imported afresh, but none can simply be woken. Untick to sweep the shelf bare as well.',
    );
    // The bundles are NOT in the total — v4 reports `files` net of them.
    expect(text).toContain('24');
  });

  it('singularizes one file', async () => {
    const { fixture } = await openPreview({ archiveBundles: 1, archiveBundlesKept: true });
    expect(fixture.nativeElement.textContent).toContain(
      'Leave the archived-character bundles on the shelf (1 file)',
    );
  });

  it('drops the (kept) marker and sends false once unticked', async () => {
    const { fixture, s } = await openPreview({ archiveBundles: 2, archiveBundlesKept: true });
    const checkbox = fixture.nativeElement.querySelector(
      'input[type="checkbox"]',
    ) as HTMLInputElement;
    expect(checkbox.checked).toBe(true);
    checkbox.checked = false;
    checkbox.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).not.toContain('(kept)');

    buttonWith(fixture, 'Continue').click();
    fixture.detectChanges();
    const input = fixture.nativeElement.querySelector('input[type="text"]') as HTMLInputElement;
    input.value = 'DELETE';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    buttonWith(fixture, 'Delete Everything').click();
    await settle(fixture);
    expect(s.calls).toContainEqual({
      type: 'systemDeleteData',
      confirm: 'DELETE_ALL_MY_DATA',
      keepArchivedCharacterBundles: false,
    });
  });

  it('notes the surviving bundles on the completion screen, pluralized', async () => {
    const { fixture } = await openPreview({ archiveBundles: 2, archiveBundlesKept: true });
    buttonWith(fixture, 'Continue').click();
    fixture.detectChanges();
    const input = fixture.nativeElement.querySelector('input[type="text"]') as HTMLInputElement;
    input.value = 'DELETE';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    buttonWith(fixture, 'Delete Everything').click();
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain(
      '2 archived-character bundles remain on the shelf, ready for import.',
    );
  });

  it('says nothing on completion when the shelf was swept (kept false)', async () => {
    const { fixture } = await openPreview({ archiveBundles: 2, archiveBundlesKept: false });
    buttonWith(fixture, 'Continue').click();
    fixture.detectChanges();
    const input = fixture.nativeElement.querySelector('input[type="text"]') as HTMLInputElement;
    input.value = 'DELETE';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    buttonWith(fixture, 'Delete Everything').click();
    await settle(fixture);
    expect(fixture.nativeElement.textContent).not.toContain('on the shelf, ready for import');
  });
});
