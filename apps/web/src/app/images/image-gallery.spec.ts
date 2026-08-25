import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ImageGallery } from './image-gallery';
import type { ImageData } from './images.api';

function image(over: Partial<ImageData>): ImageData {
  return {
    id: 'valid-image-1',
    filename: 'valid1.png',
    filepath: '/uploads/valid1.png',
    url: '/uploads/valid1.png',
    mimeType: 'image/png',
    size: 1024,
    width: 800,
    height: 600,
    createdAt: '2025-01-01T00:00:00Z',
    ...over,
  };
}

const mockImages: ImageData[] = [
  image({}),
  image({ id: 'missing-image-1', filename: 'missing1.png', filepath: '/uploads/missing1.png', url: '/uploads/missing1.png' }),
];

/**
 * v4 `__tests__/unit/image-gallery-deleted-handling.test.tsx` (254 lines)
 * ported case-for-case. v4's component self-fetches
 * `GET /api/v1/images?tagType&tagId` — a LIST route v5 has not ported — so
 * the v5 component takes `images`/`loading`/`error` as inputs (the loud
 * data-ownership divergence in its header) and the loading/error/empty cases
 * drive the inputs; the reload-after-cleanup case asserts the `reloadImages`
 * output where v4 asserts the refetch URL.
 */
describe('ImageGallery — deleted image handling', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  async function render(
    inputs: Record<string, unknown> = { images: mockImages },
  ): Promise<{ fixture: ComponentFixture<ImageGallery>; reloads: number[] }> {
    TestBed.configureTestingModule({ imports: [ImageGallery] });
    const fixture = TestBed.createComponent(ImageGallery);
    for (const [key, value] of Object.entries(inputs)) {
      fixture.componentRef.setInput(key, value);
    }
    const reloads: number[] = [];
    fixture.componentInstance.reloadImages.subscribe(() => reloads.push(1));
    fixture.detectChanges();
    for (let i = 0; i < 3; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    return { fixture, reloads };
  }

  function imgs(fixture: ComponentFixture<ImageGallery>): HTMLImageElement[] {
    return Array.from(fixture.nativeElement.querySelectorAll('img'));
  }

  async function flush(fixture: ComponentFixture<ImageGallery>): Promise<void> {
    for (let i = 0; i < 3; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  }

  describe('error detection', () => {
    // v4 :86 — onError swaps the tile for the placeholder.
    it('detects image load errors via the error handler', async () => {
      const { fixture } = await render();
      expect(imgs(fixture).length).toBe(2);
      imgs(fixture)[1].dispatchEvent(new Event('error'));
      await flush(fixture);
      const placeholder = fixture.nativeElement.querySelector('qt-deleted-image-placeholder');
      expect(placeholder).toBeTruthy();
      expect(placeholder.textContent).toContain('missing1.png');
    });

    // v4 :104 — zero-dimension onLoad detection (image-gallery.tsx:149-155).
    it('detects images with zero natural dimensions via the load handler', async () => {
      const { fixture } = await render();
      const target = imgs(fixture)[1];
      Object.defineProperty(target, 'naturalWidth', { value: 0, configurable: true });
      Object.defineProperty(target, 'naturalHeight', { value: 0, configurable: true });
      target.dispatchEvent(new Event('load'));
      await flush(fixture);
      expect(fixture.nativeElement.querySelector('qt-deleted-image-placeholder')).toBeTruthy();
    });

    // v4 :123 — no placeholder for a successfully loaded image.
    it('shows no placeholder for successfully loaded images', async () => {
      const { fixture } = await render();
      const target = imgs(fixture)[0];
      Object.defineProperty(target, 'naturalWidth', { value: 800, configurable: true });
      Object.defineProperty(target, 'naturalHeight', { value: 600, configurable: true });
      target.dispatchEvent(new Event('load'));
      await flush(fixture);
      expect(fixture.nativeElement.querySelector('qt-deleted-image-placeholder')).toBeNull();
    });
  });

  describe('cleanup functionality', () => {
    // v4 :143 — reload after cleanup (v5: the reloadImages output fires).
    it('requests a reload after a placeholder cleanup', async () => {
      vi.spyOn(window, 'confirm').mockReturnValue(true);
      vi.stubGlobal(
        'fetch',
        vi.fn(async () => ({ ok: true, json: async () => ({ success: true }) })),
      );
      const { fixture, reloads } = await render();
      imgs(fixture)[1].dispatchEvent(new Event('error'));
      await flush(fixture);

      const removeButton = Array.from(
        fixture.nativeElement.querySelectorAll('qt-deleted-image-placeholder button'),
      ).find((b) => /remove/i.test((b as HTMLElement).textContent ?? '')) as HTMLButtonElement;
      removeButton.click();
      await flush(fixture);

      expect(reloads.length).toBe(1);
    });
  });

  describe('UI state management', () => {
    // v4 :177 — the delete overlay is hidden for missing tiles.
    it('hides the delete overlay for missing images', async () => {
      const { fixture } = await render({ images: [mockImages[1]] });
      expect(fixture.nativeElement.querySelector('button[aria-label="Delete image"]')).toBeTruthy();
      imgs(fixture)[0].dispatchEvent(new Event('error'));
      await flush(fixture);
      expect(fixture.nativeElement.querySelector('button[aria-label="Delete image"]')).toBeNull();
    });

    // v4 :200 — separate per-image state.
    it('maintains separate state for multiple missing images', async () => {
      const { fixture } = await render();
      imgs(fixture)[0].dispatchEvent(new Event('error'));
      imgs(fixture)[1].dispatchEvent(new Event('error'));
      await flush(fixture);
      expect(
        fixture.nativeElement.querySelectorAll('qt-deleted-image-placeholder').length,
      ).toBe(2);
    });
  });

  describe('loading states', () => {
    // v4 :221 — loading state.
    it('shows the loading state', async () => {
      const { fixture } = await render({ images: [], loading: true });
      expect(fixture.nativeElement.textContent).toContain('Loading images...');
    });

    // v4 :231 — error state.
    it('shows the error state on load failure', async () => {
      const { fixture } = await render({ images: [], error: 'Failed to load' });
      expect(fixture.nativeElement.textContent).toContain('Error: Failed to load');
    });

    // v4 :241 — empty state.
    it('shows the empty state when there are no images', async () => {
      const { fixture } = await render({ images: [] });
      expect(fixture.nativeElement.textContent).toContain('No images found');
    });
  });
});

/**
 * The hover download button (v4 `af1bc479`). This component still has NO v5
 * host (see its header), so the proof is spec-only — the mirror stays in step
 * with v4 rather than drifting until a host lands.
 */
describe('ImageGallery — hover download (v4 af1bc479)', () => {
  let clicked: { download: string; href: string } | null;

  beforeEach(() => {
    clicked = null;
    (URL as unknown as { createObjectURL: unknown }).createObjectURL = () => 'blob:tile';
    (URL as unknown as { revokeObjectURL: unknown }).revokeObjectURL = () => {};
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      clicked = { download: this.download, href: this.href };
    });
  });
  afterEach(() => {
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  async function render(images: ImageData[]): Promise<ComponentFixture<ImageGallery>> {
    TestBed.configureTestingModule({ imports: [ImageGallery] });
    const fixture = TestBed.createComponent(ImageGallery);
    fixture.componentRef.setInput('images', images);
    fixture.detectChanges();
    for (let i = 0; i < 3; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    return fixture;
  }

  it('every tile carries a download button in v4’s bottom-LEFT corner', async () => {
    const fixture = await render([image({}), image({ id: 'b', filename: 'b.png' })]);
    const buttons = [
      ...fixture.nativeElement.querySelectorAll('button[aria-label="Download image"]'),
    ] as HTMLButtonElement[];
    expect(buttons).toHaveLength(2);
    // v4 puts Download bottom-left and Delete bottom-right on the same overlay.
    expect(buttons[0].className).toContain('left-2');
    expect(buttons[0].getAttribute('title')).toBe('Download image');
    expect(buttons[0].querySelector('qt-icon')?.getAttribute('name')).toBe('download');
  });

  it('a click fetches the tile’s own source and saves it under image.filename', async () => {
    const fixture = await render([image({})]);
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue({ ok: true, blob: async () => new Blob(['b']) } as unknown as Response);

    const button = fixture.nativeElement.querySelector(
      'button[aria-label="Download image"]',
    ) as HTMLButtonElement;
    button.click();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }

    expect(fetchSpy).toHaveBeenCalledWith('/uploads/valid1.png');
    expect(clicked).toEqual({ download: 'valid1.png', href: 'blob:tile' });
  });

  it('the click does NOT fall through to the tile’s select handler', async () => {
    const selected: string[] = [];
    TestBed.configureTestingModule({ imports: [ImageGallery] });
    const fixture = TestBed.createComponent(ImageGallery);
    fixture.componentRef.setInput('images', [image({})]);
    fixture.componentRef.setInput('onSelectImage', (img: ImageData) => selected.push(img.id));
    fixture.detectChanges();
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true,
      blob: async () => new Blob(['b']),
    } as unknown as Response);

    (
      fixture.nativeElement.querySelector(
        'button[aria-label="Download image"]',
      ) as HTMLButtonElement
    ).click();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(selected).toEqual([]);
  });
});
