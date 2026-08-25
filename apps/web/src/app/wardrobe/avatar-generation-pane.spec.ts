import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { AvatarGenerationPane, type ImageProfileSummary } from './avatar-generation-pane';
import { ToastService } from '../ui/toast.service';

const PROFILES: ImageProfileSummary[] = [
  { id: 'ip1', name: 'Painterly', provider: 'openai', modelName: 'gpt-image-1', isDefault: true },
  { id: 'ip2', name: 'Photo', provider: 'google', modelName: 'imagen-4', isDefault: false },
];

function render(inputs: {
  inChat: boolean;
  imageProfiles?: ImageProfileSummary[];
  selectedImageProfileId?: string | null;
  generating?: boolean;
  previewUrl?: string | null;
  previewFilename?: string | null;
}): ComponentFixture<AvatarGenerationPane> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [AvatarGenerationPane] });
  const fixture = TestBed.createComponent(AvatarGenerationPane);
  fixture.componentRef.setInput('selectedCharacterName', 'Riya');
  fixture.componentRef.setInput('imageProfiles', inputs.imageProfiles ?? PROFILES);
  fixture.componentRef.setInput(
    'selectedImageProfileId',
    inputs.selectedImageProfileId ?? 'ip1',
  );
  fixture.componentRef.setInput('generating', inputs.generating ?? false);
  fixture.componentRef.setInput('inChat', inputs.inChat);
  fixture.componentRef.setInput('previewUrl', inputs.previewUrl ?? null);
  fixture.componentRef.setInput('previewFilename', inputs.previewFilename ?? null);
  fixture.detectChanges();
  return fixture;
}

describe('AvatarGenerationPane (v4 wardrobe-control-dialog.tsx:1291-1391)', () => {
  it('labels options with v4\'s "{name}{ (default)} — {provider}/{modelName}" format (:1330-1333)', () => {
    const fixture = render({ inChat: false });
    const options = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('option'),
    ).map((o) => o.textContent?.replace(/\s+/g, ' ').trim());
    expect(options).toEqual([
      'Painterly (default) — openai/gpt-image-1',
      'Photo — google/imagen-4',
    ]);
  });

  it('the button reads Generate avatar in chat, Preview out of chat, Generating… while busy (:1342)', () => {
    expect(((render({ inChat: true }).nativeElement as HTMLElement).textContent ?? '')).toContain(
      'Generate avatar',
    );
    const out = render({ inChat: false });
    expect(((out.nativeElement as HTMLElement).textContent ?? '')).toContain('Preview');
    const busy = render({ inChat: false, generating: true });
    expect(((busy.nativeElement as HTMLElement).textContent ?? '')).toContain('Generating…');
  });

  it('disables generation with no profiles and shows the empty option (:1324-1340)', () => {
    const fixture = render({ inChat: false, imageProfiles: [], selectedImageProfileId: null });
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('No image profiles configured');
    const button = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => (b.textContent ?? '').includes('Preview')) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
  });

  it('the preview block renders ONLY out of chat, with Download + discard (:1352-1388)', () => {
    const inChat = render({ inChat: true, previewUrl: 'blob:x' });
    expect(((inChat.nativeElement as HTMLElement).textContent ?? '')).not.toContain('Download');

    const fixture = render({
      inChat: false,
      previewUrl: '/api/v1/files/f1?action=download',
      previewFilename: 'Riya_preview.webp',
    });
    const el = fixture.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Download');
    // v4 `af1bc479` retired the hidden anchor — the button fetches the bytes
    // and hands them to the shared download util.
    expect(el.querySelector('a')).toBeNull();
    const discardSpy = vi.fn();
    fixture.componentInstance.discardPreview.subscribe(discardSpy);
    (el.querySelector('button[aria-label="Discard preview"]') as HTMLButtonElement).click();
    expect(discardSpy).toHaveBeenCalledTimes(1);
  });

  it('microcopy matches v4 (:1346-1350)', () => {
    expect(((render({ inChat: true }).nativeElement as HTMLElement).textContent ?? '')).toContain(
      "Replaces this chat's avatar with the staged outfit.",
    );
    expect(((render({ inChat: false }).nativeElement as HTMLElement).textContent ?? '')).toContain(
      'Generates a one-off preview. Download to keep.',
    );
  });
});

describe('the preview download goes through the shared util (v4 af1bc479)', () => {
  afterEach(() => vi.restoreAllMocks());

  function clickDownload(fixture: ComponentFixture<AvatarGenerationPane>): void {
    const button = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => (b.textContent ?? '').trim() === 'Download') as HTMLButtonElement;
    button.click();
  }

  /**
   * `triggerBlobDownload` is an ESM binding and cannot be spied, so the probe
   * sits one level lower — at the object URL and the anchor click the util
   * itself performs. That also proves the BYTES travelled, not just that a
   * function was called (the characters-list export spec's idiom).
   */
  function stubDownloadPlumbing(): { blobs: Blob[]; names: string[] } {
    const blobs: Blob[] = [];
    const names: string[] = [];
    vi.spyOn(URL, 'createObjectURL').mockImplementation((blob: Blob | MediaSource) => {
      blobs.push(blob as Blob);
      return 'blob:mock';
    });
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      names.push(this.download);
    });
    return { blobs, names };
  }

  it('fetches the preview bytes and hands the blob to the download util', async () => {
    const blob = new Blob(['preview-bytes']);
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ ok: true, blob: async () => blob }) as unknown as Response),
    );
    const probe = stubDownloadPlumbing();
    clickDownload(
      render({
        inChat: false,
        previewUrl: '/api/v1/files/f1?action=download',
        previewFilename: 'Riya_preview.webp',
      }),
    );
    await new Promise((r) => setTimeout(r, 0));
    expect(fetch).toHaveBeenCalledWith('/api/v1/files/f1?action=download');
    expect(probe.blobs).toEqual([blob]);
    expect(probe.names).toEqual(['Riya_preview.webp']);
  });

  it("falls back to v4's default filename when none was minted", async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ ok: true, blob: async () => new Blob(['x']) }) as unknown as Response),
    );
    const probe = stubDownloadPlumbing();
    clickDownload(render({ inChat: false, previewUrl: 'blob:x' }));
    await new Promise((r) => setTimeout(r, 0));
    expect(probe.names).toEqual(['avatar-preview.webp']);
  });

  it("a failed fetch raises v4's sentence and downloads nothing", async () => {
    vi.stubGlobal('fetch', vi.fn(async () => ({ ok: false, status: 404 }) as unknown as Response));
    vi.spyOn(console, 'error').mockImplementation(() => {});
    const probe = stubDownloadPlumbing();
    clickDownload(render({ inChat: false, previewUrl: 'blob:x' }));
    await new Promise((r) => setTimeout(r, 0));
    expect(probe.names).toEqual([]);
    expect(
      TestBed.inject(ToastService)
        .toasts()
        .map((t) => ({ type: t.type, message: t.message }))
        .at(-1),
    ).toEqual({ type: 'error', message: 'Failed to download avatar preview' });
  });
});
