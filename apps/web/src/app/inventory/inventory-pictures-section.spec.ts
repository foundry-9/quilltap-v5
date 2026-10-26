import { Component } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { CoreDispatchError, type InventoryImageSummary } from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import { SWORD } from './__fixtures__/inventory-ui.fixture';
import type { InventoryContainer } from './inventory-container';
import { InventoryPicturesSection } from './inventory-pictures-section';

/** v4 `InventoryPicturesSection` (`inventory-item-editor.tsx:451-554`, `322a5488e`). */

const VIV: InventoryContainer = { scope: 'character', id: 'c-viv' };
const image = (fileId: string, inherited = false): InventoryImageSummary => ({
  fileId,
  url: `/api/v1/files/${fileId}`,
  thumbnailUrl: `/api/v1/files/${fileId}?action=thumbnail`,
  source: 'GENERATED',
  createdAt: '2026-10-18T00:00:00.000Z',
  ...(inherited ? { inherited: true } : {}),
});

@Component({
  imports: [InventoryPicturesSection],
  template: `<qt-inventory-pictures-section [container]="container" [item]="item" [refreshLists]="refreshLists" (viewingChange)="viewing.push($event)" />`,
})
class Host {
  readonly container = VIV;
  readonly item = SWORD;
  readonly refreshLists = vi.fn(async () => undefined);
  readonly viewing: boolean[] = [];
}

const seen: Array<Record<string, unknown>> = [];
let images: InventoryImageSummary[] = [];
let refuse: Error | null = null;

async function flush(fixture: ComponentFixture<Host>): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(): Promise<{ fixture: ComponentFixture<Host>; el: HTMLElement; host: Host }> {
  seen.length = 0;
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      provideTanStackQuery(new QueryClient()),
      {
        provide: CoreClient,
        useValue: {
          dispatchData: vi.fn(async (req: Record<string, unknown>) => {
            seen.push(req);
            if (req['type'] === 'inventoryItemImagesList') return { current: images[0]?.fileId ?? null, images };
            if (refuse) throw refuse;
            return {};
          }),
        },
      },
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await flush(fixture);
  return { fixture, el: fixture.nativeElement as HTMLElement, host: fixture.componentInstance };
}

const button = (el: HTMLElement, text: string) =>
  [...el.querySelectorAll('button')].find((b) => b.textContent!.trim() === text) as HTMLButtonElement;

describe('InventoryPicturesSection', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    images = [];
    refuse = null;
    document.body.style.overflow = '';
  });

  it('says there are none yet, with the Upload… label and Generate', async () => {
    const { el } = await render();
    expect(el.textContent).toContain('No pictures yet. A picture is for choosing the object in pickers; rows show its glyph.');
    const upload = el.querySelector('label.qt-button-ghost') as HTMLLabelElement;
    expect(upload.textContent!.trim()).toBe('Upload…');
    expect(upload.querySelector('input[type="file"]')!.getAttribute('accept')).toBe('image/*');
    expect(button(el, 'Generate')).toBeDefined();
  });

  it('tiles the pictures, the current one bordered primary; Use / Delete only on its own', async () => {
    images = [image('f1'), image('f2'), image('f3', true)];
    const { el } = await render();
    expect(el.textContent).toContain("Showing the archetype's pictures; upload or generate one to give this object its own.");
    const tiles = [...el.querySelectorAll('li')];
    expect(tiles).toHaveLength(3);
    expect(tiles[0].querySelector('img')!.getAttribute('class')).toBe('w-20 h-20 rounded object-cover border qt-border-primary');
    expect(tiles[1].querySelector('img')!.getAttribute('class')).toBe('w-20 h-20 rounded object-cover border qt-border-default');
    expect([...tiles[0].querySelectorAll('div.flex.gap-1 button')].map((b) => b.textContent!.trim())).toEqual(['Current', 'Delete']);
    expect([...tiles[1].querySelectorAll('div.flex.gap-1 button')].map((b) => b.textContent!.trim())).toEqual(['Use', 'Delete']);
    expect(tiles[2].querySelector('div.flex.gap-1')).toBeNull();
  });

  it('Use and Generate dispatch, then re-read the pictures and the lists', async () => {
    images = [image('f1'), image('f2')];
    const { fixture, el, host } = await render();
    button(el, 'Use').click();
    await flush(fixture);
    expect(seen.filter((r) => r['type'] !== 'inventoryItemImagesList')).toEqual([
      { type: 'inventoryItemImageSetCurrent', container: VIV, itemId: 'sword', fileId: 'f2' },
    ]);
    expect(seen.filter((r) => r['type'] === 'inventoryItemImagesList')).toHaveLength(2);
    expect(host.refreshLists).toHaveBeenCalledTimes(1);
    button(el, 'Generate').click();
    await flush(fixture);
    expect(seen.at(-2)).toEqual({ type: 'inventoryItemImageGenerate', container: VIV, itemId: 'sword' });
  });

  it('a refusal toasts in words, the fallback "The picture would not oblige" for a non-error', async () => {
    images = [image('f1')];
    const { fixture, el } = await render();
    refuse = new CoreDispatchError({ kind: 'bad-request', message: 'No image profile is set' });
    button(el, 'Generate').click();
    await flush(fixture);
    expect(TestBed.inject(ToastService).toasts().at(-1)?.message).toBe('No image profile is set');
  });

  it('a tile opens the viewer and reports it; closing reports it gone', async () => {
    images = [image('f1'), image('f2')];
    const { fixture, el, host } = await render();
    (el.querySelector('[aria-label="View picture full screen"]') as HTMLButtonElement).click();
    await flush(fixture);
    expect(document.body.querySelector('qt-inventory-image-viewer')).not.toBeNull();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await flush(fixture);
    expect(host.viewing).toEqual([true, false]);
  });
});
