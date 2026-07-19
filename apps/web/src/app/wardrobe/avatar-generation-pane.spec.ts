import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { AvatarGenerationPane, type ImageProfileSummary } from './avatar-generation-pane';

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
    const anchor = el.querySelector('a') as HTMLAnchorElement;
    expect(anchor.getAttribute('download')).toBe('Riya_preview.webp');
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
