import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

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
