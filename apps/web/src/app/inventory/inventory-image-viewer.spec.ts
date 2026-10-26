import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { ToastService } from '../ui/toast.service';
import type { InventoryContainer } from './inventory-container';
import { InventoryImageViewer, pictureFilename } from './inventory-image-viewer';

/** v4 `components/inventory/inventory-image-viewer.tsx` (`322a5488e`). Frame + dialog portal to the body. */

@Component({
  imports: [InventoryImageViewer],
  template: `
    <qt-inventory-image-viewer
      itemId="sword"
      [itemTitle]="title()"
      [container]="container"
      fileId="file-1"
      (closed)="closes.set(closes() + 1)"
    />
  `,
})
class Host {
  readonly title = signal('Excalibur');
  readonly closes = signal(0);
  readonly container: InventoryContainer = { scope: 'character', id: 'c-viv' };
}

const seen: Array<Record<string, unknown>> = [];

async function flush(fixture: ComponentFixture<Host>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(): Promise<ComponentFixture<Host>> {
  seen.length = 0;
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      {
        provide: CoreClient,
        useValue: {
          dispatchData: async (req: Record<string, unknown>) => {
            seen.push(req);
            if (req['type'] === 'inventoryItemImageSaveTargets') {
              return { albums: [{ mountPointId: 'mp-gen', name: 'Quilltap General', kind: 'general', isDefault: true }] };
            }
            return { saved: true, mountPoint: 'Quilltap General', relativePath: 'photos/Excalibur.webp' };
          },
        },
      },
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await flush(fixture);
  return fixture;
}

const byTitle = (title: string) =>
  document.body.querySelector(`qt-full-screen-image-viewer button[title="${title}"]`) as HTMLButtonElement;
const toasts = (): string[] => TestBed.inject(ToastService).toasts().map((t) => t.message);

describe('pictureFilename (v4 :30-33, spec-pinned)', () => {
  it.each([
    ['Brass Compass', 'Brass_Compass.webp'],
    ['  --Café!! ', 'Caf.webp'],
    ['!!!', 'inventory_object.webp'],
    ['', 'inventory_object.webp'],
  ])('%j → %s', (title, expected) => {
    expect(pictureFilename(title)).toBe(expected);
  });
});

describe('InventoryImageViewer', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    TestBed.resetTestingModule();
    document.body.style.overflow = '';
  });

  it('frames the picture at z-[90] with the three actions and the title caption', async () => {
    await render();
    const frame = document.body.querySelector('qt-full-screen-image-viewer')!;
    const dialog = frame.querySelector('[role="dialog"]')!;
    expect(dialog.getAttribute('aria-label')).toBe('Picture of Excalibur');
    expect(dialog.classList.contains('z-[90]')).toBe(true);
    expect(frame.querySelector('img')!.getAttribute('src')).toBe('/api/v1/files/file-1');
    const titles = [...frame.querySelectorAll<HTMLButtonElement>('.top-4 button')].map((b) => b.title);
    expect(titles).toEqual(['Save to a document store', 'Download', 'Copy to clipboard', 'Close (Escape)']);
    expect(frame.querySelector('.qt-bg-overlay-caption')!.textContent!.trim()).toBe('Excalibur');
  });

  it('Save opens the inventory door at z-[100] and toasts where it went', async () => {
    const fixture = await render();
    byTitle('Save to a document store').click();
    await flush(fixture);
    const dialog = document.body.querySelector('qt-save-image-dialog')!;
    expect(dialog.querySelector('.qt-dialog-overlay')!.classList.contains('z-[100]')).toBe(true);
    (dialog.querySelector('.qt-dialog-footer button:last-child') as HTMLButtonElement).click();
    await flush(fixture);
    expect(seen).toEqual([
      { type: 'inventoryItemImageSaveTargets', container: { scope: 'character', id: 'c-viv' }, itemId: 'sword' },
      {
        type: 'inventoryItemImageSaveToStore',
        container: { scope: 'character', id: 'c-viv' },
        itemId: 'sword',
        fileId: 'file-1',
        mountPointId: 'mp-gen',
      },
    ]);
    expect(toasts()).toContain('Saved to Quilltap General');
  });

  it('Download saves under pictureFilename; a throw logs v4’s line and toasts', async () => {
    const fixture = await render();
    const clicked: string[] = [];
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementationOnce(function (this: HTMLAnchorElement) {
      clicked.push(this.download);
    });
    byTitle('Download').click();
    await flush(fixture);
    expect(clicked).toEqual(['Excalibur.webp']);
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {
      throw new Error('blocked');
    });
    const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
    byTitle('Download').click();
    await flush(fixture);
    expect(toasts()).toContain('Failed to download image');
    expect(errors).toHaveBeenCalledWith('Failed to download inventory picture:', { error: 'blocked' });
  });

  it('Copy toasts v4’s success; a refused write logs and toasts "Failed to copy image"', async () => {
    const fixture = await render();
    vi.stubGlobal('fetch', vi.fn(async () => ({ blob: async () => new Blob(['x'], { type: 'image/png' }) })));
    vi.stubGlobal('ClipboardItem', class {
      constructor(readonly items: unknown) {}
    });
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      writable: true,
      value: { write: vi.fn(async () => undefined) },
    });
    byTitle('Copy to clipboard').click();
    await flush(fixture);
    expect(toasts()).toContain('Image copied to clipboard');
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      writable: true,
      value: { write: vi.fn(async () => Promise.reject(new Error('denied'))) },
    });
    const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
    byTitle('Copy to clipboard').click();
    await flush(fixture);
    expect(toasts()).toContain('Failed to copy image');
    expect(errors.mock.calls[0][0]).toBe('Failed to copy inventory picture:');
  });

  it('Escape closes the viewer', async () => {
    const fixture = await render();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(fixture.componentInstance.closes()).toBe(1);
  });
});
